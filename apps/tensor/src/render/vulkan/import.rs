use std::{
    collections::HashMap,
    os::fd::{AsFd, AsRawFd, IntoRawFd},
    ptr::NonNull,
};

use tensor_host::Fourcc;
use thiserror::Error;
use vulkan_renderer::vulkanalia::vk::{
    DeviceV1_0, HasBuilder, KhrExternalMemoryFdExtensionDeviceCommands,
};
use vulkan_renderer::vulkanalia::{Device, vk};

use crate::{ecs::SurfaceBufferId, render::Dmabuf};

use super::vulkan_format_for_fourcc;

/// Imported client images are kept separate from compositor-owned output images.
/// The key is a compositor-assigned stable buffer identity, never a raw file
/// descriptor or an object ID that can be recycled by a client.
#[derive(Default)]
pub(super) struct ClientImageCache {
    active: HashMap<SurfaceBufferId, ImportedClientImage>,
    retired: Vec<ImportedClientImage>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ClientImageInfo {
    pub(super) image: vk::Image,
    pub(super) view_info: vk::ImageViewCreateInfo,
    /// Imported dma-buf storage is owned by the non-Vulkan producer between
    /// submissions.  The frame executor performs an explicit FOREIGN acquire
    /// and release around every sampling pass.
    pub(super) foreign_owned: bool,
    /// The first FOREIGN acquire must pair `UNDEFINED` with the foreign queue
    /// family.  Once a queue submission succeeds, subsequent acquires use the
    /// preserved `GENERAL` layout instead.  This is intentionally a snapshot:
    /// the cache updates it only after queue submission, never while recording.
    pub(super) needs_initial_acquire: bool,
    /// A Tensor-owned SHM image has one pending staging copy after its CPU
    /// snapshot changes. dma-buf imports never carry an upload.
    pub(super) upload: Option<ClientImageUpload>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ClientImageUpload {
    pub(super) buffer: vk::Buffer,
    pub(super) extent: vk::Extent3D,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ShmUploadTarget {
    pub(super) id: SurfaceBufferId,
    pub(super) size: tensor_util::Size,
    pub(super) format: Fourcc,
    pub(super) completed_timeline: u64,
}

impl ClientImageCache {
    pub(super) fn import<F: AsFd>(
        &mut self,
        id: SurfaceBufferId,
        device: &Device,
        dmabuf: &Dmabuf<F>,
    ) -> Result<(), ClientImportError> {
        if self.active.contains_key(&id) {
            return Ok(());
        }

        let image = ImportedClientImage::create_dmabuf(device, dmabuf)?;
        self.active.insert(id, image);
        Ok(())
    }

    pub(super) fn upload_shm(
        &mut self,
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        target: ShmUploadTarget,
        fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
    ) -> Result<(), ClientImportError> {
        let ShmUploadTarget {
            id,
            size,
            format,
            completed_timeline,
        } = target;
        let mut current = self.active.remove(&id);
        if let Some(image) = current.as_mut()
            && image.matches_shm(size, format)
            && image.last_use_timeline <= completed_timeline
        {
            let result = image.write_shm(device, fill);
            self.active
                .insert(id, current.expect("active SHM image was borrowed above"));
            return result;
        }

        let mut image =
            match ImportedClientImage::create_shm(device, memory_properties, size, format) {
                Ok(image) => image,
                Err(error) => {
                    if let Some(current) = current {
                        self.active.insert(id, current);
                    }
                    return Err(error);
                }
            };
        if let Err(error) = image.write_shm(device, fill) {
            image.destroy(device);
            if let Some(current) = current {
                self.active.insert(id, current);
            }
            return Err(error);
        }
        if let Some(current) = current {
            self.retired.push(current);
        }
        self.active.insert(id, image);
        Ok(())
    }

    pub(super) fn release(&mut self, id: SurfaceBufferId) {
        if let Some(image) = self.active.remove(&id) {
            self.retired.push(image);
        }
    }

    pub(super) fn image_info(&self, id: SurfaceBufferId) -> Option<ClientImageInfo> {
        self.active.get(&id).map(|image| ClientImageInfo {
            image: image.image,
            view_info: image.view_info,
            foreign_owned: image.foreign_owned,
            needs_initial_acquire: !image.initialized,
            upload: image.upload_pending.then(|| ClientImageUpload {
                buffer: image
                    .staging
                    .as_ref()
                    .expect("only SHM images have pending uploads")
                    .buffer,
                extent: image.extent,
            }),
        })
    }

    /// Commit imported-image state after the queue accepted a frame.  This is
    /// deliberately separate from command recording: a failed queue submit
    /// must leave a fresh import on the `UNDEFINED` acquire path.
    pub(super) fn mark_submitted(
        &mut self,
        ids: impl IntoIterator<Item = SurfaceBufferId>,
        timeline: u64,
    ) {
        for id in ids {
            if let Some(image) = self.active.get_mut(&id) {
                image.initialized = true;
                image.upload_pending = false;
                image.last_use_timeline = image.last_use_timeline.max(timeline);
            }
        }
    }

    pub(super) fn retire_completed(&mut self, device: &Device, completed_timeline: u64) {
        let mut retained = Vec::with_capacity(self.retired.len());
        for image in self.retired.drain(..) {
            if image.last_use_timeline <= completed_timeline {
                image.destroy(device);
            } else {
                retained.push(image);
            }
        }
        self.retired = retained;
    }

    pub(super) fn destroy(&mut self, device: &Device) {
        for (_, image) in std::mem::take(&mut self.active) {
            image.destroy(device);
        }
        for image in self.retired.drain(..) {
            image.destroy(device);
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.active.len()
    }
}

struct ImportedClientImage {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    view_info: vk::ImageViewCreateInfo,
    extent: vk::Extent3D,
    format: Fourcc,
    foreign_owned: bool,
    staging: Option<StagingBuffer>,
    upload_pending: bool,
    initialized: bool,
    last_use_timeline: u64,
}

struct StagingBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: NonNull<u8>,
    len: usize,
    coherent: bool,
}

impl ImportedClientImage {
    fn create_dmabuf<F: AsFd>(
        device: &Device,
        dmabuf: &Dmabuf<F>,
    ) -> Result<Self, ClientImportError> {
        let format = dmabuf.format;
        let host_code = format.code;
        let vulkan_format = vulkan_format_for_fourcc(host_code)
            .ok_or(ClientImportError::UnsupportedFourcc(host_code))?;
        let shape = validate_shape(dmabuf)?;
        let fd = &dmabuf.planes.first().ok_or(ClientImportError::NoPlanes)?.fd;
        let plane_layout = vk::SubresourceLayout::builder()
            .offset(u64::from(shape.offset))
            .size(0)
            .row_pitch(u64::from(shape.stride))
            .array_pitch(0)
            .depth_pitch(0)
            .build();
        let plane_layouts = [plane_layout];
        let mut modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::builder()
            .drm_format_modifier(format.modifier.raw())
            .plane_layouts(&plane_layouts);
        let mut external_info = vk::ExternalMemoryImageCreateInfo::builder()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let image_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::_2D)
            .format(vulkan_format)
            .extent(vk::Extent3D {
                width: shape.width,
                height: shape.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut modifier_info)
            .push_next(&mut external_info);
        let image = unsafe { device.create_image(&image_info, None) }
            .map_err(ClientImportError::CreateImage)?;

        let result = Self::allocate_and_bind(device, image, fd);
        let memory = match result {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { device.destroy_image(image, None) };
                return Err(error);
            }
        };

        let components = vk::ComponentMapping {
            r: vk::ComponentSwizzle::IDENTITY,
            g: vk::ComponentSwizzle::IDENTITY,
            b: vk::ComponentSwizzle::IDENTITY,
            a: if is_opaque(host_code) {
                vk::ComponentSwizzle::ONE
            } else {
                vk::ComponentSwizzle::IDENTITY
            },
        };
        let view_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::_2D)
            .format(vulkan_format)
            .components(components)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            );
        let view_info = view_info.build();
        let view = match unsafe { device.create_image_view(&view_info, None) } {
            Ok(view) => view,
            Err(error) => {
                unsafe {
                    device.destroy_image(image, None);
                    device.free_memory(memory, None);
                }
                return Err(ClientImportError::CreateView(error));
            }
        };

        Ok(Self {
            image,
            memory,
            view,
            view_info,
            extent: vk::Extent3D {
                width: shape.width,
                height: shape.height,
                depth: 1,
            },
            format: host_code,
            foreign_owned: true,
            staging: None,
            upload_pending: false,
            initialized: false,
            last_use_timeline: 0,
        })
    }

    fn create_shm(
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        size: tensor_util::Size,
        format: Fourcc,
    ) -> Result<Self, ClientImportError> {
        let vulkan_format =
            vulkan_format_for_fourcc(format).ok_or(ClientImportError::UnsupportedFourcc(format))?;
        let extent = vk::Extent3D {
            width: size.width,
            height: size.height,
            depth: 1,
        };
        if extent.width == 0 || extent.height == 0 {
            return Err(ClientImportError::InvalidDimensions);
        }
        let image_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::_2D)
            .format(vulkan_format)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { device.create_image(&image_info, None) }
            .map_err(ClientImportError::CreateImage)?;
        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let Some(memory_type_index) = select_memory_type(
            memory_properties,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::empty(),
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        ) else {
            unsafe { device.destroy_image(image, None) };
            return Err(ClientImportError::NoCompatibleMemoryType);
        };
        let allocation = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        let memory = match unsafe { device.allocate_memory(&allocation, None) } {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { device.destroy_image(image, None) };
                return Err(ClientImportError::AllocateMemory(error));
            }
        };
        if let Err(error) = unsafe { device.bind_image_memory(image, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            return Err(ClientImportError::BindMemory(error));
        }

        let components = vk::ComponentMapping {
            r: vk::ComponentSwizzle::IDENTITY,
            g: vk::ComponentSwizzle::IDENTITY,
            b: vk::ComponentSwizzle::IDENTITY,
            a: if is_opaque(format) {
                vk::ComponentSwizzle::ONE
            } else {
                vk::ComponentSwizzle::IDENTITY
            },
        };
        let view_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::_2D)
            .format(vulkan_format)
            .components(components)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            )
            .build();
        let view = match unsafe { device.create_image_view(&view_info, None) } {
            Ok(view) => view,
            Err(error) => {
                unsafe {
                    device.free_memory(memory, None);
                    device.destroy_image(image, None);
                }
                return Err(ClientImportError::CreateView(error));
            }
        };
        let staging = match StagingBuffer::new(device, memory_properties, size) {
            Ok(staging) => staging,
            Err(error) => {
                unsafe {
                    device.destroy_image_view(view, None);
                    device.free_memory(memory, None);
                    device.destroy_image(image, None);
                }
                return Err(error);
            }
        };
        Ok(Self {
            image,
            memory,
            view,
            view_info,
            extent,
            format,
            foreign_owned: false,
            staging: Some(staging),
            upload_pending: false,
            initialized: false,
            last_use_timeline: 0,
        })
    }

    fn matches_shm(&self, size: tensor_util::Size, format: Fourcc) -> bool {
        !self.foreign_owned
            && self.extent.width == size.width
            && self.extent.height == size.height
            && self.format == format
    }

    fn write_shm(
        &mut self,
        device: &Device,
        fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
    ) -> Result<(), ClientImportError> {
        let staging = self
            .staging
            .as_mut()
            .ok_or(ClientImportError::NotShmImage)?;
        let destination =
            unsafe { std::slice::from_raw_parts_mut(staging.mapped.as_ptr(), staging.len) };
        if let Err(error) = fill(destination) {
            self.upload_pending = false;
            return Err(ClientImportError::ShmSource(error));
        }
        if !staging.coherent {
            let range = vk::MappedMemoryRange::builder()
                .memory(staging.memory)
                .offset(0)
                .size(vk::WHOLE_SIZE);
            unsafe { device.flush_mapped_memory_ranges(std::slice::from_ref(&range)) }
                .map_err(ClientImportError::FlushMemory)?;
        }
        self.upload_pending = true;
        Ok(())
    }

    fn allocate_and_bind(
        device: &Device,
        image: vk::Image,
        fd: impl AsFd,
    ) -> Result<vk::DeviceMemory, ClientImportError> {
        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
        let raw_fd = fd.as_fd().as_raw_fd();
        unsafe {
            device
                .get_memory_fd_properties_khr(
                    vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                    raw_fd,
                    &mut fd_properties,
                )
                .map_err(ClientImportError::FdProperties)?;
        }
        let compatible_bits = requirements.memory_type_bits & fd_properties.memory_type_bits;
        if compatible_bits == 0 {
            return Err(ClientImportError::NoCompatibleMemoryType);
        }
        let memory_type_index = compatible_bits.trailing_zeros();
        let owned_fd = fd
            .as_fd()
            .try_clone_to_owned()
            .map_err(|error| ClientImportError::DuplicateFd(error.to_string()))?;
        let mut import_info = vk::ImportMemoryFdInfoKHR::builder()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(owned_fd.into_raw_fd());
        let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::builder().image(image);
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut import_info)
            .push_next(&mut dedicated_info);
        let memory = unsafe { device.allocate_memory(&allocation_info, None) }
            .map_err(ClientImportError::AllocateMemory)?;
        if let Err(error) = unsafe { device.bind_image_memory(image, memory, 0) } {
            unsafe { device.free_memory(memory, None) };
            return Err(ClientImportError::BindMemory(error));
        }
        Ok(memory)
    }

    fn destroy(self, device: &Device) {
        unsafe {
            if let Some(staging) = self.staging {
                staging.destroy(device);
            }
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }
}

impl StagingBuffer {
    fn new(
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        size: tensor_util::Size,
    ) -> Result<Self, ClientImportError> {
        let len = usize::try_from(size.width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .and_then(|stride| stride.checked_mul(usize::try_from(size.height).ok()?))
            .ok_or(ClientImportError::InvalidDimensions)?;
        let buffer_info = vk::BufferCreateInfo::builder()
            .size(u64::try_from(len).map_err(|_| ClientImportError::InvalidDimensions)?)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { device.create_buffer(&buffer_info, None) }
            .map_err(ClientImportError::CreateStagingBuffer)?;
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let Some(memory_type_index) = select_memory_type(
            memory_properties,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE,
            vk::MemoryPropertyFlags::HOST_COHERENT,
        ) else {
            unsafe { device.destroy_buffer(buffer, None) };
            return Err(ClientImportError::NoHostVisibleMemoryType);
        };
        let memory_type = memory_properties.memory_types[memory_type_index as usize];
        let allocation = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        let memory = match unsafe { device.allocate_memory(&allocation, None) } {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { device.destroy_buffer(buffer, None) };
                return Err(ClientImportError::AllocateStagingMemory(error));
            }
        };
        if let Err(error) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_buffer(buffer, None);
            }
            return Err(ClientImportError::BindStagingMemory(error));
        }
        let mapped = match unsafe {
            device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
        } {
            Ok(mapped) => match NonNull::new(mapped.cast::<u8>()) {
                Some(mapped) => mapped,
                None => {
                    unsafe {
                        device.unmap_memory(memory);
                        device.free_memory(memory, None);
                        device.destroy_buffer(buffer, None);
                    }
                    return Err(ClientImportError::NullStagingMap);
                }
            },
            Err(error) => {
                unsafe {
                    device.free_memory(memory, None);
                    device.destroy_buffer(buffer, None);
                }
                return Err(ClientImportError::MapStagingMemory(error));
            }
        };
        Ok(Self {
            buffer,
            memory,
            mapped,
            len,
            coherent: memory_type
                .property_flags
                .contains(vk::MemoryPropertyFlags::HOST_COHERENT),
        })
    }

    unsafe fn destroy(self, device: &Device) {
        unsafe {
            device.unmap_memory(self.memory);
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
    }
}

fn select_memory_type(
    properties: &vk::PhysicalDeviceMemoryProperties,
    compatible_bits: u32,
    required: vk::MemoryPropertyFlags,
    preferred: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let count = usize::try_from(properties.memory_type_count).ok()?;
    let compatible =
        properties.memory_types[..count]
            .iter()
            .enumerate()
            .filter(|(index, memory_type)| {
                compatible_bits & (1 << index) != 0 && memory_type.property_flags.contains(required)
            });
    compatible
        .clone()
        .find(|(_, memory_type)| memory_type.property_flags.contains(preferred))
        .or_else(|| compatible.into_iter().next())
        .and_then(|(index, _)| u32::try_from(index).ok())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ImportShape {
    width: u32,
    height: u32,
    offset: u32,
    stride: u32,
}

fn validate_shape<F>(dmabuf: &Dmabuf<F>) -> Result<ImportShape, ClientImportError> {
    let width = dmabuf.size.width;
    let height = dmabuf.size.height;
    if width == 0 || height == 0 {
        return Err(ClientImportError::InvalidDimensions);
    }
    if dmabuf.planes.len() != 1 {
        return Err(ClientImportError::UnsupportedPlaneCount(
            dmabuf.planes.len(),
        ));
    }
    if dmabuf.format.modifier.is_invalid() {
        return Err(ClientImportError::ImplicitModifier);
    }
    let plane = dmabuf.planes.first().ok_or(ClientImportError::NoPlanes)?;
    let offset = plane.offset;
    let stride = plane.stride;
    if stride == 0 {
        return Err(ClientImportError::InvalidStride);
    }
    Ok(ImportShape {
        width,
        height,
        offset,
        stride,
    })
}

fn is_opaque(format: Fourcc) -> bool {
    matches!(
        format,
        Fourcc::XRGB8888 | Fourcc::XBGR8888 | Fourcc::XRGB2101010 | Fourcc::XBGR2101010
    )
}

#[derive(Debug, Error)]
pub(super) enum ClientImportError {
    #[error("DRM fourcc {0} has no Vulkan client-import format")]
    UnsupportedFourcc(Fourcc),
    #[error("client dma-buf dimensions must be positive and fit Vulkan's extent")]
    InvalidDimensions,
    #[error("client dma-buf has no planes")]
    NoPlanes,
    #[error("client dma-buf has unsupported plane count {0}; only one-plane RGB is enabled")]
    UnsupportedPlaneCount(usize),
    #[error("client dma-buf uses an implicit modifier")]
    ImplicitModifier,
    #[error("client dma-buf has a zero row stride")]
    InvalidStride,
    #[error("failed to query dma-buf memory compatibility: {0:?}")]
    FdProperties(vk::ErrorCode),
    #[error("client dma-buf has no Vulkan-compatible memory type")]
    NoCompatibleMemoryType,
    #[error("failed to duplicate client dma-buf fd: {0}")]
    DuplicateFd(String),
    #[error("failed to create client dma-buf image: {0:?}")]
    CreateImage(vk::ErrorCode),
    #[error("failed to allocate imported client dma-buf memory: {0:?}")]
    AllocateMemory(vk::ErrorCode),
    #[error("failed to bind imported client dma-buf memory: {0:?}")]
    BindMemory(vk::ErrorCode),
    #[error("failed to create client dma-buf image view: {0:?}")]
    CreateView(vk::ErrorCode),
    #[error("renderer image is not backed by SHM staging memory")]
    NotShmImage,
    #[error("failed to read client SHM pixels: {0}")]
    ShmSource(String),
    #[error("failed to create client SHM staging buffer: {0:?}")]
    CreateStagingBuffer(vk::ErrorCode),
    #[error("client SHM staging has no host-visible Vulkan memory type")]
    NoHostVisibleMemoryType,
    #[error("failed to allocate client SHM staging memory: {0:?}")]
    AllocateStagingMemory(vk::ErrorCode),
    #[error("failed to bind client SHM staging memory: {0:?}")]
    BindStagingMemory(vk::ErrorCode),
    #[error("failed to map client SHM staging memory: {0:?}")]
    MapStagingMemory(vk::ErrorCode),
    #[error("Vulkan returned a null client SHM staging mapping")]
    NullStagingMap,
    #[error("failed to flush client SHM staging memory: {0:?}")]
    FlushMemory(vk::ErrorCode),
}

#[cfg(test)]
mod tests;

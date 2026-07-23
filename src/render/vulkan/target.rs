use std::{
    collections::BTreeMap,
    os::fd::{FromRawFd, OwnedFd},
};

use smithay::{
    backend::{
        allocator::{
            Fourcc,
            dmabuf::{Dmabuf, DmabufFlags, MAX_PLANES},
        },
        drm::DrmNode,
    },
    reexports::rustix::fs::makedev,
};
use thiserror::Error;
use vulkanalia::vk::{
    DeviceV1_0, ExtImageDrmFormatModifierExtensionDeviceCommands, HasBuilder, InstanceV1_0,
    KhrExternalMemoryFdExtensionDeviceCommands,
};
use vulkanalia::{Device, Instance, vk};

use crate::render::{DrmNodeId, NativeOutputTarget, RenderOutputId};

use super::{native_image_usage, vulkan_format_for_fourcc};

const OUTPUT_IMAGE_COUNT: usize = 3;

#[derive(Clone, Debug)]
pub(crate) struct NativeOutputBuffer {
    pub(crate) slot: u8,
    pub(crate) dmabuf: Dmabuf,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct NativeOutputImageInfo {
    pub(super) image: vk::Image,
    pub(super) view_info: vk::ImageViewCreateInfo,
    pub(super) foreign_owned: bool,
}

impl NativeOutputBuffer {
    pub(crate) const COUNT: usize = OUTPUT_IMAGE_COUNT;
}

pub(super) struct NativeTargetManager {
    render_node: DrmNode,
    active: BTreeMap<RenderOutputId, NativeTargetSet>,
    retired: Vec<NativeTargetSet>,
}

impl NativeTargetManager {
    pub(super) fn new(render_node: DrmNodeId) -> Result<Self, NativeTargetError> {
        let device_id = makedev(render_node.major(), render_node.minor());
        let render_node = DrmNode::from_dev_id(device_id)
            .map_err(|error| NativeTargetError::DrmNode(error.to_string()))?;
        Ok(Self {
            render_node,
            active: BTreeMap::new(),
            retired: Vec::new(),
        })
    }

    pub(super) fn register(
        &mut self,
        instance: &Instance,
        device: &Device,
        physical_device: vk::PhysicalDevice,
        target: NativeOutputTarget,
    ) -> Result<Vec<NativeOutputBuffer>, NativeTargetError> {
        if self
            .active
            .get(&target.output)
            .is_some_and(|current| current.target == target)
        {
            return Ok(self.buffers(target.output));
        }

        let replacement =
            NativeTargetSet::create(instance, device, physical_device, self.render_node, target)?;
        if let Some(previous) = self.active.insert(target.output, replacement) {
            self.retired.push(previous);
        }
        Ok(self.buffers(target.output))
    }

    pub(super) fn mark_submitted(&mut self, output: RenderOutputId, slot: u8, timeline_value: u64) {
        if let Some(target) = self.active.get_mut(&output) {
            target.last_use_timeline = target.last_use_timeline.max(timeline_value);
            if let Some(image) = target.images.get_mut(usize::from(slot)) {
                image.foreign_owned = true;
            }
        }
    }

    pub(super) fn image_info(
        &self,
        output: RenderOutputId,
        slot: u8,
    ) -> Option<NativeOutputImageInfo> {
        self.active
            .get(&output)
            .and_then(|target| target.images.get(usize::from(slot)))
            .map(|image| NativeOutputImageInfo {
                image: image.image,
                view_info: image.view_info,
                foreign_owned: image.foreign_owned,
            })
    }

    pub(super) fn unregister(&mut self, output: RenderOutputId) {
        if let Some(target) = self.active.remove(&output) {
            self.retired.push(target);
        }
    }

    pub(super) fn retire_completed(&mut self, device: &Device, completed_timeline: u64) {
        let mut retained = Vec::with_capacity(self.retired.len());
        for target in self.retired.drain(..) {
            if target.last_use_timeline <= completed_timeline {
                target.destroy(device);
            } else {
                retained.push(target);
            }
        }
        self.retired = retained;
    }

    pub(super) fn destroy(&mut self, device: &Device) {
        for (_, target) in std::mem::take(&mut self.active) {
            target.destroy(device);
        }
        for target in self.retired.drain(..) {
            target.destroy(device);
        }
    }

    fn buffers(&self, output: RenderOutputId) -> Vec<NativeOutputBuffer> {
        self.active
            .get(&output)
            .into_iter()
            .flat_map(|target| target.images.iter())
            .enumerate()
            .map(|(slot, image)| NativeOutputBuffer {
                slot: u8::try_from(slot).expect("native output slot count fits in u8"),
                dmabuf: image.dmabuf.clone(),
            })
            .collect()
    }
}

struct NativeTargetSet {
    target: NativeOutputTarget,
    images: Vec<NativeOutputImage>,
    last_use_timeline: u64,
}

impl NativeTargetSet {
    fn create(
        instance: &Instance,
        device: &Device,
        physical_device: vk::PhysicalDevice,
        render_node: DrmNode,
        target: NativeOutputTarget,
    ) -> Result<Self, NativeTargetError> {
        let mut images = Vec::with_capacity(OUTPUT_IMAGE_COUNT);
        for slot in 0..OUTPUT_IMAGE_COUNT {
            match NativeOutputImage::create(instance, device, physical_device, render_node, target)
            {
                Ok(image) => images.push(image),
                Err(source) => {
                    for image in images {
                        image.destroy(device);
                    }
                    return Err(NativeTargetError::CreateSlot { slot, source });
                }
            }
        }
        Ok(Self {
            target,
            images,
            last_use_timeline: 0,
        })
    }

    fn destroy(self, device: &Device) {
        for image in self.images {
            image.destroy(device);
        }
    }
}

struct NativeOutputImage {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    view_info: vk::ImageViewCreateInfo,
    foreign_owned: bool,
    dmabuf: Dmabuf,
}

impl NativeOutputImage {
    fn create(
        instance: &Instance,
        device: &Device,
        physical_device: vk::PhysicalDevice,
        render_node: DrmNode,
        target: NativeOutputTarget,
    ) -> Result<Self, NativeImageError> {
        let vulkan_format = vulkan_format_for_fourcc(target.format.format.code).ok_or(
            NativeImageError::UnsupportedFourcc(target.format.format.code),
        )?;
        let drm_modifier = u64::from(target.format.format.modifier);
        let modifiers = [drm_modifier];
        let mut modifier_info =
            vk::ImageDrmFormatModifierListCreateInfoEXT::builder().drm_format_modifiers(&modifiers);
        let mut external_info = vk::ExternalMemoryImageCreateInfo::builder()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let image_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::_2D)
            .format(vulkan_format)
            .extent(vk::Extent3D {
                width: target.viewport.width,
                height: target.viewport.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(native_image_usage())
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut modifier_info)
            .push_next(&mut external_info);
        let image = unsafe { device.create_image(&image_info, None) }
            .map_err(NativeImageError::CreateImage)?;

        let result = Self::allocate_and_export(
            instance,
            device,
            physical_device,
            render_node,
            target,
            vulkan_format,
            drm_modifier,
            image,
        );
        if result.is_err() {
            unsafe { device.destroy_image(image, None) };
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn allocate_and_export(
        instance: &Instance,
        device: &Device,
        physical_device: vk::PhysicalDevice,
        render_node: DrmNode,
        target: NativeOutputTarget,
        vulkan_format: vk::Format,
        drm_modifier: u64,
        image: vk::Image,
    ) -> Result<Self, NativeImageError> {
        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let memory_type_index = select_memory_type(
            &memory_properties,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .ok_or(NativeImageError::NoCompatibleMemoryType)?;
        let mut export_info = vk::ExportMemoryAllocateInfo::builder()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::builder().image(image);
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut export_info)
            .push_next(&mut dedicated_info);
        let memory = unsafe { device.allocate_memory(&allocation_info, None) }
            .map_err(NativeImageError::AllocateMemory)?;
        if let Err(source) = unsafe { device.bind_image_memory(image, memory, 0) } {
            unsafe { device.free_memory(memory, None) };
            return Err(NativeImageError::BindMemory(source));
        }

        let view_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::_2D)
            .format(vulkan_format)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            );
        let view = match unsafe { device.create_image_view(&view_info, None) } {
            Ok(view) => view,
            Err(source) => {
                unsafe { device.free_memory(memory, None) };
                return Err(NativeImageError::CreateView(source));
            }
        };
        let dmabuf = match export_dmabuf(device, render_node, target, drm_modifier, image, memory) {
            Ok(dmabuf) => dmabuf,
            Err(source) => {
                unsafe {
                    device.destroy_image_view(view, None);
                    device.free_memory(memory, None);
                }
                return Err(source);
            }
        };
        Ok(Self {
            image,
            memory,
            view,
            view_info: view_info.build(),
            foreign_owned: false,
            dmabuf,
        })
    }

    fn destroy(self, device: &Device) {
        unsafe {
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }
}

fn export_dmabuf(
    device: &Device,
    render_node: DrmNode,
    target: NativeOutputTarget,
    expected_modifier: u64,
    image: vk::Image,
    memory: vk::DeviceMemory,
) -> Result<Dmabuf, NativeImageError> {
    let mut modifier_properties = vk::ImageDrmFormatModifierPropertiesEXT::default();
    unsafe { device.get_image_drm_format_modifier_properties_ext(image, &mut modifier_properties) }
        .map_err(NativeImageError::QueryModifier)?;
    if modifier_properties.drm_format_modifier != expected_modifier {
        return Err(NativeImageError::ModifierMismatch {
            expected: expected_modifier,
            actual: modifier_properties.drm_format_modifier,
        });
    }
    let fd_info = vk::MemoryGetFdInfoKHR::builder()
        .memory(memory)
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let raw_fd =
        unsafe { device.get_memory_fd_khr(&fd_info) }.map_err(NativeImageError::ExportMemoryFd)?;
    let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    let width = i32::try_from(target.viewport.width).map_err(|_| NativeImageError::SizeOverflow)?;
    let height =
        i32::try_from(target.viewport.height).map_err(|_| NativeImageError::SizeOverflow)?;
    let mut builder = Dmabuf::builder(
        (width, height),
        target.format.format.code,
        target.format.format.modifier,
        DmabufFlags::empty(),
    );
    let plane_count = usize::try_from(target.format.plane_count)
        .map_err(|_| NativeImageError::InvalidPlaneCount(target.format.plane_count))?;
    if plane_count == 0 || plane_count > MAX_PLANES {
        return Err(NativeImageError::InvalidPlaneCount(
            target.format.plane_count,
        ));
    }
    for plane in 0..plane_count {
        let subresource = vk::ImageSubresource::builder()
            .aspect_mask(memory_plane_aspect(plane))
            .mip_level(0)
            .array_layer(0);
        let layout = unsafe { device.get_image_subresource_layout(image, &subresource) };
        let offset = u32::try_from(layout.offset).map_err(|_| NativeImageError::LayoutOverflow)?;
        let stride =
            u32::try_from(layout.row_pitch).map_err(|_| NativeImageError::LayoutOverflow)?;
        let plane_fd = fd
            .try_clone()
            .map_err(|source| NativeImageError::DuplicateFd(source.to_string()))?;
        if !builder.add_plane(plane_fd, offset, stride) {
            return Err(NativeImageError::InvalidPlaneCount(
                target.format.plane_count,
            ));
        }
    }
    builder.set_node(render_node);
    builder.build().ok_or(NativeImageError::AssembleDmabuf)
}

fn memory_plane_aspect(plane: usize) -> vk::ImageAspectFlags {
    match plane {
        0 => vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
        1 => vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
        2 => vk::ImageAspectFlags::MEMORY_PLANE_2_EXT,
        3 => vk::ImageAspectFlags::MEMORY_PLANE_3_EXT,
        _ => unreachable!("plane count is bounded by Smithay MAX_PLANES"),
    }
}

fn select_memory_type(
    properties: &vk::PhysicalDeviceMemoryProperties,
    compatible_bits: u32,
    preferred: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let count = usize::try_from(properties.memory_type_count).ok()?;
    let compatible = properties.memory_types[..count]
        .iter()
        .enumerate()
        .filter(|(index, _)| compatible_bits & (1 << index) != 0);
    compatible
        .clone()
        .find(|(_, memory_type)| memory_type.property_flags.contains(preferred))
        .or_else(|| compatible.into_iter().next())
        .and_then(|(index, _)| u32::try_from(index).ok())
}

#[derive(Debug, Error)]
pub(super) enum NativeTargetError {
    #[error("failed to resolve the selected DRM render node: {0}")]
    DrmNode(String),
    #[error("failed to create native output image slot {slot}: {source}")]
    CreateSlot {
        slot: usize,
        source: NativeImageError,
    },
}

#[derive(Debug, Error)]
pub(super) enum NativeImageError {
    #[error("DRM fourcc {0} has no Vulkan output format")]
    UnsupportedFourcc(Fourcc),
    #[error("failed to create the explicit-modifier Vulkan image: {0:?}")]
    CreateImage(vk::ErrorCode),
    #[error("the output image has no compatible Vulkan memory type")]
    NoCompatibleMemoryType,
    #[error("failed to allocate exportable dedicated image memory: {0:?}")]
    AllocateMemory(vk::ErrorCode),
    #[error("failed to bind output image memory: {0:?}")]
    BindMemory(vk::ErrorCode),
    #[error("failed to create the output image view: {0:?}")]
    CreateView(vk::ErrorCode),
    #[error("failed to query the created image modifier: {0:?}")]
    QueryModifier(vk::ErrorCode),
    #[error("Vulkan created modifier {actual:#x} instead of requested modifier {expected:#x}")]
    ModifierMismatch { expected: u64, actual: u64 },
    #[error("failed to export Vulkan image memory as a dma-buf fd: {0:?}")]
    ExportMemoryFd(vk::ErrorCode),
    #[error("failed to duplicate a dma-buf plane fd: {0}")]
    DuplicateFd(String),
    #[error("output dimensions exceed Smithay's signed buffer coordinates")]
    SizeOverflow,
    #[error("dma-buf plane offset or stride exceeds the Linux u32 ABI")]
    LayoutOverflow,
    #[error("native output reports unsupported plane count {0}")]
    InvalidPlaneCount(u32),
    #[error("failed to assemble the exported Smithay dma-buf")]
    AssembleDmabuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_type_prefers_device_local_and_falls_back_to_compatible() {
        let mut properties = vk::PhysicalDeviceMemoryProperties {
            memory_type_count: 3,
            ..Default::default()
        };
        properties.memory_types[0].property_flags = vk::MemoryPropertyFlags::HOST_VISIBLE;
        properties.memory_types[1].property_flags = vk::MemoryPropertyFlags::DEVICE_LOCAL;
        properties.memory_types[2].property_flags = vk::MemoryPropertyFlags::empty();

        assert_eq!(
            select_memory_type(&properties, 0b111, vk::MemoryPropertyFlags::DEVICE_LOCAL),
            Some(1)
        );
        assert_eq!(
            select_memory_type(&properties, 0b100, vk::MemoryPropertyFlags::DEVICE_LOCAL),
            Some(2)
        );
        assert_eq!(
            select_memory_type(&properties, 0, vk::MemoryPropertyFlags::DEVICE_LOCAL),
            None
        );
    }

    #[test]
    fn every_supported_dmabuf_plane_has_a_vulkan_memory_aspect() {
        assert_eq!(
            (0..MAX_PLANES).map(memory_plane_aspect).collect::<Vec<_>>(),
            vec![
                vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
                vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
                vk::ImageAspectFlags::MEMORY_PLANE_2_EXT,
                vk::ImageAspectFlags::MEMORY_PLANE_3_EXT,
            ]
        );
    }
}

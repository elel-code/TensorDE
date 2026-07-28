//! Linux dma-buf external-memory interoperability.
//!
//! Imported allocations are always dedicated and never enter the ordinary
//! buffer/image suballocator. This keeps foreign ownership and fd lifetime
//! rules visible at the API boundary.

use std::fmt;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd};
use std::sync::Arc;

use vulkanalia::{
    Instance,
    prelude::v1_4::*,
    vk::{self, KhrExternalMemoryFdExtensionDeviceCommands},
};

use crate::backend::DeviceOwner;
use crate::{Adapter, Backend, Error, Features, ResourceBinding, Result};

mod export;

pub use export::{DmaBufExportDescriptor, DmaBufExportPlane, ExportedDmaBufImage};

/// One explicit DRM memory-plane layout inside a dma-buf allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmaBufPlaneLayout {
    pub offset: u64,
    pub row_pitch: u64,
}

/// Descriptor for importing one Linux dma-buf fd with one to four explicit DRM
/// memory-plane layouts as a Vulkan image.
///
/// DRM fourcc-to-Vulkan-format policy belongs to the host integration layer;
/// this descriptor receives the already selected Vulkan format and modifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DmaBufImageDescriptor {
    pub label: Option<String>,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub modifier: u64,
    pub planes: Vec<DmaBufPlaneLayout>,
    pub usage: vk::ImageUsageFlags,
    pub components: vk::ComponentMapping,
}

impl DmaBufImageDescriptor {
    fn validate(&self) -> Result<()> {
        if self.format == vk::Format::UNDEFINED {
            return Err(Error::Validation(
                "dma-buf image format must not be VK_FORMAT_UNDEFINED".into(),
            ));
        }
        if self.extent.width == 0 || self.extent.height == 0 {
            return Err(Error::Validation(
                "dma-buf image extent must be non-zero".into(),
            ));
        }
        if self.modifier == u64::MAX {
            return Err(Error::Validation(
                "dma-buf import requires an explicit DRM format modifier".into(),
            ));
        }
        if self.planes.is_empty() || self.planes.len() > 4 {
            return Err(Error::Validation(
                "dma-buf import requires one to four memory-plane layouts".into(),
            ));
        }
        if self.planes.iter().any(|plane| plane.row_pitch == 0) {
            return Err(Error::Validation(
                "every dma-buf plane row pitch must be non-zero".into(),
            ));
        }
        if self.usage.is_empty() {
            return Err(Error::Validation(
                "dma-buf image usage must be non-empty".into(),
            ));
        }
        Ok(())
    }
}

/// Per-modifier external-memory support for one Vulkan format and usage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrmFormatModifierCapability {
    pub modifier: u64,
    pub plane_count: u32,
    pub tiling_features: vk::FormatFeatureFlags2,
    pub importable: bool,
    pub exportable: bool,
}

/// Cloneable ownership handle for a dedicated imported dma-buf image and its
/// default full-color view.
#[derive(Clone)]
pub struct ImportedDmaBufImage {
    inner: Arc<ImportedDmaBufImageInner>,
}

impl fmt::Debug for ImportedDmaBufImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImportedDmaBufImage")
            .field("label", &self.inner.label)
            .field("image", &self.inner.image)
            .field("view", &self.inner.view)
            .field("format", &self.inner.format)
            .field("extent", &self.inner.extent)
            .field("modifier", &self.inner.modifier)
            .field("usage", &self.inner.usage)
            .finish_non_exhaustive()
    }
}

impl ImportedDmaBufImage {
    pub fn raw(&self) -> vk::Image {
        self.inner.image
    }

    pub fn view(&self) -> vk::ImageView {
        self.inner.view
    }

    pub fn format(&self) -> vk::Format {
        self.inner.format
    }

    pub fn extent(&self) -> vk::Extent2D {
        self.inner.extent
    }

    pub fn modifier(&self) -> u64 {
        self.inner.modifier
    }

    pub fn usage(&self) -> vk::ImageUsageFlags {
        self.inner.usage
    }

    pub fn label(&self) -> Option<&str> {
        self.inner.label.as_deref()
    }

    pub const fn subresource_range(&self) -> vk::ImageSubresourceRange {
        color_subresource_range()
    }

    /// Reconstructs the create-info consumed by `VK_EXT_descriptor_heap` image
    /// descriptor writes. The returned value borrows no temporary pNext data.
    pub fn view_create_info(&self) -> vk::ImageViewCreateInfo {
        vk::ImageViewCreateInfo::builder()
            .image(self.inner.image)
            .view_type(vk::ImageViewType::_2D)
            .format(self.inner.format)
            .components(self.inner.components)
            .subresource_range(self.subresource_range())
            .build()
    }

    /// Creates the raw binding used to resolve this image in a render graph.
    pub fn resource_binding(&self) -> ResourceBinding {
        ResourceBinding::Image {
            image: self.inner.image,
            subresource_range: self.subresource_range(),
        }
    }

    pub(crate) fn owner(&self) -> &Arc<DeviceOwner> {
        &self.inner.owner
    }
}

impl crate::SubmissionResource for ImportedDmaBufImage {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.inner))
    }
}

struct ImportedDmaBufImageInner {
    owner: Arc<DeviceOwner>,
    image: vk::Image,
    memories: Vec<vk::DeviceMemory>,
    view: vk::ImageView,
    format: vk::Format,
    extent: vk::Extent2D,
    modifier: u64,
    usage: vk::ImageUsageFlags,
    components: vk::ComponentMapping,
    label: Option<String>,
}

impl Drop for ImportedDmaBufImageInner {
    fn drop(&mut self) {
        unsafe {
            self.owner.device.destroy_image_view(self.view, None);
            self.owner.device.destroy_image(self.image, None);
            for memory in &self.memories {
                self.owner.device.free_memory(*memory, None);
            }
        }
    }
}

impl Adapter {
    /// Queries DRM modifier support for one exact format and usage before
    /// logical-device creation. Unsupported modifiers are omitted.
    pub fn drm_format_modifier_capabilities(
        &self,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Result<Vec<DrmFormatModifierCapability>> {
        query_modifier_capabilities(
            &self.instance_owner().instance,
            self.physical_device(),
            &self.info().extensions,
            format,
            usage,
        )
    }
}

impl Backend {
    /// Queries DRM modifier support for one exact format and usage on this
    /// device's physical adapter.
    pub fn drm_format_modifier_capabilities(
        &self,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Result<Vec<DrmFormatModifierCapability>> {
        let owner = self.shared_owner();
        query_modifier_capabilities(
            &owner.instance_owner().instance,
            owner.physical_device(),
            &self.device_info().extensions,
            format,
            usage,
        )
    }

    /// Imports one explicit-modifier Linux dma-buf fd. Multiple memory-plane
    /// layouts are supported when every plane resides in that same fd;
    /// disjoint per-plane fd import is a separate contract.
    ///
    /// The input fd is duplicated. Vulkan owns the duplicate after successful
    /// allocation; the caller retains ownership of its original fd.
    pub fn import_dma_buf_image(
        &self,
        descriptor: &DmaBufImageDescriptor,
        fd: impl AsFd,
    ) -> Result<ImportedDmaBufImage> {
        if !self.features().contains(Features::EXTERNAL_MEMORY_DMA_BUF) {
            return Err(Error::Validation(
                "EXTERNAL_MEMORY_DMA_BUF was not enabled on this Device".into(),
            ));
        }
        descriptor.validate()?;
        let capabilities =
            self.drm_format_modifier_capabilities(descriptor.format, descriptor.usage)?;
        let capability = capabilities
            .iter()
            .find(|capability| capability.modifier == descriptor.modifier)
            .ok_or_else(|| {
                Error::Validation(
                    "DRM format modifier does not support the requested image usage".into(),
                )
            })?;
        if usize::try_from(capability.plane_count).ok() != Some(descriptor.planes.len()) {
            return Err(Error::Validation(format!(
                "DRM format modifier reports {} planes but the descriptor supplies {}",
                capability.plane_count,
                descriptor.planes.len()
            )));
        }
        if !capability.importable {
            return Err(Error::Validation(
                "DRM format modifier is not importable as DMA_BUF_EXT".into(),
            ));
        }

        let owner = self.shared_owner();
        let image = create_import_image(&owner, descriptor, false)?;
        let memory = match allocate_import_memory(&owner, image, fd) {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { owner.device.destroy_image(image, None) };
                return Err(error);
            }
        };
        finish_imported_image(owner, image, vec![memory], descriptor)
    }

    /// Imports an explicit-modifier image whose one to four DRM memory planes
    /// reside in separate dma-buf fds.
    pub fn import_disjoint_dma_buf_image(
        &self,
        descriptor: &DmaBufImageDescriptor,
        plane_fds: &[BorrowedFd<'_>],
    ) -> Result<ImportedDmaBufImage> {
        if !self.features().contains(Features::EXTERNAL_MEMORY_DMA_BUF) {
            return Err(Error::Validation(
                "EXTERNAL_MEMORY_DMA_BUF was not enabled on this Device".into(),
            ));
        }
        descriptor.validate()?;
        if plane_fds.len() != descriptor.planes.len() {
            return Err(Error::Validation(format!(
                "disjoint dma-buf import supplies {} fds for {} plane layouts",
                plane_fds.len(),
                descriptor.planes.len()
            )));
        }
        let capability = self
            .drm_format_modifier_capabilities(descriptor.format, descriptor.usage)?
            .into_iter()
            .find(|capability| capability.modifier == descriptor.modifier)
            .ok_or_else(|| {
                Error::Validation(
                    "DRM format modifier does not support the requested image usage".into(),
                )
            })?;
        if usize::try_from(capability.plane_count).ok() != Some(descriptor.planes.len()) {
            return Err(Error::Validation(format!(
                "DRM format modifier reports {} planes but the descriptor supplies {}",
                capability.plane_count,
                descriptor.planes.len()
            )));
        }
        if !capability.importable
            || !capability
                .tiling_features
                .contains(vk::FormatFeatureFlags2::DISJOINT)
        {
            return Err(Error::Validation(
                "DRM format modifier is not importable with DISJOINT plane memory".into(),
            ));
        }

        let owner = self.shared_owner();
        let image = create_import_image(&owner, descriptor, true)?;
        let memories = match allocate_disjoint_import_memory(&owner, image, plane_fds) {
            Ok(memories) => memories,
            Err(error) => {
                unsafe { owner.device.destroy_image(image, None) };
                return Err(error);
            }
        };
        finish_imported_image(owner, image, memories, descriptor)
    }
}

fn finish_imported_image(
    owner: Arc<DeviceOwner>,
    image: vk::Image,
    memories: Vec<vk::DeviceMemory>,
    descriptor: &DmaBufImageDescriptor,
) -> Result<ImportedDmaBufImage> {
    let view_info = vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(descriptor.format)
        .components(descriptor.components)
        .subresource_range(color_subresource_range());
    let view = match unsafe { owner.device.create_image_view(&view_info, None) } {
        Ok(view) => view,
        Err(source) => {
            unsafe {
                owner.device.destroy_image(image, None);
                for memory in &memories {
                    owner.device.free_memory(*memory, None);
                }
            }
            return Err(Error::vulkan("vkCreateImageView(import dma-buf)", source));
        }
    };
    Ok(ImportedDmaBufImage {
        inner: Arc::new(ImportedDmaBufImageInner {
            owner,
            image,
            memories,
            view,
            format: descriptor.format,
            extent: descriptor.extent,
            modifier: descriptor.modifier,
            usage: descriptor.usage,
            components: descriptor.components,
            label: descriptor.label.clone(),
        }),
    })
}

pub(super) const fn color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}

fn create_import_image(
    owner: &Arc<DeviceOwner>,
    descriptor: &DmaBufImageDescriptor,
    disjoint: bool,
) -> Result<vk::Image> {
    let layouts = descriptor
        .planes
        .iter()
        .map(|plane| vk::SubresourceLayout {
            offset: plane.offset,
            size: 0,
            row_pitch: plane.row_pitch,
            array_pitch: 0,
            depth_pitch: 0,
        })
        .collect::<Vec<_>>();
    let mut modifier = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::builder()
        .drm_format_modifier(descriptor.modifier)
        .plane_layouts(&layouts);
    let mut external = vk::ExternalMemoryImageCreateInfo::builder()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let create = vk::ImageCreateInfo::builder()
        .flags(if disjoint {
            vk::ImageCreateFlags::DISJOINT
        } else {
            vk::ImageCreateFlags::empty()
        })
        .image_type(vk::ImageType::_2D)
        .format(descriptor.format)
        .extent(vk::Extent3D {
            width: descriptor.extent.width,
            height: descriptor.extent.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(descriptor.usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut modifier)
        .push_next(&mut external);
    unsafe { owner.device.create_image(&create, None) }
        .map_err(|source| Error::vulkan("vkCreateImage(import dma-buf)", source))
}

fn allocate_import_memory(
    owner: &Arc<DeviceOwner>,
    image: vk::Image,
    fd: impl AsFd,
) -> Result<vk::DeviceMemory> {
    let requirements = unsafe { owner.device.get_image_memory_requirements(image) };
    let memory = allocate_imported_memory(owner, image, fd.as_fd(), requirements)?;
    if let Err(source) = unsafe { owner.device.bind_image_memory(image, memory, 0) } {
        unsafe { owner.device.free_memory(memory, None) };
        return Err(Error::vulkan("vkBindImageMemory(import dma-buf)", source));
    }
    Ok(memory)
}

fn allocate_disjoint_import_memory(
    owner: &Arc<DeviceOwner>,
    image: vk::Image,
    plane_fds: &[BorrowedFd<'_>],
) -> Result<Vec<vk::DeviceMemory>> {
    let mut memories = Vec::with_capacity(plane_fds.len());
    for (plane, fd) in plane_fds.iter().copied().enumerate() {
        let aspect = memory_plane_aspect(plane);
        let requirements = image_plane_memory_requirements(owner, image, aspect);
        match allocate_imported_memory(owner, image, fd, requirements) {
            Ok(memory) => memories.push(memory),
            Err(error) => {
                unsafe {
                    for memory in &memories {
                        owner.device.free_memory(*memory, None);
                    }
                }
                return Err(error);
            }
        }
    }

    let mut plane_infos = (0..plane_fds.len())
        .map(|plane| {
            vk::BindImagePlaneMemoryInfo::builder()
                .plane_aspect(memory_plane_aspect(plane))
                .build()
        })
        .collect::<Vec<_>>();
    let binds = plane_infos
        .iter_mut()
        .zip(&memories)
        .map(|(plane, memory)| {
            vk::BindImageMemoryInfo::builder()
                .image(image)
                .memory(*memory)
                .memory_offset(0)
                .push_next(plane)
                .build()
        })
        .collect::<Vec<_>>();
    if let Err(source) = unsafe { owner.device.bind_image_memory2(&binds) } {
        unsafe {
            for memory in &memories {
                owner.device.free_memory(*memory, None);
            }
        }
        return Err(Error::vulkan(
            "vkBindImageMemory2(import disjoint dma-buf)",
            source,
        ));
    }
    Ok(memories)
}

fn image_plane_memory_requirements(
    owner: &Arc<DeviceOwner>,
    image: vk::Image,
    aspect: vk::ImageAspectFlags,
) -> vk::MemoryRequirements {
    let mut plane = vk::ImagePlaneMemoryRequirementsInfo::builder()
        .plane_aspect(aspect)
        .build();
    let info = vk::ImageMemoryRequirementsInfo2::builder()
        .image(image)
        .push_next(&mut plane);
    let mut requirements = vk::MemoryRequirements2::default();
    unsafe {
        owner
            .device
            .get_image_memory_requirements2(&info, &mut requirements)
    };
    requirements.memory_requirements
}

fn allocate_imported_memory(
    owner: &Arc<DeviceOwner>,
    image: vk::Image,
    fd: BorrowedFd<'_>,
    requirements: vk::MemoryRequirements,
) -> Result<vk::DeviceMemory> {
    let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
    unsafe {
        owner.device.get_memory_fd_properties_khr(
            vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            fd.as_raw_fd(),
            &mut fd_properties,
        )
    }
    .map_err(|source| Error::vulkan("vkGetMemoryFdPropertiesKHR(DMA_BUF_EXT)", source))?;
    let memory_type_bits = requirements.memory_type_bits & fd_properties.memory_type_bits;
    let memory_type_index = choose_import_memory_type(
        &owner.instance_owner().instance,
        owner.physical_device(),
        memory_type_bits,
        requirements.size,
    )?;
    let duplicate = fd
        .try_clone_to_owned()
        .map_err(|error| Error::Validation(format!("duplicate dma-buf fd: {error}")))?;
    let raw_fd = duplicate.into_raw_fd();
    let mut import = vk::ImportMemoryFdInfoKHR::builder()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
        .fd(raw_fd);
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::builder().image(image);
    let allocate = vk::MemoryAllocateInfo::builder()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index)
        .push_next(&mut import)
        .push_next(&mut dedicated);
    let memory = match unsafe { owner.device.allocate_memory(&allocate, None) } {
        Ok(memory) => memory,
        Err(source) => {
            unsafe { drop(OwnedFd::from_raw_fd(raw_fd)) };
            return Err(Error::vulkan(
                "vkAllocateMemory(import DMA_BUF_EXT)",
                source,
            ));
        }
    };
    Ok(memory)
}

const fn memory_plane_aspect(plane: usize) -> vk::ImageAspectFlags {
    match plane {
        0 => vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
        1 => vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
        2 => vk::ImageAspectFlags::MEMORY_PLANE_2_EXT,
        3 => vk::ImageAspectFlags::MEMORY_PLANE_3_EXT,
        _ => unreachable!(),
    }
}

pub(super) fn choose_import_memory_type(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    memory_type_bits: u32,
    allocation_size: u64,
) -> Result<u32> {
    let properties = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    (0..properties.memory_type_count)
        .filter(|index| memory_type_bits & (1_u32 << index) != 0)
        .filter(|index| {
            let memory_type = properties.memory_types[*index as usize];
            let heap = properties.memory_heaps[memory_type.heap_index as usize];
            heap.size >= allocation_size
                && !memory_type
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::LAZILY_ALLOCATED)
        })
        .min_by_key(|index| {
            let flags = properties.memory_types[*index as usize].property_flags;
            (
                !flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL),
                flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE),
                *index,
            )
        })
        .ok_or_else(|| Error::Validation("dma-buf has no compatible Vulkan memory type".into()))
}

fn query_modifier_capabilities(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    extensions: &std::collections::BTreeSet<String>,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
) -> Result<Vec<DrmFormatModifierCapability>> {
    if format == vk::Format::UNDEFINED || usage.is_empty() {
        return Err(Error::Validation(
            "DRM modifier query requires a defined format and non-empty usage".into(),
        ));
    }
    if ![
        "VK_KHR_external_memory_fd",
        "VK_EXT_external_memory_dma_buf",
        "VK_EXT_image_drm_format_modifier",
        "VK_EXT_queue_family_foreign",
    ]
    .iter()
    .all(|extension| extensions.contains(*extension))
    {
        return Ok(Vec::new());
    }

    let mut list = vk::DrmFormatModifierPropertiesList2EXT::default();
    let mut properties = vk::FormatProperties2::builder().push_next(&mut list);
    unsafe {
        instance.get_physical_device_format_properties2(physical_device, format, &mut properties)
    };
    let mut modifiers = vec![
        vk::DrmFormatModifierProperties2EXT::default();
        list.drm_format_modifier_count as usize
    ];
    if modifiers.is_empty() {
        return Ok(Vec::new());
    }
    let written = {
        let mut list = vk::DrmFormatModifierPropertiesList2EXT::builder()
            .drm_format_modifier_properties(&mut modifiers);
        let mut properties = vk::FormatProperties2::builder().push_next(&mut list);
        unsafe {
            instance.get_physical_device_format_properties2(
                physical_device,
                format,
                &mut properties,
            )
        };
        list.drm_format_modifier_count as usize
    };
    modifiers.truncate(written);

    let mut capabilities = Vec::with_capacity(modifiers.len());
    for modifier in modifiers {
        let mut drm = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::builder()
            .drm_format_modifier(modifier.drm_format_modifier)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let dma_buf = vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT;
        let mut external =
            vk::PhysicalDeviceExternalImageFormatInfo::builder().handle_type(dma_buf);
        let input = vk::PhysicalDeviceImageFormatInfo2::builder()
            .format(format)
            .type_(vk::ImageType::_2D)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(usage)
            .push_next(&mut drm)
            .push_next(&mut external);
        let mut external_properties = vk::ExternalImageFormatProperties::default();
        let mut output = vk::ImageFormatProperties2::builder().push_next(&mut external_properties);
        match unsafe {
            instance.get_physical_device_image_format_properties2(
                physical_device,
                &input,
                &mut output,
            )
        } {
            Ok(()) => {}
            Err(vk::ErrorCode::FORMAT_NOT_SUPPORTED) => continue,
            Err(source) => {
                return Err(Error::vulkan(
                    "vkGetPhysicalDeviceImageFormatProperties2(DRM modifier)",
                    source,
                ));
            }
        }
        let external = external_properties.external_memory_properties;
        let compatible = external.compatible_handle_types.contains(dma_buf);
        capabilities.push(DrmFormatModifierCapability {
            modifier: modifier.drm_format_modifier,
            plane_count: modifier.drm_format_modifier_plane_count,
            tiling_features: modifier.drm_format_modifier_tiling_features,
            importable: compatible
                && external
                    .external_memory_features
                    .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE),
            exportable: compatible
                && external
                    .external_memory_features
                    .contains(vk::ExternalMemoryFeatureFlags::EXPORTABLE),
        });
    }
    capabilities.sort_by_key(|capability| capability.modifier);
    Ok(capabilities)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> DmaBufImageDescriptor {
        DmaBufImageDescriptor {
            label: None,
            format: vk::Format::B8G8R8A8_UNORM,
            extent: vk::Extent2D {
                width: 128,
                height: 72,
            },
            modifier: 9,
            planes: vec![DmaBufPlaneLayout {
                offset: 0,
                row_pitch: 512,
            }],
            usage: vk::ImageUsageFlags::SAMPLED,
            components: vk::ComponentMapping::default(),
        }
    }

    #[test]
    fn dma_buf_descriptor_rejects_implicit_modifier_and_zero_stride() {
        let mut descriptor = descriptor();
        descriptor.modifier = u64::MAX;
        assert!(descriptor.validate().is_err());
        descriptor.modifier = 9;
        descriptor.planes[0].row_pitch = 0;
        assert!(descriptor.validate().is_err());
    }

    #[test]
    fn dma_buf_descriptor_accepts_up_to_four_same_fd_memory_planes() {
        let mut descriptor = descriptor();
        descriptor.planes = (0..4)
            .map(|plane| DmaBufPlaneLayout {
                offset: plane * 4096,
                row_pitch: 512,
            })
            .collect();
        assert!(descriptor.validate().is_ok());
        descriptor.planes.push(DmaBufPlaneLayout {
            offset: 16_384,
            row_pitch: 512,
        });
        assert!(descriptor.validate().is_err());
    }

    #[test]
    fn every_disjoint_plane_uses_a_drm_memory_plane_aspect() {
        assert_eq!(
            (0..4).map(memory_plane_aspect).collect::<Vec<_>>(),
            vec![
                vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
                vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
                vk::ImageAspectFlags::MEMORY_PLANE_2_EXT,
                vk::ImageAspectFlags::MEMORY_PLANE_3_EXT,
            ]
        );
    }
}

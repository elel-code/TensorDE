use std::fmt;
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::Arc;

use vulkanalia::{
    prelude::v1_4::*,
    vk::{
        self, ExtImageDrmFormatModifierExtensionDeviceCommands,
        KhrExternalMemoryFdExtensionDeviceCommands,
    },
};

use super::{choose_import_memory_type, color_subresource_range};
use crate::backend::DeviceOwner;
use crate::{
    Backend, ComponentMapping, Error, Extent2D, Features, ResourceBinding, Result, TextureFormat,
    TextureSubresourceRange, TextureUsages,
};

/// Descriptor for a Vulkan-owned image whose dedicated memory is exportable
/// as one Linux dma-buf fd.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DmaBufExportDescriptor {
    pub label: Option<String>,
    pub format: TextureFormat,
    pub extent: Extent2D,
    /// Acceptable modifiers. The driver may select any listed value.
    pub modifiers: Vec<u64>,
    pub usage: TextureUsages,
    pub components: ComponentMapping,
}

impl DmaBufExportDescriptor {
    fn validate(&self) -> Result<()> {
        if self.extent.width == 0 || self.extent.height == 0 {
            return Err(Error::Validation(
                "dma-buf export extent must be non-zero".into(),
            ));
        }
        if self.modifiers.is_empty() {
            return Err(Error::Validation(
                "dma-buf export requires at least one explicit DRM modifier".into(),
            ));
        }
        if self.modifiers.contains(&u64::MAX) {
            return Err(Error::Validation(
                "dma-buf export modifiers must all be explicit".into(),
            ));
        }
        let mut sorted = self.modifiers.clone();
        sorted.sort_unstable();
        if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(Error::Validation(
                "dma-buf export modifier list contains duplicates".into(),
            ));
        }
        if self.usage.is_empty() {
            return Err(Error::Validation(
                "dma-buf export image usage must be non-empty".into(),
            ));
        }
        Ok(())
    }
}

/// Layout of one DRM memory plane in an exported dma-buf allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmaBufExportPlane {
    pub offset: u64,
    pub row_pitch: u64,
}

/// Cloneable Vulkan image plus the exported dma-buf metadata kept alive by
/// its dedicated allocation.
#[derive(Clone)]
pub struct ExportedDmaBufImage {
    inner: Arc<ExportedDmaBufImageInner>,
}

impl fmt::Debug for ExportedDmaBufImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExportedDmaBufImage")
            .field("label", &self.inner.label)
            .field("image", &self.inner.image)
            .field("view", &self.inner.view)
            .field("format", &self.inner.format)
            .field("extent", &self.inner.extent)
            .field("modifier", &self.inner.modifier)
            .field("planes", &self.inner.planes)
            .field("usage", &self.inner.usage)
            .finish_non_exhaustive()
    }
}

impl ExportedDmaBufImage {
    pub(crate) fn view(&self) -> vk::ImageView {
        self.inner.view
    }

    pub fn format(&self) -> TextureFormat {
        self.inner.format
    }

    pub fn extent(&self) -> Extent2D {
        self.inner.extent
    }

    pub fn modifier(&self) -> u64 {
        self.inner.modifier
    }

    pub fn planes(&self) -> &[DmaBufExportPlane] {
        &self.inner.planes
    }

    pub fn usage(&self) -> TextureUsages {
        self.inner.usage
    }

    pub fn label(&self) -> Option<&str> {
        self.inner.label.as_deref()
    }

    /// Duplicates the exported dma-buf fd for transfer to a Wayland/DRM owner.
    pub fn try_clone_fd(&self) -> std::io::Result<OwnedFd> {
        self.inner.fd.try_clone()
    }

    pub const fn subresource_range(&self) -> TextureSubresourceRange {
        color_subresource_range()
    }

    pub(crate) fn view_create_info(&self) -> vk::ImageViewCreateInfo {
        vk::ImageViewCreateInfo::builder()
            .image(self.inner.image)
            .view_type(vk::ImageViewType::_2D)
            .format(self.inner.format.to_vk())
            .components(self.inner.components.to_vk())
            .subresource_range(self.subresource_range().to_vk())
            .build()
    }

    pub fn resource_binding(&self) -> ResourceBinding {
        ResourceBinding::raw_image(self.inner.image, self.subresource_range().to_vk())
    }

    pub(crate) fn owner(&self) -> &Arc<DeviceOwner> {
        &self.inner.owner
    }
}

impl crate::SubmissionResource for ExportedDmaBufImage {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.inner))
    }
}

struct ExportedDmaBufImageInner {
    owner: Arc<DeviceOwner>,
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    fd: OwnedFd,
    format: TextureFormat,
    extent: Extent2D,
    modifier: u64,
    planes: Vec<DmaBufExportPlane>,
    usage: TextureUsages,
    components: ComponentMapping,
    label: Option<String>,
}

impl Drop for ExportedDmaBufImageInner {
    fn drop(&mut self) {
        unsafe {
            self.owner.device.destroy_image_view(self.view, None);
            self.owner.device.destroy_image(self.image, None);
            self.owner.device.free_memory(self.memory, None);
        }
    }
}

impl Backend {
    /// Creates a dedicated explicit-modifier image and exports its allocation
    /// as a dma-buf. No ordinary allocator block is involved.
    pub fn create_exportable_dma_buf_image(
        &self,
        descriptor: &DmaBufExportDescriptor,
    ) -> Result<ExportedDmaBufImage> {
        if !self.features().contains(Features::EXTERNAL_MEMORY_DMA_BUF) {
            return Err(Error::Validation(
                "EXTERNAL_MEMORY_DMA_BUF was not enabled on this Device".into(),
            ));
        }
        descriptor.validate()?;
        let capabilities =
            self.drm_format_modifier_capabilities(descriptor.format, descriptor.usage)?;
        for modifier in &descriptor.modifiers {
            let supported = capabilities
                .iter()
                .any(|capability| capability.modifier == *modifier && capability.exportable);
            if !supported {
                return Err(Error::Validation(format!(
                    "DRM format modifier {modifier:#x} is not exportable for the requested usage"
                )));
            }
        }

        let owner = self.shared_owner();
        let image = create_export_image(&owner, descriptor)?;
        let memory = match allocate_export_memory(&owner, image) {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { owner.device.destroy_image(image, None) };
                return Err(error);
            }
        };
        match finish_export(Arc::clone(&owner), image, memory, descriptor, &capabilities) {
            Ok(image) => Ok(image),
            Err(error) => {
                unsafe {
                    owner.device.destroy_image(image, None);
                    owner.device.free_memory(memory, None);
                }
                Err(error)
            }
        }
    }
}

fn create_export_image(
    owner: &Arc<DeviceOwner>,
    descriptor: &DmaBufExportDescriptor,
) -> Result<vk::Image> {
    let mut modifiers = vk::ImageDrmFormatModifierListCreateInfoEXT::builder()
        .drm_format_modifiers(&descriptor.modifiers);
    let mut external = vk::ExternalMemoryImageCreateInfo::builder()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let create = vk::ImageCreateInfo::builder()
        .image_type(vk::ImageType::_2D)
        .format(descriptor.format.to_vk())
        .extent(vk::Extent3D {
            width: descriptor.extent.width,
            height: descriptor.extent.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(descriptor.usage.to_vk())
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut modifiers)
        .push_next(&mut external);
    unsafe { owner.device.create_image(&create, None) }
        .map_err(|source| Error::vulkan("vkCreateImage(export dma-buf)", source))
}

fn allocate_export_memory(owner: &Arc<DeviceOwner>, image: vk::Image) -> Result<vk::DeviceMemory> {
    let requirements = unsafe { owner.device.get_image_memory_requirements(image) };
    let memory_type_index = choose_import_memory_type(
        &owner.instance_owner().instance,
        owner.physical_device(),
        requirements.memory_type_bits,
        requirements.size,
    )?;
    let mut export = vk::ExportMemoryAllocateInfo::builder()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::builder().image(image);
    let allocate = vk::MemoryAllocateInfo::builder()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index)
        .push_next(&mut export)
        .push_next(&mut dedicated);
    let memory = unsafe { owner.device.allocate_memory(&allocate, None) }
        .map_err(|source| Error::vulkan("vkAllocateMemory(export DMA_BUF_EXT)", source))?;
    if let Err(source) = unsafe { owner.device.bind_image_memory(image, memory, 0) } {
        unsafe { owner.device.free_memory(memory, None) };
        return Err(Error::vulkan("vkBindImageMemory(export dma-buf)", source));
    }
    Ok(memory)
}

fn finish_export(
    owner: Arc<DeviceOwner>,
    image: vk::Image,
    memory: vk::DeviceMemory,
    descriptor: &DmaBufExportDescriptor,
    capabilities: &[super::DrmFormatModifierCapability],
) -> Result<ExportedDmaBufImage> {
    let mut selected = vk::ImageDrmFormatModifierPropertiesEXT::default();
    unsafe {
        owner
            .device
            .get_image_drm_format_modifier_properties_ext(image, &mut selected)
    }
    .map_err(|source| Error::vulkan("vkGetImageDrmFormatModifierPropertiesEXT", source))?;
    let modifier = selected.drm_format_modifier;
    if !descriptor.modifiers.contains(&modifier) {
        return Err(Error::Validation(format!(
            "Vulkan selected unrequested DRM modifier {modifier:#x}"
        )));
    }
    let plane_count = capabilities
        .iter()
        .find(|capability| capability.modifier == modifier)
        .map(|capability| capability.plane_count)
        .ok_or_else(|| Error::Validation("selected DRM modifier capability disappeared".into()))?;
    if !(1..=4).contains(&plane_count) {
        return Err(Error::Validation(format!(
            "selected DRM modifier reports unsupported plane count {plane_count}"
        )));
    }
    let planes = (0..plane_count)
        .map(|plane| {
            let subresource = vk::ImageSubresource::builder()
                .aspect_mask(memory_plane_aspect(plane))
                .mip_level(0)
                .array_layer(0);
            let layout = unsafe {
                owner
                    .device
                    .get_image_subresource_layout(image, &subresource)
            };
            DmaBufExportPlane {
                offset: layout.offset,
                row_pitch: layout.row_pitch,
            }
        })
        .collect::<Vec<_>>();
    let fd_info = vk::MemoryGetFdInfoKHR::builder()
        .memory(memory)
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let raw_fd = unsafe { owner.device.get_memory_fd_khr(&fd_info) }
        .map_err(|source| Error::vulkan("vkGetMemoryFdKHR(DMA_BUF_EXT)", source))?;
    if raw_fd < 0 {
        return Err(Error::Validation(
            "vkGetMemoryFdKHR returned an invalid dma-buf fd".into(),
        ));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    let view_info = vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(descriptor.format.to_vk())
        .components(descriptor.components.to_vk())
        .subresource_range(color_subresource_range().to_vk());
    let view = unsafe { owner.device.create_image_view(&view_info, None) }
        .map_err(|source| Error::vulkan("vkCreateImageView(export dma-buf)", source))?;
    Ok(ExportedDmaBufImage {
        inner: Arc::new(ExportedDmaBufImageInner {
            owner,
            image,
            memory,
            view,
            fd,
            format: descriptor.format,
            extent: descriptor.extent,
            modifier,
            planes,
            usage: descriptor.usage,
            components: descriptor.components,
            label: descriptor.label.clone(),
        }),
    })
}

const fn memory_plane_aspect(plane: u32) -> vk::ImageAspectFlags {
    match plane {
        0 => vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
        1 => vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
        2 => vk::ImageAspectFlags::MEMORY_PLANE_2_EXT,
        3 => vk::ImageAspectFlags::MEMORY_PLANE_3_EXT,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> DmaBufExportDescriptor {
        DmaBufExportDescriptor {
            label: None,
            format: TextureFormat::Bgra8Unorm,
            extent: Extent2D::new(1920, 1080),
            modifiers: vec![9, 10],
            usage: TextureUsages::COLOR_ATTACHMENT,
            components: ComponentMapping::default(),
        }
    }

    #[test]
    fn export_descriptor_rejects_implicit_and_duplicate_modifiers() {
        let mut descriptor = descriptor();
        descriptor.modifiers.push(u64::MAX);
        assert!(descriptor.validate().is_err());
        descriptor.modifiers = vec![9, 9];
        assert!(descriptor.validate().is_err());
    }

    #[test]
    fn every_supported_plane_has_a_memory_aspect() {
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

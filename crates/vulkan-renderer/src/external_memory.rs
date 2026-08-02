//! Linux dma-buf external-memory interoperability.
//!
//! Imported allocations are always dedicated and never enter the ordinary
//! buffer/image suballocator. This keeps foreign ownership and fd lifetime
//! rules visible at the API boundary.

use std::fmt;
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::Arc;

use vulkanalia::{Instance, prelude::v1_4::*, vk};

use crate::backend::DeviceOwner;
use crate::{
    Adapter, Backend, ComponentMapping, Error, Extent2D, Features, ResourceBinding, Result,
    TextureFormat, TextureFormatFeatures, TextureSubresourceRange, TextureUsages,
};

mod export;
mod import;

pub use export::{DmaBufExportDescriptor, DmaBufExportPlane, ExportedDmaBufImage};

/// One explicit DRM memory-plane layout inside a dma-buf allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmaBufPlaneLayout {
    pub offset: u64,
    pub row_pitch: u64,
}

/// Stable Linux DRM device-node identity reported by
/// `VK_EXT_physical_device_drm`.
///
/// This is deliberately a value type: the renderer selects Vulkan devices,
/// while a host integration uses this identity to open its matching DRM
/// primary and render nodes without receiving Vulkan handles.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DrmNodeIdentity {
    major: u32,
    minor: u32,
}

impl DrmNodeIdentity {
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    pub const fn major(self) -> u32 {
        self.major
    }

    pub const fn minor(self) -> u32 {
        self.minor
    }
}

/// Primary and render node identities advertised by one physical device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrmDeviceIdentity {
    primary: Option<DrmNodeIdentity>,
    render: Option<DrmNodeIdentity>,
}

impl DrmDeviceIdentity {
    pub const fn new(primary: Option<DrmNodeIdentity>, render: Option<DrmNodeIdentity>) -> Self {
        Self { primary, render }
    }

    pub const fn primary(self) -> Option<DrmNodeIdentity> {
        self.primary
    }

    pub const fn render(self) -> Option<DrmNodeIdentity> {
        self.render
    }

    pub const fn node_pair(self) -> Option<(DrmNodeIdentity, DrmNodeIdentity)> {
        match (self.primary, self.render) {
            (Some(primary), Some(render)) => Some((primary, render)),
            _ => None,
        }
    }
}

/// Complete Linux dma-buf and sync-file capability snapshot for one adapter.
///
/// The individual fields remain visible so hosts can produce a precise
/// startup diagnosis. [`Self::is_complete`] is the strict native-compositor
/// gate: it requires import/export memory, explicit DRM modifiers, foreign
/// queue-family transfers, and bidirectional binary `SYNC_FD` semaphores.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LinuxDmaBufCapabilities {
    pub external_memory_fd: bool,
    pub dma_buf_memory: bool,
    pub drm_format_modifier: bool,
    pub foreign_queue_family: bool,
    pub external_semaphore_fd: bool,
    pub sync_fd_semaphore: bool,
}

impl LinuxDmaBufCapabilities {
    pub const fn is_complete(self) -> bool {
        self.external_memory_fd
            && self.dma_buf_memory
            && self.drm_format_modifier
            && self.foreign_queue_family
            && self.external_semaphore_fd
            && self.sync_fd_semaphore
    }
}

/// Descriptor for importing one Linux dma-buf fd with one to four explicit DRM
/// memory-plane layouts as a Vulkan image.
///
/// DRM fourcc-to-texture-format policy belongs to the host integration layer;
/// this descriptor receives the already selected renderer format and modifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DmaBufImageDescriptor {
    pub label: Option<String>,
    pub format: TextureFormat,
    pub extent: Extent2D,
    pub modifier: u64,
    pub planes: Vec<DmaBufPlaneLayout>,
    pub usage: TextureUsages,
    pub components: ComponentMapping,
}

impl DmaBufImageDescriptor {
    fn validate(&self) -> Result<()> {
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
    pub features: TextureFormatFeatures,
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

    pub fn usage(&self) -> TextureUsages {
        self.inner.usage
    }

    pub fn label(&self) -> Option<&str> {
        self.inner.label.as_deref()
    }

    pub const fn subresource_range(&self) -> TextureSubresourceRange {
        color_subresource_range()
    }

    /// Reconstructs the create-info consumed by `VK_EXT_descriptor_heap` image
    /// descriptor writes. The returned value borrows no temporary pNext data.
    pub(crate) fn view_create_info(&self) -> vk::ImageViewCreateInfo {
        vk::ImageViewCreateInfo::builder()
            .image(self.inner.image)
            .view_type(vk::ImageViewType::_2D)
            .format(self.inner.format.to_vk())
            .components(self.inner.components.to_vk())
            .subresource_range(self.subresource_range().to_vk())
            .build()
    }

    /// Creates the typed binding used to resolve this image in a render graph.
    pub fn resource_binding(&self) -> ResourceBinding {
        ResourceBinding::raw_image(self.inner.image, self.subresource_range().to_vk())
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
    format: TextureFormat,
    extent: Extent2D,
    modifier: u64,
    usage: TextureUsages,
    components: ComponentMapping,
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
    /// Reports the complete native Linux dma-buf capability gate without
    /// exposing a physical-device handle to the host integration.
    pub fn linux_dma_buf_capabilities(&self) -> LinuxDmaBufCapabilities {
        let extensions = &self.info().extensions;
        let external_memory_fd = extensions.contains("VK_KHR_external_memory_fd");
        let dma_buf_memory = extensions.contains("VK_EXT_external_memory_dma_buf");
        let drm_format_modifier = extensions.contains("VK_EXT_image_drm_format_modifier");
        let foreign_queue_family = extensions.contains("VK_EXT_queue_family_foreign");
        let external_semaphore_fd = extensions.contains("VK_KHR_external_semaphore_fd");
        LinuxDmaBufCapabilities {
            external_memory_fd,
            dma_buf_memory,
            drm_format_modifier,
            foreign_queue_family,
            external_semaphore_fd,
            sync_fd_semaphore: external_semaphore_fd
                && supports_sync_fd_semaphore(
                    &self.instance_owner().instance,
                    self.physical_device(),
                ),
        }
    }

    /// Returns the device's primary/render DRM node pair when it advertises
    /// `VK_EXT_physical_device_drm`. Missing node classes remain explicit in
    /// the returned value so a native compositor can reject an incomplete
    /// pair with a useful diagnostic.
    pub fn drm_device_identity(&self) -> Option<DrmDeviceIdentity> {
        if !self
            .info()
            .extensions
            .contains("VK_EXT_physical_device_drm")
        {
            return None;
        }
        let mut drm = vk::PhysicalDeviceDrmPropertiesEXT::default();
        let mut properties = vk::PhysicalDeviceProperties2::builder().push_next(&mut drm);
        unsafe {
            self.instance_owner()
                .instance
                .get_physical_device_properties2(self.physical_device(), &mut properties)
        };
        let primary = drm_node_identity(drm.has_primary, drm.primary_major, drm.primary_minor);
        let render = drm_node_identity(drm.has_render, drm.render_major, drm.render_minor);
        (primary.is_some() || render.is_some()).then_some(DrmDeviceIdentity::new(primary, render))
    }

    /// Queries DRM modifier support for one exact format and usage before
    /// logical-device creation. Unsupported modifiers are omitted.
    pub fn drm_format_modifier_capabilities(
        &self,
        format: TextureFormat,
        usage: TextureUsages,
    ) -> Result<Vec<DrmFormatModifierCapability>> {
        query_modifier_capabilities(
            &self.instance_owner().instance,
            self.physical_device(),
            &self.info().extensions,
            format.to_vk(),
            usage.to_vk(),
        )
    }
}

fn drm_node_identity(present: vk::Bool32, major: i64, minor: i64) -> Option<DrmNodeIdentity> {
    if present == 0 {
        return None;
    }
    Some(DrmNodeIdentity::new(
        u32::try_from(major).ok()?,
        u32::try_from(minor).ok()?,
    ))
}

fn supports_sync_fd_semaphore(instance: &Instance, physical_device: vk::PhysicalDevice) -> bool {
    let info = vk::PhysicalDeviceExternalSemaphoreInfo::builder()
        .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
    let mut properties = vk::ExternalSemaphoreProperties::default();
    unsafe {
        instance.get_physical_device_external_semaphore_properties(
            physical_device,
            &info,
            &mut properties,
        )
    };
    let required = vk::ExternalSemaphoreFeatureFlags::IMPORTABLE
        | vk::ExternalSemaphoreFeatureFlags::EXPORTABLE;
    properties
        .compatible_handle_types
        .contains(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
        && properties.external_semaphore_features.contains(required)
}

impl Backend {
    /// Queries DRM modifier support for one exact format and usage on this
    /// device's physical adapter.
    pub fn drm_format_modifier_capabilities(
        &self,
        format: TextureFormat,
        usage: TextureUsages,
    ) -> Result<Vec<DrmFormatModifierCapability>> {
        let owner = self.shared_owner();
        query_modifier_capabilities(
            &owner.instance_owner().instance,
            owner.physical_device(),
            &self.device_info().extensions,
            format.to_vk(),
            usage.to_vk(),
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
        let image = import::create_import_image(&owner, descriptor, false)?;
        let memory = match import::allocate_import_memory(&owner, image, fd) {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { owner.device.destroy_image(image, None) };
                return Err(error);
            }
        };
        import::finish_imported_image(owner, image, vec![memory], descriptor)
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
                .features
                .contains(TextureFormatFeatures::DISJOINT)
        {
            return Err(Error::Validation(
                "DRM format modifier is not importable with DISJOINT plane memory".into(),
            ));
        }

        let owner = self.shared_owner();
        let image = import::create_import_image(&owner, descriptor, true)?;
        let memories = match import::allocate_disjoint_import_memory(&owner, image, plane_fds) {
            Ok(memories) => memories,
            Err(error) => {
                unsafe { owner.device.destroy_image(image, None) };
                return Err(error);
            }
        };
        import::finish_imported_image(owner, image, memories, descriptor)
    }
}

pub(super) const fn color_subresource_range() -> TextureSubresourceRange {
    TextureSubresourceRange::full_color(1, 1)
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
            features: TextureFormatFeatures::from_vk(modifier.drm_format_modifier_tiling_features),
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
            format: TextureFormat::Bgra8Unorm,
            extent: Extent2D::new(128, 72),
            modifier: 9,
            planes: vec![DmaBufPlaneLayout {
                offset: 0,
                row_pitch: 512,
            }],
            usage: TextureUsages::SAMPLED,
            components: ComponentMapping::default(),
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
    fn native_compositor_capability_gate_requires_every_interop_contract() {
        let complete = LinuxDmaBufCapabilities {
            external_memory_fd: true,
            dma_buf_memory: true,
            drm_format_modifier: true,
            foreign_queue_family: true,
            external_semaphore_fd: true,
            sync_fd_semaphore: true,
        };
        assert!(complete.is_complete());
        assert!(
            !LinuxDmaBufCapabilities {
                sync_fd_semaphore: false,
                ..complete
            }
            .is_complete()
        );
    }

    #[test]
    fn drm_identity_preserves_complete_primary_render_pair() {
        let primary = DrmNodeIdentity::new(226, 0);
        let render = DrmNodeIdentity::new(226, 128);
        let identity = DrmDeviceIdentity::new(Some(primary), Some(render));
        assert_eq!(identity.primary(), Some(primary));
        assert_eq!(identity.render(), Some(render));
        assert_eq!(identity.node_pair(), Some((primary, render)));
        assert_eq!(
            DrmDeviceIdentity::new(Some(primary), None).node_pair(),
            None
        );
    }

    #[test]
    fn every_disjoint_plane_uses_a_drm_memory_plane_aspect() {
        assert_eq!(
            (0..4).map(import::memory_plane_aspect).collect::<Vec<_>>(),
            vec![
                vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
                vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
                vk::ImageAspectFlags::MEMORY_PLANE_2_EXT,
                vk::ImageAspectFlags::MEMORY_PLANE_3_EXT,
            ]
        );
    }
}

#![allow(unsafe_code)]

use smithay::backend::allocator::{Format as DrmFormat, Fourcc, Modifier};
use thiserror::Error;
use tracing::{debug, info};
use vulkanalia::{
    Device, Entry, Instance, Version,
    loader::{LIBRARY, LibloadingLoader},
    prelude::v1_4::*,
};

use super::{
    DescriptorHeapProperties, DeviceCandidate, DeviceSelectionError, DrmDeviceIdentity, DrmNodeId,
    NativeInteropCapabilities, RendererTarget, VulkanFormatCapability,
};
#[cfg(feature = "tty")]
use super::{FrameScheduler, FrameSubmission, NativeOutputTarget, RenderOutputId};

#[cfg(feature = "tty")]
mod frame;

#[cfg(feature = "tty")]
const DESCRIPTOR_HEAP_BYTES: u64 = 16 * 1024 * 1024;

const OUTPUT_FORMATS: &[(Fourcc, vk::Format)] = &[
    (Fourcc::Xrgb8888, vk::Format::B8G8R8A8_SRGB),
    (Fourcc::Argb8888, vk::Format::B8G8R8A8_SRGB),
    (Fourcc::Xbgr8888, vk::Format::R8G8B8A8_SRGB),
    (Fourcc::Abgr8888, vk::Format::R8G8B8A8_SRGB),
    (Fourcc::Xrgb2101010, vk::Format::A2R10G10B10_UNORM_PACK32),
    (Fourcc::Argb2101010, vk::Format::A2R10G10B10_UNORM_PACK32),
    (Fourcc::Xbgr2101010, vk::Format::A2B10G10R10_UNORM_PACK32),
    (Fourcc::Abgr2101010, vk::Format::A2B10G10R10_UNORM_PACK32),
];

pub(crate) struct VulkanRenderer {
    _owner: VulkanOwner,
    target: RendererTarget,
    selected: SelectedDevice,
    #[cfg(feature = "tty")]
    frames: FrameScheduler,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SelectedDevice {
    pub(crate) name: String,
    pub(crate) api_version: Version,
    pub(crate) device_type: vk::PhysicalDeviceType,
    pub(crate) graphics_queue_family: u32,
    pub(crate) primary_node: DrmNodeId,
    pub(crate) render_node: DrmNodeId,
    pub(crate) interop: NativeInteropCapabilities,
    pub(crate) formats: Vec<VulkanFormatCapability>,
    pub(crate) descriptor_heap: DescriptorHeapProperties,
}

impl VulkanRenderer {
    pub(crate) fn new(target: RendererTarget) -> Result<Self, RendererError> {
        let entry = load_entry()?;
        let loader_version = entry.version().map_err(RendererError::LoaderVersion)?;
        if loader_version < target.api_version {
            return Err(RendererError::UnsupportedLoaderVersion {
                required: target.api_version,
                found: loader_version,
            });
        }

        let application = vk::ApplicationInfo::builder()
            .application_name(b"tensor-compositor\0")
            .engine_name(b"tensor-renderer\0")
            .api_version(target.api_version.into());
        let instance_info = vk::InstanceCreateInfo::builder().application_info(&application);
        let instance = unsafe { entry.create_instance(&instance_info, None) }
            .map_err(RendererError::CreateInstance)?;
        let instance = InstanceOwner { entry, instance };

        let probed = probe_devices(&instance.instance)?;
        for device in &probed {
            debug!(
                ordinal = device.candidate.ordinal,
                name = device.candidate.name,
                api = %device.candidate.api_version,
                device_type = ?device.candidate.device_type,
                descriptor_heap = device.candidate.descriptor_heap_supported,
                descriptor_heap_alignment = device.candidate.descriptor_heap.resource_heap_alignment,
                descriptor_heap_max = device.candidate.descriptor_heap.max_resource_heap_size,
                descriptor_heap_reserved = device.candidate.descriptor_heap.min_resource_heap_reserved_range,
                image_descriptor_size = device.candidate.descriptor_heap.image_descriptor_size,
                image_descriptor_alignment = device.candidate.descriptor_heap.image_descriptor_alignment,
                timeline_semaphore = device.candidate.timeline_semaphore_supported,
                graphics_queue_family = ?device.candidate.graphics_queue_family,
                native_output_formats = device.candidate.native_output_format_count,
                "Vulkan physical device probed"
            );
        }
        let selected = target
            .device
            .select(probed.iter().map(|device| &device.candidate))?;
        let selected = &probed[selected.ordinal];
        #[cfg(feature = "tty")]
        let frames = FrameScheduler::new(
            selected
                .candidate
                .descriptor_heap
                .min_resource_heap_reserved_range
                .saturating_add(DESCRIPTOR_HEAP_BYTES)
                .min(selected.candidate.descriptor_heap.max_resource_heap_size),
            selected
                .candidate
                .descriptor_heap
                .image_descriptor_alignment,
            selected
                .candidate
                .descriptor_heap
                .min_resource_heap_reserved_range,
            selected.candidate.descriptor_heap.image_descriptor_size,
        )
        .map_err(|error| RendererError::Frame(error.to_string()))?;
        let graphics_queue_family = selected
            .candidate
            .graphics_queue_family
            .ok_or(DeviceSelectionError::MissingGraphicsQueue)?;
        let device = create_device(&instance.instance, selected.handle, graphics_queue_family)?;
        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family, 0) };
        #[cfg(feature = "tty")]
        let frame_executor = match frame::VulkanFrameExecutor::new(&device, graphics_queue_family) {
            Ok(executor) => executor,
            Err(source) => {
                unsafe { device.destroy_device(None) };
                return Err(RendererError::CreateFrameResources(source));
            }
        };
        let (primary_node, render_node) = selected
            .candidate
            .drm
            .and_then(DrmDeviceIdentity::node_pair)
            .ok_or(DeviceSelectionError::MissingDrmNodePair)?;
        let selected_info = SelectedDevice {
            name: selected.candidate.name.clone(),
            api_version: selected.candidate.api_version,
            device_type: selected.candidate.device_type,
            graphics_queue_family,
            primary_node,
            render_node,
            interop: selected.candidate.interop,
            formats: selected.formats.clone(),
            descriptor_heap: selected.candidate.descriptor_heap,
        };
        let client_import_formats = selected_info
            .formats
            .iter()
            .filter(|format| format.supports_client_import())
            .count();
        let output_export_formats = selected_info
            .formats
            .iter()
            .filter(|format| format.supports_output_export())
            .count();
        info!(
            name = selected_info.name,
            api = %selected_info.api_version,
            device_type = ?selected_info.device_type,
            graphics_queue_family,
            primary_node = %selected_info.primary_node,
            render_node = %selected_info.render_node,
            descriptor_heap = true,
            descriptor_heap_alignment = selected_info.descriptor_heap.resource_heap_alignment,
            descriptor_heap_max = selected_info.descriptor_heap.max_resource_heap_size,
            descriptor_heap_reserved = selected_info.descriptor_heap.min_resource_heap_reserved_range,
            image_descriptor_size = selected_info.descriptor_heap.image_descriptor_size,
            image_descriptor_alignment = selected_info.descriptor_heap.image_descriptor_alignment,
            dma_buf = selected_info.interop.dma_buf_memory,
            drm_format_modifier = selected_info.interop.drm_format_modifier,
            foreign_queue_family = selected_info.interop.foreign_queue_family,
            sync_fd = selected_info.interop.sync_fd_semaphore,
            client_import_formats,
            output_export_formats,
            "Vulkanalia renderer device initialized"
        );

        Ok(Self {
            _owner: VulkanOwner {
                device,
                instance,
                _physical_device: selected.handle,
                _graphics_queue: graphics_queue,
                #[cfg(feature = "tty")]
                frame_executor,
            },
            target,
            selected: selected_info,
            #[cfg(feature = "tty")]
            frames,
        })
    }

    pub(crate) const fn target(&self) -> RendererTarget {
        self.target
    }

    pub(crate) fn selected(&self) -> &SelectedDevice {
        &self.selected
    }

    #[cfg(feature = "tty")]
    pub(crate) fn register_output(
        &mut self,
        target: NativeOutputTarget,
    ) -> Result<(), RendererError> {
        if !self.selected.formats.iter().copied().any(|candidate| {
            candidate.format == target.format.format
                && candidate.plane_count == target.format.plane_count
                && candidate.supports_output_export()
        }) {
            return Err(RendererError::UnsupportedOutputTarget {
                format: target.format.format.code,
                modifier: u64::from(target.format.format.modifier),
                plane_count: target.format.plane_count,
            });
        }
        self.frames
            .register_output(target)
            .map_err(|error| RendererError::Frame(error.to_string()))
    }

    #[cfg(feature = "tty")]
    pub(crate) fn unregister_output(&mut self, output: RenderOutputId) {
        self.frames.unregister_output(output);
    }

    pub(crate) fn output_count(&self) -> usize {
        #[cfg(not(feature = "tty"))]
        return 0;
        #[cfg(feature = "tty")]
        self.frames.output_count()
    }

    #[cfg(feature = "tty")]
    pub(crate) fn submit_scene(
        &mut self,
        output: RenderOutputId,
        scene: crate::scene::SceneSnapshot,
    ) -> Result<FrameSubmission, RendererError> {
        let completed = match self._owner.frame_executor.completed(&self._owner.device) {
            Ok(value) => value,
            Err(error) => {
                if error == vk::ErrorCode::DEVICE_LOST {
                    self.frames.mark_device_lost();
                }
                return Err(RendererError::QueryTimeline(error));
            }
        };
        self.frames.retire_completed(completed);
        let frame = self
            .frames
            .submit(output, scene, completed)
            .map_err(|error| RendererError::Frame(error.to_string()))?;
        if let Err(source) = self._owner.frame_executor.submit(
            &self._owner.device,
            self._owner._graphics_queue,
            frame.timeline_value,
            completed,
        ) {
            if matches!(
                source,
                frame::VulkanFrameError::Vulkan(vk::ErrorCode::DEVICE_LOST)
            ) {
                self.frames.mark_device_lost();
            }
            return Err(RendererError::SubmitFrame(format!("{source:?}")));
        }
        Ok(frame)
    }
}

struct ProbedDevice {
    handle: vk::PhysicalDevice,
    candidate: DeviceCandidate,
    formats: Vec<VulkanFormatCapability>,
}

struct InstanceOwner {
    entry: Entry,
    instance: Instance,
}

impl Drop for InstanceOwner {
    fn drop(&mut self) {
        unsafe { self.instance.destroy_instance(None) };
        let _ = &self.entry;
    }
}

struct VulkanOwner {
    device: Device,
    instance: InstanceOwner,
    _physical_device: vk::PhysicalDevice,
    _graphics_queue: vk::Queue,
    #[cfg(feature = "tty")]
    frame_executor: frame::VulkanFrameExecutor,
}

impl Drop for VulkanOwner {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            #[cfg(feature = "tty")]
            self.frame_executor.destroy(&self.device);
        }
        unsafe { self.device.destroy_device(None) };
        let _ = &self.instance;
    }
}

fn load_entry() -> Result<Entry, RendererError> {
    // Vulkanalia deliberately exposes loader and dispatch construction as unsafe. The library
    // path is its platform constant and both owners outlive every command loaded from it.
    let loader = unsafe { LibloadingLoader::new(LIBRARY) }
        .map_err(|error| RendererError::LoadLibrary(error.to_string()))?;
    unsafe { Entry::new(loader) }.map_err(|error| RendererError::LoadEntry(error.to_string()))
}

fn probe_devices(instance: &Instance) -> Result<Vec<ProbedDevice>, RendererError> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(RendererError::EnumerateDevices)?;
    let mut candidates = Vec::with_capacity(physical_devices.len());
    for (ordinal, handle) in physical_devices.into_iter().enumerate() {
        let properties = unsafe { instance.get_physical_device_properties(handle) };
        let extensions = unsafe { instance.enumerate_device_extension_properties(handle, None) }
            .map_err(RendererError::EnumerateExtensions)?;
        let has_heap_extension = extensions
            .iter()
            .any(|extension| extension.extension_name == vk::EXT_DESCRIPTOR_HEAP_EXTENSION.name);
        let descriptor_heap_supported =
            has_heap_extension && descriptor_heap_feature(instance, handle);
        let descriptor_heap = if has_heap_extension {
            descriptor_heap_properties(instance, handle)
        } else {
            DescriptorHeapProperties::default()
        };
        let timeline_semaphore_supported = timeline_semaphore_feature(instance, handle);
        let has_drm_extension = extensions.iter().any(|extension| {
            extension.extension_name == vk::EXT_PHYSICAL_DEVICE_DRM_EXTENSION.name
        });
        let drm = has_drm_extension
            .then(|| drm_device_identity(instance, handle))
            .flatten();
        let graphics_queue_family = unsafe {
            instance
                .get_physical_device_queue_family_properties(handle)
                .iter()
                .position(|family| family.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                .map(|index| index as u32)
        };
        let interop = native_interop_capabilities(instance, handle, &extensions);
        let formats = if descriptor_heap_supported
            && descriptor_heap.is_usable()
            && timeline_semaphore_supported
            && Version::from(properties.api_version) >= Version::V1_4_0
            && graphics_queue_family.is_some()
            && drm.and_then(DrmDeviceIdentity::node_pair).is_some()
            && interop.is_complete()
        {
            probe_format_capabilities(instance, handle)?
        } else {
            Vec::new()
        };
        let native_output_format_count = formats
            .iter()
            .filter(|format| format.supports_output_export())
            .count();
        candidates.push(ProbedDevice {
            handle,
            candidate: DeviceCandidate {
                ordinal,
                name: properties.device_name.to_string_lossy().into_owned(),
                device_type: properties.device_type,
                api_version: properties.api_version.into(),
                descriptor_heap_supported,
                descriptor_heap,
                timeline_semaphore_supported,
                graphics_queue_family,
                drm,
                interop,
                native_output_format_count,
            },
            formats,
        });
    }
    Ok(candidates)
}

fn probe_format_capabilities(
    instance: &Instance,
    device: vk::PhysicalDevice,
) -> Result<Vec<VulkanFormatCapability>, RendererError> {
    let mut capabilities = Vec::new();
    for &(fourcc, vulkan_format) in OUTPUT_FORMATS {
        for modifier in drm_modifier_properties(instance, device, vulkan_format) {
            let Some(capability) =
                probe_format_capability(instance, device, fourcc, vulkan_format, modifier)?
            else {
                continue;
            };
            capabilities.push(capability);
        }
    }
    Ok(capabilities)
}

fn drm_modifier_properties(
    instance: &Instance,
    device: vk::PhysicalDevice,
    format: vk::Format,
) -> Vec<vk::DrmFormatModifierProperties2EXT> {
    let mut modifier_list = vk::DrmFormatModifierPropertiesList2EXT::default();
    let mut properties = vk::FormatProperties2::builder().push_next(&mut modifier_list);
    unsafe { instance.get_physical_device_format_properties2(device, format, &mut properties) };
    if modifier_list.drm_format_modifier_count == 0 {
        return Vec::new();
    }

    let mut modifiers = vec![
        vk::DrmFormatModifierProperties2EXT::default();
        modifier_list.drm_format_modifier_count as usize
    ];
    let written = {
        let mut modifier_list = vk::DrmFormatModifierPropertiesList2EXT::builder()
            .drm_format_modifier_properties(&mut modifiers);
        let mut properties = vk::FormatProperties2::builder().push_next(&mut modifier_list);
        unsafe { instance.get_physical_device_format_properties2(device, format, &mut properties) };
        modifier_list.drm_format_modifier_count as usize
    };
    modifiers.truncate(written);
    modifiers
}

fn probe_format_capability(
    instance: &Instance,
    device: vk::PhysicalDevice,
    fourcc: Fourcc,
    vulkan_format: vk::Format,
    modifier: vk::DrmFormatModifierProperties2EXT,
) -> Result<Option<VulkanFormatCapability>, RendererError> {
    if !modifier
        .drm_format_modifier_tiling_features
        .contains(vk::FormatFeatureFlags2::COLOR_ATTACHMENT)
    {
        return Ok(None);
    }

    let mut drm = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::builder()
        .drm_format_modifier(modifier.drm_format_modifier)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let dma_buf = vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT;
    let mut external = vk::PhysicalDeviceExternalImageFormatInfo::builder().handle_type(dma_buf);
    let input = vk::PhysicalDeviceImageFormatInfo2::builder()
        .format(vulkan_format)
        .type_(vk::ImageType::_2D)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(native_image_usage())
        .push_next(&mut drm)
        .push_next(&mut external);
    let mut external_properties = vk::ExternalImageFormatProperties::default();
    let mut properties = vk::ImageFormatProperties2::builder().push_next(&mut external_properties);
    match unsafe {
        instance.get_physical_device_image_format_properties2(device, &input, &mut properties)
    } {
        Ok(()) => {}
        Err(error) if error == vk::ErrorCode::FORMAT_NOT_SUPPORTED => return Ok(None),
        Err(source) => {
            return Err(RendererError::ProbeFormat {
                format: fourcc,
                modifier: modifier.drm_format_modifier,
                source,
            });
        }
    }

    let external = external_properties.external_memory_properties;
    let compatible = external.compatible_handle_types.contains(dma_buf);
    Ok(Some(VulkanFormatCapability {
        format: DrmFormat {
            code: fourcc,
            modifier: Modifier::from(modifier.drm_format_modifier),
        },
        plane_count: modifier.drm_format_modifier_plane_count,
        renderable: true,
        importable: compatible
            && external
                .external_memory_features
                .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE),
        exportable: compatible
            && external
                .external_memory_features
                .contains(vk::ExternalMemoryFeatureFlags::EXPORTABLE),
    }))
}

fn native_image_usage() -> vk::ImageUsageFlags {
    vk::ImageUsageFlags::COLOR_ATTACHMENT
        | vk::ImageUsageFlags::SAMPLED
        | vk::ImageUsageFlags::TRANSFER_SRC
        | vk::ImageUsageFlags::TRANSFER_DST
}

fn native_interop_capabilities(
    instance: &Instance,
    device: vk::PhysicalDevice,
    extensions: &[vk::ExtensionProperties],
) -> NativeInteropCapabilities {
    let external_memory_fd = extensions
        .iter()
        .any(|extension| extension.extension_name == vk::KHR_EXTERNAL_MEMORY_FD_EXTENSION.name);
    let dma_buf_memory = extensions.iter().any(|extension| {
        extension.extension_name == vk::EXT_EXTERNAL_MEMORY_DMA_BUF_EXTENSION.name
    });
    let drm_format_modifier = extensions.iter().any(|extension| {
        extension.extension_name == vk::EXT_IMAGE_DRM_FORMAT_MODIFIER_EXTENSION.name
    });
    let foreign_queue_family = extensions
        .iter()
        .any(|extension| extension.extension_name == vk::EXT_QUEUE_FAMILY_FOREIGN_EXTENSION.name);
    let external_semaphore_fd = extensions
        .iter()
        .any(|extension| extension.extension_name == vk::KHR_EXTERNAL_SEMAPHORE_FD_EXTENSION.name);
    NativeInteropCapabilities {
        external_memory_fd,
        dma_buf_memory,
        drm_format_modifier,
        foreign_queue_family,
        external_semaphore_fd,
        sync_fd_semaphore: external_semaphore_fd && sync_fd_semaphore_supported(instance, device),
    }
}

fn sync_fd_semaphore_supported(instance: &Instance, device: vk::PhysicalDevice) -> bool {
    let handle = vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD;
    let info = vk::PhysicalDeviceExternalSemaphoreInfo::builder().handle_type(handle);
    let mut properties = vk::ExternalSemaphoreProperties::default();
    unsafe {
        instance.get_physical_device_external_semaphore_properties(device, &info, &mut properties)
    };
    let required = vk::ExternalSemaphoreFeatureFlags::IMPORTABLE
        | vk::ExternalSemaphoreFeatureFlags::EXPORTABLE;
    properties.external_semaphore_features.contains(required)
        && properties.compatible_handle_types.contains(handle)
}

fn drm_device_identity(
    instance: &Instance,
    device: vk::PhysicalDevice,
) -> Option<DrmDeviceIdentity> {
    let mut drm = vk::PhysicalDeviceDrmPropertiesEXT::default();
    let mut properties = vk::PhysicalDeviceProperties2::builder().push_next(&mut drm);
    unsafe { instance.get_physical_device_properties2(device, &mut properties) };

    let primary = drm_node(drm.has_primary, drm.primary_major, drm.primary_minor);
    let render = drm_node(drm.has_render, drm.render_major, drm.render_minor);
    (primary.is_some() || render.is_some()).then(|| DrmDeviceIdentity::new(primary, render))
}

fn drm_node(present: vk::Bool32, major: i64, minor: i64) -> Option<DrmNodeId> {
    if present == 0 {
        return None;
    }
    Some(DrmNodeId::new(
        u32::try_from(major).ok()?,
        u32::try_from(minor).ok()?,
    ))
}

fn descriptor_heap_feature(instance: &Instance, device: vk::PhysicalDevice) -> bool {
    let mut descriptor_heap = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::default();
    let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut descriptor_heap);
    unsafe { instance.get_physical_device_features2(device, &mut features) };
    descriptor_heap.descriptor_heap != 0
}

fn descriptor_heap_properties(
    instance: &Instance,
    device: vk::PhysicalDevice,
) -> DescriptorHeapProperties {
    let mut heap = vk::PhysicalDeviceDescriptorHeapPropertiesEXT::default();
    let mut properties = vk::PhysicalDeviceProperties2::builder().push_next(&mut heap);
    unsafe { instance.get_physical_device_properties2(device, &mut properties) };
    DescriptorHeapProperties {
        resource_heap_alignment: heap.resource_heap_alignment,
        max_resource_heap_size: heap.max_resource_heap_size,
        min_resource_heap_reserved_range: heap.min_resource_heap_reserved_range,
        image_descriptor_size: heap.image_descriptor_size,
        image_descriptor_alignment: heap.image_descriptor_alignment,
    }
}

fn timeline_semaphore_feature(instance: &Instance, device: vk::PhysicalDevice) -> bool {
    let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut vulkan12);
    unsafe { instance.get_physical_device_features2(device, &mut features) };
    vulkan12.timeline_semaphore != 0
}

fn create_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    graphics_queue_family: u32,
) -> Result<Device, RendererError> {
    let priorities = [1.0];
    let queue = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(graphics_queue_family)
        .queue_priorities(&priorities);
    let queues = [queue];
    let extensions = [
        vk::EXT_DESCRIPTOR_HEAP_EXTENSION.name.as_cstr().as_ptr(),
        vk::KHR_EXTERNAL_MEMORY_FD_EXTENSION.name.as_cstr().as_ptr(),
        vk::EXT_EXTERNAL_MEMORY_DMA_BUF_EXTENSION
            .name
            .as_cstr()
            .as_ptr(),
        vk::EXT_IMAGE_DRM_FORMAT_MODIFIER_EXTENSION
            .name
            .as_cstr()
            .as_ptr(),
        vk::EXT_QUEUE_FAMILY_FOREIGN_EXTENSION
            .name
            .as_cstr()
            .as_ptr(),
        vk::KHR_EXTERNAL_SEMAPHORE_FD_EXTENSION
            .name
            .as_cstr()
            .as_ptr(),
    ];
    let mut descriptor_heap = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::builder()
        .descriptor_heap(true)
        .build();
    let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::builder()
        .timeline_semaphore(true)
        .build();
    let info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queues)
        .enabled_extension_names(&extensions)
        .push_next(&mut vulkan12)
        .push_next(&mut descriptor_heap);
    unsafe { instance.create_device(physical_device, &info, None) }
        .map_err(RendererError::CreateDevice)
}

#[derive(Debug, Error)]
pub enum RendererError {
    #[error("failed to load Vulkan library {LIBRARY}: {0}")]
    LoadLibrary(String),
    #[error("failed to load the Vulkan entry points: {0}")]
    LoadEntry(String),
    #[error("failed to query the Vulkan loader version: {0:?}")]
    LoaderVersion(vk::ErrorCode),
    #[error("Vulkan {required} is required but the loader exposes {found}")]
    UnsupportedLoaderVersion { required: Version, found: Version },
    #[error("failed to create the Vulkan instance: {0:?}")]
    CreateInstance(vk::ErrorCode),
    #[error("failed to enumerate Vulkan physical devices: {0:?}")]
    EnumerateDevices(vk::ErrorCode),
    #[error("failed to enumerate Vulkan device extensions: {0:?}")]
    EnumerateExtensions(vk::ErrorCode),
    #[error("failed to probe Vulkan dma-buf format {format} modifier {modifier:#x}: {source:?}")]
    ProbeFormat {
        format: Fourcc,
        modifier: u64,
        source: vk::ErrorCode,
    },
    #[error(transparent)]
    Selection(#[from] DeviceSelectionError),
    #[error("failed to create the Vulkan descriptor-heap dma-buf device: {0:?}")]
    CreateDevice(vk::ErrorCode),
    #[error("failed to create Vulkan frame resources: {0:?}")]
    #[cfg(feature = "tty")]
    CreateFrameResources(vk::ErrorCode),
    #[error(
        "native output target {format} modifier {modifier:#x} with {plane_count} planes is not exportable by the selected Vulkan device"
    )]
    #[cfg(feature = "tty")]
    UnsupportedOutputTarget {
        format: Fourcc,
        modifier: u64,
        plane_count: u32,
    },
    #[error("failed to query the renderer timeline semaphore: {0:?}")]
    #[cfg(feature = "tty")]
    QueryTimeline(vk::ErrorCode),
    #[error("failed to submit a renderer frame: {0}")]
    #[cfg(feature = "tty")]
    SubmitFrame(String),
    #[error("renderer frame could not be prepared: {0}")]
    #[cfg(feature = "tty")]
    Frame(String),
}

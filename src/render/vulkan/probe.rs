use smithay::backend::allocator::{Format as DrmFormat, Fourcc, Modifier};
use vulkanalia::{Instance, Version, prelude::v1_4::*, vk};

use crate::render::{
    DescriptorHeapProperties, DeviceCandidate, DrmDeviceIdentity, DrmNodeId,
    NativeInteropCapabilities, VulkanFormatCapability,
};

use super::{OUTPUT_FORMATS, RendererError, native_image_usage};

pub(super) struct ProbedDevice {
    pub(super) handle: vk::PhysicalDevice,
    pub(super) candidate: DeviceCandidate,
    pub(super) formats: Vec<VulkanFormatCapability>,
}

pub(super) fn probe_devices(instance: &Instance) -> Result<Vec<ProbedDevice>, RendererError> {
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
        let buffer_device_address_supported = buffer_device_address_feature(instance, handle);
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
            && buffer_device_address_supported
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
                buffer_device_address_supported,
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
        buffer_descriptor_alignment: heap.buffer_descriptor_alignment,
        image_descriptor_size: heap.image_descriptor_size,
        image_descriptor_alignment: heap.image_descriptor_alignment,
    }
}

fn buffer_device_address_feature(instance: &Instance, device: vk::PhysicalDevice) -> bool {
    let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut vulkan12);
    unsafe { instance.get_physical_device_features2(device, &mut features) };
    vulkan12.buffer_device_address != 0
}

fn timeline_semaphore_feature(instance: &Instance, device: vk::PhysicalDevice) -> bool {
    let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut vulkan12);
    unsafe { instance.get_physical_device_features2(device, &mut features) };
    vulkan12.timeline_semaphore != 0
}

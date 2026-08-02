use std::collections::BTreeSet;

use vulkanalia::{Instance, Version, prelude::v1_4::*, vk};

use super::{Candidate, DeviceInfo, PciAddress};
use crate::capabilities::{
    CoreFeatures, DescriptorHeapLimits, DeviceProperties, Features, Limits,
    PipelineBinaryProperties,
};
use crate::memory::MemoryTypeInfo;
use crate::queue::{QueueFamilyInfo, QueuePlan};
use crate::roadmap_2026::query_roadmap_2026_device_requirements;
use crate::video::{
    VideoDecodeOperations, query_supported_decode_profiles, query_video_queue_operations,
};
use crate::{Error, Result};

pub(crate) fn probe_devices(instance: &Instance) -> Result<Vec<Candidate>> {
    let handles = unsafe { instance.enumerate_physical_devices() }
        .map_err(|source| Error::vulkan("vkEnumeratePhysicalDevices", source))?;
    handles
        .into_iter()
        .enumerate()
        .map(|(ordinal, handle)| {
            let properties = unsafe { instance.get_physical_device_properties(handle) };
            let extensions: BTreeSet<String> =
                unsafe { instance.enumerate_device_extension_properties(handle, None) }
                    .map_err(|source| {
                        Error::vulkan("vkEnumerateDeviceExtensionProperties", source)
                    })?
                    .into_iter()
                    .map(|extension| extension.extension_name.to_string_lossy().into_owned())
                    .collect();
            let (device_uuid, driver_uuid, pci_address) =
                query_device_identity(instance, handle, extensions.contains("VK_EXT_pci_bus_info"));
            let video_queue_operations = query_video_queue_operations(
                instance,
                handle,
                extensions.contains("VK_KHR_video_queue"),
            );
            let queue_families =
                unsafe { instance.get_physical_device_queue_family_properties(handle) }
                    .into_iter()
                    .enumerate()
                    .map(|(index, family)| QueueFamilyInfo {
                        index: index as u32,
                        queue_count: family.queue_count,
                        flags: family.queue_flags,
                        video_decode_operations: video_queue_operations
                            .get(index)
                            .copied()
                            .unwrap_or_default(),
                    })
                    .collect::<Vec<_>>();
            let queues = QueuePlan::select(&queue_families).unwrap_or(QueuePlan {
                graphics: u32::MAX,
                compute: u32::MAX,
                transfer: u32::MAX,
                video_decode: None,
            });
            let combined_video_operations = video_queue_operations
                .into_iter()
                .fold(VideoDecodeOperations::empty(), VideoDecodeOperations::union);
            let supported_video_decode_profiles = query_supported_decode_profiles(
                instance,
                handle,
                &extensions,
                combined_video_operations,
            );
            let (features, descriptor_heap, device_properties, pipeline_binary_properties) =
                query_features(instance, handle, &extensions);
            let extension_names = extensions.iter().cloned().collect::<Vec<_>>();
            let roadmap = query_roadmap_2026_device_requirements(
                instance,
                handle,
                properties.api_version,
                &extension_names,
            );
            let roadmap_2026_ready = roadmap.ready();
            let mut roadmap_2026_failures = Vec::new();
            if !roadmap.api_version_ready {
                roadmap_2026_failures.push("apiVersion".into());
            }
            roadmap_2026_failures.extend(
                roadmap
                    .missing_device_extensions
                    .iter()
                    .map(|name| format!("extension:{name}")),
            );
            roadmap_2026_failures.extend(
                roadmap
                    .missing_core_features
                    .iter()
                    .map(|name| format!("feature:{name}")),
            );
            roadmap_2026_failures.extend(
                roadmap
                    .missing_properties
                    .iter()
                    .map(|name| format!("property:{name}")),
            );
            roadmap_2026_failures.extend(
                roadmap
                    .missing_extension_features
                    .iter()
                    .map(|name| format!("extension-feature:{name}")),
            );
            let supported_features = Features::from_core(features);
            let limits = Limits {
                max_image_dimension_2d: properties.limits.max_image_dimension_2d,
                max_memory_allocation_count: properties.limits.max_memory_allocation_count,
                max_bound_descriptor_sets: properties.limits.max_bound_descriptor_sets,
                max_push_constants_size: properties.limits.max_push_constants_size,
                max_color_attachments: properties.limits.max_color_attachments,
                max_per_stage_descriptor_input_attachments: properties
                    .limits
                    .max_per_stage_descriptor_input_attachments,
                descriptor_heap,
            };
            let memory_properties =
                unsafe { instance.get_physical_device_memory_properties(handle) };
            let memory_types = (0..memory_properties.memory_type_count)
                .map(|index| {
                    let memory_type = memory_properties.memory_types[index as usize];
                    let heap = memory_properties.memory_heaps[memory_type.heap_index as usize];
                    MemoryTypeInfo {
                        index,
                        heap_index: memory_type.heap_index,
                        properties: memory_type.property_flags,
                        heap_size: heap.size,
                    }
                })
                .collect();
            Ok(Candidate {
                handle,
                info: DeviceInfo {
                    ordinal,
                    name: properties.device_name.to_string_lossy().into_owned(),
                    api_version: Version::from(properties.api_version),
                    device_type: properties.device_type,
                    vendor_id: properties.vendor_id,
                    device_id: properties.device_id,
                    driver_version: properties.driver_version,
                    device_uuid,
                    driver_uuid,
                    pci_address,
                    features,
                    supported_features,
                    properties: device_properties,
                    pipeline_binary_properties,
                    limits,
                    memory_types,
                    non_coherent_atom_size: properties.limits.non_coherent_atom_size,
                    queues,
                    queue_families,
                    supported_video_decode_profiles,
                    extensions,
                    roadmap_2026_ready,
                    roadmap_2026_failures,
                },
            })
        })
        .collect()
}

fn query_device_identity(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    pci_bus_info_available: bool,
) -> ([u8; vk::UUID_SIZE], [u8; vk::UUID_SIZE], Option<PciAddress>) {
    let mut identity = vk::PhysicalDeviceIDProperties::default();
    let mut pci = vk::PhysicalDevicePCIBusInfoPropertiesEXT::default();
    let mut builder = vk::PhysicalDeviceProperties2::builder().push_next(&mut identity);
    if pci_bus_info_available {
        builder = builder.push_next(&mut pci);
    }
    let mut properties = builder.build();
    unsafe { instance.get_physical_device_properties2(physical_device, &mut properties) };
    (
        identity.device_uuid.0,
        identity.driver_uuid.0,
        pci_bus_info_available.then_some(PciAddress {
            domain: pci.pci_domain,
            bus: pci.pci_bus,
            device: pci.pci_device,
            function: pci.pci_function,
        }),
    )
}

fn query_features(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    extensions: &BTreeSet<String>,
) -> (
    CoreFeatures,
    DescriptorHeapLimits,
    DeviceProperties,
    PipelineBinaryProperties,
) {
    let mut vulkan14_properties = vk::PhysicalDeviceVulkan14Properties::default();
    let mut properties2 =
        vk::PhysicalDeviceProperties2::builder().push_next(&mut vulkan14_properties);
    unsafe { instance.get_physical_device_properties2(physical_device, &mut properties2) };
    let mut vulkan11 = vk::PhysicalDeviceVulkan11Features::default();
    let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut vulkan14 = vk::PhysicalDeviceVulkan14Features::default();
    let mut features = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut vulkan11)
        .push_next(&mut vulkan12)
        .push_next(&mut vulkan13)
        .push_next(&mut vulkan14);
    unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
    let descriptor_heap_available = extensions.contains("VK_EXT_descriptor_heap");
    let descriptor_heap = if descriptor_heap_available {
        let mut extension = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::default();
        let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
        unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
        extension.descriptor_heap != 0
    } else {
        false
    };
    let present_mode_fifo_latest_ready =
        if extensions.contains("VK_KHR_present_mode_fifo_latest_ready") {
            let mut extension = vk::PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR::default();
            let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
            unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
            extension.present_mode_fifo_latest_ready != 0
        } else {
            false
        };
    let pipeline_binaries = if extensions.contains("VK_KHR_pipeline_binary") {
        let mut extension = vk::PhysicalDevicePipelineBinaryFeaturesKHR::default();
        let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
        unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
        extension.pipeline_binaries != 0
    } else {
        false
    };
    let shader_untyped_pointers = query_extension_feature(
        instance,
        physical_device,
        extensions,
        "VK_KHR_shader_untyped_pointers",
        |instance, physical_device| {
            let mut extension = vk::PhysicalDeviceShaderUntypedPointersFeaturesKHR::default();
            let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
            unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
            extension.shader_untyped_pointers != 0
        },
    );
    let present_id2 = query_extension_feature(
        instance,
        physical_device,
        extensions,
        "VK_KHR_present_id2",
        |instance, physical_device| {
            let mut extension = vk::PhysicalDevicePresentId2FeaturesKHR::default();
            let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
            unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
            extension.present_id2 != 0
        },
    );
    let present_wait2 = query_extension_feature(
        instance,
        physical_device,
        extensions,
        "VK_KHR_present_wait2",
        |instance, physical_device| {
            let mut extension = vk::PhysicalDevicePresentWait2FeaturesKHR::default();
            let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
            unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
            extension.present_wait2 != 0
        },
    );
    let swapchain_maintenance1 = query_extension_feature(
        instance,
        physical_device,
        extensions,
        "VK_KHR_swapchain_maintenance1",
        |instance, physical_device| {
            let mut extension = vk::PhysicalDeviceSwapchainMaintenance1FeaturesKHR::default();
            let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
            unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
            extension.swapchain_maintenance1 != 0
        },
    );
    let advanced_blend = extensions.contains("VK_EXT_blend_operation_advanced");
    let advanced_blend_coherent_operations = advanced_blend && {
        let mut extension = vk::PhysicalDeviceBlendOperationAdvancedFeaturesEXT::default();
        let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
        unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
        extension.advanced_blend_coherent_operations != 0
    };
    let multisampled_render_to_single_sampled = query_extension_feature(
        instance,
        physical_device,
        extensions,
        "VK_EXT_multisampled_render_to_single_sampled",
        |instance, physical_device| {
            let mut extension =
                vk::PhysicalDeviceMultisampledRenderToSingleSampledFeaturesEXT::default();
            let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
            unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
            extension.multisampled_render_to_single_sampled != 0
        },
    );
    let maintenance7 = query_maintenance7(instance, physical_device, extensions);
    let maintenance8 = query_maintenance8(instance, physical_device, extensions);
    let maintenance9 = query_maintenance9(instance, physical_device, extensions);
    let maintenance10 = query_maintenance10(instance, physical_device, extensions);
    let descriptor_heap_limits = if descriptor_heap_available {
        query_descriptor_heap_limits(instance, physical_device)
    } else {
        DescriptorHeapLimits::default()
    };
    let external_memory_dma_buf = [
        "VK_KHR_external_memory_fd",
        "VK_EXT_external_memory_dma_buf",
        "VK_EXT_image_drm_format_modifier",
        "VK_EXT_queue_family_foreign",
    ]
    .iter()
    .all(|extension| extensions.contains(*extension));
    let external_semaphore_sync_fd = extensions.contains("VK_KHR_external_semaphore_fd")
        && supports_sync_fd_semaphore(instance, physical_device);
    let device_properties = DeviceProperties {
        min_uniform_buffer_offset_alignment: properties2
            .properties
            .limits
            .min_uniform_buffer_offset_alignment,
        max_sampler_anisotropy_x1: properties2
            .properties
            .limits
            .max_sampler_anisotropy
            .floor()
            .max(1.0) as u32,
        framebuffer_color_sample_counts: crate::SampleCounts::from_vk(
            properties2
                .properties
                .limits
                .framebuffer_color_sample_counts,
        ),
        dynamic_rendering_local_read_depth_stencil_attachments: vulkan14_properties
            .dynamic_rendering_local_read_depth_stencil_attachments
            != 0,
        dynamic_rendering_local_read_multisampled_attachments: vulkan14_properties
            .dynamic_rendering_local_read_multisampled_attachments
            != 0,
        identical_memory_type_requirements: vulkan14_properties.identical_memory_type_requirements
            != 0,
    };
    (
        CoreFeatures {
            sampler_anisotropy: features.features.sampler_anisotropy != 0,
            sample_rate_shading: features.features.sample_rate_shading != 0,
            texture_compression_bc: features.features.texture_compression_bc != 0,
            shader_draw_parameters: vulkan11.shader_draw_parameters != 0,
            timeline_semaphore: vulkan12.timeline_semaphore != 0,
            scalar_block_layout: vulkan12.scalar_block_layout != 0,
            descriptor_indexing: vulkan12.descriptor_indexing != 0,
            runtime_descriptor_array: vulkan12.runtime_descriptor_array != 0,
            buffer_device_address: vulkan12.buffer_device_address != 0,
            synchronization2: vulkan13.synchronization2 != 0,
            dynamic_rendering: vulkan13.dynamic_rendering != 0,
            shader_demote_to_helper_invocation: vulkan13.shader_demote_to_helper_invocation != 0,
            maintenance4: vulkan13.maintenance4 != 0,
            maintenance5: vulkan14.maintenance5 != 0,
            maintenance6: vulkan14.maintenance6 != 0,
            dynamic_rendering_local_read: vulkan14.dynamic_rendering_local_read != 0,
            host_image_copy: vulkan14.host_image_copy != 0,
            shader_untyped_pointers,
            descriptor_heap,
            pipeline_binaries,
            present_mode_fifo_latest_ready,
            present_id2,
            present_wait2,
            swapchain_maintenance1,
            advanced_blend,
            advanced_blend_coherent_operations,
            multisampled_render_to_single_sampled,
            maintenance7,
            maintenance8,
            maintenance9,
            maintenance10,
            external_memory_dma_buf,
            external_semaphore_sync_fd,
        },
        descriptor_heap_limits,
        device_properties,
        query_pipeline_binary_properties(instance, physical_device, pipeline_binaries),
    )
}

fn query_extension_feature(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    extensions: &BTreeSet<String>,
    extension: &str,
    query: impl FnOnce(&Instance, vk::PhysicalDevice) -> bool,
) -> bool {
    extensions.contains(extension) && query(instance, physical_device)
}

macro_rules! query_maintenance_feature {
    ($name:ident, $extension:literal, $feature:ident, $field:ident) => {
        fn $name(
            instance: &Instance,
            physical_device: vk::PhysicalDevice,
            extensions: &BTreeSet<String>,
        ) -> bool {
            query_extension_feature(
                instance,
                physical_device,
                extensions,
                $extension,
                |instance, physical_device| {
                    let mut extension = vk::$feature::default();
                    let mut features =
                        vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
                    unsafe {
                        instance.get_physical_device_features2(physical_device, &mut features)
                    };
                    extension.$field != 0
                },
            )
        }
    };
}

query_maintenance_feature!(
    query_maintenance7,
    "VK_KHR_maintenance7",
    PhysicalDeviceMaintenance7FeaturesKHR,
    maintenance7
);
query_maintenance_feature!(
    query_maintenance8,
    "VK_KHR_maintenance8",
    PhysicalDeviceMaintenance8FeaturesKHR,
    maintenance8
);
query_maintenance_feature!(
    query_maintenance9,
    "VK_KHR_maintenance9",
    PhysicalDeviceMaintenance9FeaturesKHR,
    maintenance9
);
query_maintenance_feature!(
    query_maintenance10,
    "VK_KHR_maintenance10",
    PhysicalDeviceMaintenance10FeaturesKHR,
    maintenance10
);

fn query_pipeline_binary_properties(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    available: bool,
) -> PipelineBinaryProperties {
    if !available {
        return PipelineBinaryProperties::default();
    }
    let mut binary = vk::PhysicalDevicePipelineBinaryPropertiesKHR::default();
    let mut properties = vk::PhysicalDeviceProperties2::builder().push_next(&mut binary);
    unsafe { instance.get_physical_device_properties2(physical_device, &mut properties) };
    PipelineBinaryProperties {
        internal_cache: binary.pipeline_binary_internal_cache != 0,
        internal_cache_control: binary.pipeline_binary_internal_cache_control != 0,
        prefers_internal_cache: binary.pipeline_binary_prefers_internal_cache != 0,
        precompiled_internal_cache: binary.pipeline_binary_precompiled_internal_cache != 0,
        compressed_data: binary.pipeline_binary_compressed_data != 0,
    }
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
    properties
        .compatible_handle_types
        .contains(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
        && properties.external_semaphore_features.contains(
            vk::ExternalSemaphoreFeatureFlags::IMPORTABLE
                | vk::ExternalSemaphoreFeatureFlags::EXPORTABLE,
        )
}

fn query_descriptor_heap_limits(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> DescriptorHeapLimits {
    let mut heap = vk::PhysicalDeviceDescriptorHeapPropertiesEXT::default();
    let mut properties = vk::PhysicalDeviceProperties2::builder().push_next(&mut heap);
    unsafe { instance.get_physical_device_properties2(physical_device, &mut properties) };
    DescriptorHeapLimits {
        sampler_heap_alignment: heap.sampler_heap_alignment,
        resource_heap_alignment: heap.resource_heap_alignment,
        max_sampler_heap_size: heap.max_sampler_heap_size,
        max_resource_heap_size: heap.max_resource_heap_size,
        min_sampler_heap_reserved_range: heap.min_sampler_heap_reserved_range,
        min_sampler_heap_reserved_range_with_embedded: heap
            .min_sampler_heap_reserved_range_with_embedded,
        min_resource_heap_reserved_range: heap.min_resource_heap_reserved_range,
        sampler_descriptor_size: heap.sampler_descriptor_size,
        image_descriptor_size: heap.image_descriptor_size,
        buffer_descriptor_size: heap.buffer_descriptor_size,
        sampler_descriptor_alignment: heap.sampler_descriptor_alignment,
        image_descriptor_alignment: heap.image_descriptor_alignment,
        buffer_descriptor_alignment: heap.buffer_descriptor_alignment,
        max_push_data_size: heap.max_push_data_size,
        max_embedded_samplers: heap.max_descriptor_heap_embedded_samplers,
    }
}

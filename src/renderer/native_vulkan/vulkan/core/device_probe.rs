use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;

use super::features::{
    NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaVulkan14PropertySnapshot, native_vulkan_vulkanalia_core_feature_snapshot,
};
use super::instance::{
    NATIVE_VULKAN_VULKANALIA_REQUIRED_LOADER,
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};
use super::queue_probe::native_vulkan_vulkanalia_video_decode_queue_family_indices;
use super::video_device::{
    VIDEO_MAINTENANCE1_EXTENSION_NAME, VIDEO_MAINTENANCE2_EXTENSION_NAME,
    native_vulkan_vulkanalia_video_device_extension_available,
    native_vulkan_vulkanalia_video_device_feature_selection,
};
use super::video_format_probe::{
    NativeVulkanVulkanaliaVideoFormatProbeSnapshot, native_vulkan_vulkanalia_video_format_probe,
};
use super::video_profile_probe::{
    NativeVulkanVulkanaliaVideoProfileProbeSnapshot, native_vulkan_vulkanalia_video_profile_probe,
};
use super::video_session::{
    NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe,
};

const REQUIRED_INSTANCE_EXTENSIONS: &[&str] = &["VK_KHR_surface", "VK_KHR_wayland_surface"];
const REQUIRED_VIDEO_DEVICE_EXTENSIONS: &[&str] = &[
    "VK_KHR_video_queue",
    "VK_KHR_video_decode_queue",
    "VK_KHR_video_decode_h264",
    "VK_KHR_video_decode_h265",
    "VK_KHR_video_decode_av1",
];
const REQUIRED_EXTERNAL_MEMORY_DEVICE_EXTENSIONS: &[&str] = &[
    "VK_KHR_external_memory_fd",
    "VK_KHR_external_semaphore_fd",
    "VK_KHR_timeline_semaphore",
    "VK_EXT_external_memory_dma_buf",
    "VK_EXT_image_drm_format_modifier",
];
const PREFERRED_VIDEO_MAINTENANCE_DEVICE_EXTENSIONS: &[&str] =
    &["VK_KHR_video_maintenance1", "VK_KHR_video_maintenance2"];
const ROADMAP_2026_TRACKED_DEVICE_EXTENSIONS: &[&str] = &[
    "VK_KHR_present_mode_fifo_latest_ready",
    "VK_KHR_present_id2",
    "VK_KHR_present_wait2",
    "VK_KHR_pipeline_binary",
    "VK_KHR_robustness2",
    "VK_KHR_fragment_shading_rate",
    "VK_KHR_shader_clock",
    "VK_KHR_cooperative_matrix",
    "VK_KHR_compute_shader_derivatives",
    "VK_KHR_depth_clamp_zero_one",
    "VK_KHR_copy_memory_indirect",
    "VK_KHR_maintenance7",
    "VK_KHR_maintenance8",
    "VK_KHR_maintenance9",
    "VK_KHR_maintenance10",
    "VK_KHR_shader_untyped_pointers",
    "VK_KHR_swapchain_maintenance1",
];
const ROADMAP_2026_REFERENCE: &str =
    "Khronos Vulkan Roadmap 2026 tracked probe; runtime adoption remains path-specific";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDeviceProbeTemplate {
    pub binding: &'static str,
    pub required_loader: &'static str,
    pub requested_api_version: String,
    pub required_instance_extensions: &'static [&'static str],
    pub required_video_device_extensions: &'static [&'static str],
    pub required_external_memory_device_extensions: &'static [&'static str],
    pub preferred_video_maintenance_device_extensions: &'static [&'static str],
    pub probe_scope: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDeviceProbeSnapshot {
    pub binding: &'static str,
    pub loader: String,
    pub entry_version: String,
    pub requested_api_version: String,
    pub available_instance_extensions: Vec<String>,
    pub enabled_instance_extensions: Vec<&'static str>,
    pub missing_instance_extensions: Vec<&'static str>,
    pub physical_device_count: usize,
    pub devices: Vec<NativeVulkanVulkanaliaPhysicalDeviceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaPhysicalDeviceSnapshot {
    pub physical_device_index: usize,
    pub physical_device_name: String,
    pub physical_device_type: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub api_version: String,
    pub driver_version: u32,
    pub device_extensions: Vec<String>,
    pub has_required_video_device_extensions: bool,
    pub has_required_external_memory_device_extensions: bool,
    pub has_video_decode_queue_family: bool,
    pub video_decode_queue_family_indices: Vec<u32>,
    pub selected_video_decode_queue_family_index: Option<u32>,
    pub video_profile_capabilities: NativeVulkanVulkanaliaVideoProfileProbeSnapshot,
    pub video_format_capabilities: NativeVulkanVulkanaliaVideoFormatProbeSnapshot,
    pub video_session_resource_plans: Vec<NativeVulkanVulkanaliaVideoSessionResourceProbePlan>,
    pub core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
    pub vulkan_1_4_properties: NativeVulkanVulkanaliaVulkan14PropertySnapshot,
    pub descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    pub video_maintenance_features: NativeVulkanVulkanaliaVideoMaintenanceFeatureSnapshot,
    pub roadmap_2026: NativeVulkanVulkanaliaRoadmap2026ProbeSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoMaintenanceFeatureSnapshot {
    pub video_maintenance1_extension_available: bool,
    pub video_maintenance2_extension_available: bool,
    pub video_maintenance1_feature: bool,
    pub video_maintenance2_feature: bool,
    pub inline_session_parameters_supported: bool,
    pub inline_session_parameter_codecs: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaRoadmap2026ProbeSnapshot {
    pub probe_scope: &'static str,
    pub source_reference: &'static str,
    pub api_version_1_4_or_newer: bool,
    pub core_vulkan_1_4_features_ready: bool,
    pub tracked_device_extensions: &'static [&'static str],
    pub tracked_device_extensions_available: Vec<&'static str>,
    pub tracked_device_extensions_missing: Vec<&'static str>,
    pub features: NativeVulkanVulkanaliaRoadmap2026FeatureProbeSnapshot,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaRoadmap2026FeatureProbeSnapshot {
    pub host_image_copy: bool,
    pub present_id2_extension_available: bool,
    pub present_wait2_extension_available: bool,
    pub present_mode_fifo_latest_ready_extension_available: bool,
    pub pipeline_binary_extension_available: bool,
    pub pipeline_binaries: bool,
    pub robustness2_extension_available: bool,
    pub robust_buffer_access2: bool,
    pub robust_image_access2: bool,
    pub null_descriptor: bool,
    pub fragment_shading_rate_extension_available: bool,
    pub pipeline_fragment_shading_rate: bool,
    pub primitive_fragment_shading_rate: bool,
    pub attachment_fragment_shading_rate: bool,
    pub shader_clock_extension_available: bool,
    pub shader_subgroup_clock: bool,
    pub shader_device_clock: bool,
    pub cooperative_matrix_extension_available: bool,
    pub cooperative_matrix: bool,
    pub cooperative_matrix_robust_buffer_access: bool,
    pub compute_shader_derivatives_extension_available: bool,
    pub compute_derivative_group_quads: bool,
    pub compute_derivative_group_linear: bool,
    pub depth_clamp_zero_one_extension_available: bool,
    pub depth_clamp_zero_one: bool,
    pub copy_memory_indirect_extension_available: bool,
    pub indirect_memory_copy: bool,
    pub indirect_memory_to_image_copy: bool,
    pub maintenance7_extension_available: bool,
    pub maintenance7: bool,
    pub maintenance8_extension_available: bool,
    pub maintenance8: bool,
    pub maintenance9_extension_available: bool,
    pub maintenance9: bool,
    pub maintenance10_extension_available: bool,
    pub maintenance10: bool,
    pub maintenance10_rgba4_opaque_black_swizzled: bool,
    pub maintenance10_resolve_srgb_format_applies_transfer_function: bool,
    pub maintenance10_resolve_srgb_format_supports_transfer_function_control: bool,
    pub shader_untyped_pointers_extension_available: bool,
    pub shader_untyped_pointers: bool,
    pub swapchain_maintenance1_extension_available: bool,
}

pub fn native_vulkan_vulkanalia_device_probe_template() -> NativeVulkanVulkanaliaDeviceProbeTemplate
{
    NativeVulkanVulkanaliaDeviceProbeTemplate {
        binding: "vulkanalia",
        required_loader: NATIVE_VULKAN_VULKANALIA_REQUIRED_LOADER,
        requested_api_version: Version::V1_4_0.to_string(),
        required_instance_extensions: REQUIRED_INSTANCE_EXTENSIONS,
        required_video_device_extensions: REQUIRED_VIDEO_DEVICE_EXTENSIONS,
        required_external_memory_device_extensions: REQUIRED_EXTERNAL_MEMORY_DEVICE_EXTENSIONS,
        preferred_video_maintenance_device_extensions:
            PREFERRED_VIDEO_MAINTENANCE_DEVICE_EXTENSIONS,
        probe_scope: "entry/instance/physical-device capability enumeration only; no logical device, surface, swapchain or submit work",
    }
}

pub fn probe_native_vulkan_vulkanalia_devices()
-> Result<NativeVulkanVulkanaliaDeviceProbeSnapshot, String> {
    let vulkan = native_vulkan_vulkanalia_create_instance_with_required_extensions(
        REQUIRED_INSTANCE_EXTENSIONS,
    )?;
    let devices = probe_vulkanalia_instance_devices(&vulkan.instance);
    let loader_name = vulkan.loader_name.to_owned();
    let entry_version = vulkan.entry_version.to_string();
    let available_instance_extensions = sorted_strings(
        vulkan
            .extension_selection
            .available_instance_extensions
            .clone(),
    );
    let enabled_instance_extensions = vulkan
        .extension_selection
        .enabled_instance_extensions
        .clone();
    let missing_instance_extensions = vulkan
        .extension_selection
        .missing_instance_extensions
        .clone();
    native_vulkan_vulkanalia_destroy_instance(vulkan);

    let devices = devices?;
    Ok(NativeVulkanVulkanaliaDeviceProbeSnapshot {
        binding: "vulkanalia",
        loader: loader_name,
        entry_version,
        requested_api_version: Version::V1_4_0.to_string(),
        available_instance_extensions,
        enabled_instance_extensions,
        missing_instance_extensions,
        physical_device_count: devices.len(),
        devices,
    })
}

fn probe_vulkanalia_instance_devices(
    instance: &Instance,
) -> Result<Vec<NativeVulkanVulkanaliaPhysicalDeviceSnapshot>, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|err| format!("vkEnumeratePhysicalDevices(vulkanalia): {err:?}"))?;
    physical_devices
        .iter()
        .copied()
        .enumerate()
        .map(|(physical_device_index, physical_device)| {
            let properties = unsafe { instance.get_physical_device_properties(physical_device) };
            let api_version_raw = properties.api_version;
            let (core_features, vulkan_1_4_properties, descriptor_heap_properties) =
                native_vulkan_vulkanalia_core_feature_snapshot(instance, physical_device);
            let device_extensions =
                unsafe { instance.enumerate_device_extension_properties(physical_device, None) }
                    .map_err(|err| {
                        format!("vkEnumerateDeviceExtensionProperties(vulkanalia): {err:?}")
                    })?
                    .into_iter()
                    .map(|property| property.extension_name.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();

            let has_required_video_device_extensions =
                has_all_extensions(&device_extensions, REQUIRED_VIDEO_DEVICE_EXTENSIONS);
            let has_required_external_memory_device_extensions = has_all_extensions(
                &device_extensions,
                REQUIRED_EXTERNAL_MEMORY_DEVICE_EXTENSIONS,
            );
            let video_maintenance_features = query_vulkanalia_video_maintenance_features(
                instance,
                physical_device,
                &device_extensions,
            );
            let roadmap_2026 = query_vulkanalia_roadmap_2026_probe(
                instance,
                physical_device,
                api_version_raw,
                &device_extensions,
                core_features,
            );
            let video_decode_queue_family_indices =
                native_vulkan_vulkanalia_video_decode_queue_family_indices(
                    instance,
                    physical_device,
                );
            let selected_video_decode_queue_family_index =
                video_decode_queue_family_indices.first().copied();
            let has_video_decode_queue_family = selected_video_decode_queue_family_index.is_some();
            let video_profile_capabilities = native_vulkan_vulkanalia_video_profile_probe(
                instance,
                physical_device,
                &device_extensions,
                has_video_decode_queue_family,
            );
            let video_format_capabilities = native_vulkan_vulkanalia_video_format_probe(
                instance,
                physical_device,
                &device_extensions,
                has_video_decode_queue_family,
            );
            let video_session_resource_plans =
                native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe(
                    &video_format_capabilities,
                );

            Ok(NativeVulkanVulkanaliaPhysicalDeviceSnapshot {
                physical_device_index,
                physical_device_name: properties.device_name.to_string_lossy().into_owned(),
                physical_device_type: format!("{:?}", properties.device_type),
                vendor_id: properties.vendor_id,
                device_id: properties.device_id,
                api_version: Version::from(properties.api_version).to_string(),
                driver_version: properties.driver_version,
                has_required_video_device_extensions,
                has_required_external_memory_device_extensions,
                has_video_decode_queue_family,
                video_decode_queue_family_indices,
                selected_video_decode_queue_family_index,
                video_profile_capabilities,
                video_format_capabilities,
                video_session_resource_plans,
                device_extensions: sorted_strings(device_extensions),
                core_features,
                vulkan_1_4_properties,
                descriptor_heap_properties,
                video_maintenance_features,
                roadmap_2026,
            })
        })
        .collect()
}

fn query_vulkanalia_roadmap_2026_probe(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    api_version_raw: u32,
    device_extensions: &[String],
    core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
) -> NativeVulkanVulkanaliaRoadmap2026ProbeSnapshot {
    let tracked_device_extensions_available =
        available_extensions(device_extensions, ROADMAP_2026_TRACKED_DEVICE_EXTENSIONS);
    let tracked_device_extensions_missing =
        missing_extensions(device_extensions, ROADMAP_2026_TRACKED_DEVICE_EXTENSIONS);
    let features = query_vulkanalia_roadmap_2026_feature_probe(
        instance,
        physical_device,
        device_extensions,
        core_features,
    );

    NativeVulkanVulkanaliaRoadmap2026ProbeSnapshot {
        probe_scope: "physical-device feature/extension enumeration only; no logical device enablement and no runtime adoption claim",
        source_reference: ROADMAP_2026_REFERENCE,
        api_version_1_4_or_newer: api_version_raw >= u32::from(Version::V1_4_0),
        core_vulkan_1_4_features_ready: core_features.synchronization2
            && core_features.dynamic_rendering
            && core_features.dynamic_rendering_local_read
            && core_features.maintenance5
            && core_features.maintenance6
            && core_features.host_image_copy,
        tracked_device_extensions: ROADMAP_2026_TRACKED_DEVICE_EXTENSIONS,
        tracked_device_extensions_available,
        tracked_device_extensions_missing,
        features,
    }
}

fn query_vulkanalia_roadmap_2026_feature_probe(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
) -> NativeVulkanVulkanaliaRoadmap2026FeatureProbeSnapshot {
    let mut features = NativeVulkanVulkanaliaRoadmap2026FeatureProbeSnapshot {
        host_image_copy: core_features.host_image_copy,
        present_id2_extension_available: extension_available(
            device_extensions,
            "VK_KHR_present_id2",
        ),
        present_wait2_extension_available: extension_available(
            device_extensions,
            "VK_KHR_present_wait2",
        ),
        present_mode_fifo_latest_ready_extension_available: extension_available(
            device_extensions,
            "VK_KHR_present_mode_fifo_latest_ready",
        ),
        pipeline_binary_extension_available: extension_available(
            device_extensions,
            "VK_KHR_pipeline_binary",
        ),
        robustness2_extension_available: extension_available(
            device_extensions,
            "VK_KHR_robustness2",
        ),
        fragment_shading_rate_extension_available: extension_available(
            device_extensions,
            "VK_KHR_fragment_shading_rate",
        ),
        shader_clock_extension_available: extension_available(
            device_extensions,
            "VK_KHR_shader_clock",
        ),
        cooperative_matrix_extension_available: extension_available(
            device_extensions,
            "VK_KHR_cooperative_matrix",
        ),
        compute_shader_derivatives_extension_available: extension_available(
            device_extensions,
            "VK_KHR_compute_shader_derivatives",
        ),
        depth_clamp_zero_one_extension_available: extension_available(
            device_extensions,
            "VK_KHR_depth_clamp_zero_one",
        ),
        copy_memory_indirect_extension_available: extension_available(
            device_extensions,
            "VK_KHR_copy_memory_indirect",
        ),
        maintenance7_extension_available: extension_available(
            device_extensions,
            "VK_KHR_maintenance7",
        ),
        maintenance8_extension_available: extension_available(
            device_extensions,
            "VK_KHR_maintenance8",
        ),
        maintenance9_extension_available: extension_available(
            device_extensions,
            "VK_KHR_maintenance9",
        ),
        maintenance10_extension_available: extension_available(
            device_extensions,
            "VK_KHR_maintenance10",
        ),
        shader_untyped_pointers_extension_available: extension_available(
            device_extensions,
            "VK_KHR_shader_untyped_pointers",
        ),
        swapchain_maintenance1_extension_available: extension_available(
            device_extensions,
            "VK_KHR_swapchain_maintenance1",
        ),
        ..NativeVulkanVulkanaliaRoadmap2026FeatureProbeSnapshot::default()
    };

    if features.pipeline_binary_extension_available {
        let mut pipeline_binary = vk::PhysicalDevicePipelineBinaryFeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut pipeline_binary);
        features.pipeline_binaries = pipeline_binary.pipeline_binaries != 0;
    }
    if features.robustness2_extension_available {
        let mut robustness2 = vk::PhysicalDeviceRobustness2FeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut robustness2);
        features.robust_buffer_access2 = robustness2.robust_buffer_access2 != 0;
        features.robust_image_access2 = robustness2.robust_image_access2 != 0;
        features.null_descriptor = robustness2.null_descriptor != 0;
    }
    if features.fragment_shading_rate_extension_available {
        let mut fragment_shading_rate = vk::PhysicalDeviceFragmentShadingRateFeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut fragment_shading_rate);
        features.pipeline_fragment_shading_rate =
            fragment_shading_rate.pipeline_fragment_shading_rate != 0;
        features.primitive_fragment_shading_rate =
            fragment_shading_rate.primitive_fragment_shading_rate != 0;
        features.attachment_fragment_shading_rate =
            fragment_shading_rate.attachment_fragment_shading_rate != 0;
    }
    if features.shader_clock_extension_available {
        let mut shader_clock = vk::PhysicalDeviceShaderClockFeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut shader_clock);
        features.shader_subgroup_clock = shader_clock.shader_subgroup_clock != 0;
        features.shader_device_clock = shader_clock.shader_device_clock != 0;
    }
    if features.cooperative_matrix_extension_available {
        let mut cooperative_matrix = vk::PhysicalDeviceCooperativeMatrixFeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut cooperative_matrix);
        features.cooperative_matrix = cooperative_matrix.cooperative_matrix != 0;
        features.cooperative_matrix_robust_buffer_access =
            cooperative_matrix.cooperative_matrix_robust_buffer_access != 0;
    }
    if features.compute_shader_derivatives_extension_available {
        let mut compute_derivatives =
            vk::PhysicalDeviceComputeShaderDerivativesFeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut compute_derivatives);
        features.compute_derivative_group_quads =
            compute_derivatives.compute_derivative_group_quads != 0;
        features.compute_derivative_group_linear =
            compute_derivatives.compute_derivative_group_linear != 0;
    }
    if features.depth_clamp_zero_one_extension_available {
        let mut depth_clamp_zero_one = vk::PhysicalDeviceDepthClampZeroOneFeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut depth_clamp_zero_one);
        features.depth_clamp_zero_one = depth_clamp_zero_one.depth_clamp_zero_one != 0;
    }
    if features.copy_memory_indirect_extension_available {
        let mut copy_memory_indirect = vk::PhysicalDeviceCopyMemoryIndirectFeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut copy_memory_indirect);
        features.indirect_memory_copy = copy_memory_indirect.indirect_memory_copy != 0;
        features.indirect_memory_to_image_copy =
            copy_memory_indirect.indirect_memory_to_image_copy != 0;
    }
    if features.maintenance7_extension_available {
        let mut maintenance7 = vk::PhysicalDeviceMaintenance7FeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut maintenance7);
        features.maintenance7 = maintenance7.maintenance7 != 0;
    }
    if features.maintenance8_extension_available {
        let mut maintenance8 = vk::PhysicalDeviceMaintenance8FeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut maintenance8);
        features.maintenance8 = maintenance8.maintenance8 != 0;
    }
    if features.maintenance9_extension_available {
        let mut maintenance9 = vk::PhysicalDeviceMaintenance9FeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut maintenance9);
        features.maintenance9 = maintenance9.maintenance9 != 0;
    }
    if features.maintenance10_extension_available {
        let mut maintenance10 = vk::PhysicalDeviceMaintenance10FeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut maintenance10);
        features.maintenance10 = maintenance10.maintenance10 != 0;

        let mut maintenance10_properties = vk::PhysicalDeviceMaintenance10PropertiesKHR::default();
        query_property_struct(instance, physical_device, &mut maintenance10_properties);
        features.maintenance10_rgba4_opaque_black_swizzled =
            maintenance10_properties.rgba4_opaque_black_swizzled != 0;
        features.maintenance10_resolve_srgb_format_applies_transfer_function =
            maintenance10_properties.resolve_srgb_format_applies_transfer_function != 0;
        features.maintenance10_resolve_srgb_format_supports_transfer_function_control =
            maintenance10_properties.resolve_srgb_format_supports_transfer_function_control != 0;
    }
    if features.shader_untyped_pointers_extension_available {
        let mut shader_untyped_pointers =
            vk::PhysicalDeviceShaderUntypedPointersFeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut shader_untyped_pointers);
        features.shader_untyped_pointers = shader_untyped_pointers.shader_untyped_pointers != 0;
    }

    features
}

fn query_feature_struct<T>(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    feature: &mut T,
) where
    T: vk::Cast<Target = T> + vk::ExtendsPhysicalDeviceFeatures2,
{
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
}

fn query_property_struct<T>(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    property: &mut T,
) where
    T: vk::Cast<Target = T> + vk::ExtendsPhysicalDeviceProperties2,
{
    let mut properties2 = vk::PhysicalDeviceProperties2::builder()
        .push_next(property)
        .build();
    unsafe {
        instance.get_physical_device_properties2(physical_device, &mut properties2);
    }
}

fn query_vulkanalia_video_maintenance_features(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
) -> NativeVulkanVulkanaliaVideoMaintenanceFeatureSnapshot {
    let video_maintenance1_extension_available =
        native_vulkan_vulkanalia_video_device_extension_available(
            device_extensions,
            VIDEO_MAINTENANCE1_EXTENSION_NAME,
        );
    let video_maintenance2_extension_available =
        native_vulkan_vulkanalia_video_device_extension_available(
            device_extensions,
            VIDEO_MAINTENANCE2_EXTENSION_NAME,
        );
    let feature_selection = native_vulkan_vulkanalia_video_device_feature_selection(
        instance,
        physical_device,
        device_extensions,
    );

    NativeVulkanVulkanaliaVideoMaintenanceFeatureSnapshot {
        video_maintenance1_extension_available,
        video_maintenance2_extension_available,
        video_maintenance1_feature: feature_selection.video_maintenance1_enabled,
        video_maintenance2_feature: feature_selection.video_maintenance2_enabled,
        inline_session_parameters_supported: feature_selection.inline_session_parameters_enabled,
        inline_session_parameter_codecs: feature_selection.inline_session_parameter_codecs(),
    }
}

fn has_all_extensions(available: &[String], required: &[&str]) -> bool {
    required
        .iter()
        .all(|required| extension_available(available, required))
}

fn extension_available(available: &[String], required: &str) -> bool {
    available.iter().any(|available| available == required)
}

fn available_extensions(
    available: &[String],
    tracked: &'static [&'static str],
) -> Vec<&'static str> {
    tracked
        .iter()
        .copied()
        .filter(|extension| extension_available(available, extension))
        .collect()
}

fn missing_extensions(available: &[String], tracked: &'static [&'static str]) -> Vec<&'static str> {
    tracked
        .iter()
        .copied()
        .filter(|extension| !extension_available(available, extension))
        .collect()
}

fn sorted_strings(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_probe_template_tracks_vulkan_1_4_and_video_gates() {
        let template = native_vulkan_vulkanalia_device_probe_template();
        assert_eq!(template.binding, "vulkanalia");
        assert_eq!(template.requested_api_version, "1.4.0");
        assert!(
            template
                .required_instance_extensions
                .contains(&"VK_KHR_wayland_surface")
        );
        assert!(
            template
                .required_video_device_extensions
                .contains(&"VK_KHR_video_decode_h265")
        );
        assert!(
            template
                .required_external_memory_device_extensions
                .contains(&"VK_EXT_image_drm_format_modifier")
        );
        assert!(
            template
                .preferred_video_maintenance_device_extensions
                .contains(&"VK_KHR_video_maintenance2")
        );
        assert!(template.probe_scope.contains("no logical device"));
    }

    #[test]
    fn extension_gate_requires_every_extension() {
        let available = vec![
            "VK_KHR_video_queue".to_owned(),
            "VK_KHR_video_decode_queue".to_owned(),
        ];
        assert!(has_all_extensions(
            &available,
            &["VK_KHR_video_queue", "VK_KHR_video_decode_queue"]
        ));
        assert!(!has_all_extensions(
            &available,
            &["VK_KHR_video_queue", "VK_KHR_video_decode_h265"]
        ));
    }

    #[test]
    fn roadmap_2026_extension_tracking_splits_available_and_missing() {
        let available = vec![
            "VK_KHR_present_id2".to_owned(),
            "VK_KHR_pipeline_binary".to_owned(),
            "VK_KHR_maintenance9".to_owned(),
            "VK_KHR_maintenance10".to_owned(),
        ];

        let available_tracked =
            available_extensions(&available, ROADMAP_2026_TRACKED_DEVICE_EXTENSIONS);
        let missing_tracked =
            missing_extensions(&available, ROADMAP_2026_TRACKED_DEVICE_EXTENSIONS);

        assert!(available_tracked.contains(&"VK_KHR_present_id2"));
        assert!(available_tracked.contains(&"VK_KHR_pipeline_binary"));
        assert!(available_tracked.contains(&"VK_KHR_maintenance9"));
        assert!(available_tracked.contains(&"VK_KHR_maintenance10"));
        assert!(!available_tracked.contains(&"VK_KHR_present_wait2"));
        assert!(missing_tracked.contains(&"VK_KHR_present_wait2"));
        assert!(missing_tracked.contains(&"VK_KHR_robustness2"));
    }

    #[test]
    fn roadmap_2026_probe_snapshot_keeps_extension_and_feature_bits_separate() {
        let snapshot = NativeVulkanVulkanaliaRoadmap2026ProbeSnapshot {
            probe_scope: "unit-test",
            source_reference: ROADMAP_2026_REFERENCE,
            api_version_1_4_or_newer: true,
            core_vulkan_1_4_features_ready: true,
            tracked_device_extensions: ROADMAP_2026_TRACKED_DEVICE_EXTENSIONS,
            tracked_device_extensions_available: vec!["VK_KHR_pipeline_binary"],
            tracked_device_extensions_missing: vec!["VK_KHR_robustness2"],
            features: NativeVulkanVulkanaliaRoadmap2026FeatureProbeSnapshot {
                host_image_copy: true,
                pipeline_binary_extension_available: true,
                pipeline_binaries: false,
                robustness2_extension_available: false,
                robust_buffer_access2: false,
                robust_image_access2: false,
                null_descriptor: false,
                ..NativeVulkanVulkanaliaRoadmap2026FeatureProbeSnapshot::default()
            },
        };

        assert!(snapshot.api_version_1_4_or_newer);
        assert!(snapshot.core_vulkan_1_4_features_ready);
        assert!(snapshot.features.host_image_copy);
        assert!(snapshot.features.pipeline_binary_extension_available);
        assert!(!snapshot.features.pipeline_binaries);
        assert!(!snapshot.features.robustness2_extension_available);
        assert!(!snapshot.features.robust_buffer_access2);
    }

    #[test]
    fn video_maintenance_snapshot_only_claims_inline_when_feature_is_enabled() {
        let disabled = NativeVulkanVulkanaliaVideoMaintenanceFeatureSnapshot {
            video_maintenance1_extension_available: true,
            video_maintenance2_extension_available: true,
            video_maintenance1_feature: true,
            video_maintenance2_feature: false,
            inline_session_parameters_supported: false,
            inline_session_parameter_codecs: Vec::new(),
        };
        let enabled = NativeVulkanVulkanaliaVideoMaintenanceFeatureSnapshot {
            video_maintenance2_feature: true,
            inline_session_parameters_supported: true,
            inline_session_parameter_codecs: vec!["h264", "h265", "av1"],
            ..disabled.clone()
        };

        assert!(!disabled.inline_session_parameters_supported);
        assert!(enabled.inline_session_parameters_supported);
        assert_eq!(
            enabled.inline_session_parameter_codecs,
            vec!["h264", "h265", "av1"]
        );
    }
}

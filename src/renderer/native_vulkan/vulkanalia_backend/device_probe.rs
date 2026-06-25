use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;

use super::features::{
    NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaVulkan14PropertySnapshot, native_vulkan_vulkanalia_core_feature_snapshot,
};
use super::instance::{
    NATIVE_VULKAN_VULKANALIA_LOADER_CANDIDATES,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDeviceProbeTemplate {
    pub binding: &'static str,
    pub loader_candidates: &'static [&'static str],
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

pub fn native_vulkan_vulkanalia_device_probe_template() -> NativeVulkanVulkanaliaDeviceProbeTemplate
{
    NativeVulkanVulkanaliaDeviceProbeTemplate {
        binding: "vulkanalia",
        loader_candidates: NATIVE_VULKAN_VULKANALIA_LOADER_CANDIDATES,
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
            })
        })
        .collect()
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
        .all(|required| available.iter().any(|available| available == required))
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

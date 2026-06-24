use serde::Serialize;

use super::{
    NativeVulkanVulkanaliaDeviceProbeTemplate, NativeVulkanVulkanaliaFeatureChainTemplate,
    NativeVulkanVulkanaliaVideoProfileTemplate, native_vulkan_vulkanalia_device_probe_template,
    native_vulkan_vulkanalia_feature_chain_template,
    native_vulkan_vulkanalia_video_profile_templates,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaBackendPlan {
    pub binding: &'static str,
    pub phase: &'static str,
    pub api_baseline: &'static str,
    pub api_type_evidence: Vec<&'static str>,
    pub feature_chain_template: NativeVulkanVulkanaliaFeatureChainTemplate,
    pub device_probe_template: NativeVulkanVulkanaliaDeviceProbeTemplate,
    pub video_profile_templates: Vec<NativeVulkanVulkanaliaVideoProfileTemplate>,
    pub required_instance_extensions: &'static [&'static str],
    pub required_device_extensions: &'static [&'static str],
    pub prioritized_vulkan_1_4_features: &'static [&'static str],
    pub migration_gates: &'static [&'static str],
}

pub fn native_vulkan_vulkanalia_backend_plan() -> NativeVulkanVulkanaliaBackendPlan {
    NativeVulkanVulkanaliaBackendPlan {
        binding: "vulkanalia",
        phase: "early-parallel-backend-spike",
        api_baseline: "Vulkan 1.4 binding surface plus Vulkan Video/Wayland/Swapchain extensions",
        api_type_evidence: vec![
            std::any::type_name::<vulkanalia::Version>(),
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceVulkan14Features>(),
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceVulkan14Properties>(),
            std::any::type_name::<vulkanalia::vk::VideoBeginCodingInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeH264PictureInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeH265PictureInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeAV1PictureInfoKHR>(),
        ],
        feature_chain_template: native_vulkan_vulkanalia_feature_chain_template(),
        device_probe_template: native_vulkan_vulkanalia_device_probe_template(),
        video_profile_templates: native_vulkan_vulkanalia_video_profile_templates(),
        required_instance_extensions: &["VK_KHR_surface", "VK_KHR_wayland_surface"],
        required_device_extensions: &[
            "VK_KHR_swapchain",
            "VK_KHR_video_queue",
            "VK_KHR_video_decode_queue",
            "VK_KHR_video_decode_h264",
            "VK_KHR_video_decode_h265",
            "VK_KHR_video_decode_av1",
            "VK_KHR_external_memory_fd",
            "VK_KHR_external_semaphore_fd",
            "VK_KHR_timeline_semaphore",
            "VK_EXT_external_memory_dma_buf",
            "VK_EXT_image_drm_format_modifier",
        ],
        prioritized_vulkan_1_4_features: &[
            "dynamic-rendering-local-read",
            "push-descriptor",
            "maintenance5",
            "maintenance6",
            "scalar-block-layout",
            "synchronization2",
            "larger-portable-limits",
        ],
        migration_gates: &[
            "create Vulkan 1.4 instance/device and report PhysicalDeviceVulkan14Features",
            "probe Wayland surface and swapchain parity with the ash backend",
            "probe Vulkan Video H.264/H.265/AV1 profile and format parity",
            "port one H.265 first-frame submit path without raw FFI regressions",
            "port direct present timing telemetry before replacing the ash main path",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_plan_names_vulkan_1_4_and_video_gates() {
        let plan = native_vulkan_vulkanalia_backend_plan();
        assert_eq!(plan.binding, "vulkanalia");
        assert!(plan.api_baseline.contains("Vulkan 1.4"));
        assert!(
            plan.required_device_extensions
                .contains(&"VK_KHR_video_queue")
        );
        assert!(
            plan.required_device_extensions
                .contains(&"VK_KHR_video_decode_h265")
        );
        assert!(
            plan.prioritized_vulkan_1_4_features
                .contains(&"dynamic-rendering-local-read")
        );
        assert!(
            plan.api_type_evidence
                .iter()
                .any(|name| { name.ends_with("PhysicalDeviceVulkan14Features") })
        );
        assert!(
            plan.api_type_evidence
                .iter()
                .any(|name| { name.ends_with("VideoDecodeAV1PictureInfoKHR") })
        );
        assert_eq!(plan.feature_chain_template.api, "Vulkan 1.4");
        assert_eq!(plan.device_probe_template.requested_api_version, "1.4.0");
        assert_eq!(plan.video_profile_templates.len(), 7);
    }
}

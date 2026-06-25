use serde::Serialize;

use super::{
    NativeVulkanVulkanaliaDeviceProbeTemplate, NativeVulkanVulkanaliaDirectRuntimeContract,
    NativeVulkanVulkanaliaFeatureChainTemplate, NativeVulkanVulkanaliaMigrationContract,
    NativeVulkanVulkanaliaVideoProfileTemplate, NativeVulkanVulkanaliaVideoSessionTemplate,
    native_vulkan_vulkanalia_device_probe_template,
    native_vulkan_vulkanalia_direct_runtime_contract,
    native_vulkan_vulkanalia_feature_chain_template, native_vulkan_vulkanalia_migration_contract,
    native_vulkan_vulkanalia_video_profile_templates,
    native_vulkan_vulkanalia_video_session_template,
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
    pub video_session_template: NativeVulkanVulkanaliaVideoSessionTemplate,
    pub direct_runtime_contract: NativeVulkanVulkanaliaDirectRuntimeContract,
    pub migration_contract: NativeVulkanVulkanaliaMigrationContract,
    pub required_instance_extensions: &'static [&'static str],
    pub required_device_extensions: &'static [&'static str],
    pub preferred_optional_device_extensions: &'static [&'static str],
    pub prioritized_vulkan_1_4_features: &'static [&'static str],
    pub migration_gates: &'static [&'static str],
}

pub fn native_vulkan_vulkanalia_backend_plan() -> NativeVulkanVulkanaliaBackendPlan {
    NativeVulkanVulkanaliaBackendPlan {
        binding: "vulkanalia",
        phase: "primary-migration-backend",
        api_baseline: "Vulkan 1.4 binding surface plus Vulkan Video/Wayland/Swapchain extensions",
        api_type_evidence: vec![
            std::any::type_name::<vulkanalia::Version>(),
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceVulkan14Features>(),
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceVulkan14Properties>(),
            std::any::type_name::<vulkanalia::vk::SurfaceKHR>(),
            std::any::type_name::<vulkanalia::vk::SwapchainCreateInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::PresentIdKHR>(),
            std::any::type_name::<vulkanalia::vk::PresentWait2InfoKHR>(),
            std::any::type_name::<vulkanalia::vk::RenderingInfo>(),
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceDescriptorHeapFeaturesEXT>(),
            std::any::type_name::<vulkanalia::vk::BindHeapInfoEXT>(),
            std::any::type_name::<vulkanalia::vk::SamplerYcbcrConversionCreateInfo>(),
            std::any::type_name::<vulkanalia::vk::VideoBeginCodingInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeH264PictureInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeH265PictureInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeAV1PictureInfoKHR>(),
        ],
        feature_chain_template: native_vulkan_vulkanalia_feature_chain_template(),
        device_probe_template: native_vulkan_vulkanalia_device_probe_template(),
        video_profile_templates: native_vulkan_vulkanalia_video_profile_templates(),
        video_session_template: native_vulkan_vulkanalia_video_session_template(),
        direct_runtime_contract: native_vulkan_vulkanalia_direct_runtime_contract(),
        migration_contract: native_vulkan_vulkanalia_migration_contract(),
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
        preferred_optional_device_extensions: &[
            "VK_KHR_video_maintenance1",
            "VK_KHR_video_maintenance2",
            "VK_EXT_descriptor_heap",
            "VK_KHR_present_id",
            "VK_KHR_present_wait",
            "VK_KHR_present_id2",
            "VK_KHR_present_wait2",
            "VK_KHR_swapchain_maintenance1",
        ],
        prioritized_vulkan_1_4_features: &[
            "dynamic-rendering",
            "dynamic-rendering-local-read",
            "sampler-ycbcr-conversion",
            "descriptor-heap",
            "push-descriptor",
            "maintenance5",
            "maintenance6",
            "fifo-latest-ready-present-mode",
            "present-id2",
            "present-wait2",
            "scalar-block-layout",
            "synchronization2",
            "larger-portable-limits",
        ],
        migration_gates: &[
            "make Vulkanalia visible through ordinary native-vulkan-renderer builds",
            "create Vulkan 1.4 instance/device and report PhysicalDeviceVulkan14Features",
            "probe Wayland surface and swapchain through the Vulkanalia backend",
            "probe Vulkan Video H.264/H.265/AV1 profile and format parity",
            "route H.264/H.265/AV1 direct-video submit through Vulkanalia-owned session/image/bitstream/command resources",
            "keep direct present timing telemetry on the Vulkanalia main path",
            "keep all native Vulkan renderer smoke/probe paths passing without dep:ash",
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
        assert_eq!(plan.phase, "primary-migration-backend");
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
            plan.preferred_optional_device_extensions
                .contains(&"VK_KHR_video_maintenance2")
        );
        assert!(
            plan.preferred_optional_device_extensions
                .contains(&"VK_KHR_present_wait2")
        );
        assert!(
            plan.preferred_optional_device_extensions
                .contains(&"VK_EXT_descriptor_heap")
        );
        assert!(
            plan.api_type_evidence
                .iter()
                .any(|name| { name.ends_with("PhysicalDeviceVulkan14Features") })
        );
        assert!(
            plan.api_type_evidence
                .iter()
                .any(|name| { name.ends_with("SwapchainCreateInfoKHR") })
        );
        assert!(
            plan.api_type_evidence
                .iter()
                .any(|name| { name.ends_with("RenderingInfo") })
        );
        assert!(
            plan.api_type_evidence
                .iter()
                .any(|name| { name.ends_with("PhysicalDeviceDescriptorHeapFeaturesEXT") })
        );
        assert!(
            plan.api_type_evidence
                .iter()
                .any(|name| { name.ends_with("BindHeapInfoEXT") })
        );
        assert!(
            plan.api_type_evidence
                .iter()
                .any(|name| { name.ends_with("SamplerYcbcrConversionCreateInfo") })
        );
        assert!(
            plan.api_type_evidence
                .iter()
                .any(|name| { name.ends_with("VideoDecodeAV1PictureInfoKHR") })
        );
        assert_eq!(plan.direct_runtime_contract.binding, "vulkanalia");
        assert_eq!(plan.direct_runtime_contract.route_name, "direct-video");
        assert!(
            plan.direct_runtime_contract
                .required_submit_order
                .contains(&"queue_submit2")
        );
        assert_eq!(plan.feature_chain_template.api, "Vulkan 1.4");
        assert_eq!(plan.device_probe_template.requested_api_version, "1.4.0");
        assert_eq!(plan.video_profile_templates.len(), 7);
        assert!(
            plan.video_session_template
                .api_type_evidence
                .iter()
                .any(|name| name.ends_with("VideoSessionCreateInfoKHR"))
        );
        assert_eq!(plan.migration_contract.primary_binding, "vulkanalia");
        assert_eq!(plan.migration_contract.retired_binding, "ash");
        assert!(
            plan.migration_gates
                .iter()
                .any(|gate| gate.contains("ordinary native-vulkan-renderer"))
        );
    }
}

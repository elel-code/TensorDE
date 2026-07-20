use serde::Serialize;

use super::device_probe::{
    NativeVulkanVulkanaliaDeviceProbeTemplate, native_vulkan_vulkanalia_device_probe_template,
};
use super::features::{
    NativeVulkanVulkanaliaFeatureChainTemplate, native_vulkan_vulkanalia_feature_chain_template,
};
use super::profiles::{
    NativeVulkanVulkanaliaVideoProfileTemplate, native_vulkan_vulkanalia_video_profile_templates,
};
use super::roadmap_2026::{
    GILDER_ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS, ROADMAP_2026_API_VERSION,
    ROADMAP_2026_PROFILE_NAME, ROADMAP_2026_PROFILE_REVISION,
    ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanBackendPlan {
    pub binding: &'static str,
    pub phase: &'static str,
    pub api_baseline: String,
    pub profile_name: &'static str,
    pub profile_revision: u32,
    pub required_api_version: String,
    pub api_type_evidence: Vec<&'static str>,
    pub feature_chain_template: NativeVulkanVulkanaliaFeatureChainTemplate,
    pub device_probe_template: NativeVulkanVulkanaliaDeviceProbeTemplate,
    pub video_profile_templates: Vec<NativeVulkanVulkanaliaVideoProfileTemplate>,
    pub required_instance_extensions: &'static [&'static str],
    pub required_profile_device_extensions: &'static [&'static str],
    pub required_scene_device_extensions: &'static [&'static str],
    pub required_video_route_device_extensions: &'static [&'static str],
    pub video_acceleration_extensions: &'static [&'static str],
    pub required_profile_feature_structs: &'static [&'static str],
    pub runtime_gates: &'static [&'static str],
}

pub fn native_vulkan_backend_plan() -> NativeVulkanBackendPlan {
    NativeVulkanBackendPlan {
        binding: "vulkanalia",
        phase: "single-vulkan-backend",
        api_baseline: format!(
            "Vulkan {ROADMAP_2026_API_VERSION} + {ROADMAP_2026_PROFILE_NAME} revision {ROADMAP_2026_PROFILE_REVISION}"
        ),
        profile_name: ROADMAP_2026_PROFILE_NAME,
        profile_revision: ROADMAP_2026_PROFILE_REVISION,
        required_api_version: ROADMAP_2026_API_VERSION.to_string(),
        api_type_evidence: vec![
            std::any::type_name::<vulkanalia::Version>(),
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceVulkan14Features>(),
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceVulkan14Properties>(),
            std::any::type_name::<vulkanalia::vk::SurfaceKHR>(),
            std::any::type_name::<vulkanalia::vk::SwapchainCreateInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::PresentId2KHR>(),
            std::any::type_name::<vulkanalia::vk::PresentWait2InfoKHR>(),
            std::any::type_name::<vulkanalia::vk::RenderingInfo>(),
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceDescriptorHeapFeaturesEXT>(),
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceShaderQuadControlFeaturesKHR>(),
            std::any::type_name::<
                vulkanalia::vk::PhysicalDeviceShaderMaximalReconvergenceFeaturesKHR,
            >(),
            std::any::type_name::<
                vulkanalia::vk::PhysicalDeviceShaderSubgroupUniformControlFlowFeaturesKHR,
            >(),
            std::any::type_name::<vulkanalia::vk::PhysicalDevicePipelineBinaryFeaturesKHR>(),
            std::any::type_name::<vulkanalia::vk::PhysicalDeviceRobustness2FeaturesKHR>(),
            std::any::type_name::<
                vulkanalia::vk::PhysicalDeviceWorkgroupMemoryExplicitLayoutFeaturesKHR,
            >(),
            std::any::type_name::<
                vulkanalia::vk::PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR,
            >(),
            std::any::type_name::<vulkanalia::vk::PhysicalDevicePresentId2FeaturesKHR>(),
            std::any::type_name::<vulkanalia::vk::PhysicalDevicePresentWait2FeaturesKHR>(),
            std::any::type_name::<
                vulkanalia::vk::PhysicalDeviceSwapchainMaintenance1FeaturesKHR,
            >(),
            std::any::type_name::<vulkanalia::vk::BindHeapInfoEXT>(),
            std::any::type_name::<vulkanalia::vk::VideoBeginCodingInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeH264PictureInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeH265PictureInfoKHR>(),
            std::any::type_name::<vulkanalia::vk::VideoDecodeAV1PictureInfoKHR>(),
        ],
        feature_chain_template: native_vulkan_vulkanalia_feature_chain_template(),
        device_probe_template: native_vulkan_vulkanalia_device_probe_template(),
        video_profile_templates: native_vulkan_vulkanalia_video_profile_templates(),
        required_instance_extensions: GILDER_ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS,
        required_profile_device_extensions: ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS,
        required_scene_device_extensions: &["VK_EXT_descriptor_heap"],
        required_video_route_device_extensions: &[
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
        video_acceleration_extensions: &[
            "VK_KHR_video_maintenance1",
            "VK_KHR_video_maintenance2",
        ],
        required_profile_feature_structs: &[
            "PhysicalDeviceVulkan11Features",
            "PhysicalDeviceVulkan12Features",
            "PhysicalDeviceVulkan13Features",
            "PhysicalDeviceVulkan14Features",
            "PhysicalDeviceShaderQuadControlFeaturesKHR",
            "PhysicalDeviceShaderMaximalReconvergenceFeaturesKHR",
            "PhysicalDeviceShaderSubgroupUniformControlFlowFeaturesKHR",
            "PhysicalDeviceRobustness2FeaturesKHR",
            "PhysicalDevicePipelineBinaryFeaturesKHR",
            "PhysicalDeviceFragmentShadingRateFeaturesKHR",
            "PhysicalDeviceShaderClockFeaturesKHR",
            "PhysicalDeviceWorkgroupMemoryExplicitLayoutFeaturesKHR",
            "PhysicalDeviceComputeShaderDerivativesFeaturesKHR",
            "PhysicalDeviceMaintenance7FeaturesKHR",
            "PhysicalDeviceMaintenance8FeaturesKHR",
            "PhysicalDeviceMaintenance9FeaturesKHR",
            "PhysicalDeviceDepthClampZeroOneFeaturesKHR",
            "PhysicalDeviceCopyMemoryIndirectFeaturesKHR",
            "PhysicalDeviceShaderUntypedPointersFeaturesKHR",
            "PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR",
            "PhysicalDevicePresentId2FeaturesKHR",
            "PhysicalDevicePresentWait2FeaturesKHR",
            "PhysicalDeviceSwapchainMaintenance1FeaturesKHR",
            "PhysicalDeviceCooperativeMatrixFeaturesKHR",
        ],
        runtime_gates: &[
            "reject loaders below Vulkan 1.4.328 before vkCreateInstance",
            "reject every physical device that does not satisfy the complete VP_KHR_roadmap_2026 revision 11 contract",
            "reject an explicitly selected non-conforming physical device without selecting another device",
            "require Wayland surface capabilities2 and surface maintenance1 instance extensions",
            "require VK_EXT_descriptor_heap as the sole scene and decoded-image binding model",
            "require FIFO_LATEST_READY present mode; no FIFO, mailbox, or immediate substitution",
            "probe Vulkan Video H.264/H.265/AV1 profile and format parity on the selected conforming device",
            "report descriptor_heap_only=true in runtime evidence",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_plan_is_the_exact_roadmap_2026_contract() {
        let plan = native_vulkan_backend_plan();
        assert_eq!(plan.binding, "vulkanalia");
        assert_eq!(plan.phase, "single-vulkan-backend");
        assert_eq!(plan.profile_name, "VP_KHR_roadmap_2026");
        assert_eq!(plan.profile_revision, 11);
        assert_eq!(plan.required_api_version, "1.4.328");
        assert_eq!(
            plan.api_baseline,
            "Vulkan 1.4.328 + VP_KHR_roadmap_2026 revision 11"
        );
        assert!(
            plan.required_instance_extensions
                .contains(&"VK_KHR_surface_maintenance1")
        );
        assert!(
            plan.required_profile_device_extensions
                .contains(&"VK_KHR_shader_quad_control")
        );
        assert!(
            plan.required_profile_device_extensions
                .contains(&"VK_KHR_present_wait2")
        );
        assert!(
            !plan
                .required_profile_device_extensions
                .contains(&"VK_KHR_maintenance10")
        );
        assert_eq!(
            plan.required_scene_device_extensions,
            &["VK_EXT_descriptor_heap"]
        );
        assert!(
            plan.required_video_route_device_extensions
                .contains(&"VK_KHR_video_decode_h265")
        );
        assert!(
            plan.required_profile_feature_structs
                .contains(&"PhysicalDeviceRobustness2FeaturesKHR")
        );
        assert!(
            plan.required_profile_feature_structs
                .contains(&"PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR")
        );
        assert_eq!(plan.feature_chain_template.api, "Vulkan 1.4.328");
        assert_eq!(plan.device_probe_template.requested_api_version, "1.4.328");
        assert_eq!(plan.video_profile_templates.len(), 7);
        assert!(
            plan.runtime_gates
                .iter()
                .any(|gate| gate.contains("without selecting another device"))
        );
    }
}

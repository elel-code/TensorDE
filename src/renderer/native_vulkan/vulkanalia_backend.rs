//! Early vulkanalia backend spike boundary.
//!
//! This module is intentionally small: it makes vulkanalia a compile-checked
//! backend candidate before the ash runtime is replaced in-place.

use serde::Serialize;
use vulkanalia::vk::{self, HasBuilder};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaBackendPlan {
    pub binding: &'static str,
    pub phase: &'static str,
    pub api_baseline: &'static str,
    pub api_type_evidence: Vec<&'static str>,
    pub feature_chain_template: NativeVulkanVulkanaliaFeatureChainTemplate,
    pub video_profile_templates: Vec<NativeVulkanVulkanaliaVideoProfileTemplate>,
    pub required_instance_extensions: &'static [&'static str],
    pub required_device_extensions: &'static [&'static str],
    pub prioritized_vulkan_1_4_features: &'static [&'static str],
    pub migration_gates: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaFeatureChainTemplate {
    pub api: &'static str,
    pub chain_root: &'static str,
    pub feature_struct: &'static str,
    pub requested_feature_fields: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoProfileTemplate {
    pub codec: &'static str,
    pub profile: &'static str,
    pub operation_bits: u32,
    pub chroma_bits: u32,
    pub luma_bit_depth_bits: u32,
    pub chroma_bit_depth_bits: u32,
    pub usage_bits: u32,
    pub profile_struct: &'static str,
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

pub fn native_vulkan_vulkanalia_feature_chain_template()
-> NativeVulkanVulkanaliaFeatureChainTemplate {
    let mut vulkan14_features = vk::PhysicalDeviceVulkan14Features::builder()
        .dynamic_rendering_local_read(true)
        .maintenance5(true)
        .maintenance6(true)
        .push_descriptor(true)
        .build();
    let features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut vulkan14_features)
        .build();

    NativeVulkanVulkanaliaFeatureChainTemplate {
        api: "Vulkan 1.4",
        chain_root: std::any::type_name_of_val(&features2),
        feature_struct: std::any::type_name_of_val(&vulkan14_features),
        requested_feature_fields: &[
            "dynamic_rendering_local_read",
            "maintenance5",
            "maintenance6",
            "push_descriptor",
        ],
    }
}

pub fn native_vulkan_vulkanalia_video_profile_templates()
-> Vec<NativeVulkanVulkanaliaVideoProfileTemplate> {
    vec![
        h264_profile_template("baseline", vk::video::STD_VIDEO_H264_PROFILE_IDC_BASELINE),
        h264_profile_template("main", vk::video::STD_VIDEO_H264_PROFILE_IDC_MAIN),
        h264_profile_template("high", vk::video::STD_VIDEO_H264_PROFILE_IDC_HIGH),
        h265_profile_template(
            "main-8",
            vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN,
            vk::VideoComponentBitDepthFlagsKHR::_8,
        ),
        h265_profile_template(
            "main-10",
            vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN_10,
            vk::VideoComponentBitDepthFlagsKHR::_10,
        ),
        av1_profile_template("main-8", vk::VideoComponentBitDepthFlagsKHR::_8),
        av1_profile_template("main-10", vk::VideoComponentBitDepthFlagsKHR::_10),
    ]
}

fn h264_profile_template(
    profile: &'static str,
    std_profile_idc: vk::video::StdVideoH264ProfileIdc,
) -> NativeVulkanVulkanaliaVideoProfileTemplate {
    let chroma_subsampling = vk::VideoChromaSubsamplingFlagsKHR::_420;
    let bit_depth = vk::VideoComponentBitDepthFlagsKHR::_8;
    let usage_info = vk::VideoDecodeUsageInfoKHR::builder()
        .video_usage_hints(vk::VideoDecodeUsageFlagsKHR::DEFAULT)
        .build();
    let mut h264_profile_info = vk::VideoDecodeH264ProfileInfoKHR::builder()
        .std_profile_idc(std_profile_idc)
        .picture_layout(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE)
        .build();
    let profile_info = vk::VideoProfileInfoKHR::builder()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
        .chroma_subsampling(chroma_subsampling)
        .luma_bit_depth(bit_depth)
        .chroma_bit_depth(bit_depth)
        .push_next(&mut h264_profile_info)
        .build();

    NativeVulkanVulkanaliaVideoProfileTemplate {
        codec: "h264",
        profile,
        operation_bits: profile_info.video_codec_operation.bits(),
        chroma_bits: profile_info.chroma_subsampling.bits(),
        luma_bit_depth_bits: profile_info.luma_bit_depth.bits(),
        chroma_bit_depth_bits: profile_info.chroma_bit_depth.bits(),
        usage_bits: usage_info.video_usage_hints.bits(),
        profile_struct: std::any::type_name_of_val(&h264_profile_info),
    }
}

fn h265_profile_template(
    profile: &'static str,
    std_profile_idc: vk::video::StdVideoH265ProfileIdc,
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
) -> NativeVulkanVulkanaliaVideoProfileTemplate {
    let chroma_subsampling = vk::VideoChromaSubsamplingFlagsKHR::_420;
    let usage_info = vk::VideoDecodeUsageInfoKHR::builder()
        .video_usage_hints(vk::VideoDecodeUsageFlagsKHR::DEFAULT)
        .build();
    let mut h265_profile_info = vk::VideoDecodeH265ProfileInfoKHR::builder()
        .std_profile_idc(std_profile_idc)
        .build();
    let profile_info = vk::VideoProfileInfoKHR::builder()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H265)
        .chroma_subsampling(chroma_subsampling)
        .luma_bit_depth(bit_depth)
        .chroma_bit_depth(bit_depth)
        .push_next(&mut h265_profile_info)
        .build();

    NativeVulkanVulkanaliaVideoProfileTemplate {
        codec: "h265",
        profile,
        operation_bits: profile_info.video_codec_operation.bits(),
        chroma_bits: profile_info.chroma_subsampling.bits(),
        luma_bit_depth_bits: profile_info.luma_bit_depth.bits(),
        chroma_bit_depth_bits: profile_info.chroma_bit_depth.bits(),
        usage_bits: usage_info.video_usage_hints.bits(),
        profile_struct: std::any::type_name_of_val(&h265_profile_info),
    }
}

fn av1_profile_template(
    profile: &'static str,
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
) -> NativeVulkanVulkanaliaVideoProfileTemplate {
    let chroma_subsampling = vk::VideoChromaSubsamplingFlagsKHR::_420;
    let usage_info = vk::VideoDecodeUsageInfoKHR::builder()
        .video_usage_hints(vk::VideoDecodeUsageFlagsKHR::DEFAULT)
        .build();
    let mut av1_profile_info = vk::VideoDecodeAV1ProfileInfoKHR::builder()
        .std_profile(vk::video::STD_VIDEO_AV1_PROFILE_MAIN)
        .film_grain_support(true)
        .build();
    let profile_info = vk::VideoProfileInfoKHR::builder()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_AV1)
        .chroma_subsampling(chroma_subsampling)
        .luma_bit_depth(bit_depth)
        .chroma_bit_depth(bit_depth)
        .push_next(&mut av1_profile_info)
        .build();

    NativeVulkanVulkanaliaVideoProfileTemplate {
        codec: "av1",
        profile,
        operation_bits: profile_info.video_codec_operation.bits(),
        chroma_bits: profile_info.chroma_subsampling.bits(),
        luma_bit_depth_bits: profile_info.luma_bit_depth.bits(),
        chroma_bit_depth_bits: profile_info.chroma_bit_depth.bits(),
        usage_bits: usage_info.video_usage_hints.bits(),
        profile_struct: std::any::type_name_of_val(&av1_profile_info),
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
        assert_eq!(plan.video_profile_templates.len(), 7);
    }

    #[test]
    fn feature_chain_template_uses_vulkan_1_4_feature_struct() {
        let template = native_vulkan_vulkanalia_feature_chain_template();
        assert_eq!(template.api, "Vulkan 1.4");
        assert!(template.chain_root.ends_with("PhysicalDeviceFeatures2"));
        assert!(
            template
                .feature_struct
                .ends_with("PhysicalDeviceVulkan14Features")
        );
        assert!(
            template
                .requested_feature_fields
                .contains(&"dynamic_rendering_local_read")
        );
        assert!(template.requested_feature_fields.contains(&"maintenance6"));
    }

    #[test]
    fn video_profile_templates_cover_current_direct_codecs() {
        let templates = native_vulkan_vulkanalia_video_profile_templates();
        assert_eq!(templates.len(), 7);
        assert!(templates.iter().any(|template| {
            template.codec == "h264"
                && template.profile == "high"
                && template
                    .profile_struct
                    .ends_with("VideoDecodeH264ProfileInfoKHR")
        }));
        assert!(templates.iter().any(|template| {
            template.codec == "h265"
                && template.profile == "main-10"
                && template.luma_bit_depth_bits == vk::VideoComponentBitDepthFlagsKHR::_10.bits()
        }));
        assert!(templates.iter().any(|template| {
            template.codec == "av1"
                && template.profile == "main-10"
                && template
                    .profile_struct
                    .ends_with("VideoDecodeAV1ProfileInfoKHR")
        }));
    }
}

use serde::Serialize;
use vulkanalia::vk::{self, HasBuilder};

use super::video_profile_info::{
    with_vulkanalia_av1_video_profile_info, with_vulkanalia_h264_video_profile_info,
    with_vulkanalia_h265_video_profile_info,
};

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
    let usage_info = vk::VideoDecodeUsageInfoKHR::builder()
        .video_usage_hints(vk::VideoDecodeUsageFlagsKHR::DEFAULT)
        .build();
    with_vulkanalia_h264_video_profile_info(
        std_profile_idc,
        vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE,
        |profile_info, h264_profile_info| NativeVulkanVulkanaliaVideoProfileTemplate {
            codec: "h264",
            profile,
            operation_bits: profile_info.video_codec_operation.bits(),
            chroma_bits: profile_info.chroma_subsampling.bits(),
            luma_bit_depth_bits: profile_info.luma_bit_depth.bits(),
            chroma_bit_depth_bits: profile_info.chroma_bit_depth.bits(),
            usage_bits: usage_info.video_usage_hints.bits(),
            profile_struct: std::any::type_name_of_val(h264_profile_info),
        },
    )
}

fn h265_profile_template(
    profile: &'static str,
    std_profile_idc: vk::video::StdVideoH265ProfileIdc,
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
) -> NativeVulkanVulkanaliaVideoProfileTemplate {
    let usage_info = vk::VideoDecodeUsageInfoKHR::builder()
        .video_usage_hints(vk::VideoDecodeUsageFlagsKHR::DEFAULT)
        .build();
    with_vulkanalia_h265_video_profile_info(
        std_profile_idc,
        bit_depth,
        |profile_info, h265_profile_info| NativeVulkanVulkanaliaVideoProfileTemplate {
            codec: "h265",
            profile,
            operation_bits: profile_info.video_codec_operation.bits(),
            chroma_bits: profile_info.chroma_subsampling.bits(),
            luma_bit_depth_bits: profile_info.luma_bit_depth.bits(),
            chroma_bit_depth_bits: profile_info.chroma_bit_depth.bits(),
            usage_bits: usage_info.video_usage_hints.bits(),
            profile_struct: std::any::type_name_of_val(h265_profile_info),
        },
    )
}

fn av1_profile_template(
    profile: &'static str,
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
) -> NativeVulkanVulkanaliaVideoProfileTemplate {
    let usage_info = vk::VideoDecodeUsageInfoKHR::builder()
        .video_usage_hints(vk::VideoDecodeUsageFlagsKHR::DEFAULT)
        .build();
    with_vulkanalia_av1_video_profile_info(bit_depth, true, |profile_info, av1_profile_info| {
        NativeVulkanVulkanaliaVideoProfileTemplate {
            codec: "av1",
            profile,
            operation_bits: profile_info.video_codec_operation.bits(),
            chroma_bits: profile_info.chroma_subsampling.bits(),
            luma_bit_depth_bits: profile_info.luma_bit_depth.bits(),
            chroma_bit_depth_bits: profile_info.chroma_bit_depth.bits(),
            usage_bits: usage_info.video_usage_hints.bits(),
            profile_struct: std::any::type_name_of_val(av1_profile_info),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

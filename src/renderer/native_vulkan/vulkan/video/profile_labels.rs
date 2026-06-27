use vulkanalia::vk;

pub(in crate::renderer::native_vulkan::vulkan) fn video_chroma_subsampling_labels(
    flags: vk::VideoChromaSubsamplingFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::MONOCHROME) {
        labels.push("monochrome");
    }
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::_420) {
        labels.push("420");
    }
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::_422) {
        labels.push("422");
    }
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::_444) {
        labels.push("444");
    }
    labels
}

pub(in crate::renderer::native_vulkan::vulkan) fn video_component_bit_depth_labels(
    flags: vk::VideoComponentBitDepthFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoComponentBitDepthFlagsKHR::_8) {
        labels.push("8-bit");
    }
    if flags.contains(vk::VideoComponentBitDepthFlagsKHR::_10) {
        labels.push("10-bit");
    }
    if flags.contains(vk::VideoComponentBitDepthFlagsKHR::_12) {
        labels.push("12-bit");
    }
    labels
}

pub(in crate::renderer::native_vulkan::vulkan) fn video_capability_flag_labels(
    flags: vk::VideoCapabilityFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoCapabilityFlagsKHR::PROTECTED_CONTENT) {
        labels.push("protected-content");
    }
    if flags.contains(vk::VideoCapabilityFlagsKHR::SEPARATE_REFERENCE_IMAGES) {
        labels.push("separate-reference-images");
    }
    labels
}

pub(in crate::renderer::native_vulkan::vulkan) fn video_decode_capability_flag_labels(
    flags: vk::VideoDecodeCapabilityFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE) {
        labels.push("dpb-and-output-coincide");
    }
    if flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_DISTINCT) {
        labels.push("dpb-and-output-distinct");
    }
    labels
}

pub(in crate::renderer::native_vulkan::vulkan) fn h264_picture_layout_label(
    layout: vk::VideoDecodeH264PictureLayoutFlagsKHR,
) -> &'static str {
    if layout.contains(vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES) {
        "interlaced-interleaved-lines"
    } else if layout.contains(vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES)
    {
        "interlaced-separate-planes"
    } else if layout.bits() == vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE.bits() {
        "progressive"
    } else {
        "unknown"
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn h264_level_label(
    level: vk::video::StdVideoH264LevelIdc,
) -> Option<&'static str> {
    [
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_1_0, "1.0"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_1_1, "1.1"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_1_2, "1.2"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_1_3, "1.3"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_2_0, "2.0"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_2_1, "2.1"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_2_2, "2.2"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_3_0, "3.0"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_3_1, "3.1"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_3_2, "3.2"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_4_0, "4.0"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_4_1, "4.1"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_4_2, "4.2"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_5_0, "5.0"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_5_1, "5.1"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_5_2, "5.2"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_6_0, "6.0"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_6_1, "6.1"),
        (vk::video::STD_VIDEO_H264_LEVEL_IDC_6_2, "6.2"),
    ]
    .into_iter()
    .find_map(|(value, label)| (value == level).then_some(label))
}

pub(in crate::renderer::native_vulkan::vulkan) fn h265_level_label(
    level: vk::video::StdVideoH265LevelIdc,
) -> Option<&'static str> {
    [
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_1_0, "1.0"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_2_0, "2.0"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_2_1, "2.1"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_3_0, "3.0"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_3_1, "3.1"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_4_0, "4.0"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_4_1, "4.1"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_5_0, "5.0"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_5_1, "5.1"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_5_2, "5.2"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_6_0, "6.0"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_6_1, "6.1"),
        (vk::video::STD_VIDEO_H265_LEVEL_IDC_6_2, "6.2"),
    ]
    .into_iter()
    .find_map(|(value, label)| (value == level).then_some(label))
}

pub(in crate::renderer::native_vulkan::vulkan) fn av1_level_label(
    level: vk::video::StdVideoAV1Level,
) -> Option<&'static str> {
    [
        (vk::video::STD_VIDEO_AV1_LEVEL_2_0, "2.0"),
        (vk::video::STD_VIDEO_AV1_LEVEL_2_1, "2.1"),
        (vk::video::STD_VIDEO_AV1_LEVEL_2_2, "2.2"),
        (vk::video::STD_VIDEO_AV1_LEVEL_2_3, "2.3"),
        (vk::video::STD_VIDEO_AV1_LEVEL_3_0, "3.0"),
        (vk::video::STD_VIDEO_AV1_LEVEL_3_1, "3.1"),
        (vk::video::STD_VIDEO_AV1_LEVEL_3_2, "3.2"),
        (vk::video::STD_VIDEO_AV1_LEVEL_3_3, "3.3"),
        (vk::video::STD_VIDEO_AV1_LEVEL_4_0, "4.0"),
        (vk::video::STD_VIDEO_AV1_LEVEL_4_1, "4.1"),
        (vk::video::STD_VIDEO_AV1_LEVEL_4_2, "4.2"),
        (vk::video::STD_VIDEO_AV1_LEVEL_4_3, "4.3"),
        (vk::video::STD_VIDEO_AV1_LEVEL_5_0, "5.0"),
        (vk::video::STD_VIDEO_AV1_LEVEL_5_1, "5.1"),
        (vk::video::STD_VIDEO_AV1_LEVEL_5_2, "5.2"),
        (vk::video::STD_VIDEO_AV1_LEVEL_5_3, "5.3"),
        (vk::video::STD_VIDEO_AV1_LEVEL_6_0, "6.0"),
        (vk::video::STD_VIDEO_AV1_LEVEL_6_1, "6.1"),
        (vk::video::STD_VIDEO_AV1_LEVEL_6_2, "6.2"),
        (vk::video::STD_VIDEO_AV1_LEVEL_6_3, "6.3"),
        (vk::video::STD_VIDEO_AV1_LEVEL_7_0, "7.0"),
        (vk::video::STD_VIDEO_AV1_LEVEL_7_1, "7.1"),
        (vk::video::STD_VIDEO_AV1_LEVEL_7_2, "7.2"),
        (vk::video::STD_VIDEO_AV1_LEVEL_7_3, "7.3"),
    ]
    .into_iter()
    .find_map(|(value, label)| (value == level).then_some(label))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_match_existing_probe_terms() {
        assert_eq!(
            h264_picture_layout_label(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE),
            "progressive"
        );
        assert_eq!(
            h264_level_label(vk::video::STD_VIDEO_H264_LEVEL_IDC_5_2),
            Some("5.2")
        );
        assert_eq!(
            h265_level_label(vk::video::STD_VIDEO_H265_LEVEL_IDC_6_2),
            Some("6.2")
        );
        assert_eq!(
            av1_level_label(vk::video::STD_VIDEO_AV1_LEVEL_6_3),
            Some("6.3")
        );
    }
}

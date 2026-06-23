use ash::vk;

pub(super) fn native_vulkan_h264_profile_has_high_syntax(profile_idc: u8) -> bool {
    matches!(
        profile_idc,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
    )
}

pub(super) fn native_vulkan_h264_profile_idc_label(profile_idc: u8) -> &'static str {
    match profile_idc {
        66 => "baseline",
        77 => "main",
        88 => "extended",
        100 => "high",
        110 => "high-10",
        122 => "high-422",
        244 => "high-444-predictive",
        _ => "unknown",
    }
}

pub(super) fn native_vulkan_h264_profile_is_8bit_420_decode_candidate(profile_idc: u8) -> bool {
    matches!(profile_idc, 66 | 77 | 100)
}

pub(super) fn native_vulkan_h264_std_profile_idc(
    profile_idc: u8,
) -> Option<vk::native::StdVideoH264ProfileIdc> {
    match profile_idc {
        66 => Some(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE),
        77 => Some(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN),
        100 => Some(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_supported_8bit_420_h264_profiles_to_vulkan_std_profiles() {
        assert_eq!(
            native_vulkan_h264_std_profile_idc(66),
            Some(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE)
        );
        assert_eq!(
            native_vulkan_h264_std_profile_idc(77),
            Some(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN)
        );
        assert_eq!(
            native_vulkan_h264_std_profile_idc(100),
            Some(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH)
        );
    }

    #[test]
    fn keeps_non_420_or_non_8bit_profiles_out_of_the_direct_decode_candidate_set() {
        assert!(native_vulkan_h264_profile_is_8bit_420_decode_candidate(66));
        assert!(native_vulkan_h264_profile_is_8bit_420_decode_candidate(77));
        assert!(native_vulkan_h264_profile_is_8bit_420_decode_candidate(100));
        assert!(!native_vulkan_h264_profile_is_8bit_420_decode_candidate(
            110
        ));
        assert!(!native_vulkan_h264_profile_is_8bit_420_decode_candidate(
            244
        ));
    }
}

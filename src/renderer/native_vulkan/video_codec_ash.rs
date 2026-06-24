use std::ffi::CStr;

use ash::vk;

use super::NativeVulkanVideoSessionCodec;

impl NativeVulkanVideoSessionCodec {
    pub(super) fn codec_extension_name(self) -> &'static CStr {
        match self {
            Self::H264High8 => vk::KHR_VIDEO_DECODE_H264_NAME,
            Self::H265Main8 | Self::H265Main10 => vk::KHR_VIDEO_DECODE_H265_NAME,
            Self::Av1Main8 | Self::Av1Main10 => vk::KHR_VIDEO_DECODE_AV1_NAME,
        }
    }

    pub(super) fn codec_operation(self) -> vk::VideoCodecOperationFlagsKHR {
        match self {
            Self::H264High8 => vk::VideoCodecOperationFlagsKHR::DECODE_H264,
            Self::H265Main8 | Self::H265Main10 => vk::VideoCodecOperationFlagsKHR::DECODE_H265,
            Self::Av1Main8 | Self::Av1Main10 => vk::VideoCodecOperationFlagsKHR::DECODE_AV1,
        }
    }

    pub(super) fn bit_depth_flags(self) -> vk::VideoComponentBitDepthFlagsKHR {
        match self {
            Self::H264High8 | Self::H265Main8 | Self::Av1Main8 => {
                vk::VideoComponentBitDepthFlagsKHR::TYPE_8
            }
            Self::H265Main10 | Self::Av1Main10 => vk::VideoComponentBitDepthFlagsKHR::TYPE_10,
        }
    }

    pub(super) fn picture_format(self) -> vk::Format {
        match self {
            Self::H264High8 | Self::H265Main8 | Self::Av1Main8 => {
                vk::Format::G8_B8R8_2PLANE_420_UNORM
            }
            Self::H265Main10 | Self::Av1Main10 => {
                vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
            }
        }
    }

    pub(super) fn h265_std_profile_idc(self) -> Option<vk::native::StdVideoH265ProfileIdc> {
        match self {
            Self::H265Main8 => {
                Some(vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN)
            }
            Self::H265Main10 => {
                Some(vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN_10)
            }
            Self::H264High8 | Self::Av1Main8 | Self::Av1Main10 => None,
        }
    }

    pub(super) fn av1_std_profile(self) -> Option<vk::native::StdVideoAV1Profile> {
        match self {
            Self::Av1Main8 | Self::Av1Main10 => {
                Some(vk::native::StdVideoAV1Profile_STD_VIDEO_AV1_PROFILE_MAIN)
            }
            Self::H264High8 | Self::H265Main8 | Self::H265Main10 => None,
        }
    }

    pub(super) fn h264_std_profile_idc(self) -> Option<vk::native::StdVideoH264ProfileIdc> {
        match self {
            Self::H264High8 => {
                Some(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH)
            }
            Self::H265Main8 | Self::H265Main10 | Self::Av1Main8 | Self::Av1Main10 => None,
        }
    }
}

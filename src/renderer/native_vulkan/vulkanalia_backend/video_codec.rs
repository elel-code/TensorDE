use crate::renderer::native_vulkan::NativeVulkanVideoSessionCodec;
use vulkanalia::vk;

pub(super) fn native_vulkan_vulkanalia_video_session_codec_name(
    codec: NativeVulkanVideoSessionCodec,
) -> &'static str {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => "h264",
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            "h265"
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => "av1",
    }
}

pub(super) fn native_vulkan_vulkanalia_video_session_label(
    codec: NativeVulkanVideoSessionCodec,
) -> &'static str {
    codec.label()
}

pub(super) fn native_vulkan_vulkanalia_video_session_profile_label(
    codec: NativeVulkanVideoSessionCodec,
) -> &'static str {
    codec.profile_label()
}

pub(super) fn native_vulkan_vulkanalia_video_session_format_probe_profile(
    codec: NativeVulkanVideoSessionCodec,
) -> &'static str {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => "high",
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::Av1Main8 => {
            "main-8"
        }
        NativeVulkanVideoSessionCodec::H265Main10 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            "main-10"
        }
    }
}

pub(super) fn native_vulkan_vulkanalia_video_session_bit_depth(
    codec: NativeVulkanVideoSessionCodec,
) -> vk::VideoComponentBitDepthFlagsKHR {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8
        | NativeVulkanVideoSessionCodec::H265Main8
        | NativeVulkanVideoSessionCodec::Av1Main8 => vk::VideoComponentBitDepthFlagsKHR::_8,
        NativeVulkanVideoSessionCodec::H265Main10 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            vk::VideoComponentBitDepthFlagsKHR::_10
        }
    }
}

pub(super) fn native_vulkan_vulkanalia_video_session_picture_format(
    codec: NativeVulkanVideoSessionCodec,
) -> vk::Format {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8
        | NativeVulkanVideoSessionCodec::H265Main8
        | NativeVulkanVideoSessionCodec::Av1Main8 => vk::Format::G8_B8R8_2PLANE_420_UNORM,
        NativeVulkanVideoSessionCodec::H265Main10 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
        }
    }
}

pub(super) fn native_vulkan_vulkanalia_video_session_codec_operation(
    codec: NativeVulkanVideoSessionCodec,
) -> vk::VideoCodecOperationFlagsKHR {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => vk::VideoCodecOperationFlagsKHR::DECODE_H264,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            vk::VideoCodecOperationFlagsKHR::DECODE_H265
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            vk::VideoCodecOperationFlagsKHR::DECODE_AV1
        }
    }
}

pub(super) fn native_vulkan_vulkanalia_video_decode_codec_label(
    codec: NativeVulkanVideoSessionCodec,
) -> &'static str {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => "H.264 high-8",
        NativeVulkanVideoSessionCodec::H265Main8 => "H.265 main-8",
        NativeVulkanVideoSessionCodec::H265Main10 => "H.265 main-10",
        NativeVulkanVideoSessionCodec::Av1Main8 => "AV1 main-8",
        NativeVulkanVideoSessionCodec::Av1Main10 => "AV1 main-10",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_current_codecs_to_vulkanalia_video_profile_primitives() {
        assert_eq!(
            native_vulkan_vulkanalia_video_session_codec_name(
                NativeVulkanVideoSessionCodec::H264High8
            ),
            "h264"
        );
        assert_eq!(
            native_vulkan_vulkanalia_video_session_format_probe_profile(
                NativeVulkanVideoSessionCodec::H264High8
            ),
            "high"
        );
        assert_eq!(
            native_vulkan_vulkanalia_video_session_bit_depth(
                NativeVulkanVideoSessionCodec::H265Main10
            ),
            vk::VideoComponentBitDepthFlagsKHR::_10
        );
        assert_eq!(
            native_vulkan_vulkanalia_video_session_picture_format(
                NativeVulkanVideoSessionCodec::Av1Main8
            ),
            vk::Format::G8_B8R8_2PLANE_420_UNORM
        );
        assert_eq!(
            native_vulkan_vulkanalia_video_session_codec_operation(
                NativeVulkanVideoSessionCodec::Av1Main10
            ),
            vk::VideoCodecOperationFlagsKHR::DECODE_AV1
        );
    }
}

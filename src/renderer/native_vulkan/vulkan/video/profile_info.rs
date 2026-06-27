use vulkanalia::vk::{self, HasBuilder};

pub(in crate::renderer::native_vulkan::vulkan) fn with_vulkanalia_h264_video_profile_info<R>(
    std_profile_idc: vk::video::StdVideoH264ProfileIdc,
    picture_layout: vk::VideoDecodeH264PictureLayoutFlagsKHR,
    callback: impl FnOnce(&vk::VideoProfileInfoKHR, &vk::VideoDecodeH264ProfileInfoKHR) -> R,
) -> R {
    let bit_depth = vk::VideoComponentBitDepthFlagsKHR::_8;
    let mut h264_profile_info = vk::VideoDecodeH264ProfileInfoKHR::builder()
        .std_profile_idc(std_profile_idc)
        .picture_layout(picture_layout)
        .build();
    let profile_info = vk::VideoProfileInfoKHR::builder()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
        .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::_420)
        .luma_bit_depth(bit_depth)
        .chroma_bit_depth(bit_depth)
        .push_next(&mut h264_profile_info)
        .build();
    callback(&profile_info, &h264_profile_info)
}

pub(in crate::renderer::native_vulkan::vulkan) fn with_vulkanalia_h265_video_profile_info<R>(
    std_profile_idc: vk::video::StdVideoH265ProfileIdc,
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
    callback: impl FnOnce(&vk::VideoProfileInfoKHR, &vk::VideoDecodeH265ProfileInfoKHR) -> R,
) -> R {
    let mut h265_profile_info = vk::VideoDecodeH265ProfileInfoKHR::builder()
        .std_profile_idc(std_profile_idc)
        .build();
    let profile_info = vk::VideoProfileInfoKHR::builder()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H265)
        .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::_420)
        .luma_bit_depth(bit_depth)
        .chroma_bit_depth(bit_depth)
        .push_next(&mut h265_profile_info)
        .build();
    callback(&profile_info, &h265_profile_info)
}

pub(in crate::renderer::native_vulkan::vulkan) fn with_vulkanalia_av1_video_profile_info<R>(
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
    film_grain_support: bool,
    callback: impl FnOnce(&vk::VideoProfileInfoKHR, &vk::VideoDecodeAV1ProfileInfoKHR) -> R,
) -> R {
    let mut av1_profile_info = vk::VideoDecodeAV1ProfileInfoKHR::builder()
        .std_profile(vk::video::STD_VIDEO_AV1_PROFILE_MAIN)
        .film_grain_support(film_grain_support)
        .build();
    let profile_info = vk::VideoProfileInfoKHR::builder()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_AV1)
        .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::_420)
        .luma_bit_depth(bit_depth)
        .chroma_bit_depth(bit_depth)
        .push_next(&mut av1_profile_info)
        .build();
    callback(&profile_info, &av1_profile_info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_info_builders_keep_codec_specific_pnext_chains() {
        with_vulkanalia_h264_video_profile_info(
            vk::video::STD_VIDEO_H264_PROFILE_IDC_HIGH,
            vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE,
            |profile_info, h264_profile_info| {
                assert_eq!(
                    profile_info.video_codec_operation,
                    vk::VideoCodecOperationFlagsKHR::DECODE_H264
                );
                assert_eq!(
                    profile_info.luma_bit_depth,
                    vk::VideoComponentBitDepthFlagsKHR::_8
                );
                assert!(profile_info.next == h264_profile_info as *const _ as *const _);
                assert_eq!(
                    h264_profile_info.picture_layout,
                    vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE
                );
            },
        );

        with_vulkanalia_h265_video_profile_info(
            vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN_10,
            vk::VideoComponentBitDepthFlagsKHR::_10,
            |profile_info, h265_profile_info| {
                assert_eq!(
                    profile_info.video_codec_operation,
                    vk::VideoCodecOperationFlagsKHR::DECODE_H265
                );
                assert_eq!(
                    profile_info.luma_bit_depth,
                    vk::VideoComponentBitDepthFlagsKHR::_10
                );
                assert!(profile_info.next == h265_profile_info as *const _ as *const _);
            },
        );

        with_vulkanalia_av1_video_profile_info(
            vk::VideoComponentBitDepthFlagsKHR::_10,
            false,
            |profile_info, av1_profile_info| {
                assert_eq!(
                    profile_info.video_codec_operation,
                    vk::VideoCodecOperationFlagsKHR::DECODE_AV1
                );
                assert_eq!(
                    profile_info.chroma_subsampling,
                    vk::VideoChromaSubsamplingFlagsKHR::_420
                );
                assert!(profile_info.next == av1_profile_info as *const _ as *const _);
                assert_eq!(av1_profile_info.film_grain_support, 0);
            },
        );
    }
}

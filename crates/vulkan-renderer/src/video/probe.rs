use std::collections::BTreeSet;
use std::ptr;

use vulkanalia::vk::{HasBuilder, KhrVideoQueueExtensionInstanceCommands};
use vulkanalia::{Instance, prelude::v1_4::*, vk};

use super::{VideoDecodeCodecs, VideoDecodeOperations};

pub(crate) fn query_video_queue_operations(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    video_queue_extension_available: bool,
) -> Vec<VideoDecodeOperations> {
    let fallback_count =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) }.len();
    if !video_queue_extension_available {
        return vec![VideoDecodeOperations::empty(); fallback_count];
    }

    let commands = vk::InstanceV1_0::commands(instance);
    let mut count = 0;
    unsafe {
        (commands.get_physical_device_queue_family_properties2)(
            physical_device,
            &mut count,
            ptr::null_mut(),
        );
    }
    let mut properties = vec![vk::QueueFamilyProperties2::default(); count as usize];
    let mut video = vec![vk::QueueFamilyVideoPropertiesKHR::default(); count as usize];
    for (properties, video) in properties.iter_mut().zip(&mut video) {
        properties.next = (video as *mut vk::QueueFamilyVideoPropertiesKHR).cast();
    }
    unsafe {
        (commands.get_physical_device_queue_family_properties2)(
            physical_device,
            &mut count,
            properties.as_mut_ptr(),
        );
    }
    video.truncate(count as usize);
    video
        .into_iter()
        .map(|video| operations_from_vk(video.video_codec_operations))
        .collect()
}

fn operations_from_vk(operations: vk::VideoCodecOperationFlagsKHR) -> VideoDecodeOperations {
    let mut supported = VideoDecodeOperations::empty();
    if operations.contains(vk::VideoCodecOperationFlagsKHR::DECODE_H264) {
        supported = supported.union(VideoDecodeOperations::H264);
    }
    if operations.contains(vk::VideoCodecOperationFlagsKHR::DECODE_H265) {
        supported = supported.union(VideoDecodeOperations::H265);
    }
    if operations.contains(vk::VideoCodecOperationFlagsKHR::DECODE_AV1) {
        supported = supported.union(VideoDecodeOperations::AV1);
    }
    supported
}

pub(crate) fn query_supported_decode_profiles(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    extensions: &BTreeSet<String>,
    queue_operations: VideoDecodeOperations,
) -> VideoDecodeCodecs {
    if !extensions.contains("VK_KHR_video_queue")
        || !extensions.contains("VK_KHR_video_decode_queue")
    {
        return VideoDecodeCodecs::empty();
    }
    let mut supported = VideoDecodeCodecs::empty();
    if extensions.contains("VK_KHR_video_decode_h264")
        && queue_operations.contains(VideoDecodeOperations::H264)
        && query_h264_high_8(instance, physical_device)
    {
        supported |= VideoDecodeCodecs::H264_HIGH_8;
    }
    if extensions.contains("VK_KHR_video_decode_h265")
        && queue_operations.contains(VideoDecodeOperations::H265)
    {
        if query_h265(instance, physical_device, false) {
            supported |= VideoDecodeCodecs::H265_MAIN_8;
        }
        if query_h265(instance, physical_device, true) {
            supported |= VideoDecodeCodecs::H265_MAIN_10;
        }
    }
    if extensions.contains("VK_KHR_video_decode_av1")
        && queue_operations.contains(VideoDecodeOperations::AV1)
    {
        if query_av1(instance, physical_device, false) {
            supported |= VideoDecodeCodecs::AV1_MAIN_8;
        }
        if query_av1(instance, physical_device, true) {
            supported |= VideoDecodeCodecs::AV1_MAIN_10;
        }
    }
    supported
}

fn query_h264_high_8(instance: &Instance, physical_device: vk::PhysicalDevice) -> bool {
    let mut codec = vk::VideoDecodeH264ProfileInfoKHR::builder()
        .std_profile_idc(vk::video::STD_VIDEO_H264_PROFILE_IDC_HIGH)
        .picture_layout(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE)
        .build();
    let profile = vk::VideoProfileInfoKHR::builder()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
        .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::_420)
        .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::_8)
        .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::_8)
        .push_next(&mut codec)
        .build();
    let mut codec_capabilities = vk::VideoDecodeH264CapabilitiesKHR::default();
    query_profile(instance, physical_device, &profile, &mut codec_capabilities)
}

fn query_h265(instance: &Instance, physical_device: vk::PhysicalDevice, ten_bit: bool) -> bool {
    let (profile_id, bit_depth) = if ten_bit {
        (
            vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN_10,
            vk::VideoComponentBitDepthFlagsKHR::_10,
        )
    } else {
        (
            vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN,
            vk::VideoComponentBitDepthFlagsKHR::_8,
        )
    };
    let mut codec = vk::VideoDecodeH265ProfileInfoKHR::builder()
        .std_profile_idc(profile_id)
        .build();
    let profile = vk::VideoProfileInfoKHR::builder()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H265)
        .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::_420)
        .luma_bit_depth(bit_depth)
        .chroma_bit_depth(bit_depth)
        .push_next(&mut codec)
        .build();
    let mut codec_capabilities = vk::VideoDecodeH265CapabilitiesKHR::default();
    query_profile(instance, physical_device, &profile, &mut codec_capabilities)
}

fn query_av1(instance: &Instance, physical_device: vk::PhysicalDevice, ten_bit: bool) -> bool {
    let bit_depth = if ten_bit {
        vk::VideoComponentBitDepthFlagsKHR::_10
    } else {
        vk::VideoComponentBitDepthFlagsKHR::_8
    };
    let mut codec = vk::VideoDecodeAV1ProfileInfoKHR::builder()
        .std_profile(vk::video::STD_VIDEO_AV1_PROFILE_MAIN)
        .film_grain_support(false)
        .build();
    let profile = vk::VideoProfileInfoKHR::builder()
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_AV1)
        .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::_420)
        .luma_bit_depth(bit_depth)
        .chroma_bit_depth(bit_depth)
        .push_next(&mut codec)
        .build();
    let mut codec_capabilities = vk::VideoDecodeAV1CapabilitiesKHR::default();
    query_profile(instance, physical_device, &profile, &mut codec_capabilities)
}

fn query_profile<T: vk::Cast>(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    profile: &vk::VideoProfileInfoKHR,
    codec_capabilities: &mut T,
) -> bool
where
    <T as vk::Cast>::Target: vk::ExtendsVideoCapabilitiesKHR,
{
    let mut decode = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::builder()
        .push_next(codec_capabilities)
        .push_next(&mut decode)
        .build();
    unsafe {
        instance.get_physical_device_video_capabilities_khr(
            physical_device,
            profile,
            &mut capabilities,
        )
    }
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_codec_operations_lower_without_encode_bits() {
        let operations = operations_from_vk(
            vk::VideoCodecOperationFlagsKHR::DECODE_H264
                | vk::VideoCodecOperationFlagsKHR::DECODE_AV1
                | vk::VideoCodecOperationFlagsKHR::ENCODE_H265,
        );
        assert!(operations.contains(VideoDecodeOperations::H264));
        assert!(operations.contains(VideoDecodeOperations::AV1));
        assert!(!operations.contains(VideoDecodeOperations::H265));
    }
}

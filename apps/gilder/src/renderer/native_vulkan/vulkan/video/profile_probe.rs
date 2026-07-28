use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder, KhrVideoQueueExtensionInstanceCommands};

use super::video_profile_gate::query_disabled_reason;
use super::video_profile_info::{
    with_vulkanalia_av1_video_profile_info, with_vulkanalia_h264_video_profile_info,
    with_vulkanalia_h265_video_profile_info,
};
use super::video_profile_labels::{
    av1_level_label, h264_level_label, h264_picture_layout_label, h265_level_label,
    video_capability_flag_labels, video_chroma_subsampling_labels,
    video_component_bit_depth_labels, video_decode_capability_flag_labels,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoProfileProbeSnapshot {
    pub h264_profiles: Vec<NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot>,
    pub h265_profiles: Vec<NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot>,
    pub av1_profiles: Vec<NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot {
    pub codec: &'static str,
    pub profile: &'static str,
    pub std_profile_raw: i32,
    pub picture_layout: Option<&'static str>,
    pub chroma_subsampling: Vec<&'static str>,
    pub luma_bit_depth: Vec<&'static str>,
    pub chroma_bit_depth: Vec<&'static str>,
    pub supported: bool,
    pub max_level: Option<&'static str>,
    pub max_level_raw: Option<i32>,
    pub std_header_version_name: Option<String>,
    pub std_header_version_spec_version: Option<u32>,
    pub capability_flags: Vec<&'static str>,
    pub decode_capability_flags: Vec<&'static str>,
    pub min_bitstream_buffer_offset_alignment: Option<u64>,
    pub min_bitstream_buffer_size_alignment: Option<u64>,
    pub picture_access_granularity: Option<(u32, u32)>,
    pub min_coded_extent: Option<(u32, u32)>,
    pub max_coded_extent: Option<(u32, u32)>,
    pub max_dpb_slots: Option<u32>,
    pub max_active_reference_pictures: Option<u32>,
    pub field_offset_granularity: Option<(i32, i32)>,
    pub query_error: Option<String>,
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_video_profile_probe(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    has_video_decode_queue_family: bool,
) -> NativeVulkanVulkanaliaVideoProfileProbeSnapshot {
    NativeVulkanVulkanaliaVideoProfileProbeSnapshot {
        h264_profiles: h264_profiles(
            instance,
            physical_device,
            device_extensions,
            has_video_decode_queue_family,
        ),
        h265_profiles: h265_profiles(
            instance,
            physical_device,
            device_extensions,
            has_video_decode_queue_family,
        ),
        av1_profiles: av1_profiles(
            instance,
            physical_device,
            device_extensions,
            has_video_decode_queue_family,
        ),
    }
}

fn h264_profiles(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    has_video_decode_queue_family: bool,
) -> Vec<NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot> {
    let profiles = [
        ("baseline", vk::video::STD_VIDEO_H264_PROFILE_IDC_BASELINE),
        ("main", vk::video::STD_VIDEO_H264_PROFILE_IDC_MAIN),
        ("high", vk::video::STD_VIDEO_H264_PROFILE_IDC_HIGH),
    ];
    let picture_layouts = [
        vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE,
        vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES,
        vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES,
    ];
    profiles
        .into_iter()
        .flat_map(|(profile, std_profile_idc)| {
            picture_layouts.into_iter().map(move |picture_layout| {
                if let Some(error) = query_disabled_reason(
                    device_extensions,
                    has_video_decode_queue_family,
                    "VK_KHR_video_decode_h264",
                ) {
                    return unsupported_profile(
                        "h264",
                        profile,
                        std_profile_idc.0,
                        Some(h264_picture_layout_label(picture_layout)),
                        vec!["420"],
                        vec!["8-bit"],
                        vec!["8-bit"],
                        error,
                    );
                }
                query_h264_profile(
                    instance,
                    physical_device,
                    profile,
                    std_profile_idc,
                    picture_layout,
                )
            })
        })
        .collect()
}

fn query_h264_profile(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    profile: &'static str,
    std_profile_idc: vk::video::StdVideoH264ProfileIdc,
    picture_layout: vk::VideoDecodeH264PictureLayoutFlagsKHR,
) -> NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot {
    let chroma_subsampling = vk::VideoChromaSubsamplingFlagsKHR::_420;
    let bit_depth = vk::VideoComponentBitDepthFlagsKHR::_8;
    let mut h264_capabilities = vk::VideoDecodeH264CapabilitiesKHR::default();
    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::builder()
        .push_next(&mut h264_capabilities)
        .push_next(&mut decode_capabilities)
        .build();

    with_vulkanalia_h264_video_profile_info(std_profile_idc, picture_layout, |profile_info, _| {
        if let Err(err) = unsafe {
            instance.get_physical_device_video_capabilities_khr(
                physical_device,
                profile_info,
                &mut capabilities,
            )
        } {
            return unsupported_profile(
                "h264",
                profile,
                std_profile_idc.0,
                Some(h264_picture_layout_label(picture_layout)),
                video_chroma_subsampling_labels(chroma_subsampling),
                video_component_bit_depth_labels(bit_depth),
                video_component_bit_depth_labels(bit_depth),
                format!("vkGetPhysicalDeviceVideoCapabilitiesKHR: {err:?}"),
            );
        }

        supported_profile(
            "h264",
            profile,
            std_profile_idc.0,
            Some(h264_picture_layout_label(picture_layout)),
            video_chroma_subsampling_labels(chroma_subsampling),
            video_component_bit_depth_labels(bit_depth),
            video_component_bit_depth_labels(bit_depth),
            h264_level_label(h264_capabilities.max_level_idc),
            Some(h264_capabilities.max_level_idc.0),
            capabilities,
            decode_capabilities.flags,
            Some((
                h264_capabilities.field_offset_granularity.x,
                h264_capabilities.field_offset_granularity.y,
            )),
        )
    })
}

fn h265_profiles(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    has_video_decode_queue_family: bool,
) -> Vec<NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot> {
    [
        (
            "main-8",
            vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN,
            vk::VideoComponentBitDepthFlagsKHR::_8,
        ),
        (
            "main-10",
            vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN_10,
            vk::VideoComponentBitDepthFlagsKHR::_10,
        ),
    ]
    .into_iter()
    .map(|(profile, std_profile_idc, bit_depth)| {
        if let Some(error) = query_disabled_reason(
            device_extensions,
            has_video_decode_queue_family,
            "VK_KHR_video_decode_h265",
        ) {
            return unsupported_profile(
                "h265",
                profile,
                std_profile_idc.0,
                None,
                vec!["420"],
                video_component_bit_depth_labels(bit_depth),
                video_component_bit_depth_labels(bit_depth),
                error,
            );
        }
        query_h265_profile(
            instance,
            physical_device,
            profile,
            std_profile_idc,
            bit_depth,
        )
    })
    .collect()
}

fn query_h265_profile(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    profile: &'static str,
    std_profile_idc: vk::video::StdVideoH265ProfileIdc,
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
) -> NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot {
    let chroma_subsampling = vk::VideoChromaSubsamplingFlagsKHR::_420;
    let mut h265_capabilities = vk::VideoDecodeH265CapabilitiesKHR::default();
    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::builder()
        .push_next(&mut h265_capabilities)
        .push_next(&mut decode_capabilities)
        .build();

    with_vulkanalia_h265_video_profile_info(std_profile_idc, bit_depth, |profile_info, _| {
        if let Err(err) = unsafe {
            instance.get_physical_device_video_capabilities_khr(
                physical_device,
                profile_info,
                &mut capabilities,
            )
        } {
            return unsupported_profile(
                "h265",
                profile,
                std_profile_idc.0,
                None,
                video_chroma_subsampling_labels(chroma_subsampling),
                video_component_bit_depth_labels(bit_depth),
                video_component_bit_depth_labels(bit_depth),
                format!("vkGetPhysicalDeviceVideoCapabilitiesKHR: {err:?}"),
            );
        }

        supported_profile(
            "h265",
            profile,
            std_profile_idc.0,
            None,
            video_chroma_subsampling_labels(chroma_subsampling),
            video_component_bit_depth_labels(bit_depth),
            video_component_bit_depth_labels(bit_depth),
            h265_level_label(h265_capabilities.max_level_idc),
            Some(h265_capabilities.max_level_idc.0),
            capabilities,
            decode_capabilities.flags,
            None,
        )
    })
}

fn av1_profiles(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    has_video_decode_queue_family: bool,
) -> Vec<NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot> {
    [
        (
            "main-8",
            vk::video::STD_VIDEO_AV1_PROFILE_MAIN,
            vk::VideoComponentBitDepthFlagsKHR::_8,
        ),
        (
            "main-10",
            vk::video::STD_VIDEO_AV1_PROFILE_MAIN,
            vk::VideoComponentBitDepthFlagsKHR::_10,
        ),
    ]
    .into_iter()
    .map(|(profile, std_profile, bit_depth)| {
        if let Some(error) = query_disabled_reason(
            device_extensions,
            has_video_decode_queue_family,
            "VK_KHR_video_decode_av1",
        ) {
            return unsupported_profile(
                "av1",
                profile,
                std_profile.0,
                None,
                vec!["420"],
                video_component_bit_depth_labels(bit_depth),
                video_component_bit_depth_labels(bit_depth),
                error,
            );
        }
        query_av1_profile(instance, physical_device, profile, std_profile, bit_depth)
    })
    .collect()
}

fn query_av1_profile(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    profile: &'static str,
    std_profile: vk::video::StdVideoAV1Profile,
    bit_depth: vk::VideoComponentBitDepthFlagsKHR,
) -> NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot {
    let chroma_subsampling = vk::VideoChromaSubsamplingFlagsKHR::_420;
    let mut av1_capabilities = vk::VideoDecodeAV1CapabilitiesKHR::default();
    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::builder()
        .push_next(&mut av1_capabilities)
        .push_next(&mut decode_capabilities)
        .build();

    with_vulkanalia_av1_video_profile_info(bit_depth, false, |profile_info, _| {
        if let Err(err) = unsafe {
            instance.get_physical_device_video_capabilities_khr(
                physical_device,
                profile_info,
                &mut capabilities,
            )
        } {
            return unsupported_profile(
                "av1",
                profile,
                std_profile.0,
                None,
                video_chroma_subsampling_labels(chroma_subsampling),
                video_component_bit_depth_labels(bit_depth),
                video_component_bit_depth_labels(bit_depth),
                format!("vkGetPhysicalDeviceVideoCapabilitiesKHR: {err:?}"),
            );
        }

        supported_profile(
            "av1",
            profile,
            std_profile.0,
            None,
            video_chroma_subsampling_labels(chroma_subsampling),
            video_component_bit_depth_labels(bit_depth),
            video_component_bit_depth_labels(bit_depth),
            av1_level_label(av1_capabilities.max_level),
            Some(av1_capabilities.max_level.0),
            capabilities,
            decode_capabilities.flags,
            None,
        )
    })
}

fn supported_profile(
    codec: &'static str,
    profile: &'static str,
    std_profile_raw: i32,
    picture_layout: Option<&'static str>,
    chroma_subsampling: Vec<&'static str>,
    luma_bit_depth: Vec<&'static str>,
    chroma_bit_depth: Vec<&'static str>,
    max_level: Option<&'static str>,
    max_level_raw: Option<i32>,
    capabilities: vk::VideoCapabilitiesKHR,
    decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
    field_offset_granularity: Option<(i32, i32)>,
) -> NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot {
    NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot {
        codec,
        profile,
        std_profile_raw,
        picture_layout,
        chroma_subsampling,
        luma_bit_depth,
        chroma_bit_depth,
        supported: true,
        max_level,
        max_level_raw,
        std_header_version_name: Some(
            capabilities
                .std_header_version
                .extension_name
                .to_string_lossy()
                .into_owned(),
        ),
        std_header_version_spec_version: Some(capabilities.std_header_version.spec_version),
        capability_flags: video_capability_flag_labels(capabilities.flags),
        decode_capability_flags: video_decode_capability_flag_labels(decode_capability_flags),
        min_bitstream_buffer_offset_alignment: Some(
            capabilities.min_bitstream_buffer_offset_alignment,
        ),
        min_bitstream_buffer_size_alignment: Some(capabilities.min_bitstream_buffer_size_alignment),
        picture_access_granularity: Some((
            capabilities.picture_access_granularity.width,
            capabilities.picture_access_granularity.height,
        )),
        min_coded_extent: Some((
            capabilities.min_coded_extent.width,
            capabilities.min_coded_extent.height,
        )),
        max_coded_extent: Some((
            capabilities.max_coded_extent.width,
            capabilities.max_coded_extent.height,
        )),
        max_dpb_slots: Some(capabilities.max_dpb_slots),
        max_active_reference_pictures: Some(capabilities.max_active_reference_pictures),
        field_offset_granularity,
        query_error: None,
    }
}

fn unsupported_profile(
    codec: &'static str,
    profile: &'static str,
    std_profile_raw: i32,
    picture_layout: Option<&'static str>,
    chroma_subsampling: Vec<&'static str>,
    luma_bit_depth: Vec<&'static str>,
    chroma_bit_depth: Vec<&'static str>,
    query_error: String,
) -> NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot {
    NativeVulkanVulkanaliaVideoProfileCapabilitySnapshot {
        codec,
        profile,
        std_profile_raw,
        picture_layout,
        chroma_subsampling,
        luma_bit_depth,
        chroma_bit_depth,
        supported: false,
        max_level: None,
        max_level_raw: None,
        std_header_version_name: None,
        std_header_version_spec_version: None,
        capability_flags: Vec::new(),
        decode_capability_flags: Vec::new(),
        min_bitstream_buffer_offset_alignment: None,
        min_bitstream_buffer_size_alignment: None,
        picture_access_granularity: None,
        min_coded_extent: None,
        max_coded_extent: None,
        max_dpb_slots: None,
        max_active_reference_pictures: None,
        field_offset_granularity: None,
        query_error: Some(query_error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_profile_preserves_std_header_version() {
        let mut capabilities = vk::VideoCapabilitiesKHR::default();
        capabilities.std_header_version.spec_version = 7;

        let profile = supported_profile(
            "h265",
            "main-10",
            vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN_10.0,
            None,
            vec!["420"],
            vec!["10-bit"],
            vec!["10-bit"],
            Some("6.2"),
            Some(vk::video::STD_VIDEO_H265_LEVEL_IDC_6_2.0),
            capabilities,
            vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE,
            None,
        );

        assert!(profile.supported);
        assert_eq!(profile.std_header_version_spec_version, Some(7));
        assert_eq!(
            profile.decode_capability_flags,
            vec!["dpb-and-output-coincide"]
        );
    }

    #[test]
    fn unsupported_profile_has_no_std_header_version() {
        let profile = unsupported_profile(
            "av1",
            "main-10",
            vk::video::STD_VIDEO_AV1_PROFILE_MAIN.0,
            None,
            vec!["420"],
            vec!["10-bit"],
            vec!["10-bit"],
            "missing required Vulkan Video decode extensions: VK_KHR_video_decode_av1".to_owned(),
        );

        assert!(!profile.supported);
        assert_eq!(profile.std_header_version_name, None);
        assert_eq!(profile.std_header_version_spec_version, None);
    }
}

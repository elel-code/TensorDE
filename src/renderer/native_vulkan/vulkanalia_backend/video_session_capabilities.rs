use crate::renderer::native_vulkan::{
    NativeVulkanAv1SequenceHeaderSnapshot, NativeVulkanH264ParameterSetSnapshot,
    NativeVulkanVideoSessionCodec,
};
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder, KhrVideoQueueExtensionInstanceCommands};

use super::video_codec::{
    native_vulkan_vulkanalia_video_session_bit_depth as vulkanalia_video_session_bit_depth,
    native_vulkan_vulkanalia_video_session_format_probe_profile as vulkanalia_video_session_format_probe_profile,
    native_vulkan_vulkanalia_video_session_picture_format as vulkanalia_video_session_picture_format,
    native_vulkan_vulkanalia_video_session_profile_label as vulkanalia_video_session_profile_label,
};
use super::video_format_probe::NativeVulkanVulkanaliaVideoFormatQuerySnapshot;
use super::video_profile_info::{
    with_vulkanalia_av1_video_profile_info, with_vulkanalia_h264_video_profile_info,
    with_vulkanalia_h265_video_profile_info,
};
use super::video_profile_labels::{av1_level_label, h264_level_label, h265_level_label};
use super::video_session_parameters_av1::{
    native_vulkan_vulkanalia_av1_sequence_header_bit_depth,
    native_vulkan_vulkanalia_av1_sequence_header_profile_label,
};
use super::video_session_parameters_h264::{
    native_vulkan_vulkanalia_h264_std_profile_idc, native_vulkan_vulkanalia_h264_std_profile_label,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct VulkanaliaVideoSessionCapabilityQuery {
    pub(super) capabilities: vk::VideoCapabilitiesKHR,
    pub(super) decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
    pub(super) codec_max_level: Option<&'static str>,
    pub(super) codec_max_level_raw: Option<i32>,
}

pub(super) fn with_native_vulkan_vulkanalia_video_session_capabilities<R>(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    codec: NativeVulkanVideoSessionCodec,
    h264_parameter_sets: Option<&NativeVulkanH264ParameterSetSnapshot>,
    av1_sequence_header: Option<&NativeVulkanAv1SequenceHeaderSnapshot>,
    callback: impl FnOnce(
        &vk::VideoProfileInfoKHR,
        VulkanaliaVideoSessionCapabilityQuery,
    ) -> Result<R, String>,
) -> Result<R, String> {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => {
            let h264_std_profile_idc =
                native_vulkan_vulkanalia_video_session_effective_h264_std_profile_idc(
                    h264_parameter_sets,
                )?;
            with_vulkanalia_h264_video_profile_info(
                h264_std_profile_idc,
                vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE,
                |profile_info, _| {
                    let mut h264_capabilities = vk::VideoDecodeH264CapabilitiesKHR::default();
                    let queried = query_vulkanalia_h264_video_session_capabilities(
                        instance,
                        physical_device,
                        profile_info,
                        &mut h264_capabilities,
                    )?;
                    callback(profile_info, queried)
                },
            )
        }
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            let std_profile_idc = match codec {
                NativeVulkanVideoSessionCodec::H265Main8 => {
                    vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN
                }
                NativeVulkanVideoSessionCodec::H265Main10 => {
                    vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN_10
                }
                _ => unreachable!("matched H.265 codec"),
            };
            let bit_depth = vulkanalia_video_session_bit_depth(codec);
            with_vulkanalia_h265_video_profile_info(
                std_profile_idc,
                bit_depth,
                |profile_info, _| {
                    let mut h265_capabilities = vk::VideoDecodeH265CapabilitiesKHR::default();
                    let queried = query_vulkanalia_h265_video_session_capabilities(
                        instance,
                        physical_device,
                        profile_info,
                        &mut h265_capabilities,
                    )?;
                    callback(profile_info, queried)
                },
            )
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            let bit_depth = native_vulkan_vulkanalia_video_session_effective_bit_depth(
                codec,
                av1_sequence_header,
            );
            with_vulkanalia_av1_video_profile_info(bit_depth, false, |profile_info, _| {
                let mut av1_capabilities = vk::VideoDecodeAV1CapabilitiesKHR::default();
                let queried = query_vulkanalia_av1_video_session_capabilities(
                    instance,
                    physical_device,
                    profile_info,
                    &mut av1_capabilities,
                )?;
                callback(profile_info, queried)
            })
        }
    }
}

pub(super) fn native_vulkan_vulkanalia_video_session_effective_profile_label(
    codec: NativeVulkanVideoSessionCodec,
    h264_parameter_sets: Option<&NativeVulkanH264ParameterSetSnapshot>,
    av1_sequence_header: Option<&NativeVulkanAv1SequenceHeaderSnapshot>,
) -> Result<&'static str, String> {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => {
            if let Some(parameter_sets) = h264_parameter_sets {
                let profile = native_vulkan_vulkanalia_h264_std_profile_label(
                    parameter_sets.sps.profile_idc,
                )?;
                Ok(match profile {
                    "baseline" => "baseline-8",
                    "main" => "main-8",
                    "high" => "high-8",
                    _ => unreachable!("mapper returns a fixed H.264 profile label"),
                })
            } else {
                Ok(vulkanalia_video_session_profile_label(codec))
            }
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            if let Some(sequence_header) = av1_sequence_header {
                Ok(native_vulkan_vulkanalia_av1_sequence_header_profile_label(
                    sequence_header,
                ))
            } else {
                Ok(vulkanalia_video_session_profile_label(codec))
            }
        }
        _ => Ok(vulkanalia_video_session_profile_label(codec)),
    }
}

pub(super) fn native_vulkan_vulkanalia_video_session_effective_format_probe_profile(
    codec: NativeVulkanVideoSessionCodec,
    h264_parameter_sets: Option<&NativeVulkanH264ParameterSetSnapshot>,
    av1_sequence_header: Option<&NativeVulkanAv1SequenceHeaderSnapshot>,
) -> Result<&'static str, String> {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => h264_parameter_sets
            .map(|parameter_sets| {
                native_vulkan_vulkanalia_h264_std_profile_label(parameter_sets.sps.profile_idc)
            })
            .transpose()
            .map(|profile| profile.unwrap_or("high")),
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            Ok(av1_sequence_header
                .map(native_vulkan_vulkanalia_av1_sequence_header_profile_label)
                .unwrap_or_else(|| vulkanalia_video_session_format_probe_profile(codec)))
        }
        _ => Ok(vulkanalia_video_session_format_probe_profile(codec)),
    }
}

pub(super) fn native_vulkan_vulkanalia_video_session_effective_bit_depth(
    codec: NativeVulkanVideoSessionCodec,
    av1_sequence_header: Option<&NativeVulkanAv1SequenceHeaderSnapshot>,
) -> vk::VideoComponentBitDepthFlagsKHR {
    if matches!(
        codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) && let Some(sequence_header) = av1_sequence_header
    {
        return match native_vulkan_vulkanalia_av1_sequence_header_bit_depth(sequence_header) {
            10 => vk::VideoComponentBitDepthFlagsKHR::_10,
            _ => vk::VideoComponentBitDepthFlagsKHR::_8,
        };
    }
    vulkanalia_video_session_bit_depth(codec)
}

pub(super) fn native_vulkan_vulkanalia_video_session_effective_picture_format(
    codec: NativeVulkanVideoSessionCodec,
    av1_sequence_header: Option<&NativeVulkanAv1SequenceHeaderSnapshot>,
) -> vk::Format {
    if matches!(
        codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) && let Some(sequence_header) = av1_sequence_header
    {
        return match native_vulkan_vulkanalia_av1_sequence_header_bit_depth(sequence_header) {
            10 => vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
            _ => vk::Format::G8_B8R8_2PLANE_420_UNORM,
        };
    }
    vulkanalia_video_session_picture_format(codec)
}

pub(super) fn native_vulkan_vulkanalia_video_format_probe_includes_format(
    queries: &[NativeVulkanVulkanaliaVideoFormatQuerySnapshot],
    codec: &'static str,
    profile: &'static str,
    format: &str,
) -> bool {
    queries
        .iter()
        .find(|query| query.codec == codec && query.profile == profile)
        .is_some_and(|query| {
            query
                .formats
                .iter()
                .any(|property| property.format == format)
        })
}

pub(super) fn native_vulkan_vulkanalia_video_session_extent_supported(
    extent: vk::Extent2D,
    capabilities: vk::VideoCapabilitiesKHR,
) -> bool {
    extent.width >= capabilities.min_coded_extent.width
        && extent.height >= capabilities.min_coded_extent.height
        && extent.width <= capabilities.max_coded_extent.width
        && extent.height <= capabilities.max_coded_extent.height
        && vulkanalia_video_session_extent_aligned(
            extent.width,
            capabilities.picture_access_granularity.width,
        )
        && vulkanalia_video_session_extent_aligned(
            extent.height,
            capabilities.picture_access_granularity.height,
        )
}

pub(super) fn native_vulkan_vulkanalia_video_session_max_dpb_slots(
    driver_max_dpb_slots: u32,
) -> u32 {
    if driver_max_dpb_slots == 0 {
        0
    } else {
        driver_max_dpb_slots.min(8).max(1)
    }
}

pub(super) fn native_vulkan_vulkanalia_video_session_max_active_reference_pictures(
    driver_max_active_reference_pictures: u32,
    session_max_dpb_slots: u32,
) -> u32 {
    if driver_max_active_reference_pictures == 0 || session_max_dpb_slots == 0 {
        0
    } else {
        driver_max_active_reference_pictures
            .min(session_max_dpb_slots)
            .min(session_max_dpb_slots.max(1))
    }
}

fn query_vulkanalia_h264_video_session_capabilities(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR,
    h264_capabilities: &mut vk::VideoDecodeH264CapabilitiesKHR,
) -> Result<VulkanaliaVideoSessionCapabilityQuery, String> {
    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::builder()
        .push_next(h264_capabilities)
        .push_next(&mut decode_capabilities)
        .build();
    unsafe {
        instance.get_physical_device_video_capabilities_khr(
            physical_device,
            profile_info,
            &mut capabilities,
        )
    }
    .map_err(|err| format!("vkGetPhysicalDeviceVideoCapabilitiesKHR(h264): {err:?}"))?;
    Ok(VulkanaliaVideoSessionCapabilityQuery {
        capabilities,
        decode_capability_flags: decode_capabilities.flags,
        codec_max_level: h264_level_label(h264_capabilities.max_level_idc),
        codec_max_level_raw: Some(h264_capabilities.max_level_idc.0),
    })
}

fn query_vulkanalia_h265_video_session_capabilities(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR,
    h265_capabilities: &mut vk::VideoDecodeH265CapabilitiesKHR,
) -> Result<VulkanaliaVideoSessionCapabilityQuery, String> {
    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::builder()
        .push_next(h265_capabilities)
        .push_next(&mut decode_capabilities)
        .build();
    unsafe {
        instance.get_physical_device_video_capabilities_khr(
            physical_device,
            profile_info,
            &mut capabilities,
        )
    }
    .map_err(|err| format!("vkGetPhysicalDeviceVideoCapabilitiesKHR(h265): {err:?}"))?;
    Ok(VulkanaliaVideoSessionCapabilityQuery {
        capabilities,
        decode_capability_flags: decode_capabilities.flags,
        codec_max_level: h265_level_label(h265_capabilities.max_level_idc),
        codec_max_level_raw: Some(h265_capabilities.max_level_idc.0),
    })
}

fn query_vulkanalia_av1_video_session_capabilities(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR,
    av1_capabilities: &mut vk::VideoDecodeAV1CapabilitiesKHR,
) -> Result<VulkanaliaVideoSessionCapabilityQuery, String> {
    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::builder()
        .push_next(av1_capabilities)
        .push_next(&mut decode_capabilities)
        .build();
    unsafe {
        instance.get_physical_device_video_capabilities_khr(
            physical_device,
            profile_info,
            &mut capabilities,
        )
    }
    .map_err(|err| format!("vkGetPhysicalDeviceVideoCapabilitiesKHR(av1): {err:?}"))?;
    Ok(VulkanaliaVideoSessionCapabilityQuery {
        capabilities,
        decode_capability_flags: decode_capabilities.flags,
        codec_max_level: av1_level_label(av1_capabilities.max_level),
        codec_max_level_raw: Some(av1_capabilities.max_level.0),
    })
}

fn native_vulkan_vulkanalia_video_session_effective_h264_std_profile_idc(
    h264_parameter_sets: Option<&NativeVulkanH264ParameterSetSnapshot>,
) -> Result<vk::video::StdVideoH264ProfileIdc, String> {
    h264_parameter_sets
        .map(|parameter_sets| {
            native_vulkan_vulkanalia_h264_std_profile_idc(parameter_sets.sps.profile_idc)
        })
        .transpose()
        .map(|profile| profile.unwrap_or(vk::video::STD_VIDEO_H264_PROFILE_IDC_HIGH))
}

fn vulkanalia_video_session_extent_aligned(value: u32, granularity: u32) -> bool {
    granularity == 0 || value.is_multiple_of(granularity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::native_vulkan::{
        NativeVulkanAv1ColorConfigSnapshot, NativeVulkanAv1OperatingPointSnapshot,
    };

    #[test]
    fn session_capabilities_apply_av1_stream_bit_depth_only_to_av1() {
        let av1_main10_header = test_av1_sequence_header(10);
        assert_eq!(
            native_vulkan_vulkanalia_video_session_effective_bit_depth(
                NativeVulkanVideoSessionCodec::Av1Main8,
                Some(&av1_main10_header)
            ),
            vk::VideoComponentBitDepthFlagsKHR::_10
        );
        assert_eq!(
            native_vulkan_vulkanalia_video_session_effective_picture_format(
                NativeVulkanVideoSessionCodec::Av1Main8,
                Some(&av1_main10_header)
            ),
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
        );
        assert_eq!(
            native_vulkan_vulkanalia_video_session_effective_bit_depth(
                NativeVulkanVideoSessionCodec::H265Main8,
                Some(&av1_main10_header)
            ),
            vk::VideoComponentBitDepthFlagsKHR::_8
        );
        assert_eq!(
            native_vulkan_vulkanalia_video_session_effective_picture_format(
                NativeVulkanVideoSessionCodec::H265Main8,
                Some(&av1_main10_header)
            ),
            vk::Format::G8_B8R8_2PLANE_420_UNORM
        );
    }

    #[test]
    fn session_capabilities_extent_check_matches_driver_granularity() {
        let capabilities = vk::VideoCapabilitiesKHR::builder()
            .min_coded_extent(vk::Extent2D {
                width: 64,
                height: 64,
            })
            .max_coded_extent(vk::Extent2D {
                width: 3840,
                height: 2160,
            })
            .picture_access_granularity(vk::Extent2D {
                width: 16,
                height: 16,
            })
            .build();

        assert!(native_vulkan_vulkanalia_video_session_extent_supported(
            vk::Extent2D {
                width: 1920,
                height: 1088,
            },
            capabilities
        ));
        assert!(!native_vulkan_vulkanalia_video_session_extent_supported(
            vk::Extent2D {
                width: 1921,
                height: 1088,
            },
            capabilities
        ));
    }

    fn test_av1_sequence_header(bit_depth: u8) -> NativeVulkanAv1SequenceHeaderSnapshot {
        NativeVulkanAv1SequenceHeaderSnapshot {
            parser: "test",
            seq_profile: 0,
            seq_profile_label: "main",
            still_picture: false,
            reduced_still_picture_header: false,
            timing_info_present_flag: false,
            timing_info: None,
            decoder_model_info_present_flag: false,
            buffer_delay_length_minus_1: 0,
            frame_presentation_time_length_minus_1: 0,
            initial_display_delay_present_flag: false,
            operating_points_cnt_minus_1: 0,
            operating_points: vec![NativeVulkanAv1OperatingPointSnapshot {
                index: 0,
                idc: 0,
                seq_level_idx: 0,
                seq_level_label: None,
                seq_tier: false,
                decoder_model_present_for_this_op: false,
                initial_display_delay_present_for_this_op: false,
                initial_display_delay_minus_1: None,
            }],
            frame_width_bits_minus_1: 15,
            frame_height_bits_minus_1: 15,
            max_frame_width_minus_1: 639,
            max_frame_height_minus_1: 367,
            max_frame_width: 640,
            max_frame_height: 368,
            frame_id_numbers_present_flag: false,
            delta_frame_id_length_minus_2: None,
            additional_frame_id_length_minus_1: None,
            use_128x128_superblock: false,
            enable_filter_intra: true,
            enable_intra_edge_filter: true,
            enable_interintra_compound: true,
            enable_masked_compound: true,
            enable_warped_motion: true,
            enable_dual_filter: true,
            enable_order_hint: true,
            enable_jnt_comp: true,
            enable_ref_frame_mvs: true,
            seq_force_screen_content_tools: 2,
            seq_force_integer_mv: 2,
            order_hint_bits_minus_1: Some(6),
            enable_superres: false,
            enable_cdef: true,
            enable_restoration: true,
            film_grain_params_present: false,
            color_config: NativeVulkanAv1ColorConfigSnapshot {
                high_bitdepth: bit_depth > 8,
                twelve_bit: bit_depth == 12,
                mono_chrome: false,
                color_description_present_flag: false,
                color_primaries: 2,
                transfer_characteristics: 2,
                matrix_coefficients: 2,
                color_range: false,
                subsampling_x: true,
                subsampling_y: true,
                chroma_sample_position: 0,
                separate_uv_delta_q: false,
                bit_depth,
                num_planes: 3,
            },
            requested_profile_compatible: matches!(bit_depth, 8 | 10),
            vulkan_std_session_parameters_ready: matches!(bit_depth, 8 | 10),
        }
    }
}

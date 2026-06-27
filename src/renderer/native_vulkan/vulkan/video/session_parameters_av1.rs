use std::ptr;

use crate::renderer::native_vulkan::{
    NativeVulkanAv1ColorConfigSnapshot, NativeVulkanAv1SequenceHeaderSnapshot,
    NativeVulkanAv1TimingInfoSnapshot, NativeVulkanVideoSessionCodec,
};
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::video_session_parameters::{
    NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot,
    NativeVulkanVulkanaliaVideoSessionParametersSnapshot, VulkanaliaVideoSessionParameters,
    native_vulkan_vulkanalia_create_video_session_parameters,
    native_vulkan_vulkanalia_destroy_video_session_parameters,
    vulkanalia_session_parameters_codec_label,
};

const AV1_SESSION_PARAMETERS_SOURCE: &str = "native-rust-av1-sequence-header-to-vulkanalia-std";

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_smoke_create_av1_video_session_parameters(
    device: &Device,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
    match native_vulkan_vulkanalia_create_av1_video_session_parameters(
        device,
        session,
        codec,
        sequence_header,
    ) {
        Ok(parameters) => {
            let snapshot = parameters.snapshot.clone();
            native_vulkan_vulkanalia_destroy_video_session_parameters(device, parameters);
            NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
                requested: true,
                supported: true,
                created: true,
                destroyed: true,
                error: None,
                parameters: snapshot,
            }
        }
        Err(err) => native_vulkan_vulkanalia_av1_session_parameters_error(codec, err),
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_av1_video_session_parameters(
    device: &Device,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<VulkanaliaVideoSessionParameters, String> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) {
        return Err("Vulkanalia AV1 session parameters require an AV1 session codec".to_owned());
    }
    if !sequence_header.vulkan_std_session_parameters_ready {
        return Err(
            "AV1 sequence header is not in the first supported Vulkanalia STD subset".to_owned(),
        );
    }

    native_vulkan_vulkanalia_smoke_create_av1_video_session_parameters_inner(
        device,
        session,
        sequence_header,
    )
}

fn native_vulkan_vulkanalia_smoke_create_av1_video_session_parameters_inner(
    device: &Device,
    session: vk::VideoSessionKHR,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<VulkanaliaVideoSessionParameters, String> {
    let color_config =
        native_vulkan_vulkanalia_av1_std_color_config(&sequence_header.color_config)?;
    let timing_info = sequence_header
        .timing_info
        .as_ref()
        .map(native_vulkan_vulkanalia_av1_std_timing_info);
    let timing_info_ptr = timing_info
        .as_ref()
        .map(|timing| timing as *const vk::video::StdVideoAV1TimingInfo)
        .unwrap_or_else(ptr::null);
    let std_sequence_header = vk::video::StdVideoAV1SequenceHeader {
        flags: vk::video::StdVideoAV1SequenceHeaderFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoAV1SequenceHeaderFlags::new_bitfield_1(
                av1_bool_u32(sequence_header.still_picture),
                av1_bool_u32(sequence_header.reduced_still_picture_header),
                av1_bool_u32(sequence_header.use_128x128_superblock),
                av1_bool_u32(sequence_header.enable_filter_intra),
                av1_bool_u32(sequence_header.enable_intra_edge_filter),
                av1_bool_u32(sequence_header.enable_interintra_compound),
                av1_bool_u32(sequence_header.enable_masked_compound),
                av1_bool_u32(sequence_header.enable_warped_motion),
                av1_bool_u32(sequence_header.enable_dual_filter),
                av1_bool_u32(sequence_header.enable_order_hint),
                av1_bool_u32(sequence_header.enable_jnt_comp),
                av1_bool_u32(sequence_header.enable_ref_frame_mvs),
                av1_bool_u32(sequence_header.frame_id_numbers_present_flag),
                av1_bool_u32(sequence_header.enable_superres),
                av1_bool_u32(sequence_header.enable_cdef),
                av1_bool_u32(sequence_header.enable_restoration),
                av1_bool_u32(sequence_header.film_grain_params_present),
                av1_bool_u32(sequence_header.timing_info_present_flag),
                av1_bool_u32(sequence_header.initial_display_delay_present_flag),
                0,
            ),
        },
        seq_profile: native_vulkan_vulkanalia_av1_std_profile(sequence_header.seq_profile)?,
        frame_width_bits_minus_1: sequence_header.frame_width_bits_minus_1,
        frame_height_bits_minus_1: sequence_header.frame_height_bits_minus_1,
        max_frame_width_minus_1: av1_u16(
            sequence_header.max_frame_width_minus_1,
            "max_frame_width_minus_1",
        )?,
        max_frame_height_minus_1: av1_u16(
            sequence_header.max_frame_height_minus_1,
            "max_frame_height_minus_1",
        )?,
        delta_frame_id_length_minus_2: sequence_header.delta_frame_id_length_minus_2.unwrap_or(0),
        additional_frame_id_length_minus_1: sequence_header
            .additional_frame_id_length_minus_1
            .unwrap_or(0),
        order_hint_bits_minus_1: sequence_header.order_hint_bits_minus_1.unwrap_or(0),
        seq_force_integer_mv: sequence_header.seq_force_integer_mv,
        seq_force_screen_content_tools: sequence_header.seq_force_screen_content_tools,
        reserved1: [0; 5],
        pColorConfig: &color_config,
        pTimingInfo: timing_info_ptr,
    };
    let mut av1_create_info = vk::VideoDecodeAV1SessionParametersCreateInfoKHR::builder()
        .std_sequence_header(&std_sequence_header)
        .build();
    let create_info = vk::VideoSessionParametersCreateInfoKHR::builder()
        .video_session(session)
        .push_next(&mut av1_create_info)
        .build();

    native_vulkan_vulkanalia_create_video_session_parameters(
        device,
        &create_info,
        NativeVulkanVulkanaliaVideoSessionParametersSnapshot {
            codec: native_vulkan_vulkanalia_av1_sequence_header_codec_label(sequence_header),
            source: AV1_SESSION_PARAMETERS_SOURCE,
            max_std_vps_count: 0,
            max_std_sps_count: 1,
            max_std_pps_count: 0,
            std_vps_count: 0,
            std_sps_count: 1,
            std_pps_count: 0,
        },
        "vulkanalia real av1 session parameters",
    )
    .map_err(|err| err.error)
}

fn native_vulkan_vulkanalia_av1_session_parameters_error(
    codec: NativeVulkanVideoSessionCodec,
    error: String,
) -> NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
    NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
        requested: true,
        supported: false,
        created: false,
        destroyed: false,
        error: Some(error),
        parameters: NativeVulkanVulkanaliaVideoSessionParametersSnapshot {
            codec: vulkanalia_session_parameters_codec_label(codec),
            source: AV1_SESSION_PARAMETERS_SOURCE,
            max_std_vps_count: 0,
            max_std_sps_count: 1,
            max_std_pps_count: 0,
            std_vps_count: 0,
            std_sps_count: 0,
            std_pps_count: 0,
        },
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_av1_sequence_header_codec_label(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> &'static str {
    match sequence_header.color_config.bit_depth {
        10 => "av1-main-10",
        _ => "av1-main-8",
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_av1_sequence_header_profile_label(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> &'static str {
    match sequence_header.color_config.bit_depth {
        10 => "main-10",
        _ => "main-8",
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_av1_sequence_header_bit_depth(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> u8 {
    sequence_header.color_config.bit_depth
}

fn native_vulkan_vulkanalia_av1_std_color_config(
    color_config: &NativeVulkanAv1ColorConfigSnapshot,
) -> Result<vk::video::StdVideoAV1ColorConfig, String> {
    Ok(vk::video::StdVideoAV1ColorConfig {
        flags: vk::video::StdVideoAV1ColorConfigFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoAV1ColorConfigFlags::new_bitfield_1(
                av1_bool_u32(color_config.mono_chrome),
                av1_bool_u32(color_config.color_range),
                av1_bool_u32(color_config.separate_uv_delta_q),
                av1_bool_u32(color_config.color_description_present_flag),
                0,
            ),
        },
        BitDepth: color_config.bit_depth,
        subsampling_x: u8::from(color_config.subsampling_x),
        subsampling_y: u8::from(color_config.subsampling_y),
        reserved1: 0,
        color_primaries: native_vulkan_vulkanalia_av1_std_color_primaries(
            color_config.color_primaries,
        )?,
        transfer_characteristics: native_vulkan_vulkanalia_av1_std_transfer_characteristics(
            color_config.transfer_characteristics,
        )?,
        matrix_coefficients: native_vulkan_vulkanalia_av1_std_matrix_coefficients(
            color_config.matrix_coefficients,
        )?,
        chroma_sample_position: native_vulkan_vulkanalia_av1_std_chroma_sample_position(
            color_config.chroma_sample_position,
        )?,
    })
}

fn native_vulkan_vulkanalia_av1_std_timing_info(
    timing_info: &NativeVulkanAv1TimingInfoSnapshot,
) -> vk::video::StdVideoAV1TimingInfo {
    vk::video::StdVideoAV1TimingInfo {
        flags: vk::video::StdVideoAV1TimingInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoAV1TimingInfoFlags::new_bitfield_1(
                av1_bool_u32(timing_info.equal_picture_interval),
                0,
            ),
        },
        num_units_in_display_tick: timing_info.num_units_in_display_tick,
        time_scale: timing_info.time_scale,
        num_ticks_per_picture_minus_1: timing_info.num_ticks_per_picture_minus_1.unwrap_or(0),
    }
}

fn native_vulkan_vulkanalia_av1_std_profile(
    profile: u8,
) -> Result<vk::video::StdVideoAV1Profile, String> {
    match profile {
        0 => Ok(vk::video::STD_VIDEO_AV1_PROFILE_MAIN),
        1 => Ok(vk::video::STD_VIDEO_AV1_PROFILE_HIGH),
        2 => Ok(vk::video::STD_VIDEO_AV1_PROFILE_PROFESSIONAL),
        other => Err(format!(
            "unsupported AV1 profile for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn native_vulkan_vulkanalia_av1_std_color_primaries(
    value: u8,
) -> Result<vk::video::StdVideoAV1ColorPrimaries, String> {
    match value {
        1 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_BT_709),
        2 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_UNSPECIFIED),
        4 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_BT_470_M),
        5 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_BT_470_B_G),
        6 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_BT_601),
        7 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_SMPTE_240),
        8 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_GENERIC_FILM),
        9 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_BT_2020),
        10 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_XYZ),
        11 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_SMPTE_431),
        12 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_SMPTE_432),
        22 => Ok(vk::video::STD_VIDEO_AV1_COLOR_PRIMARIES_EBU_3213),
        other => Err(format!(
            "unsupported AV1 color_primaries for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn native_vulkan_vulkanalia_av1_std_transfer_characteristics(
    value: u8,
) -> Result<vk::video::StdVideoAV1TransferCharacteristics, String> {
    match value {
        0 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_RESERVED_0),
        1 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_709),
        2 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_UNSPECIFIED),
        3 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_RESERVED_3),
        4 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_470_M),
        5 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_470_B_G),
        6 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_601),
        7 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_SMPTE_240),
        8 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_LINEAR),
        9 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_LOG_100),
        10 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_LOG_100_SQRT10),
        11 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_IEC_61966),
        12 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_1361),
        13 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_SRGB),
        14 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_2020_10_BIT),
        15 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_2020_12_BIT),
        16 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_SMPTE_2084),
        17 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_SMPTE_428),
        18 => Ok(vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_HLG),
        other => Err(format!(
            "unsupported AV1 transfer_characteristics for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn native_vulkan_vulkanalia_av1_std_matrix_coefficients(
    value: u8,
) -> Result<vk::video::StdVideoAV1MatrixCoefficients, String> {
    match value {
        0 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_IDENTITY),
        1 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_BT_709),
        2 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_UNSPECIFIED),
        3 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_RESERVED_3),
        4 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_FCC),
        5 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_BT_470_B_G),
        6 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_BT_601),
        7 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_SMPTE_240),
        8 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_SMPTE_YCGCO),
        9 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_BT_2020_NCL),
        10 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_BT_2020_CL),
        11 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_SMPTE_2085),
        12 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_CHROMAT_NCL),
        13 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_CHROMAT_CL),
        14 => Ok(vk::video::STD_VIDEO_AV1_MATRIX_COEFFICIENTS_ICTCP),
        other => Err(format!(
            "unsupported AV1 matrix_coefficients for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn native_vulkan_vulkanalia_av1_std_chroma_sample_position(
    value: u8,
) -> Result<vk::video::StdVideoAV1ChromaSamplePosition, String> {
    match value {
        0 => Ok(vk::video::STD_VIDEO_AV1_CHROMA_SAMPLE_POSITION_UNKNOWN),
        1 => Ok(vk::video::STD_VIDEO_AV1_CHROMA_SAMPLE_POSITION_VERTICAL),
        2 => Ok(vk::video::STD_VIDEO_AV1_CHROMA_SAMPLE_POSITION_COLOCATED),
        3 => Ok(vk::video::STD_VIDEO_AV1_CHROMA_SAMPLE_POSITION_RESERVED),
        other => Err(format!(
            "unsupported AV1 chroma_sample_position for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn av1_bool_u32(value: bool) -> u32 {
    u32::from(value)
}

fn av1_u16(value: u32, name: &'static str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("AV1 {name} exceeds u16: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn av1_profile_mapping_matches_vulkanalia_std_values() {
        assert_eq!(
            native_vulkan_vulkanalia_av1_std_profile(0).unwrap(),
            vk::video::STD_VIDEO_AV1_PROFILE_MAIN
        );
        assert_eq!(
            native_vulkan_vulkanalia_av1_std_profile(2).unwrap(),
            vk::video::STD_VIDEO_AV1_PROFILE_PROFESSIONAL
        );
        assert!(native_vulkan_vulkanalia_av1_std_profile(3).is_err());
    }

    #[test]
    fn av1_color_mapping_keeps_reserved_values_explicit() {
        assert_eq!(
            native_vulkan_vulkanalia_av1_std_transfer_characteristics(3).unwrap(),
            vk::video::STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_RESERVED_3
        );
        assert!(native_vulkan_vulkanalia_av1_std_matrix_coefficients(15).is_err());
    }
}

use std::ptr;

use crate::renderer::native_vulkan::{
    NativeVulkanH264ParameterSetSnapshot, NativeVulkanVideoSessionCodec,
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

const H264_SESSION_PARAMETERS_SOURCE: &str = "native-rust-h264-sps-pps-to-vulkanalia-std";

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_smoke_create_h264_video_session_parameters(
    device: &Device,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
    match native_vulkan_vulkanalia_create_h264_video_session_parameters(
        device,
        session,
        codec,
        parameter_sets,
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
        Err(err) => native_vulkan_vulkanalia_h264_session_parameters_error(codec, err),
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_h264_video_session_parameters(
    device: &Device,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<VulkanaliaVideoSessionParameters, String> {
    if codec != NativeVulkanVideoSessionCodec::H264High8 {
        return Err(
            "Vulkanalia H.264 session parameters require the h264-high-8 session codec".to_owned(),
        );
    }
    if !parameter_sets.vulkan_std_session_parameters_ready {
        return Err(
            "H.264 parameter sets are not in the first supported Vulkanalia STD subset".to_owned(),
        );
    }

    native_vulkan_vulkanalia_smoke_create_h264_video_session_parameters_inner(
        device,
        session,
        parameter_sets,
    )
}

fn native_vulkan_vulkanalia_smoke_create_h264_video_session_parameters_inner(
    device: &Device,
    session: vk::VideoSessionKHR,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<VulkanaliaVideoSessionParameters, String> {
    let offset_for_ref_frame = parameter_sets.sps.offset_for_ref_frame.clone();
    let offset_for_ref_frame_ptr = if offset_for_ref_frame.is_empty() {
        ptr::null()
    } else {
        offset_for_ref_frame.as_ptr()
    };

    let sps = [vk::video::StdVideoH264SequenceParameterSet {
        flags: vk::video::StdVideoH264SpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoH264SpsFlags::new_bitfield_1(
                h264_bool_u32(parameter_sets.sps.constraint_set0_flag),
                h264_bool_u32(parameter_sets.sps.constraint_set1_flag),
                h264_bool_u32(parameter_sets.sps.constraint_set2_flag),
                h264_bool_u32(parameter_sets.sps.constraint_set3_flag),
                h264_bool_u32(parameter_sets.sps.constraint_set4_flag),
                h264_bool_u32(parameter_sets.sps.constraint_set5_flag),
                h264_bool_u32(parameter_sets.sps.direct_8x8_inference_flag),
                h264_bool_u32(parameter_sets.sps.mb_adaptive_frame_field_flag),
                h264_bool_u32(parameter_sets.sps.frame_mbs_only_flag),
                h264_bool_u32(parameter_sets.sps.delta_pic_order_always_zero_flag),
                h264_bool_u32(parameter_sets.sps.separate_colour_plane_flag),
                h264_bool_u32(parameter_sets.sps.gaps_in_frame_num_value_allowed_flag),
                h264_bool_u32(parameter_sets.sps.qpprime_y_zero_transform_bypass_flag),
                h264_bool_u32(parameter_sets.sps.frame_cropping_flag),
                h264_bool_u32(parameter_sets.sps.seq_scaling_matrix_present_flag),
                h264_bool_u32(parameter_sets.sps.vui_parameters_present_flag),
            ),
            __bindgen_padding_0: 0,
        },
        profile_idc: native_vulkan_vulkanalia_h264_std_profile_idc(parameter_sets.sps.profile_idc)?,
        level_idc: native_vulkan_vulkanalia_h264_std_level_idc(parameter_sets.sps.level_idc)?,
        chroma_format_idc: native_vulkan_vulkanalia_h264_std_chroma_format_idc(
            parameter_sets.sps.chroma_format_idc,
        )?,
        seq_parameter_set_id: h264_u8(parameter_sets.sps.id, "seq_parameter_set_id")?,
        bit_depth_luma_minus8: h264_u8(
            parameter_sets.sps.bit_depth_luma_minus8,
            "bit_depth_luma_minus8",
        )?,
        bit_depth_chroma_minus8: h264_u8(
            parameter_sets.sps.bit_depth_chroma_minus8,
            "bit_depth_chroma_minus8",
        )?,
        log2_max_frame_num_minus4: h264_u8(
            parameter_sets.sps.log2_max_frame_num_minus4,
            "log2_max_frame_num_minus4",
        )?,
        pic_order_cnt_type: native_vulkan_vulkanalia_h264_std_poc_type(
            parameter_sets.sps.pic_order_cnt_type,
        )?,
        offset_for_non_ref_pic: parameter_sets.sps.offset_for_non_ref_pic,
        offset_for_top_to_bottom_field: parameter_sets.sps.offset_for_top_to_bottom_field,
        log2_max_pic_order_cnt_lsb_minus4: h264_u8(
            parameter_sets.sps.log2_max_pic_order_cnt_lsb_minus4,
            "log2_max_pic_order_cnt_lsb_minus4",
        )?,
        num_ref_frames_in_pic_order_cnt_cycle: h264_u8(
            parameter_sets.sps.offset_for_ref_frame.len() as u32,
            "num_ref_frames_in_pic_order_cnt_cycle",
        )?,
        max_num_ref_frames: h264_u8(parameter_sets.sps.max_num_ref_frames, "max_num_ref_frames")?,
        reserved1: 0,
        pic_width_in_mbs_minus1: parameter_sets.sps.pic_width_in_mbs_minus1,
        pic_height_in_map_units_minus1: parameter_sets.sps.pic_height_in_map_units_minus1,
        frame_crop_left_offset: parameter_sets.sps.frame_crop_left_offset,
        frame_crop_right_offset: parameter_sets.sps.frame_crop_right_offset,
        frame_crop_top_offset: parameter_sets.sps.frame_crop_top_offset,
        frame_crop_bottom_offset: parameter_sets.sps.frame_crop_bottom_offset,
        reserved2: 0,
        pOffsetForRefFrame: offset_for_ref_frame_ptr,
        pScalingLists: ptr::null(),
        pSequenceParameterSetVui: ptr::null(),
    }];

    let pps = [vk::video::StdVideoH264PictureParameterSet {
        flags: vk::video::StdVideoH264PpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoH264PpsFlags::new_bitfield_1(
                h264_bool_u32(parameter_sets.pps.transform_8x8_mode_flag),
                h264_bool_u32(parameter_sets.pps.redundant_pic_cnt_present_flag),
                h264_bool_u32(parameter_sets.pps.constrained_intra_pred_flag),
                h264_bool_u32(parameter_sets.pps.deblocking_filter_control_present_flag),
                h264_bool_u32(parameter_sets.pps.weighted_pred_flag),
                h264_bool_u32(
                    parameter_sets
                        .pps
                        .bottom_field_pic_order_in_frame_present_flag,
                ),
                h264_bool_u32(parameter_sets.pps.entropy_coding_mode_flag),
                h264_bool_u32(parameter_sets.pps.pic_scaling_matrix_present_flag),
            ),
            __bindgen_padding_0: [0; 3],
        },
        seq_parameter_set_id: h264_u8(parameter_sets.pps.sps_id, "pps.seq_parameter_set_id")?,
        pic_parameter_set_id: h264_u8(parameter_sets.pps.id, "pic_parameter_set_id")?,
        num_ref_idx_l0_default_active_minus1: h264_u8(
            parameter_sets.pps.num_ref_idx_l0_default_active_minus1,
            "num_ref_idx_l0_default_active_minus1",
        )?,
        num_ref_idx_l1_default_active_minus1: h264_u8(
            parameter_sets.pps.num_ref_idx_l1_default_active_minus1,
            "num_ref_idx_l1_default_active_minus1",
        )?,
        weighted_bipred_idc: native_vulkan_vulkanalia_h264_std_weighted_bipred_idc(
            parameter_sets.pps.weighted_bipred_idc,
        )?,
        pic_init_qp_minus26: h264_i8(
            parameter_sets.pps.pic_init_qp_minus26,
            "pic_init_qp_minus26",
        )?,
        pic_init_qs_minus26: h264_i8(
            parameter_sets.pps.pic_init_qs_minus26,
            "pic_init_qs_minus26",
        )?,
        chroma_qp_index_offset: h264_i8(
            parameter_sets.pps.chroma_qp_index_offset,
            "chroma_qp_index_offset",
        )?,
        second_chroma_qp_index_offset: h264_i8(
            parameter_sets.pps.second_chroma_qp_index_offset,
            "second_chroma_qp_index_offset",
        )?,
        pScalingLists: ptr::null(),
    }];

    let add_info = vk::VideoDecodeH264SessionParametersAddInfoKHR::builder()
        .std_sp_ss(&sps)
        .std_pp_ss(&pps)
        .build();
    let max_std_sps_count = 32;
    let max_std_pps_count = 32;
    let mut h264_create_info = vk::VideoDecodeH264SessionParametersCreateInfoKHR::builder()
        .max_std_sps_count(max_std_sps_count)
        .max_std_pps_count(max_std_pps_count)
        .parameters_add_info(&add_info)
        .build();
    let create_info = vk::VideoSessionParametersCreateInfoKHR::builder()
        .video_session(session)
        .push_next(&mut h264_create_info)
        .build();

    native_vulkan_vulkanalia_create_video_session_parameters(
        device,
        &create_info,
        NativeVulkanVulkanaliaVideoSessionParametersSnapshot {
            codec: native_vulkan_vulkanalia_h264_parameter_sets_codec_label(parameter_sets)?,
            source: H264_SESSION_PARAMETERS_SOURCE,
            max_std_vps_count: 0,
            max_std_sps_count,
            max_std_pps_count,
            std_vps_count: 0,
            std_sps_count: sps.len() as u32,
            std_pps_count: pps.len() as u32,
        },
        "vulkanalia real h264 session parameters",
    )
    .map_err(|err| err.error)
}

fn native_vulkan_vulkanalia_h264_parameter_sets_codec_label(
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<&'static str, String> {
    Ok(
        match native_vulkan_vulkanalia_h264_std_profile_label(parameter_sets.sps.profile_idc)? {
            "baseline" => "h264-baseline-8",
            "main" => "h264-main-8",
            "high" => "h264-high-8",
            _ => unreachable!("mapper returns a fixed H.264 profile label"),
        },
    )
}

fn native_vulkan_vulkanalia_h264_session_parameters_error(
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
            source: H264_SESSION_PARAMETERS_SOURCE,
            max_std_vps_count: 0,
            max_std_sps_count: 32,
            max_std_pps_count: 32,
            std_vps_count: 0,
            std_sps_count: 0,
            std_pps_count: 0,
        },
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_h264_std_profile_idc(
    profile_idc: u8,
) -> Result<vk::video::StdVideoH264ProfileIdc, String> {
    match profile_idc {
        66 => Ok(vk::video::STD_VIDEO_H264_PROFILE_IDC_BASELINE),
        77 => Ok(vk::video::STD_VIDEO_H264_PROFILE_IDC_MAIN),
        100 => Ok(vk::video::STD_VIDEO_H264_PROFILE_IDC_HIGH),
        other => Err(format!(
            "unsupported H.264 profile_idc for Vulkanalia STD session parameters: {other}"
        )),
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_h264_std_profile_label(
    profile_idc: u8,
) -> Result<&'static str, String> {
    match profile_idc {
        66 => Ok("baseline"),
        77 => Ok("main"),
        100 => Ok("high"),
        other => Err(format!(
            "unsupported H.264 profile_idc for Vulkanalia profile selection: {other}"
        )),
    }
}

fn native_vulkan_vulkanalia_h264_std_level_idc(
    level_idc: u8,
) -> Result<vk::video::StdVideoH264LevelIdc, String> {
    match level_idc {
        10 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_1_0),
        11 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_1_1),
        12 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_1_2),
        13 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_1_3),
        20 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_2_0),
        21 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_2_1),
        22 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_2_2),
        30 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_3_0),
        31 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_3_1),
        32 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_3_2),
        40 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_4_0),
        41 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_4_1),
        42 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_4_2),
        50 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_5_0),
        51 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_5_1),
        52 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_5_2),
        60 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_6_0),
        61 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_6_1),
        62 => Ok(vk::video::STD_VIDEO_H264_LEVEL_IDC_6_2),
        other => Err(format!(
            "unsupported H.264 level_idc for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn native_vulkan_vulkanalia_h264_std_chroma_format_idc(
    chroma_format_idc: u32,
) -> Result<vk::video::StdVideoH264ChromaFormatIdc, String> {
    match chroma_format_idc {
        0 => Ok(vk::video::STD_VIDEO_H264_CHROMA_FORMAT_IDC_MONOCHROME),
        1 => Ok(vk::video::STD_VIDEO_H264_CHROMA_FORMAT_IDC_420),
        2 => Ok(vk::video::STD_VIDEO_H264_CHROMA_FORMAT_IDC_422),
        3 => Ok(vk::video::STD_VIDEO_H264_CHROMA_FORMAT_IDC_444),
        other => Err(format!(
            "unsupported H.264 chroma_format_idc for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn native_vulkan_vulkanalia_h264_std_poc_type(
    pic_order_cnt_type: u32,
) -> Result<vk::video::StdVideoH264PocType, String> {
    match pic_order_cnt_type {
        0 => Ok(vk::video::STD_VIDEO_H264_POC_TYPE_0),
        1 => Ok(vk::video::STD_VIDEO_H264_POC_TYPE_1),
        2 => Ok(vk::video::STD_VIDEO_H264_POC_TYPE_2),
        other => Err(format!(
            "unsupported H.264 pic_order_cnt_type for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn native_vulkan_vulkanalia_h264_std_weighted_bipred_idc(
    weighted_bipred_idc: u32,
) -> Result<vk::video::StdVideoH264WeightedBipredIdc, String> {
    match weighted_bipred_idc {
        0 => Ok(vk::video::STD_VIDEO_H264_WEIGHTED_BIPRED_IDC_DEFAULT),
        1 => Ok(vk::video::STD_VIDEO_H264_WEIGHTED_BIPRED_IDC_EXPLICIT),
        2 => Ok(vk::video::STD_VIDEO_H264_WEIGHTED_BIPRED_IDC_IMPLICIT),
        other => Err(format!(
            "unsupported H.264 weighted_bipred_idc for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn h264_bool_u32(value: bool) -> u32 {
    u32::from(value)
}

fn h264_u8(value: u32, name: &'static str) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("H.264 {name} exceeds u8: {value}"))
}

fn h264_i8(value: i32, name: &'static str) -> Result<i8, String> {
    i8::try_from(value).map_err(|_| format!("H.264 {name} exceeds i8: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h264_profile_mapping_covers_real_world_8bit_420_profiles() {
        assert_eq!(
            native_vulkan_vulkanalia_h264_std_profile_idc(77).unwrap(),
            vk::video::STD_VIDEO_H264_PROFILE_IDC_MAIN
        );
        assert_eq!(
            native_vulkan_vulkanalia_h264_std_profile_idc(100).unwrap(),
            vk::video::STD_VIDEO_H264_PROFILE_IDC_HIGH
        );
        assert!(native_vulkan_vulkanalia_h264_std_profile_idc(110).is_err());
    }

    #[test]
    fn h264_weighted_bipred_mapping_rejects_unknown_values() {
        assert_eq!(
            native_vulkan_vulkanalia_h264_std_weighted_bipred_idc(2).unwrap(),
            vk::video::STD_VIDEO_H264_WEIGHTED_BIPRED_IDC_IMPLICIT
        );
        assert!(native_vulkan_vulkanalia_h264_std_weighted_bipred_idc(3).is_err());
    }
}

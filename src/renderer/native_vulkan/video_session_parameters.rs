//! Vulkan Video codec parameter conversion and session parameter objects.

use std::ptr;

use ash::vk;

use super::codec_snapshots::*;
use super::video_session_snapshots::NativeVulkanVideoSessionParametersSnapshot;
use super::{
    NativeVulkanError, h264, native_vulkan_bool_u32, native_vulkan_h264_i8, native_vulkan_h264_u8,
    native_vulkan_h265_i8, native_vulkan_h265_u8, native_vulkan_h265_u16,
};

pub(super) struct NativeVulkanVideoSessionParameters {
    pub(super) parameters: vk::VideoSessionParametersKHR,
    pub(super) snapshot: NativeVulkanVideoSessionParametersSnapshot,
}

pub(super) fn native_vulkan_h264_std_level_idc(
    level_idc: u8,
) -> Result<vk::native::StdVideoH264LevelIdc, NativeVulkanError> {
    let level = match level_idc {
        10 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_0,
        11 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_1,
        12 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_2,
        13 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_3,
        20 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_0,
        21 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_1,
        22 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_2,
        30 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_0,
        31 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_1,
        32 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_2,
        40 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_0,
        41 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_1,
        42 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_2,
        50 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_0,
        51 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_1,
        52 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_2,
        60 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_0,
        61 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_1,
        62 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_2,
        _ => {
            return Err(NativeVulkanError::Video(format!(
                "H.264 level_idc {level_idc} is not supported by the Vulkan STD mapper"
            )));
        }
    };
    Ok(level)
}

pub(super) fn native_vulkan_h264_std_chroma_format_idc(
    chroma_format_idc: u32,
) -> Result<vk::native::StdVideoH264ChromaFormatIdc, NativeVulkanError> {
    match chroma_format_idc {
        0 => {
            Ok(vk::native::StdVideoH264ChromaFormatIdc_STD_VIDEO_H264_CHROMA_FORMAT_IDC_MONOCHROME)
        }
        1 => Ok(vk::native::StdVideoH264ChromaFormatIdc_STD_VIDEO_H264_CHROMA_FORMAT_IDC_420),
        2 => Ok(vk::native::StdVideoH264ChromaFormatIdc_STD_VIDEO_H264_CHROMA_FORMAT_IDC_422),
        3 => Ok(vk::native::StdVideoH264ChromaFormatIdc_STD_VIDEO_H264_CHROMA_FORMAT_IDC_444),
        _ => Err(NativeVulkanError::Video(format!(
            "H.264 chroma_format_idc {chroma_format_idc} is not supported by the Vulkan STD mapper"
        ))),
    }
}

pub(super) fn native_vulkan_h264_std_poc_type(
    pic_order_cnt_type: u32,
) -> Result<vk::native::StdVideoH264PocType, NativeVulkanError> {
    match pic_order_cnt_type {
        0 => Ok(vk::native::StdVideoH264PocType_STD_VIDEO_H264_POC_TYPE_0),
        1 => Ok(vk::native::StdVideoH264PocType_STD_VIDEO_H264_POC_TYPE_1),
        2 => Ok(vk::native::StdVideoH264PocType_STD_VIDEO_H264_POC_TYPE_2),
        _ => Err(NativeVulkanError::Video(format!(
            "H.264 pic_order_cnt_type {pic_order_cnt_type} is not supported by the Vulkan STD mapper"
        ))),
    }
}

pub(super) fn native_vulkan_h264_std_weighted_bipred_idc(
    weighted_bipred_idc: u32,
) -> Result<vk::native::StdVideoH264WeightedBipredIdc, NativeVulkanError> {
    match weighted_bipred_idc {
        0 => Ok(
            vk::native::StdVideoH264WeightedBipredIdc_STD_VIDEO_H264_WEIGHTED_BIPRED_IDC_DEFAULT,
        ),
        1 => Ok(
            vk::native::StdVideoH264WeightedBipredIdc_STD_VIDEO_H264_WEIGHTED_BIPRED_IDC_EXPLICIT,
        ),
        2 => Ok(
            vk::native::StdVideoH264WeightedBipredIdc_STD_VIDEO_H264_WEIGHTED_BIPRED_IDC_IMPLICIT,
        ),
        _ => Err(NativeVulkanError::Video(format!(
            "H.264 weighted_bipred_idc {weighted_bipred_idc} is not supported by the Vulkan STD mapper"
        ))),
    }
}

pub(super) fn native_vulkan_h265_parameter_sets_codec_label(
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> &'static str {
    if parameter_sets.sps.bit_depth_luma_minus8 == 2
        && parameter_sets.sps.bit_depth_chroma_minus8 == 2
        && parameter_sets.sps.profile_idc == 2
    {
        "h265-main-10"
    } else {
        "h265-main-8"
    }
}

pub(super) fn native_vulkan_av1_sequence_header_codec_label(
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> &'static str {
    match sequence_header.color_config.bit_depth {
        10 => "av1-main-10",
        _ => "av1-main-8",
    }
}

pub(super) fn native_vulkan_create_h264_video_session_parameters(
    video_queue_device: &ash::khr::video_queue::Device,
    session: vk::VideoSessionKHR,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<NativeVulkanVideoSessionParameters, NativeVulkanError> {
    if !parameter_sets.vulkan_std_session_parameters_ready {
        return Err(NativeVulkanError::Video(
            "H.264 parameter sets are not in the first supported Vulkan STD subset".to_owned(),
        ));
    }

    let offset_for_ref_frame = parameter_sets.sps.offset_for_ref_frame.clone();
    let offset_for_ref_frame_ptr = if offset_for_ref_frame.is_empty() {
        ptr::null()
    } else {
        offset_for_ref_frame.as_ptr()
    };
    let sps = [vk::native::StdVideoH264SequenceParameterSet {
        flags: vk::native::StdVideoH264SpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH264SpsFlags::new_bitfield_1(
                native_vulkan_bool_u32(parameter_sets.sps.constraint_set0_flag),
                native_vulkan_bool_u32(parameter_sets.sps.constraint_set1_flag),
                native_vulkan_bool_u32(parameter_sets.sps.constraint_set2_flag),
                native_vulkan_bool_u32(parameter_sets.sps.constraint_set3_flag),
                native_vulkan_bool_u32(parameter_sets.sps.constraint_set4_flag),
                native_vulkan_bool_u32(parameter_sets.sps.constraint_set5_flag),
                native_vulkan_bool_u32(parameter_sets.sps.direct_8x8_inference_flag),
                native_vulkan_bool_u32(parameter_sets.sps.mb_adaptive_frame_field_flag),
                native_vulkan_bool_u32(parameter_sets.sps.frame_mbs_only_flag),
                native_vulkan_bool_u32(parameter_sets.sps.delta_pic_order_always_zero_flag),
                native_vulkan_bool_u32(parameter_sets.sps.separate_colour_plane_flag),
                native_vulkan_bool_u32(parameter_sets.sps.gaps_in_frame_num_value_allowed_flag),
                native_vulkan_bool_u32(parameter_sets.sps.qpprime_y_zero_transform_bypass_flag),
                native_vulkan_bool_u32(parameter_sets.sps.frame_cropping_flag),
                native_vulkan_bool_u32(parameter_sets.sps.seq_scaling_matrix_present_flag),
                native_vulkan_bool_u32(parameter_sets.sps.vui_parameters_present_flag),
            ),
            __bindgen_padding_0: 0,
        },
        profile_idc: h264::native_vulkan_h264_std_profile_idc(parameter_sets.sps.profile_idc)
            .ok_or_else(|| {
                NativeVulkanError::Video(format!(
                    "H.264 {} profile_idc {} is not supported by the Vulkan STD mapper",
                    parameter_sets.sps.profile_label, parameter_sets.sps.profile_idc
                ))
            })?,
        level_idc: native_vulkan_h264_std_level_idc(parameter_sets.sps.level_idc)?,
        chroma_format_idc: native_vulkan_h264_std_chroma_format_idc(
            parameter_sets.sps.chroma_format_idc,
        )?,
        seq_parameter_set_id: native_vulkan_h264_u8(parameter_sets.sps.id, "seq_parameter_set_id")
            .map_err(NativeVulkanError::Video)?,
        bit_depth_luma_minus8: native_vulkan_h264_u8(
            parameter_sets.sps.bit_depth_luma_minus8,
            "bit_depth_luma_minus8",
        )
        .map_err(NativeVulkanError::Video)?,
        bit_depth_chroma_minus8: native_vulkan_h264_u8(
            parameter_sets.sps.bit_depth_chroma_minus8,
            "bit_depth_chroma_minus8",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_max_frame_num_minus4: native_vulkan_h264_u8(
            parameter_sets.sps.log2_max_frame_num_minus4,
            "log2_max_frame_num_minus4",
        )
        .map_err(NativeVulkanError::Video)?,
        pic_order_cnt_type: native_vulkan_h264_std_poc_type(parameter_sets.sps.pic_order_cnt_type)?,
        offset_for_non_ref_pic: parameter_sets.sps.offset_for_non_ref_pic,
        offset_for_top_to_bottom_field: parameter_sets.sps.offset_for_top_to_bottom_field,
        log2_max_pic_order_cnt_lsb_minus4: native_vulkan_h264_u8(
            parameter_sets.sps.log2_max_pic_order_cnt_lsb_minus4,
            "log2_max_pic_order_cnt_lsb_minus4",
        )
        .map_err(NativeVulkanError::Video)?,
        num_ref_frames_in_pic_order_cnt_cycle: native_vulkan_h264_u8(
            parameter_sets.sps.offset_for_ref_frame.len() as u32,
            "num_ref_frames_in_pic_order_cnt_cycle",
        )
        .map_err(NativeVulkanError::Video)?,
        max_num_ref_frames: native_vulkan_h264_u8(
            parameter_sets.sps.max_num_ref_frames,
            "max_num_ref_frames",
        )
        .map_err(NativeVulkanError::Video)?,
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

    let pps = [vk::native::StdVideoH264PictureParameterSet {
        flags: vk::native::StdVideoH264PpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH264PpsFlags::new_bitfield_1(
                native_vulkan_bool_u32(parameter_sets.pps.transform_8x8_mode_flag),
                native_vulkan_bool_u32(parameter_sets.pps.redundant_pic_cnt_present_flag),
                native_vulkan_bool_u32(parameter_sets.pps.constrained_intra_pred_flag),
                native_vulkan_bool_u32(parameter_sets.pps.deblocking_filter_control_present_flag),
                native_vulkan_bool_u32(parameter_sets.pps.weighted_pred_flag),
                native_vulkan_bool_u32(
                    parameter_sets
                        .pps
                        .bottom_field_pic_order_in_frame_present_flag,
                ),
                native_vulkan_bool_u32(parameter_sets.pps.entropy_coding_mode_flag),
                native_vulkan_bool_u32(parameter_sets.pps.pic_scaling_matrix_present_flag),
            ),
            __bindgen_padding_0: [0; 3],
        },
        seq_parameter_set_id: native_vulkan_h264_u8(
            parameter_sets.pps.sps_id,
            "pps.seq_parameter_set_id",
        )
        .map_err(NativeVulkanError::Video)?,
        pic_parameter_set_id: native_vulkan_h264_u8(parameter_sets.pps.id, "pic_parameter_set_id")
            .map_err(NativeVulkanError::Video)?,
        num_ref_idx_l0_default_active_minus1: native_vulkan_h264_u8(
            parameter_sets.pps.num_ref_idx_l0_default_active_minus1,
            "num_ref_idx_l0_default_active_minus1",
        )
        .map_err(NativeVulkanError::Video)?,
        num_ref_idx_l1_default_active_minus1: native_vulkan_h264_u8(
            parameter_sets.pps.num_ref_idx_l1_default_active_minus1,
            "num_ref_idx_l1_default_active_minus1",
        )
        .map_err(NativeVulkanError::Video)?,
        weighted_bipred_idc: native_vulkan_h264_std_weighted_bipred_idc(
            parameter_sets.pps.weighted_bipred_idc,
        )?,
        pic_init_qp_minus26: native_vulkan_h264_i8(
            parameter_sets.pps.pic_init_qp_minus26,
            "pic_init_qp_minus26",
        )
        .map_err(NativeVulkanError::Video)?,
        pic_init_qs_minus26: native_vulkan_h264_i8(
            parameter_sets.pps.pic_init_qs_minus26,
            "pic_init_qs_minus26",
        )
        .map_err(NativeVulkanError::Video)?,
        chroma_qp_index_offset: native_vulkan_h264_i8(
            parameter_sets.pps.chroma_qp_index_offset,
            "chroma_qp_index_offset",
        )
        .map_err(NativeVulkanError::Video)?,
        second_chroma_qp_index_offset: native_vulkan_h264_i8(
            parameter_sets.pps.second_chroma_qp_index_offset,
            "second_chroma_qp_index_offset",
        )
        .map_err(NativeVulkanError::Video)?,
        pScalingLists: ptr::null(),
    }];

    let add_info = vk::VideoDecodeH264SessionParametersAddInfoKHR::default()
        .std_sp_ss(&sps)
        .std_pp_ss(&pps);
    let max_std_sps_count = 32;
    let max_std_pps_count = 32;
    let mut h264_create_info = vk::VideoDecodeH264SessionParametersCreateInfoKHR::default()
        .max_std_sps_count(max_std_sps_count)
        .max_std_pps_count(max_std_pps_count)
        .parameters_add_info(&add_info);
    let create_info = vk::VideoSessionParametersCreateInfoKHR::default()
        .video_session(session)
        .push_next(&mut h264_create_info);
    let mut parameters = vk::VideoSessionParametersKHR::null();
    unsafe {
        (video_queue_device.fp().create_video_session_parameters_khr)(
            video_queue_device.device(),
            &create_info,
            ptr::null(),
            &mut parameters,
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkCreateVideoSessionParametersKHR(h264)",
        result,
    })?;

    Ok(NativeVulkanVideoSessionParameters {
        parameters,
        snapshot: NativeVulkanVideoSessionParametersSnapshot {
            codec: "h264-high-8",
            source: "native-rust-h264-sps-pps-to-vulkan-std",
            max_std_vps_count: 0,
            max_std_sps_count,
            max_std_pps_count,
            std_vps_count: 0,
            std_sps_count: sps.len() as u32,
            std_pps_count: pps.len() as u32,
            vps_id: 0,
            sps_id: parameter_sets.sps.id,
            pps_id: parameter_sets.pps.id,
            profile_idc: parameter_sets.sps.profile_idc,
            level_idc: parameter_sets.sps.level_idc,
            width: parameter_sets.sps.width,
            height: parameter_sets.sps.height,
            created: true,
        },
    })
}

pub(super) fn native_vulkan_create_h265_video_session_parameters(
    video_queue_device: &ash::khr::video_queue::Device,
    session: vk::VideoSessionKHR,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<NativeVulkanVideoSessionParameters, NativeVulkanError> {
    if !parameter_sets.vulkan_std_session_parameters_ready {
        return Err(NativeVulkanError::Video(
            "H.265 parameter sets are not in the first supported Vulkan STD subset".to_owned(),
        ));
    }

    let vps_profile_tier_level = native_vulkan_h265_std_profile_tier_level(
        parameter_sets.vps.profile_idc,
        parameter_sets.vps.level_idc,
        parameter_sets.vps.tier_flag,
        parameter_sets.vps.progressive_source_flag,
        parameter_sets.vps.interlaced_source_flag,
        parameter_sets.vps.non_packed_constraint_flag,
        parameter_sets.vps.frame_only_constraint_flag,
    )?;
    let sps_profile_tier_level = native_vulkan_h265_std_profile_tier_level(
        parameter_sets.sps.profile_idc,
        parameter_sets.sps.level_idc,
        parameter_sets.sps.tier_flag,
        parameter_sets.sps.progressive_source_flag,
        parameter_sets.sps.interlaced_source_flag,
        parameter_sets.sps.non_packed_constraint_flag,
        parameter_sets.sps.frame_only_constraint_flag,
    )?;
    let vps_dec_pic_buf_mgr =
        native_vulkan_h265_std_dec_pic_buf_mgr(&parameter_sets.vps.dec_pic_buf_mgr);
    let sps_dec_pic_buf_mgr =
        native_vulkan_h265_std_dec_pic_buf_mgr(&parameter_sets.sps.dec_pic_buf_mgr);
    let sps_vui = parameter_sets
        .sps
        .vui
        .as_ref()
        .map(native_vulkan_h265_std_vui)
        .transpose()?;
    let sps_vui_ptr = sps_vui
        .as_ref()
        .map(|vui| vui as *const vk::native::StdVideoH265SequenceParameterSetVui)
        .unwrap_or_else(ptr::null);
    let sps_short_term_ref_pic_sets =
        native_vulkan_h265_std_short_term_ref_pic_sets(&parameter_sets.sps.short_term_ref_pic_sets)
            .map_err(NativeVulkanError::Video)?;
    let sps_short_term_ref_pic_sets_ptr = if sps_short_term_ref_pic_sets.is_empty() {
        ptr::null()
    } else {
        sps_short_term_ref_pic_sets.as_ptr()
    };
    let sps_long_term_ref_pics =
        native_vulkan_h265_std_long_term_ref_pics_sps(&parameter_sets.sps.long_term_ref_pics_sps)
            .map_err(NativeVulkanError::Video)?;
    let sps_long_term_ref_pics_ptr = sps_long_term_ref_pics
        .as_ref()
        .map(|ref_pics| ref_pics as *const vk::native::StdVideoH265LongTermRefPicsSps)
        .unwrap_or_else(ptr::null);

    let vps = [vk::native::StdVideoH265VideoParameterSet {
        flags: vk::native::StdVideoH265VpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265VpsFlags::new_bitfield_1(
                native_vulkan_bool_u32(parameter_sets.vps.temporal_id_nesting_flag),
                native_vulkan_bool_u32(parameter_sets.vps.sub_layer_ordering_info_present_flag),
                native_vulkan_bool_u32(parameter_sets.vps.timing_info_present_flag),
                native_vulkan_bool_u32(parameter_sets.vps.poc_proportional_to_timing_flag),
            ),
            __bindgen_padding_0: [0; 3],
        },
        vps_video_parameter_set_id: parameter_sets.vps.id,
        vps_max_sub_layers_minus1: parameter_sets.vps.max_sub_layers_minus1,
        reserved1: 0,
        reserved2: 0,
        vps_num_units_in_tick: parameter_sets.vps.num_units_in_tick.unwrap_or(0),
        vps_time_scale: parameter_sets.vps.time_scale.unwrap_or(0),
        vps_num_ticks_poc_diff_one_minus1: parameter_sets
            .vps
            .num_ticks_poc_diff_one_minus1
            .unwrap_or(0),
        reserved3: 0,
        pDecPicBufMgr: &vps_dec_pic_buf_mgr,
        pHrdParameters: ptr::null(),
        pProfileTierLevel: &vps_profile_tier_level,
    }];

    let sps = [vk::native::StdVideoH265SequenceParameterSet {
        flags: vk::native::StdVideoH265SpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265SpsFlags::new_bitfield_1(
                native_vulkan_bool_u32(parameter_sets.sps.temporal_id_nesting_flag),
                native_vulkan_bool_u32(parameter_sets.sps.separate_colour_plane_flag),
                native_vulkan_bool_u32(parameter_sets.sps.conformance_window_flag),
                native_vulkan_bool_u32(parameter_sets.sps.sub_layer_ordering_info_present_flag),
                native_vulkan_bool_u32(parameter_sets.sps.scaling_list_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.sps_scaling_list_data_present_flag),
                native_vulkan_bool_u32(parameter_sets.sps.amp_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.sample_adaptive_offset_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.pcm_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.pcm_loop_filter_disabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.long_term_ref_pics_present_flag),
                native_vulkan_bool_u32(parameter_sets.sps.temporal_mvp_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.strong_intra_smoothing_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.sps.vui_parameters_present_flag),
                native_vulkan_bool_u32(parameter_sets.sps.sps_extension_present_flag),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ),
        },
        chroma_format_idc: native_vulkan_h265_std_chroma_format_idc(
            parameter_sets.sps.chroma_format_idc,
        )?,
        pic_width_in_luma_samples: parameter_sets.sps.width,
        pic_height_in_luma_samples: parameter_sets.sps.height,
        sps_video_parameter_set_id: parameter_sets.sps.vps_id,
        sps_max_sub_layers_minus1: parameter_sets.sps.max_sub_layers_minus1,
        sps_seq_parameter_set_id: native_vulkan_h265_u8(
            parameter_sets.sps.id,
            "sps_seq_parameter_set_id",
        )
        .map_err(NativeVulkanError::Video)?,
        bit_depth_luma_minus8: native_vulkan_h265_u8(
            parameter_sets.sps.bit_depth_luma_minus8,
            "bit_depth_luma_minus8",
        )
        .map_err(NativeVulkanError::Video)?,
        bit_depth_chroma_minus8: native_vulkan_h265_u8(
            parameter_sets.sps.bit_depth_chroma_minus8,
            "bit_depth_chroma_minus8",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_max_pic_order_cnt_lsb_minus4: native_vulkan_h265_u8(
            parameter_sets.sps.log2_max_pic_order_cnt_lsb_minus4,
            "log2_max_pic_order_cnt_lsb_minus4",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_min_luma_coding_block_size_minus3: native_vulkan_h265_u8(
            parameter_sets.sps.log2_min_luma_coding_block_size_minus3,
            "log2_min_luma_coding_block_size_minus3",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_diff_max_min_luma_coding_block_size: native_vulkan_h265_u8(
            parameter_sets.sps.log2_diff_max_min_luma_coding_block_size,
            "log2_diff_max_min_luma_coding_block_size",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_min_luma_transform_block_size_minus2: native_vulkan_h265_u8(
            parameter_sets.sps.log2_min_luma_transform_block_size_minus2,
            "log2_min_luma_transform_block_size_minus2",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_diff_max_min_luma_transform_block_size: native_vulkan_h265_u8(
            parameter_sets
                .sps
                .log2_diff_max_min_luma_transform_block_size,
            "log2_diff_max_min_luma_transform_block_size",
        )
        .map_err(NativeVulkanError::Video)?,
        max_transform_hierarchy_depth_inter: native_vulkan_h265_u8(
            parameter_sets.sps.max_transform_hierarchy_depth_inter,
            "max_transform_hierarchy_depth_inter",
        )
        .map_err(NativeVulkanError::Video)?,
        max_transform_hierarchy_depth_intra: native_vulkan_h265_u8(
            parameter_sets.sps.max_transform_hierarchy_depth_intra,
            "max_transform_hierarchy_depth_intra",
        )
        .map_err(NativeVulkanError::Video)?,
        num_short_term_ref_pic_sets: native_vulkan_h265_u8(
            parameter_sets.sps.num_short_term_ref_pic_sets,
            "num_short_term_ref_pic_sets",
        )
        .map_err(NativeVulkanError::Video)?,
        num_long_term_ref_pics_sps: native_vulkan_h265_u8(
            parameter_sets.sps.long_term_ref_pics_sps.len() as u32,
            "num_long_term_ref_pics_sps",
        )
        .map_err(NativeVulkanError::Video)?,
        pcm_sample_bit_depth_luma_minus1: 0,
        pcm_sample_bit_depth_chroma_minus1: 0,
        log2_min_pcm_luma_coding_block_size_minus3: 0,
        log2_diff_max_min_pcm_luma_coding_block_size: 0,
        reserved1: 0,
        reserved2: 0,
        palette_max_size: 0,
        delta_palette_max_predictor_size: 0,
        motion_vector_resolution_control_idc: 0,
        sps_num_palette_predictor_initializers_minus1: 0,
        conf_win_left_offset: parameter_sets.sps.conf_win_left_offset,
        conf_win_right_offset: parameter_sets.sps.conf_win_right_offset,
        conf_win_top_offset: parameter_sets.sps.conf_win_top_offset,
        conf_win_bottom_offset: parameter_sets.sps.conf_win_bottom_offset,
        pProfileTierLevel: &sps_profile_tier_level,
        pDecPicBufMgr: &sps_dec_pic_buf_mgr,
        pScalingLists: ptr::null(),
        pShortTermRefPicSet: sps_short_term_ref_pic_sets_ptr,
        pLongTermRefPicsSps: sps_long_term_ref_pics_ptr,
        pSequenceParameterSetVui: sps_vui_ptr,
        pPredictorPaletteEntries: ptr::null(),
    }];

    let pps = [vk::native::StdVideoH265PictureParameterSet {
        flags: vk::native::StdVideoH265PpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265PpsFlags::new_bitfield_1(
                native_vulkan_bool_u32(parameter_sets.pps.dependent_slice_segments_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.output_flag_present_flag),
                native_vulkan_bool_u32(parameter_sets.pps.sign_data_hiding_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.cabac_init_present_flag),
                native_vulkan_bool_u32(parameter_sets.pps.constrained_intra_pred_flag),
                native_vulkan_bool_u32(parameter_sets.pps.transform_skip_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.cu_qp_delta_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.slice_chroma_qp_offsets_present_flag),
                native_vulkan_bool_u32(parameter_sets.pps.weighted_pred_flag),
                native_vulkan_bool_u32(parameter_sets.pps.weighted_bipred_flag),
                native_vulkan_bool_u32(parameter_sets.pps.transquant_bypass_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.tiles_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.entropy_coding_sync_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.uniform_spacing_flag),
                native_vulkan_bool_u32(
                    parameter_sets
                        .pps
                        .loop_filter_across_tiles_enabled_flag
                        .unwrap_or(false),
                ),
                native_vulkan_bool_u32(parameter_sets.pps.loop_filter_across_slices_enabled_flag),
                native_vulkan_bool_u32(parameter_sets.pps.deblocking_filter_control_present_flag),
                native_vulkan_bool_u32(
                    parameter_sets
                        .pps
                        .deblocking_filter_override_enabled_flag
                        .unwrap_or(false),
                ),
                native_vulkan_bool_u32(
                    parameter_sets
                        .pps
                        .pps_deblocking_filter_disabled_flag
                        .unwrap_or(false),
                ),
                native_vulkan_bool_u32(parameter_sets.pps.pps_scaling_list_data_present_flag),
                native_vulkan_bool_u32(parameter_sets.pps.lists_modification_present_flag),
                native_vulkan_bool_u32(
                    parameter_sets
                        .pps
                        .slice_segment_header_extension_present_flag,
                ),
                native_vulkan_bool_u32(parameter_sets.pps.pps_extension_present_flag),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ),
        },
        pps_pic_parameter_set_id: native_vulkan_h265_u8(
            parameter_sets.pps.id,
            "pps_pic_parameter_set_id",
        )
        .map_err(NativeVulkanError::Video)?,
        pps_seq_parameter_set_id: native_vulkan_h265_u8(
            parameter_sets.pps.sps_id,
            "pps_seq_parameter_set_id",
        )
        .map_err(NativeVulkanError::Video)?,
        sps_video_parameter_set_id: parameter_sets.sps.vps_id,
        num_extra_slice_header_bits: parameter_sets.pps.num_extra_slice_header_bits,
        num_ref_idx_l0_default_active_minus1: native_vulkan_h265_u8(
            parameter_sets.pps.num_ref_idx_l0_default_active_minus1,
            "num_ref_idx_l0_default_active_minus1",
        )
        .map_err(NativeVulkanError::Video)?,
        num_ref_idx_l1_default_active_minus1: native_vulkan_h265_u8(
            parameter_sets.pps.num_ref_idx_l1_default_active_minus1,
            "num_ref_idx_l1_default_active_minus1",
        )
        .map_err(NativeVulkanError::Video)?,
        init_qp_minus26: native_vulkan_h265_i8(
            parameter_sets.pps.init_qp_minus26,
            "init_qp_minus26",
        )
        .map_err(NativeVulkanError::Video)?,
        diff_cu_qp_delta_depth: native_vulkan_h265_u8(
            parameter_sets.pps.diff_cu_qp_delta_depth.unwrap_or(0),
            "diff_cu_qp_delta_depth",
        )
        .map_err(NativeVulkanError::Video)?,
        pps_cb_qp_offset: native_vulkan_h265_i8(
            parameter_sets.pps.cb_qp_offset,
            "pps_cb_qp_offset",
        )
        .map_err(NativeVulkanError::Video)?,
        pps_cr_qp_offset: native_vulkan_h265_i8(
            parameter_sets.pps.cr_qp_offset,
            "pps_cr_qp_offset",
        )
        .map_err(NativeVulkanError::Video)?,
        pps_beta_offset_div2: native_vulkan_h265_i8(
            parameter_sets.pps.pps_beta_offset_div2,
            "pps_beta_offset_div2",
        )
        .map_err(NativeVulkanError::Video)?,
        pps_tc_offset_div2: native_vulkan_h265_i8(
            parameter_sets.pps.pps_tc_offset_div2,
            "pps_tc_offset_div2",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_parallel_merge_level_minus2: native_vulkan_h265_u8(
            parameter_sets.pps.log2_parallel_merge_level_minus2,
            "log2_parallel_merge_level_minus2",
        )
        .map_err(NativeVulkanError::Video)?,
        log2_max_transform_skip_block_size_minus2: 0,
        diff_cu_chroma_qp_offset_depth: 0,
        chroma_qp_offset_list_len_minus1: 0,
        cb_qp_offset_list: [0; 6],
        cr_qp_offset_list: [0; 6],
        log2_sao_offset_scale_luma: 0,
        log2_sao_offset_scale_chroma: 0,
        pps_act_y_qp_offset_plus5: 0,
        pps_act_cb_qp_offset_plus5: 0,
        pps_act_cr_qp_offset_plus3: 0,
        pps_num_palette_predictor_initializers: 0,
        luma_bit_depth_entry_minus8: 0,
        chroma_bit_depth_entry_minus8: 0,
        num_tile_columns_minus1: native_vulkan_h265_u8(
            parameter_sets.pps.num_tile_columns_minus1,
            "num_tile_columns_minus1",
        )
        .map_err(NativeVulkanError::Video)?,
        num_tile_rows_minus1: native_vulkan_h265_u8(
            parameter_sets.pps.num_tile_rows_minus1,
            "num_tile_rows_minus1",
        )
        .map_err(NativeVulkanError::Video)?,
        reserved1: 0,
        reserved2: 0,
        column_width_minus1: [0; 19],
        row_height_minus1: [0; 21],
        reserved3: 0,
        pScalingLists: ptr::null(),
        pPredictorPaletteEntries: ptr::null(),
    }];

    let add_info = vk::VideoDecodeH265SessionParametersAddInfoKHR::default()
        .std_vp_ss(&vps)
        .std_sp_ss(&sps)
        .std_pp_ss(&pps);
    let max_std_vps_count = 32;
    let max_std_sps_count = 32;
    let max_std_pps_count = 64;
    let mut h265_create_info = vk::VideoDecodeH265SessionParametersCreateInfoKHR::default()
        .max_std_vps_count(max_std_vps_count)
        .max_std_sps_count(max_std_sps_count)
        .max_std_pps_count(max_std_pps_count)
        .parameters_add_info(&add_info);
    let create_info = vk::VideoSessionParametersCreateInfoKHR::default()
        .video_session(session)
        .push_next(&mut h265_create_info);
    let mut parameters = vk::VideoSessionParametersKHR::null();
    unsafe {
        (video_queue_device.fp().create_video_session_parameters_khr)(
            video_queue_device.device(),
            &create_info,
            ptr::null(),
            &mut parameters,
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkCreateVideoSessionParametersKHR(h265)",
        result,
    })?;

    Ok(NativeVulkanVideoSessionParameters {
        parameters,
        snapshot: NativeVulkanVideoSessionParametersSnapshot {
            codec: native_vulkan_h265_parameter_sets_codec_label(parameter_sets),
            source: "native-rust-h265-vps-sps-pps-to-vulkan-std",
            max_std_vps_count,
            max_std_sps_count,
            max_std_pps_count,
            std_vps_count: vps.len() as u32,
            std_sps_count: sps.len() as u32,
            std_pps_count: pps.len() as u32,
            vps_id: parameter_sets.vps.id,
            sps_id: parameter_sets.sps.id,
            pps_id: parameter_sets.pps.id,
            profile_idc: parameter_sets.sps.profile_idc,
            level_idc: parameter_sets.sps.level_idc,
            width: parameter_sets.sps.width,
            height: parameter_sets.sps.height,
            created: true,
        },
    })
}

pub(super) fn native_vulkan_create_av1_video_session_parameters(
    video_queue_device: &ash::khr::video_queue::Device,
    session: vk::VideoSessionKHR,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<NativeVulkanVideoSessionParameters, NativeVulkanError> {
    if !sequence_header.vulkan_std_session_parameters_ready {
        return Err(NativeVulkanError::Video(
            "AV1 sequence header is not in the first supported Vulkan STD subset".to_owned(),
        ));
    }

    let color_config = native_vulkan_av1_std_color_config(&sequence_header.color_config)?;
    let timing_info = sequence_header
        .timing_info
        .as_ref()
        .map(native_vulkan_av1_std_timing_info);
    let timing_info_ptr = timing_info
        .as_ref()
        .map(|timing| timing as *const vk::native::StdVideoAV1TimingInfo)
        .unwrap_or_else(ptr::null);
    let std_sequence_header = vk::native::StdVideoAV1SequenceHeader {
        flags: vk::native::StdVideoAV1SequenceHeaderFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoAV1SequenceHeaderFlags::new_bitfield_1(
                native_vulkan_bool_u32(sequence_header.still_picture),
                native_vulkan_bool_u32(sequence_header.reduced_still_picture_header),
                native_vulkan_bool_u32(sequence_header.use_128x128_superblock),
                native_vulkan_bool_u32(sequence_header.enable_filter_intra),
                native_vulkan_bool_u32(sequence_header.enable_intra_edge_filter),
                native_vulkan_bool_u32(sequence_header.enable_interintra_compound),
                native_vulkan_bool_u32(sequence_header.enable_masked_compound),
                native_vulkan_bool_u32(sequence_header.enable_warped_motion),
                native_vulkan_bool_u32(sequence_header.enable_dual_filter),
                native_vulkan_bool_u32(sequence_header.enable_order_hint),
                native_vulkan_bool_u32(sequence_header.enable_jnt_comp),
                native_vulkan_bool_u32(sequence_header.enable_ref_frame_mvs),
                native_vulkan_bool_u32(sequence_header.frame_id_numbers_present_flag),
                native_vulkan_bool_u32(sequence_header.enable_superres),
                native_vulkan_bool_u32(sequence_header.enable_cdef),
                native_vulkan_bool_u32(sequence_header.enable_restoration),
                native_vulkan_bool_u32(sequence_header.film_grain_params_present),
                native_vulkan_bool_u32(sequence_header.timing_info_present_flag),
                native_vulkan_bool_u32(sequence_header.initial_display_delay_present_flag),
                0,
            ),
        },
        seq_profile: native_vulkan_av1_std_profile(sequence_header.seq_profile)?,
        frame_width_bits_minus_1: sequence_header.frame_width_bits_minus_1,
        frame_height_bits_minus_1: sequence_header.frame_height_bits_minus_1,
        max_frame_width_minus_1: u16::try_from(sequence_header.max_frame_width_minus_1).map_err(
            |_| {
                NativeVulkanError::Video(format!(
                    "AV1 max_frame_width_minus_1 {} exceeds u16 range",
                    sequence_header.max_frame_width_minus_1
                ))
            },
        )?,
        max_frame_height_minus_1: u16::try_from(sequence_header.max_frame_height_minus_1).map_err(
            |_| {
                NativeVulkanError::Video(format!(
                    "AV1 max_frame_height_minus_1 {} exceeds u16 range",
                    sequence_header.max_frame_height_minus_1
                ))
            },
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
    let mut av1_create_info = vk::VideoDecodeAV1SessionParametersCreateInfoKHR::default()
        .std_sequence_header(&std_sequence_header);
    let create_info = vk::VideoSessionParametersCreateInfoKHR::default()
        .video_session(session)
        .push_next(&mut av1_create_info);
    let mut parameters = vk::VideoSessionParametersKHR::null();
    unsafe {
        (video_queue_device.fp().create_video_session_parameters_khr)(
            video_queue_device.device(),
            &create_info,
            ptr::null(),
            &mut parameters,
        )
    }
    .result()
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkCreateVideoSessionParametersKHR(av1)",
        result,
    })?;

    Ok(NativeVulkanVideoSessionParameters {
        parameters,
        snapshot: NativeVulkanVideoSessionParametersSnapshot {
            codec: native_vulkan_av1_sequence_header_codec_label(sequence_header),
            source: "native-rust-av1-sequence-header-to-vulkan-std",
            max_std_vps_count: 0,
            max_std_sps_count: 1,
            max_std_pps_count: 0,
            std_vps_count: 0,
            std_sps_count: 1,
            std_pps_count: 0,
            vps_id: 0,
            sps_id: 0,
            pps_id: 0,
            profile_idc: sequence_header.seq_profile,
            level_idc: sequence_header
                .operating_points
                .first()
                .map(|point| point.seq_level_idx)
                .unwrap_or(0),
            width: sequence_header.max_frame_width,
            height: sequence_header.max_frame_height,
            created: true,
        },
    })
}

pub(super) fn native_vulkan_av1_std_color_config(
    color_config: &NativeVulkanAv1ColorConfigSnapshot,
) -> Result<vk::native::StdVideoAV1ColorConfig, NativeVulkanError> {
    Ok(vk::native::StdVideoAV1ColorConfig {
        flags: vk::native::StdVideoAV1ColorConfigFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoAV1ColorConfigFlags::new_bitfield_1(
                native_vulkan_bool_u32(color_config.mono_chrome),
                native_vulkan_bool_u32(color_config.color_range),
                native_vulkan_bool_u32(color_config.separate_uv_delta_q),
                native_vulkan_bool_u32(color_config.color_description_present_flag),
                0,
            ),
        },
        BitDepth: color_config.bit_depth,
        subsampling_x: u8::from(color_config.subsampling_x),
        subsampling_y: u8::from(color_config.subsampling_y),
        reserved1: 0,
        color_primaries: native_vulkan_av1_std_color_primaries(color_config.color_primaries)?,
        transfer_characteristics: native_vulkan_av1_std_transfer_characteristics(
            color_config.transfer_characteristics,
        )?,
        matrix_coefficients: native_vulkan_av1_std_matrix_coefficients(
            color_config.matrix_coefficients,
        )?,
        chroma_sample_position: native_vulkan_av1_std_chroma_sample_position(
            color_config.chroma_sample_position,
        )?,
    })
}

pub(super) fn native_vulkan_av1_std_timing_info(
    timing_info: &NativeVulkanAv1TimingInfoSnapshot,
) -> vk::native::StdVideoAV1TimingInfo {
    vk::native::StdVideoAV1TimingInfo {
        flags: vk::native::StdVideoAV1TimingInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoAV1TimingInfoFlags::new_bitfield_1(
                native_vulkan_bool_u32(timing_info.equal_picture_interval),
                0,
            ),
        },
        num_units_in_display_tick: timing_info.num_units_in_display_tick,
        time_scale: timing_info.time_scale,
        num_ticks_per_picture_minus_1: timing_info.num_ticks_per_picture_minus_1.unwrap_or(0),
    }
}

pub(super) fn native_vulkan_h265_std_profile_tier_level(
    profile_idc: u8,
    level_idc: u8,
    tier_flag: bool,
    progressive_source_flag: bool,
    interlaced_source_flag: bool,
    non_packed_constraint_flag: bool,
    frame_only_constraint_flag: bool,
) -> Result<vk::native::StdVideoH265ProfileTierLevel, NativeVulkanError> {
    Ok(vk::native::StdVideoH265ProfileTierLevel {
        flags: vk::native::StdVideoH265ProfileTierLevelFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265ProfileTierLevelFlags::new_bitfield_1(
                native_vulkan_bool_u32(tier_flag),
                native_vulkan_bool_u32(progressive_source_flag),
                native_vulkan_bool_u32(interlaced_source_flag),
                native_vulkan_bool_u32(non_packed_constraint_flag),
                native_vulkan_bool_u32(frame_only_constraint_flag),
            ),
            __bindgen_padding_0: [0; 3],
        },
        general_profile_idc: native_vulkan_h265_std_profile_idc(profile_idc)?,
        general_level_idc: native_vulkan_h265_std_level_idc(level_idc)?,
    })
}

pub(super) fn native_vulkan_h265_std_dec_pic_buf_mgr(
    snapshot: &NativeVulkanH265DecPicBufMgrSnapshot,
) -> vk::native::StdVideoH265DecPicBufMgr {
    vk::native::StdVideoH265DecPicBufMgr {
        max_latency_increase_plus1: snapshot.max_latency_increase_plus1,
        max_dec_pic_buffering_minus1: snapshot.max_dec_pic_buffering_minus1,
        max_num_reorder_pics: snapshot.max_num_reorder_pics,
    }
}

pub(super) fn native_vulkan_h265_std_short_term_ref_pic_sets(
    ref_pic_sets: &[NativeVulkanH265ShortTermRefPicSetSnapshot],
) -> Result<Vec<vk::native::StdVideoH265ShortTermRefPicSet>, String> {
    ref_pic_sets
        .iter()
        .map(native_vulkan_h265_std_short_term_ref_pic_set)
        .collect()
}

pub(super) fn native_vulkan_h265_std_long_term_ref_pics_sps(
    ref_pics: &[NativeVulkanH265LongTermRefPicSpsSnapshot],
) -> Result<Option<vk::native::StdVideoH265LongTermRefPicsSps>, String> {
    if ref_pics.is_empty() {
        return Ok(None);
    }
    if ref_pics.len() > 32 {
        return Err("H.265 SPS long-term reference picture set exceeds 32 refs".to_owned());
    }

    let mut used_by_curr_pic_lt_sps_flag = 0u32;
    let mut lt_ref_pic_poc_lsb_sps = [0u32; 32];
    for (index, ref_pic) in ref_pics.iter().enumerate() {
        if ref_pic.used_by_curr_pic_lt_sps_flag {
            used_by_curr_pic_lt_sps_flag |= 1u32 << index;
        }
        lt_ref_pic_poc_lsb_sps[index] = ref_pic.lt_ref_pic_poc_lsb_sps;
    }

    Ok(Some(vk::native::StdVideoH265LongTermRefPicsSps {
        used_by_curr_pic_lt_sps_flag,
        lt_ref_pic_poc_lsb_sps,
    }))
}

pub(super) fn native_vulkan_h265_std_short_term_ref_pic_set(
    ref_pic_set: &NativeVulkanH265ShortTermRefPicSetSnapshot,
) -> Result<vk::native::StdVideoH265ShortTermRefPicSet, String> {
    let num_negative_pics = native_vulkan_h265_u8(
        ref_pic_set.num_negative_pics,
        "short_term_ref_pic_set.num_negative_pics",
    )?;
    let num_positive_pics = native_vulkan_h265_u8(
        ref_pic_set.num_positive_pics,
        "short_term_ref_pic_set.num_positive_pics",
    )?;
    if num_negative_pics as usize > 16 || num_positive_pics as usize > 16 {
        return Err("H.265 short-term reference picture set exceeds 16 refs".to_owned());
    }

    let mut delta_poc_s0_minus1 = [0u16; 16];
    let mut previous_delta_poc = 0i32;
    for (index, delta_poc) in ref_pic_set.negative_delta_pocs.iter().copied().enumerate() {
        let encoded_delta = previous_delta_poc
            .checked_sub(delta_poc)
            .and_then(|value| value.checked_sub(1))
            .ok_or_else(|| "negative short-term delta POC encode underflow".to_owned())?;
        delta_poc_s0_minus1[index] = native_vulkan_h265_u16(
            u32::try_from(encoded_delta)
                .map_err(|_| "negative short-term delta POC is not encodable".to_owned())?,
            "delta_poc_s0_minus1",
        )?;
        previous_delta_poc = delta_poc;
    }

    let mut delta_poc_s1_minus1 = [0u16; 16];
    let mut previous_delta_poc = 0i32;
    for (index, delta_poc) in ref_pic_set.positive_delta_pocs.iter().copied().enumerate() {
        let encoded_delta = delta_poc
            .checked_sub(previous_delta_poc)
            .and_then(|value| value.checked_sub(1))
            .ok_or_else(|| "positive short-term delta POC encode underflow".to_owned())?;
        delta_poc_s1_minus1[index] = native_vulkan_h265_u16(
            u32::try_from(encoded_delta)
                .map_err(|_| "positive short-term delta POC is not encodable".to_owned())?,
            "delta_poc_s1_minus1",
        )?;
        previous_delta_poc = delta_poc;
    }

    let used_by_curr_pic_s0_flag =
        native_vulkan_h265_used_by_current_mask(&ref_pic_set.negative_used_by_curr_pic)?;
    let used_by_curr_pic_s1_flag =
        native_vulkan_h265_used_by_current_mask(&ref_pic_set.positive_used_by_curr_pic)?;

    Ok(vk::native::StdVideoH265ShortTermRefPicSet {
        flags: vk::native::StdVideoH265ShortTermRefPicSetFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265ShortTermRefPicSetFlags::new_bitfield_1(
                native_vulkan_bool_u32(ref_pic_set.inter_ref_pic_set_prediction_flag),
                native_vulkan_bool_u32(ref_pic_set.delta_rps_sign.unwrap_or(false)),
            ),
            __bindgen_padding_0: [0; 3],
        },
        delta_idx_minus1: ref_pic_set.delta_idx_minus1.unwrap_or(0),
        use_delta_flag: native_vulkan_h265_used_by_current_mask(&ref_pic_set.use_delta_flags)?,
        abs_delta_rps_minus1: ref_pic_set
            .abs_delta_rps_minus1
            .map(|value| native_vulkan_h265_u16(value, "abs_delta_rps_minus1"))
            .transpose()?
            .unwrap_or(0),
        used_by_curr_pic_flag: native_vulkan_h265_used_by_current_mask(
            &ref_pic_set.used_by_current_flags,
        )?,
        used_by_curr_pic_s0_flag,
        used_by_curr_pic_s1_flag,
        reserved1: 0,
        reserved2: 0,
        reserved3: 0,
        num_negative_pics,
        num_positive_pics,
        delta_poc_s0_minus1,
        delta_poc_s1_minus1,
    })
}

pub(super) fn native_vulkan_h265_used_by_current_mask(flags: &[bool]) -> Result<u16, String> {
    if flags.len() > 16 {
        return Err("H.265 short-term reference picture set has more than 16 flags".to_owned());
    }
    Ok(flags
        .iter()
        .copied()
        .enumerate()
        .fold(
            0u16,
            |mask, (index, used)| {
                if used { mask | (1u16 << index) } else { mask }
            },
        ))
}

pub(super) fn native_vulkan_h265_std_vui(
    vui: &NativeVulkanH265VuiSnapshot,
) -> Result<vk::native::StdVideoH265SequenceParameterSetVui, NativeVulkanError> {
    if vui.vui_hrd_parameters_present_flag {
        return Err(NativeVulkanError::Video(
            "H.265 VUI HRD parameters are not converted to Vulkan STD yet".to_owned(),
        ));
    }
    Ok(vk::native::StdVideoH265SequenceParameterSetVui {
        flags: vk::native::StdVideoH265SpsVuiFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265SpsVuiFlags::new_bitfield_1(
                native_vulkan_bool_u32(vui.aspect_ratio_info_present_flag),
                native_vulkan_bool_u32(vui.overscan_info_present_flag),
                native_vulkan_bool_u32(vui.overscan_appropriate_flag),
                native_vulkan_bool_u32(vui.video_signal_type_present_flag),
                native_vulkan_bool_u32(vui.video_full_range_flag),
                native_vulkan_bool_u32(vui.colour_description_present_flag),
                native_vulkan_bool_u32(vui.chroma_loc_info_present_flag),
                native_vulkan_bool_u32(vui.neutral_chroma_indication_flag),
                native_vulkan_bool_u32(vui.field_seq_flag),
                native_vulkan_bool_u32(vui.frame_field_info_present_flag),
                native_vulkan_bool_u32(vui.default_display_window_flag),
                native_vulkan_bool_u32(vui.vui_timing_info_present_flag),
                native_vulkan_bool_u32(vui.vui_poc_proportional_to_timing_flag),
                native_vulkan_bool_u32(vui.vui_hrd_parameters_present_flag),
                native_vulkan_bool_u32(vui.bitstream_restriction_flag),
                native_vulkan_bool_u32(vui.tiles_fixed_structure_flag),
                native_vulkan_bool_u32(vui.motion_vectors_over_pic_boundaries_flag),
                native_vulkan_bool_u32(vui.restricted_ref_pic_lists_flag),
            ),
            __bindgen_padding_0: 0,
        },
        aspect_ratio_idc: vui.aspect_ratio_idc,
        sar_width: vui.sar_width,
        sar_height: vui.sar_height,
        video_format: vui.video_format,
        colour_primaries: vui.colour_primaries,
        transfer_characteristics: vui.transfer_characteristics,
        matrix_coeffs: vui.matrix_coeffs,
        chroma_sample_loc_type_top_field: vui.chroma_sample_loc_type_top_field,
        chroma_sample_loc_type_bottom_field: vui.chroma_sample_loc_type_bottom_field,
        reserved1: 0,
        reserved2: 0,
        def_disp_win_left_offset: vui.def_disp_win_left_offset,
        def_disp_win_right_offset: vui.def_disp_win_right_offset,
        def_disp_win_top_offset: vui.def_disp_win_top_offset,
        def_disp_win_bottom_offset: vui.def_disp_win_bottom_offset,
        vui_num_units_in_tick: vui.vui_num_units_in_tick,
        vui_time_scale: vui.vui_time_scale,
        vui_num_ticks_poc_diff_one_minus1: vui.vui_num_ticks_poc_diff_one_minus1,
        min_spatial_segmentation_idc: vui.min_spatial_segmentation_idc,
        reserved3: 0,
        max_bytes_per_pic_denom: vui.max_bytes_per_pic_denom,
        max_bits_per_min_cu_denom: vui.max_bits_per_min_cu_denom,
        log2_max_mv_length_horizontal: vui.log2_max_mv_length_horizontal,
        log2_max_mv_length_vertical: vui.log2_max_mv_length_vertical,
        pHrdParameters: ptr::null(),
    })
}

pub(super) fn native_vulkan_h265_std_chroma_format_idc(
    chroma_format_idc: u32,
) -> Result<vk::native::StdVideoH265ChromaFormatIdc, NativeVulkanError> {
    match chroma_format_idc {
        1 => Ok(vk::native::StdVideoH265ChromaFormatIdc_STD_VIDEO_H265_CHROMA_FORMAT_IDC_420),
        other => Err(NativeVulkanError::Video(format!(
            "unsupported H.265 chroma_format_idc for Vulkan STD session parameters: {other}"
        ))),
    }
}

pub(super) fn native_vulkan_h265_std_profile_idc(
    profile_idc: u8,
) -> Result<vk::native::StdVideoH265ProfileIdc, NativeVulkanError> {
    match profile_idc {
        1 => Ok(vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN),
        2 => Ok(vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN_10),
        other => Err(NativeVulkanError::Video(format!(
            "unsupported H.265 profile_idc for Vulkan STD session parameters: {other}"
        ))),
    }
}

pub(super) fn native_vulkan_h265_std_level_idc(
    level_idc: u8,
) -> Result<vk::native::StdVideoH265LevelIdc, NativeVulkanError> {
    match level_idc {
        30 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_1_0),
        60 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_0),
        63 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_1),
        90 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_0),
        93 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_1),
        120 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_0),
        123 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_1),
        150 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_0),
        153 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_1),
        156 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_2),
        180 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_0),
        183 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_1),
        186 => Ok(vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_2),
        other => Err(NativeVulkanError::Video(format!(
            "unsupported H.265 level_idc for Vulkan STD session parameters: {other}"
        ))),
    }
}

pub(super) fn native_vulkan_av1_std_profile(
    profile: u8,
) -> Result<vk::native::StdVideoAV1Profile, NativeVulkanError> {
    match profile {
        0 => Ok(vk::native::StdVideoAV1Profile_STD_VIDEO_AV1_PROFILE_MAIN),
        1 => Ok(vk::native::StdVideoAV1Profile_STD_VIDEO_AV1_PROFILE_HIGH),
        2 => Ok(vk::native::StdVideoAV1Profile_STD_VIDEO_AV1_PROFILE_PROFESSIONAL),
        other => Err(NativeVulkanError::Video(format!(
            "unsupported AV1 profile for Vulkan STD session parameters: {other}"
        ))),
    }
}

pub(super) fn native_vulkan_av1_std_color_primaries(
    value: u8,
) -> Result<vk::native::StdVideoAV1ColorPrimaries, NativeVulkanError> {
    match value {
        1 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_BT_709),
        2 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_BT_UNSPECIFIED),
        4 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_BT_470_M),
        5 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_BT_470_B_G),
        6 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_BT_601),
        7 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_SMPTE_240),
        8 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_GENERIC_FILM),
        9 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_BT_2020),
        10 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_XYZ),
        11 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_SMPTE_431),
        12 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_SMPTE_432),
        22 => Ok(vk::native::StdVideoAV1ColorPrimaries_STD_VIDEO_AV1_COLOR_PRIMARIES_EBU_3213),
        other => Err(NativeVulkanError::Video(format!(
            "unsupported AV1 color_primaries for Vulkan STD session parameters: {other}"
        ))),
    }
}

pub(super) fn native_vulkan_av1_std_transfer_characteristics(
    value: u8,
) -> Result<vk::native::StdVideoAV1TransferCharacteristics, NativeVulkanError> {
    match value {
        0 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_RESERVED_0),
        1 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_709),
        2 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_UNSPECIFIED),
        3 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_RESERVED_3),
        4 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_470_M),
        5 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_470_B_G),
        6 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_601),
        7 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_SMPTE_240),
        8 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_LINEAR),
        9 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_LOG_100),
        10 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_LOG_100_SQRT10),
        11 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_IEC_61966),
        12 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_1361),
        13 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_SRGB),
        14 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_2020_10_BIT),
        15 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_BT_2020_12_BIT),
        16 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_SMPTE_2084),
        17 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_SMPTE_428),
        18 => Ok(vk::native::StdVideoAV1TransferCharacteristics_STD_VIDEO_AV1_TRANSFER_CHARACTERISTICS_HLG),
        other => Err(NativeVulkanError::Video(format!(
            "unsupported AV1 transfer_characteristics for Vulkan STD session parameters: {other}"
        ))),
    }
}

pub(super) fn native_vulkan_av1_std_matrix_coefficients(
    value: u8,
) -> Result<vk::native::StdVideoAV1MatrixCoefficients, NativeVulkanError> {
    match value {
        0 => Ok(
            vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_IDENTITY,
        ),
        1 => Ok(vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_BT_709),
        2 => Ok(
            vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_UNSPECIFIED,
        ),
        3 => Ok(
            vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_RESERVED_3,
        ),
        4 => Ok(vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_FCC),
        5 => Ok(
            vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_BT_470_B_G,
        ),
        6 => Ok(vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_BT_601),
        7 => Ok(
            vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_SMPTE_240,
        ),
        8 => Ok(
            vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_SMPTE_YCGCO,
        ),
        9 => Ok(
            vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_BT_2020_NCL,
        ),
        10 => Ok(
            vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_BT_2020_CL,
        ),
        11 => Ok(
            vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_SMPTE_2085,
        ),
        12 => Ok(
            vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_CHROMAT_NCL,
        ),
        13 => Ok(
            vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_CHROMAT_CL,
        ),
        14 => Ok(vk::native::StdVideoAV1MatrixCoefficients_STD_VIDEO_AV1_MATRIX_COEFFICIENTS_ICTCP),
        other => Err(NativeVulkanError::Video(format!(
            "unsupported AV1 matrix_coefficients for Vulkan STD session parameters: {other}"
        ))),
    }
}

pub(super) fn native_vulkan_av1_std_chroma_sample_position(
    value: u8,
) -> Result<vk::native::StdVideoAV1ChromaSamplePosition, NativeVulkanError> {
    match value {
        0 => Ok(vk::native::StdVideoAV1ChromaSamplePosition_STD_VIDEO_AV1_CHROMA_SAMPLE_POSITION_UNKNOWN),
        1 => Ok(vk::native::StdVideoAV1ChromaSamplePosition_STD_VIDEO_AV1_CHROMA_SAMPLE_POSITION_VERTICAL),
        2 => Ok(vk::native::StdVideoAV1ChromaSamplePosition_STD_VIDEO_AV1_CHROMA_SAMPLE_POSITION_COLOCATED),
        3 => Ok(vk::native::StdVideoAV1ChromaSamplePosition_STD_VIDEO_AV1_CHROMA_SAMPLE_POSITION_RESERVED),
        other => Err(NativeVulkanError::Video(format!(
            "unsupported AV1 chroma_sample_position for Vulkan STD session parameters: {other}"
        ))),
    }
}

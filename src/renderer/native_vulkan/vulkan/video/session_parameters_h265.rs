use std::ptr;

use crate::renderer::native_vulkan::{
    NativeVulkanH265DecPicBufMgrSnapshot, NativeVulkanH265LongTermRefPicSpsSnapshot,
    NativeVulkanH265ParameterSetSnapshot, NativeVulkanH265ShortTermRefPicSetSnapshot,
    NativeVulkanH265VuiSnapshot, NativeVulkanVideoSessionCodec,
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

const H265_SESSION_PARAMETERS_SOURCE: &str = "native-rust-h265-vps-sps-pps-to-vulkanalia-std";

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaH265InlineSessionParameters
{
    _vps_profile_tier_level: Box<vk::video::StdVideoH265ProfileTierLevel>,
    _sps_profile_tier_level: Box<vk::video::StdVideoH265ProfileTierLevel>,
    _vps_dec_pic_buf_mgr: Box<vk::video::StdVideoH265DecPicBufMgr>,
    _sps_dec_pic_buf_mgr: Box<vk::video::StdVideoH265DecPicBufMgr>,
    _sps_vui: Option<Box<vk::video::StdVideoH265SequenceParameterSetVui>>,
    _sps_short_term_ref_pic_sets: Vec<vk::video::StdVideoH265ShortTermRefPicSet>,
    _sps_long_term_ref_pics: Option<Box<vk::video::StdVideoH265LongTermRefPicsSps>>,
    vps: [vk::video::StdVideoH265VideoParameterSet; 1],
    sps: [vk::video::StdVideoH265SequenceParameterSet; 1],
    pps: [vk::video::StdVideoH265PictureParameterSet; 1],
}

impl NativeVulkanVulkanaliaH265InlineSessionParameters {
    pub(in crate::renderer::native_vulkan::vulkan) fn inline_info(
        &self,
    ) -> vk::VideoDecodeH265InlineSessionParametersInfoKHR {
        vk::VideoDecodeH265InlineSessionParametersInfoKHR::builder()
            .std_vps(&self.vps[0])
            .std_sps(&self.sps[0])
            .std_pps(&self.pps[0])
            .build()
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_h265_inline_session_parameters(
    codec: NativeVulkanVideoSessionCodec,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<NativeVulkanVulkanaliaH265InlineSessionParameters, String> {
    native_vulkan_vulkanalia_validate_h265_session_parameter_inputs(codec, parameter_sets)?;
    native_vulkan_vulkanalia_h265_inline_session_parameters_inner(parameter_sets)
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_smoke_create_h265_video_session_parameters(
    device: &Device,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot {
    match native_vulkan_vulkanalia_create_h265_video_session_parameters(
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
        Err(err) => native_vulkan_vulkanalia_h265_session_parameters_error(codec, err),
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_h265_video_session_parameters(
    device: &Device,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<VulkanaliaVideoSessionParameters, String> {
    native_vulkan_vulkanalia_validate_h265_session_parameter_inputs(codec, parameter_sets)?;

    native_vulkan_vulkanalia_smoke_create_h265_video_session_parameters_inner(
        device,
        session,
        codec,
        parameter_sets,
    )
}

fn native_vulkan_vulkanalia_validate_h265_session_parameter_inputs(
    codec: NativeVulkanVideoSessionCodec,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<(), String> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) {
        return Err("Vulkanalia real session parameters currently support H.265 only".to_owned());
    }
    if !parameter_sets.vulkan_std_session_parameters_ready {
        return Err(
            "H.265 parameter sets are not in the first supported Vulkanalia STD subset".to_owned(),
        );
    }
    native_vulkan_vulkanalia_validate_h265_requested_profile_bit_depth(
        codec,
        parameter_sets.vps.profile_idc,
        parameter_sets.sps.profile_idc,
        parameter_sets.sps.bit_depth_luma_minus8,
        parameter_sets.sps.bit_depth_chroma_minus8,
    )?;

    Ok(())
}

fn native_vulkan_vulkanalia_validate_h265_requested_profile_bit_depth(
    codec: NativeVulkanVideoSessionCodec,
    vps_profile_idc: u8,
    sps_profile_idc: u8,
    bit_depth_luma_minus8: u32,
    bit_depth_chroma_minus8: u32,
) -> Result<(), String> {
    let (expected_profile_idc, expected_bit_depth_minus8, label) = match codec {
        NativeVulkanVideoSessionCodec::H265Main8 => (1, 0, "h265-main-8"),
        NativeVulkanVideoSessionCodec::H265Main10 => (2, 2, "h265-main-10"),
        _ => {
            return Err(
                "Vulkanalia H.265 profile/bit-depth validation requires an H.265 codec".to_owned(),
            );
        }
    };
    if vps_profile_idc != expected_profile_idc
        || sps_profile_idc != expected_profile_idc
        || bit_depth_luma_minus8 != expected_bit_depth_minus8
        || bit_depth_chroma_minus8 != expected_bit_depth_minus8
    {
        return Err(format!(
            "H.265 stream profile/bit-depth does not match requested {label}: VPS profile_idc={vps_profile_idc}, SPS profile_idc={sps_profile_idc}, SPS bit_depth_luma_minus8={bit_depth_luma_minus8}, SPS bit_depth_chroma_minus8={bit_depth_chroma_minus8}"
        ));
    }
    Ok(())
}

fn native_vulkan_vulkanalia_smoke_create_h265_video_session_parameters_inner(
    device: &Device,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<VulkanaliaVideoSessionParameters, String> {
    let inline_parameters =
        native_vulkan_vulkanalia_h265_inline_session_parameters_inner(parameter_sets)?;
    let vps = &inline_parameters.vps;
    let sps = &inline_parameters.sps;
    let pps = &inline_parameters.pps;
    let add_info = vk::VideoDecodeH265SessionParametersAddInfoKHR::builder()
        .std_vp_ss(vps)
        .std_sp_ss(sps)
        .std_pp_ss(pps)
        .build();
    let max_std_vps_count = 32;
    let max_std_sps_count = 32;
    let max_std_pps_count = 64;
    let mut h265_create_info = vk::VideoDecodeH265SessionParametersCreateInfoKHR::builder()
        .max_std_vps_count(max_std_vps_count)
        .max_std_sps_count(max_std_sps_count)
        .max_std_pps_count(max_std_pps_count)
        .parameters_add_info(&add_info)
        .build();
    let create_info = vk::VideoSessionParametersCreateInfoKHR::builder()
        .video_session(session)
        .push_next(&mut h265_create_info)
        .build();

    native_vulkan_vulkanalia_create_video_session_parameters(
        device,
        &create_info,
        NativeVulkanVulkanaliaVideoSessionParametersSnapshot {
            codec: vulkanalia_session_parameters_codec_label(codec),
            source: H265_SESSION_PARAMETERS_SOURCE,
            max_std_vps_count,
            max_std_sps_count,
            max_std_pps_count,
            std_vps_count: vps.len() as u32,
            std_sps_count: sps.len() as u32,
            std_pps_count: pps.len() as u32,
        },
        "vulkanalia real h265 session parameters",
    )
    .map_err(|err| err.error)
}

fn native_vulkan_vulkanalia_h265_inline_session_parameters_inner(
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<NativeVulkanVulkanaliaH265InlineSessionParameters, String> {
    let vps_profile_tier_level = Box::new(native_vulkan_vulkanalia_h265_std_profile_tier_level(
        parameter_sets.vps.profile_idc,
        parameter_sets.vps.level_idc,
        parameter_sets.vps.tier_flag,
        parameter_sets.vps.progressive_source_flag,
        parameter_sets.vps.interlaced_source_flag,
        parameter_sets.vps.non_packed_constraint_flag,
        parameter_sets.vps.frame_only_constraint_flag,
    )?);
    let sps_profile_tier_level = Box::new(native_vulkan_vulkanalia_h265_std_profile_tier_level(
        parameter_sets.sps.profile_idc,
        parameter_sets.sps.level_idc,
        parameter_sets.sps.tier_flag,
        parameter_sets.sps.progressive_source_flag,
        parameter_sets.sps.interlaced_source_flag,
        parameter_sets.sps.non_packed_constraint_flag,
        parameter_sets.sps.frame_only_constraint_flag,
    )?);
    let vps_dec_pic_buf_mgr = Box::new(native_vulkan_vulkanalia_h265_std_dec_pic_buf_mgr(
        &parameter_sets.vps.dec_pic_buf_mgr,
    ));
    let sps_dec_pic_buf_mgr = Box::new(native_vulkan_vulkanalia_h265_std_dec_pic_buf_mgr(
        &parameter_sets.sps.dec_pic_buf_mgr,
    ));
    let sps_vui = parameter_sets
        .sps
        .vui
        .as_ref()
        .map(native_vulkan_vulkanalia_h265_std_vui)
        .transpose()?
        .map(Box::new);
    let sps_vui_ptr = sps_vui
        .as_deref()
        .map(|vui| vui as *const vk::video::StdVideoH265SequenceParameterSetVui)
        .unwrap_or_else(ptr::null);
    let sps_short_term_ref_pic_sets = native_vulkan_vulkanalia_h265_std_short_term_ref_pic_sets(
        &parameter_sets.sps.short_term_ref_pic_sets,
    )?;
    let sps_short_term_ref_pic_sets_ptr = if sps_short_term_ref_pic_sets.is_empty() {
        ptr::null()
    } else {
        sps_short_term_ref_pic_sets.as_ptr()
    };
    let sps_long_term_ref_pics = native_vulkan_vulkanalia_h265_std_long_term_ref_pics_sps(
        &parameter_sets.sps.long_term_ref_pics_sps,
    )?
    .map(Box::new);
    let sps_long_term_ref_pics_ptr = sps_long_term_ref_pics
        .as_deref()
        .map(|ref_pics| ref_pics as *const vk::video::StdVideoH265LongTermRefPicsSps)
        .unwrap_or_else(ptr::null);

    let vps = [vk::video::StdVideoH265VideoParameterSet {
        flags: vk::video::StdVideoH265VpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoH265VpsFlags::new_bitfield_1(
                h265_bool_u32(parameter_sets.vps.temporal_id_nesting_flag),
                h265_bool_u32(parameter_sets.vps.sub_layer_ordering_info_present_flag),
                h265_bool_u32(parameter_sets.vps.timing_info_present_flag),
                h265_bool_u32(parameter_sets.vps.poc_proportional_to_timing_flag),
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
        pDecPicBufMgr: &*vps_dec_pic_buf_mgr,
        pHrdParameters: ptr::null(),
        pProfileTierLevel: &*vps_profile_tier_level,
    }];

    let sps = [vk::video::StdVideoH265SequenceParameterSet {
        flags: vk::video::StdVideoH265SpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoH265SpsFlags::new_bitfield_1(
                h265_bool_u32(parameter_sets.sps.temporal_id_nesting_flag),
                h265_bool_u32(parameter_sets.sps.separate_colour_plane_flag),
                h265_bool_u32(parameter_sets.sps.conformance_window_flag),
                h265_bool_u32(parameter_sets.sps.sub_layer_ordering_info_present_flag),
                h265_bool_u32(parameter_sets.sps.scaling_list_enabled_flag),
                h265_bool_u32(parameter_sets.sps.sps_scaling_list_data_present_flag),
                h265_bool_u32(parameter_sets.sps.amp_enabled_flag),
                h265_bool_u32(parameter_sets.sps.sample_adaptive_offset_enabled_flag),
                h265_bool_u32(parameter_sets.sps.pcm_enabled_flag),
                h265_bool_u32(parameter_sets.sps.pcm_loop_filter_disabled_flag),
                h265_bool_u32(parameter_sets.sps.long_term_ref_pics_present_flag),
                h265_bool_u32(parameter_sets.sps.temporal_mvp_enabled_flag),
                h265_bool_u32(parameter_sets.sps.strong_intra_smoothing_enabled_flag),
                h265_bool_u32(parameter_sets.sps.vui_parameters_present_flag),
                h265_bool_u32(parameter_sets.sps.sps_extension_present_flag),
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
        chroma_format_idc: native_vulkan_vulkanalia_h265_std_chroma_format_idc(
            parameter_sets.sps.chroma_format_idc,
        )?,
        pic_width_in_luma_samples: parameter_sets.sps.width,
        pic_height_in_luma_samples: parameter_sets.sps.height,
        sps_video_parameter_set_id: parameter_sets.sps.vps_id,
        sps_max_sub_layers_minus1: parameter_sets.sps.max_sub_layers_minus1,
        sps_seq_parameter_set_id: h265_u8(parameter_sets.sps.id, "sps_seq_parameter_set_id")?,
        bit_depth_luma_minus8: h265_u8(
            parameter_sets.sps.bit_depth_luma_minus8,
            "bit_depth_luma_minus8",
        )?,
        bit_depth_chroma_minus8: h265_u8(
            parameter_sets.sps.bit_depth_chroma_minus8,
            "bit_depth_chroma_minus8",
        )?,
        log2_max_pic_order_cnt_lsb_minus4: h265_u8(
            parameter_sets.sps.log2_max_pic_order_cnt_lsb_minus4,
            "log2_max_pic_order_cnt_lsb_minus4",
        )?,
        log2_min_luma_coding_block_size_minus3: h265_u8(
            parameter_sets.sps.log2_min_luma_coding_block_size_minus3,
            "log2_min_luma_coding_block_size_minus3",
        )?,
        log2_diff_max_min_luma_coding_block_size: h265_u8(
            parameter_sets.sps.log2_diff_max_min_luma_coding_block_size,
            "log2_diff_max_min_luma_coding_block_size",
        )?,
        log2_min_luma_transform_block_size_minus2: h265_u8(
            parameter_sets.sps.log2_min_luma_transform_block_size_minus2,
            "log2_min_luma_transform_block_size_minus2",
        )?,
        log2_diff_max_min_luma_transform_block_size: h265_u8(
            parameter_sets
                .sps
                .log2_diff_max_min_luma_transform_block_size,
            "log2_diff_max_min_luma_transform_block_size",
        )?,
        max_transform_hierarchy_depth_inter: h265_u8(
            parameter_sets.sps.max_transform_hierarchy_depth_inter,
            "max_transform_hierarchy_depth_inter",
        )?,
        max_transform_hierarchy_depth_intra: h265_u8(
            parameter_sets.sps.max_transform_hierarchy_depth_intra,
            "max_transform_hierarchy_depth_intra",
        )?,
        num_short_term_ref_pic_sets: h265_u8(
            parameter_sets.sps.num_short_term_ref_pic_sets,
            "num_short_term_ref_pic_sets",
        )?,
        num_long_term_ref_pics_sps: h265_u8(
            parameter_sets.sps.long_term_ref_pics_sps.len() as u32,
            "num_long_term_ref_pics_sps",
        )?,
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
        pProfileTierLevel: &*sps_profile_tier_level,
        pDecPicBufMgr: &*sps_dec_pic_buf_mgr,
        pScalingLists: ptr::null(),
        pShortTermRefPicSet: sps_short_term_ref_pic_sets_ptr,
        pLongTermRefPicsSps: sps_long_term_ref_pics_ptr,
        pSequenceParameterSetVui: sps_vui_ptr,
        pPredictorPaletteEntries: ptr::null(),
    }];

    let pps = [vk::video::StdVideoH265PictureParameterSet {
        flags: vk::video::StdVideoH265PpsFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoH265PpsFlags::new_bitfield_1(
                h265_bool_u32(parameter_sets.pps.dependent_slice_segments_enabled_flag),
                h265_bool_u32(parameter_sets.pps.output_flag_present_flag),
                h265_bool_u32(parameter_sets.pps.sign_data_hiding_enabled_flag),
                h265_bool_u32(parameter_sets.pps.cabac_init_present_flag),
                h265_bool_u32(parameter_sets.pps.constrained_intra_pred_flag),
                h265_bool_u32(parameter_sets.pps.transform_skip_enabled_flag),
                h265_bool_u32(parameter_sets.pps.cu_qp_delta_enabled_flag),
                h265_bool_u32(parameter_sets.pps.slice_chroma_qp_offsets_present_flag),
                h265_bool_u32(parameter_sets.pps.weighted_pred_flag),
                h265_bool_u32(parameter_sets.pps.weighted_bipred_flag),
                h265_bool_u32(parameter_sets.pps.transquant_bypass_enabled_flag),
                h265_bool_u32(parameter_sets.pps.tiles_enabled_flag),
                h265_bool_u32(parameter_sets.pps.entropy_coding_sync_enabled_flag),
                h265_bool_u32(parameter_sets.pps.uniform_spacing_flag),
                h265_bool_u32(
                    parameter_sets
                        .pps
                        .loop_filter_across_tiles_enabled_flag
                        .unwrap_or(false),
                ),
                h265_bool_u32(parameter_sets.pps.loop_filter_across_slices_enabled_flag),
                h265_bool_u32(parameter_sets.pps.deblocking_filter_control_present_flag),
                h265_bool_u32(
                    parameter_sets
                        .pps
                        .deblocking_filter_override_enabled_flag
                        .unwrap_or(false),
                ),
                h265_bool_u32(
                    parameter_sets
                        .pps
                        .pps_deblocking_filter_disabled_flag
                        .unwrap_or(false),
                ),
                h265_bool_u32(parameter_sets.pps.pps_scaling_list_data_present_flag),
                h265_bool_u32(parameter_sets.pps.lists_modification_present_flag),
                h265_bool_u32(
                    parameter_sets
                        .pps
                        .slice_segment_header_extension_present_flag,
                ),
                h265_bool_u32(parameter_sets.pps.pps_extension_present_flag),
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
        pps_pic_parameter_set_id: h265_u8(parameter_sets.pps.id, "pps_pic_parameter_set_id")?,
        pps_seq_parameter_set_id: h265_u8(parameter_sets.pps.sps_id, "pps_seq_parameter_set_id")?,
        sps_video_parameter_set_id: parameter_sets.sps.vps_id,
        num_extra_slice_header_bits: parameter_sets.pps.num_extra_slice_header_bits,
        num_ref_idx_l0_default_active_minus1: h265_u8(
            parameter_sets.pps.num_ref_idx_l0_default_active_minus1,
            "num_ref_idx_l0_default_active_minus1",
        )?,
        num_ref_idx_l1_default_active_minus1: h265_u8(
            parameter_sets.pps.num_ref_idx_l1_default_active_minus1,
            "num_ref_idx_l1_default_active_minus1",
        )?,
        init_qp_minus26: h265_i8(parameter_sets.pps.init_qp_minus26, "init_qp_minus26")?,
        diff_cu_qp_delta_depth: h265_u8(
            parameter_sets.pps.diff_cu_qp_delta_depth.unwrap_or(0),
            "diff_cu_qp_delta_depth",
        )?,
        pps_cb_qp_offset: h265_i8(parameter_sets.pps.cb_qp_offset, "pps_cb_qp_offset")?,
        pps_cr_qp_offset: h265_i8(parameter_sets.pps.cr_qp_offset, "pps_cr_qp_offset")?,
        pps_beta_offset_div2: h265_i8(
            parameter_sets.pps.pps_beta_offset_div2,
            "pps_beta_offset_div2",
        )?,
        pps_tc_offset_div2: h265_i8(parameter_sets.pps.pps_tc_offset_div2, "pps_tc_offset_div2")?,
        log2_parallel_merge_level_minus2: h265_u8(
            parameter_sets.pps.log2_parallel_merge_level_minus2,
            "log2_parallel_merge_level_minus2",
        )?,
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
        num_tile_columns_minus1: h265_u8(
            parameter_sets.pps.num_tile_columns_minus1,
            "num_tile_columns_minus1",
        )?,
        num_tile_rows_minus1: h265_u8(
            parameter_sets.pps.num_tile_rows_minus1,
            "num_tile_rows_minus1",
        )?,
        reserved1: 0,
        reserved2: 0,
        column_width_minus1: [0; 19],
        row_height_minus1: [0; 21],
        reserved3: 0,
        pScalingLists: ptr::null(),
        pPredictorPaletteEntries: ptr::null(),
    }];

    Ok(NativeVulkanVulkanaliaH265InlineSessionParameters {
        _vps_profile_tier_level: vps_profile_tier_level,
        _sps_profile_tier_level: sps_profile_tier_level,
        _vps_dec_pic_buf_mgr: vps_dec_pic_buf_mgr,
        _sps_dec_pic_buf_mgr: sps_dec_pic_buf_mgr,
        _sps_vui: sps_vui,
        _sps_short_term_ref_pic_sets: sps_short_term_ref_pic_sets,
        _sps_long_term_ref_pics: sps_long_term_ref_pics,
        vps,
        sps,
        pps,
    })
}

fn native_vulkan_vulkanalia_h265_session_parameters_error(
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
            source: H265_SESSION_PARAMETERS_SOURCE,
            max_std_vps_count: 32,
            max_std_sps_count: 32,
            max_std_pps_count: 64,
            std_vps_count: 0,
            std_sps_count: 0,
            std_pps_count: 0,
        },
    }
}

fn native_vulkan_vulkanalia_h265_std_profile_tier_level(
    profile_idc: u8,
    level_idc: u8,
    tier_flag: bool,
    progressive_source_flag: bool,
    interlaced_source_flag: bool,
    non_packed_constraint_flag: bool,
    frame_only_constraint_flag: bool,
) -> Result<vk::video::StdVideoH265ProfileTierLevel, String> {
    Ok(vk::video::StdVideoH265ProfileTierLevel {
        flags: vk::video::StdVideoH265ProfileTierLevelFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoH265ProfileTierLevelFlags::new_bitfield_1(
                h265_bool_u32(tier_flag),
                h265_bool_u32(progressive_source_flag),
                h265_bool_u32(interlaced_source_flag),
                h265_bool_u32(non_packed_constraint_flag),
                h265_bool_u32(frame_only_constraint_flag),
            ),
            __bindgen_padding_0: [0; 3],
        },
        general_profile_idc: native_vulkan_vulkanalia_h265_std_profile_idc(profile_idc)?,
        general_level_idc: native_vulkan_vulkanalia_h265_std_level_idc(level_idc)?,
    })
}

fn native_vulkan_vulkanalia_h265_std_dec_pic_buf_mgr(
    snapshot: &NativeVulkanH265DecPicBufMgrSnapshot,
) -> vk::video::StdVideoH265DecPicBufMgr {
    vk::video::StdVideoH265DecPicBufMgr {
        max_latency_increase_plus1: snapshot.max_latency_increase_plus1,
        max_dec_pic_buffering_minus1: snapshot.max_dec_pic_buffering_minus1,
        max_num_reorder_pics: snapshot.max_num_reorder_pics,
    }
}

fn native_vulkan_vulkanalia_h265_std_short_term_ref_pic_sets(
    ref_pic_sets: &[NativeVulkanH265ShortTermRefPicSetSnapshot],
) -> Result<Vec<vk::video::StdVideoH265ShortTermRefPicSet>, String> {
    ref_pic_sets
        .iter()
        .map(native_vulkan_vulkanalia_h265_std_short_term_ref_pic_set)
        .collect()
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_h265_std_long_term_ref_pics_sps(
    ref_pics: &[NativeVulkanH265LongTermRefPicSpsSnapshot],
) -> Result<Option<vk::video::StdVideoH265LongTermRefPicsSps>, String> {
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

    Ok(Some(vk::video::StdVideoH265LongTermRefPicsSps {
        used_by_curr_pic_lt_sps_flag,
        lt_ref_pic_poc_lsb_sps,
    }))
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_h265_std_short_term_ref_pic_set(
    ref_pic_set: &NativeVulkanH265ShortTermRefPicSetSnapshot,
) -> Result<vk::video::StdVideoH265ShortTermRefPicSet, String> {
    let num_negative_pics = h265_u8(
        ref_pic_set.num_negative_pics,
        "short_term_ref_pic_set.num_negative_pics",
    )?;
    let num_positive_pics = h265_u8(
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
        delta_poc_s0_minus1[index] = h265_u16(
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
        delta_poc_s1_minus1[index] = h265_u16(
            u32::try_from(encoded_delta)
                .map_err(|_| "positive short-term delta POC is not encodable".to_owned())?,
            "delta_poc_s1_minus1",
        )?;
        previous_delta_poc = delta_poc;
    }

    Ok(vk::video::StdVideoH265ShortTermRefPicSet {
        flags: vk::video::StdVideoH265ShortTermRefPicSetFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoH265ShortTermRefPicSetFlags::new_bitfield_1(
                h265_bool_u32(ref_pic_set.inter_ref_pic_set_prediction_flag),
                h265_bool_u32(ref_pic_set.delta_rps_sign.unwrap_or(false)),
            ),
            __bindgen_padding_0: [0; 3],
        },
        delta_idx_minus1: ref_pic_set.delta_idx_minus1.unwrap_or(0),
        use_delta_flag: native_vulkan_vulkanalia_h265_used_by_current_mask(
            &ref_pic_set.use_delta_flags,
        )?,
        abs_delta_rps_minus1: ref_pic_set
            .abs_delta_rps_minus1
            .map(|value| h265_u16(value, "abs_delta_rps_minus1"))
            .transpose()?
            .unwrap_or(0),
        used_by_curr_pic_flag: native_vulkan_vulkanalia_h265_used_by_current_mask(
            &ref_pic_set.used_by_current_flags,
        )?,
        used_by_curr_pic_s0_flag: native_vulkan_vulkanalia_h265_used_by_current_mask(
            &ref_pic_set.negative_used_by_curr_pic,
        )?,
        used_by_curr_pic_s1_flag: native_vulkan_vulkanalia_h265_used_by_current_mask(
            &ref_pic_set.positive_used_by_curr_pic,
        )?,
        reserved1: 0,
        reserved2: 0,
        reserved3: 0,
        num_negative_pics,
        num_positive_pics,
        delta_poc_s0_minus1,
        delta_poc_s1_minus1,
    })
}

fn native_vulkan_vulkanalia_h265_used_by_current_mask(flags: &[bool]) -> Result<u16, String> {
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

fn native_vulkan_vulkanalia_h265_std_vui(
    vui: &NativeVulkanH265VuiSnapshot,
) -> Result<vk::video::StdVideoH265SequenceParameterSetVui, String> {
    if vui.vui_hrd_parameters_present_flag {
        return Err("H.265 VUI HRD parameters are not converted to Vulkanalia STD yet".to_owned());
    }
    Ok(vk::video::StdVideoH265SequenceParameterSetVui {
        flags: vk::video::StdVideoH265SpsVuiFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoH265SpsVuiFlags::new_bitfield_1(
                h265_bool_u32(vui.aspect_ratio_info_present_flag),
                h265_bool_u32(vui.overscan_info_present_flag),
                h265_bool_u32(vui.overscan_appropriate_flag),
                h265_bool_u32(vui.video_signal_type_present_flag),
                h265_bool_u32(vui.video_full_range_flag),
                h265_bool_u32(vui.colour_description_present_flag),
                h265_bool_u32(vui.chroma_loc_info_present_flag),
                h265_bool_u32(vui.neutral_chroma_indication_flag),
                h265_bool_u32(vui.field_seq_flag),
                h265_bool_u32(vui.frame_field_info_present_flag),
                h265_bool_u32(vui.default_display_window_flag),
                h265_bool_u32(vui.vui_timing_info_present_flag),
                h265_bool_u32(vui.vui_poc_proportional_to_timing_flag),
                h265_bool_u32(vui.vui_hrd_parameters_present_flag),
                h265_bool_u32(vui.bitstream_restriction_flag),
                h265_bool_u32(vui.tiles_fixed_structure_flag),
                h265_bool_u32(vui.motion_vectors_over_pic_boundaries_flag),
                h265_bool_u32(vui.restricted_ref_pic_lists_flag),
            ),
            __bindgen_padding_0: 0,
        },
        aspect_ratio_idc: native_vulkan_vulkanalia_h265_std_aspect_ratio_idc(vui.aspect_ratio_idc)?,
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

fn native_vulkan_vulkanalia_h265_std_chroma_format_idc(
    chroma_format_idc: u32,
) -> Result<vk::video::StdVideoH265ChromaFormatIdc, String> {
    match chroma_format_idc {
        1 => Ok(vk::video::STD_VIDEO_H265_CHROMA_FORMAT_IDC_420),
        other => Err(format!(
            "unsupported H.265 chroma_format_idc for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn native_vulkan_vulkanalia_h265_std_profile_idc(
    profile_idc: u8,
) -> Result<vk::video::StdVideoH265ProfileIdc, String> {
    match profile_idc {
        1 => Ok(vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN),
        2 => Ok(vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN_10),
        other => Err(format!(
            "unsupported H.265 profile_idc for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn native_vulkan_vulkanalia_h265_std_level_idc(
    level_idc: u8,
) -> Result<vk::video::StdVideoH265LevelIdc, String> {
    match level_idc {
        30 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_1_0),
        60 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_2_0),
        63 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_2_1),
        90 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_3_0),
        93 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_3_1),
        120 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_4_0),
        123 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_4_1),
        150 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_5_0),
        153 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_5_1),
        156 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_5_2),
        180 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_6_0),
        183 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_6_1),
        186 => Ok(vk::video::STD_VIDEO_H265_LEVEL_IDC_6_2),
        other => Err(format!(
            "unsupported H.265 level_idc for Vulkanalia STD session parameters: {other}"
        )),
    }
}

fn native_vulkan_vulkanalia_h265_std_aspect_ratio_idc(
    aspect_ratio_idc: u32,
) -> Result<vk::video::StdVideoH265AspectRatioIdc, String> {
    let value = i32::try_from(aspect_ratio_idc).map_err(|_| {
        format!("unsupported H.265 aspect_ratio_idc for Vulkanalia STD: {aspect_ratio_idc}")
    })?;
    Ok(vk::video::StdVideoH265AspectRatioIdc(value))
}

fn h265_bool_u32(value: bool) -> u32 {
    u32::from(value)
}

fn h265_u8(value: u32, name: &'static str) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("H.265 {name} exceeds u8: {value}"))
}

fn h265_i8(value: i32, name: &'static str) -> Result<i8, String> {
    i8::try_from(value).map_err(|_| format!("H.265 {name} exceeds i8: {value}"))
}

fn h265_u16(value: u32, name: &'static str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("H.265 {name} exceeds u16: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h265_level_mapping_uses_vulkanalia_std_values() {
        assert_eq!(
            native_vulkan_vulkanalia_h265_std_level_idc(153).unwrap(),
            vk::video::STD_VIDEO_H265_LEVEL_IDC_5_1
        );
        assert!(native_vulkan_vulkanalia_h265_std_level_idc(42).is_err());
    }

    #[test]
    fn h265_used_by_current_mask_rejects_overflow() {
        assert_eq!(
            native_vulkan_vulkanalia_h265_used_by_current_mask(&[true, false, true]).unwrap(),
            0b101
        );
        assert!(native_vulkan_vulkanalia_h265_used_by_current_mask(&[false; 17]).is_err());
    }

    #[test]
    fn h265_profile_bit_depth_validation_rejects_main10_as_main8() {
        assert!(
            native_vulkan_vulkanalia_validate_h265_requested_profile_bit_depth(
                NativeVulkanVideoSessionCodec::H265Main8,
                2,
                2,
                2,
                2,
            )
            .is_err()
        );
        assert!(
            native_vulkan_vulkanalia_validate_h265_requested_profile_bit_depth(
                NativeVulkanVideoSessionCodec::H265Main10,
                2,
                2,
                2,
                2,
            )
            .is_ok()
        );
    }
}

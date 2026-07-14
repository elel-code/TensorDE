#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_slice_decode_info(
    slice: &NativeVulkanH264NalPayload<'_>,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<NativeVulkanH264FirstFrameDecodeInfo, String> {
    let rbsp = native_vulkan_h264_rbsp(slice.payload)?;
    if rbsp.len() < 2 {
        return Err("H.264 slice NAL is too short".to_owned());
    }
    let mut bits = NativeVulkanH264BitReader::new(&rbsp[1..]);
    let first_mb_in_slice = bits.read_ue("first_mb_in_slice")?;
    let slice_type = bits.read_ue("slice_type")?;
    let normalized_slice_type = slice_type % 5;
    let is_intra = normalized_slice_type == vk::video::STD_VIDEO_H264_SLICE_TYPE_I.0 as u32;
    let is_p = normalized_slice_type == vk::video::STD_VIDEO_H264_SLICE_TYPE_P.0 as u32;
    let is_b = normalized_slice_type == vk::video::STD_VIDEO_H264_SLICE_TYPE_B.0 as u32;
    let pps_id = bits.read_ue("pic_parameter_set_id")?;
    if pps_id != parameter_sets.pps.id {
        return Err(format!(
            "H.264 slice PPS id {pps_id} does not match session PPS id {}",
            parameter_sets.pps.id
        ));
    }
    let frame_num_bits = parameter_sets
        .sps
        .log2_max_frame_num_minus4
        .checked_add(4)
        .ok_or_else(|| "H.264 frame_num bit count overflow".to_owned())?;
    let frame_num =
        native_vulkan_h264_u16(bits.read_bits(frame_num_bits, "frame_num")?, "frame_num")?;
    let mut field_pic_flag = false;
    let mut bottom_field_flag = false;
    if !parameter_sets.sps.frame_mbs_only_flag {
        field_pic_flag = bits.read_bool("field_pic_flag")?;
        if field_pic_flag {
            bottom_field_flag = bits.read_bool("bottom_field_flag")?;
        }
    }
    let idr = slice.nal_type == 5;
    let idr_pic_id = if idr {
        native_vulkan_h264_u16(bits.read_ue("idr_pic_id")?, "idr_pic_id")?
    } else {
        0
    };
    let mut delta_pic_order_cnt_bottom = 0;
    let pic_order_cnt = match parameter_sets.sps.pic_order_cnt_type {
        0 => {
            let pic_order_cnt_lsb_bits = parameter_sets
                .sps
                .log2_max_pic_order_cnt_lsb_minus4
                .checked_add(4)
                .ok_or_else(|| "H.264 pic_order_cnt_lsb bit count overflow".to_owned())?;
            let pic_order_cnt_lsb =
                bits.read_bits(pic_order_cnt_lsb_bits, "pic_order_cnt_lsb")? as i32;
            if parameter_sets
                .pps
                .bottom_field_pic_order_in_frame_present_flag
                && !field_pic_flag
            {
                delta_pic_order_cnt_bottom = bits.read_se("delta_pic_order_cnt_bottom")?;
            }
            [
                pic_order_cnt_lsb,
                pic_order_cnt_lsb + delta_pic_order_cnt_bottom,
            ]
        }
        1 if !parameter_sets.sps.delta_pic_order_always_zero_flag => {
            let delta_pic_order_cnt_0 = bits.read_se("delta_pic_order_cnt[0]")?;
            if parameter_sets
                .pps
                .bottom_field_pic_order_in_frame_present_flag
                && !field_pic_flag
            {
                let _delta_pic_order_cnt_top = bits.read_se("delta_pic_order_cnt[1]")?;
            }
            if idr {
                [0, 0]
            } else {
                [i32::from(frame_num).saturating_add(delta_pic_order_cnt_0); 2]
            }
        }
        1 | 2 => {
            if idr {
                [0, 0]
            } else {
                [i32::from(frame_num); 2]
            }
        }
        other => {
            return Err(format!("H.264 pic_order_cnt_type {other} is not supported"));
        }
    };
    if parameter_sets.pps.redundant_pic_cnt_present_flag {
        bits.read_ue("redundant_pic_cnt")?;
    }
    if is_b {
        bits.read_bool("direct_spatial_mv_pred_flag")?;
    }
    let mut num_ref_idx_l0_active_minus1 = None::<u32>;
    let mut num_ref_idx_l1_active_minus1 = None::<u32>;
    if is_p || is_b {
        if bits.read_bool("num_ref_idx_active_override_flag")? {
            num_ref_idx_l0_active_minus1 = Some(bits.read_ue("num_ref_idx_l0_active_minus1")?);
            if is_b {
                num_ref_idx_l1_active_minus1 = Some(bits.read_ue("num_ref_idx_l1_active_minus1")?);
            }
        } else {
            num_ref_idx_l0_active_minus1 =
                Some(parameter_sets.pps.num_ref_idx_l0_default_active_minus1);
            if is_b {
                num_ref_idx_l1_active_minus1 =
                    Some(parameter_sets.pps.num_ref_idx_l1_default_active_minus1);
            }
        }
    }
    let (ref_pic_list_modification_l0, ref_pic_list_modifications_l0) = if is_p || is_b {
        native_vulkan_h264_read_ref_pic_list_modifications(
            &mut bits,
            "ref_pic_list_modification_flag_l0",
            "l0",
        )?
    } else {
        (false, Vec::new())
    };
    let (ref_pic_list_modification_l1, ref_pic_list_modifications_l1) = if is_b {
        native_vulkan_h264_read_ref_pic_list_modifications(
            &mut bits,
            "ref_pic_list_modification_flag_l1",
            "l1",
        )?
    } else {
        (false, Vec::new())
    };
    native_vulkan_h264_skip_pred_weight_table(
        &mut bits,
        parameter_sets,
        is_p,
        is_b,
        num_ref_idx_l0_active_minus1,
        num_ref_idx_l1_active_minus1,
    )?;
    let mut long_term_reference_flag = false;
    let mut adaptive_ref_pic_marking_mode_flag = false;
    let mut memory_management_control_operations =
        Vec::<NativeVulkanH264MemoryManagementControlOperationSnapshot>::new();
    if slice.nal_ref_idc != 0 && idr {
        bits.read_bool("no_output_of_prior_pics_flag")?;
        long_term_reference_flag = bits.read_bool("long_term_reference_flag")?;
    } else if slice.nal_ref_idc != 0 {
        adaptive_ref_pic_marking_mode_flag =
            bits.read_bool("adaptive_ref_pic_marking_mode_flag")?;
        if adaptive_ref_pic_marking_mode_flag {
            loop {
                let memory_management_control_operation =
                    bits.read_ue("memory_management_control_operation")?;
                if memory_management_control_operation == 0 {
                    break;
                }
                let mut difference_of_pic_nums_minus1 = None;
                let mut long_term_pic_num = None;
                let mut long_term_frame_idx = None;
                let mut max_long_term_frame_idx_plus1 = None;
                match memory_management_control_operation {
                    1 => {
                        difference_of_pic_nums_minus1 =
                            Some(bits.read_ue("difference_of_pic_nums_minus1")?);
                    }
                    2 => {
                        long_term_pic_num = Some(bits.read_ue("long_term_pic_num")?);
                    }
                    3 => {
                        difference_of_pic_nums_minus1 =
                            Some(bits.read_ue("difference_of_pic_nums_minus1")?);
                        long_term_frame_idx = Some(bits.read_ue("long_term_frame_idx")?);
                    }
                    4 => {
                        max_long_term_frame_idx_plus1 =
                            Some(bits.read_ue("max_long_term_frame_idx_plus1")?);
                    }
                    5 => {}
                    6 => {
                        long_term_frame_idx = Some(bits.read_ue("long_term_frame_idx")?);
                    }
                    other => {
                        return Err(format!(
                            "H.264 memory_management_control_operation {other} is not supported"
                        ));
                    }
                }
                memory_management_control_operations.push(
                    NativeVulkanH264MemoryManagementControlOperationSnapshot {
                        memory_management_control_operation,
                        difference_of_pic_nums_minus1,
                        long_term_pic_num,
                        long_term_frame_idx,
                        max_long_term_frame_idx_plus1,
                    },
                );
            }
        }
    }

    Ok(NativeVulkanH264FirstFrameDecodeInfo {
        nal_type: slice.nal_type,
        nal_type_label: native_vulkan_h264_nal_type_label(slice.nal_type),
        nal_ref_idc: slice.nal_ref_idc,
        first_mb_in_slice,
        first_slice_segment_in_pic_flag: first_mb_in_slice == 0,
        slice_type,
        slice_type_normalized: normalized_slice_type,
        pps_id,
        frame_num,
        idr_pic_id,
        num_ref_idx_l0_active_minus1,
        num_ref_idx_l1_active_minus1,
        ref_pic_list_modification_l0,
        ref_pic_list_modifications_l0,
        ref_pic_list_modification_l1,
        ref_pic_list_modifications_l1,
        adaptive_ref_pic_marking_mode_flag,
        memory_management_control_operations,
        field_pic_flag,
        bottom_field_flag,
        is_reference: slice.nal_ref_idc != 0,
        is_intra,
        is_p,
        is_b,
        long_term_reference_flag,
        pic_order_cnt,
        slice_offsets: NativeVulkanH264SliceOffsets::new(),
        idr,
        irap: idr,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH265ParsedVps {
    id: u8,
    max_layers_minus1: u8,
    max_sub_layers_minus1: u8,
    temporal_id_nesting_flag: bool,
    dec_pic_buf_mgr: NativeVulkanH265ParsedDecPicBufMgr,
    profile: NativeVulkanH265ParsedProfileTierLevel,
    timing_info_present_flag: bool,
    poc_proportional_to_timing_flag: bool,
    num_units_in_tick: Option<u32>,
    time_scale: Option<u32>,
    num_ticks_poc_diff_one_minus1: Option<u32>,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanH265ParsedDecPicBufMgr {
    sub_layer_ordering_info_present_flag: bool,
    max_latency_increase_plus1: [u32; 7],
    max_dec_pic_buffering_minus1: [u8; 7],
    max_num_reorder_pics: [u8; 7],
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH265ParsedSps {
    id: u32,
    vps_id: u8,
    max_sub_layers_minus1: u8,
    temporal_id_nesting_flag: bool,
    dec_pic_buf_mgr: NativeVulkanH265ParsedDecPicBufMgr,
    profile: NativeVulkanH265ParsedProfileTierLevel,
    chroma_format_idc: u32,
    separate_colour_plane_flag: bool,
    width: u32,
    height: u32,
    conformance_window_flag: bool,
    conf_win_left_offset: u32,
    conf_win_right_offset: u32,
    conf_win_top_offset: u32,
    conf_win_bottom_offset: u32,
    bit_depth_luma_minus8: u32,
    bit_depth_chroma_minus8: u32,
    log2_max_pic_order_cnt_lsb_minus4: u32,
    log2_min_luma_coding_block_size_minus3: u32,
    log2_diff_max_min_luma_coding_block_size: u32,
    log2_min_luma_transform_block_size_minus2: u32,
    log2_diff_max_min_luma_transform_block_size: u32,
    max_transform_hierarchy_depth_inter: u32,
    max_transform_hierarchy_depth_intra: u32,
    scaling_list_enabled_flag: bool,
    sps_scaling_list_data_present_flag: bool,
    amp_enabled_flag: bool,
    sample_adaptive_offset_enabled_flag: bool,
    pcm_enabled_flag: bool,
    pcm_loop_filter_disabled_flag: bool,
    num_short_term_ref_pic_sets: u32,
    short_term_ref_pic_sets: Vec<NativeVulkanH265ShortTermRefPicSetSnapshot>,
    long_term_ref_pics_present_flag: bool,
    long_term_ref_pics_sps: Vec<NativeVulkanH265LongTermRefPicSpsSnapshot>,
    temporal_mvp_enabled_flag: bool,
    strong_intra_smoothing_enabled_flag: bool,
    vui_parameters_present_flag: bool,
    vui: Option<NativeVulkanH265ParsedVui>,
    sps_extension_present_flag: bool,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH265ParsedVui {
    aspect_ratio_info_present_flag: bool,
    aspect_ratio_idc: u32,
    sar_width: u16,
    sar_height: u16,
    overscan_info_present_flag: bool,
    overscan_appropriate_flag: bool,
    video_signal_type_present_flag: bool,
    video_format: u8,
    video_full_range_flag: bool,
    colour_description_present_flag: bool,
    colour_primaries: u8,
    transfer_characteristics: u8,
    matrix_coeffs: u8,
    chroma_loc_info_present_flag: bool,
    chroma_sample_loc_type_top_field: u8,
    chroma_sample_loc_type_bottom_field: u8,
    neutral_chroma_indication_flag: bool,
    field_seq_flag: bool,
    frame_field_info_present_flag: bool,
    default_display_window_flag: bool,
    def_disp_win_left_offset: u16,
    def_disp_win_right_offset: u16,
    def_disp_win_top_offset: u16,
    def_disp_win_bottom_offset: u16,
    vui_timing_info_present_flag: bool,
    vui_num_units_in_tick: u32,
    vui_time_scale: u32,
    vui_poc_proportional_to_timing_flag: bool,
    vui_num_ticks_poc_diff_one_minus1: u32,
    vui_hrd_parameters_present_flag: bool,
    bitstream_restriction_flag: bool,
    tiles_fixed_structure_flag: bool,
    motion_vectors_over_pic_boundaries_flag: bool,
    restricted_ref_pic_lists_flag: bool,
    min_spatial_segmentation_idc: u16,
    max_bytes_per_pic_denom: u8,
    max_bits_per_min_cu_denom: u8,
    log2_max_mv_length_horizontal: u8,
    log2_max_mv_length_vertical: u8,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH265ParsedPps {
    id: u32,
    sps_id: u32,
    dependent_slice_segments_enabled_flag: bool,
    output_flag_present_flag: bool,
    num_extra_slice_header_bits: u8,
    sign_data_hiding_enabled_flag: bool,
    cabac_init_present_flag: bool,
    num_ref_idx_l0_default_active_minus1: u32,
    num_ref_idx_l1_default_active_minus1: u32,
    init_qp_minus26: i32,
    constrained_intra_pred_flag: bool,
    transform_skip_enabled_flag: bool,
    cu_qp_delta_enabled_flag: bool,
    diff_cu_qp_delta_depth: Option<u32>,
    cb_qp_offset: i32,
    cr_qp_offset: i32,
    slice_chroma_qp_offsets_present_flag: bool,
    weighted_pred_flag: bool,
    weighted_bipred_flag: bool,
    transquant_bypass_enabled_flag: bool,
    tiles_enabled_flag: bool,
    entropy_coding_sync_enabled_flag: bool,
    uniform_spacing_flag: bool,
    num_tile_columns_minus1: u32,
    num_tile_rows_minus1: u32,
    loop_filter_across_tiles_enabled_flag: Option<bool>,
    loop_filter_across_slices_enabled_flag: bool,
    deblocking_filter_control_present_flag: bool,
    deblocking_filter_override_enabled_flag: Option<bool>,
    pps_deblocking_filter_disabled_flag: Option<bool>,
    pps_beta_offset_div2: i32,
    pps_tc_offset_div2: i32,
    pps_scaling_list_data_present_flag: bool,
    lists_modification_present_flag: bool,
    log2_parallel_merge_level_minus2: u32,
    slice_segment_header_extension_present_flag: bool,
    pps_extension_present_flag: bool,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanH265ParsedProfileTierLevel {
    profile_idc: u8,
    tier_flag: bool,
    progressive_source_flag: bool,
    interlaced_source_flag: bool,
    non_packed_constraint_flag: bool,
    frame_only_constraint_flag: bool,
    profile_compatibility_flags: [bool; 32],
    level_idc: u8,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanH265ParsedProfileTierLevel {
    fn main_compatible(&self) -> bool {
        self.profile_idc == 1 || self.profile_compatibility_flags[1]
    }

    fn main10_compatible(&self) -> bool {
        self.profile_idc == 2 || self.profile_compatibility_flags[2]
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h265_dec_pic_buf_mgr(
    bits: &mut NativeVulkanH265BitReader<'_>,
    max_sub_layers_minus1: u8,
    label_prefix: &'static str,
) -> Result<NativeVulkanH265ParsedDecPicBufMgr, String> {
    let sub_layer_ordering_info_present_flag =
        bits.read_bool("sub_layer_ordering_info_present_flag")?;
    let ordering_start = if sub_layer_ordering_info_present_flag {
        0
    } else {
        max_sub_layers_minus1
    };
    let mut max_latency_increase_plus1 = [0u32; 7];
    let mut max_dec_pic_buffering_minus1 = [0u8; 7];
    let mut max_num_reorder_pics = [0u8; 7];
    for index in ordering_start..=max_sub_layers_minus1 {
        let max_dec_pic_buffering = bits.read_ue("max_dec_pic_buffering_minus1")?;
        let max_reorder_pics = bits.read_ue("max_num_reorder_pics")?;
        let max_latency_increase = bits.read_ue("max_latency_increase_plus1")?;
        max_dec_pic_buffering_minus1[index as usize] =
            native_vulkan_h265_u8(max_dec_pic_buffering, "max_dec_pic_buffering_minus1")?;
        max_num_reorder_pics[index as usize] =
            native_vulkan_h265_u8(max_reorder_pics, "max_num_reorder_pics")?;
        max_latency_increase_plus1[index as usize] = max_latency_increase;
    }
    if !sub_layer_ordering_info_present_flag {
        let source_index = max_sub_layers_minus1 as usize;
        for index in 0..source_index {
            max_dec_pic_buffering_minus1[index] = max_dec_pic_buffering_minus1[source_index];
            max_num_reorder_pics[index] = max_num_reorder_pics[source_index];
            max_latency_increase_plus1[index] = max_latency_increase_plus1[source_index];
        }
    }
    let _ = label_prefix;

    Ok(NativeVulkanH265ParsedDecPicBufMgr {
        sub_layer_ordering_info_present_flag,
        max_latency_increase_plus1,
        max_dec_pic_buffering_minus1,
        max_num_reorder_pics,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h265_vps(payload: &[u8]) -> Result<NativeVulkanH265ParsedVps, String> {
    let rbsp = native_vulkan_h265_rbsp(payload)?;
    let mut bits = NativeVulkanH265BitReader::new(&rbsp);
    bits.skip_bits(16, "vps nal header")?;
    let id = bits.read_bits(4, "vps_video_parameter_set_id")? as u8;
    bits.skip_bits(2, "vps base layer flags")?;
    let max_layers_minus1 = bits.read_bits(6, "vps_max_layers_minus1")? as u8;
    let max_sub_layers_minus1 = bits.read_bits(3, "vps_max_sub_layers_minus1")? as u8;
    if max_sub_layers_minus1 >= 8 {
        return Err(format!(
            "invalid vps_max_sub_layers_minus1={max_sub_layers_minus1}"
        ));
    }
    let temporal_id_nesting_flag = bits.read_bool("vps_temporal_id_nesting_flag")?;
    bits.skip_bits(16, "vps_reserved_0xffff_16bits")?;
    let profile = native_vulkan_parse_h265_profile_tier_level(&mut bits, max_sub_layers_minus1)?;
    let dec_pic_buf_mgr =
        native_vulkan_parse_h265_dec_pic_buf_mgr(&mut bits, max_sub_layers_minus1, "vps")?;
    bits.skip_bits(6, "vps_max_layer_id")?;
    let num_layer_sets_minus1 = bits.read_ue("vps_num_layer_sets_minus1")?;
    for _ in 1..=num_layer_sets_minus1 {
        for _ in 0..=max_layers_minus1 {
            bits.read_bool("layer_id_included_flag")?;
        }
    }
    let timing_info_present_flag = bits.read_bool("vps_timing_info_present_flag")?;
    let mut poc_proportional_to_timing_flag = false;
    let mut num_units_in_tick = None;
    let mut time_scale = None;
    let mut num_ticks_poc_diff_one_minus1 = None;
    if timing_info_present_flag {
        num_units_in_tick = Some(bits.read_bits(32, "vps_num_units_in_tick")?);
        time_scale = Some(bits.read_bits(32, "vps_time_scale")?);
        poc_proportional_to_timing_flag = bits.read_bool("vps_poc_proportional_to_timing_flag")?;
        if poc_proportional_to_timing_flag {
            num_ticks_poc_diff_one_minus1 =
                Some(bits.read_ue("vps_num_ticks_poc_diff_one_minus1")?);
        }
    }

    Ok(NativeVulkanH265ParsedVps {
        id,
        max_layers_minus1,
        max_sub_layers_minus1,
        temporal_id_nesting_flag,
        dec_pic_buf_mgr,
        profile,
        timing_info_present_flag,
        poc_proportional_to_timing_flag,
        num_units_in_tick,
        time_scale,
        num_ticks_poc_diff_one_minus1,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h265_sps(payload: &[u8]) -> Result<NativeVulkanH265ParsedSps, String> {
    let rbsp = native_vulkan_h265_rbsp(payload)?;
    let mut bits = NativeVulkanH265BitReader::new(&rbsp);
    bits.skip_bits(16, "sps nal header")?;
    let vps_id = bits.read_bits(4, "sps_video_parameter_set_id")? as u8;
    let max_sub_layers_minus1 = bits.read_bits(3, "sps_max_sub_layers_minus1")? as u8;
    if max_sub_layers_minus1 >= 8 {
        return Err(format!(
            "invalid sps_max_sub_layers_minus1={max_sub_layers_minus1}"
        ));
    }
    let temporal_id_nesting_flag = bits.read_bool("sps_temporal_id_nesting_flag")?;
    let profile = native_vulkan_parse_h265_profile_tier_level(&mut bits, max_sub_layers_minus1)?;
    let id = bits.read_ue("sps_seq_parameter_set_id")?;
    let chroma_format_idc = bits.read_ue("chroma_format_idc")?;
    let separate_colour_plane_flag =
        chroma_format_idc == 3 && bits.read_bool("separate_colour_plane_flag")?;
    let width = bits.read_ue("pic_width_in_luma_samples")?;
    let height = bits.read_ue("pic_height_in_luma_samples")?;
    let conformance_window_flag = bits.read_bool("conformance_window_flag")?;
    let (mut conf_win_left_offset, mut conf_win_right_offset) = (0, 0);
    let (mut conf_win_top_offset, mut conf_win_bottom_offset) = (0, 0);
    if conformance_window_flag {
        conf_win_left_offset = bits.read_ue("conf_win_left_offset")?;
        conf_win_right_offset = bits.read_ue("conf_win_right_offset")?;
        conf_win_top_offset = bits.read_ue("conf_win_top_offset")?;
        conf_win_bottom_offset = bits.read_ue("conf_win_bottom_offset")?;
    }
    let bit_depth_luma_minus8 = bits.read_ue("bit_depth_luma_minus8")?;
    let bit_depth_chroma_minus8 = bits.read_ue("bit_depth_chroma_minus8")?;
    let log2_max_pic_order_cnt_lsb_minus4 = bits.read_ue("log2_max_pic_order_cnt_lsb_minus4")?;
    let dec_pic_buf_mgr =
        native_vulkan_parse_h265_dec_pic_buf_mgr(&mut bits, max_sub_layers_minus1, "sps")?;
    let log2_min_luma_coding_block_size_minus3 =
        bits.read_ue("log2_min_luma_coding_block_size_minus3")?;
    let log2_diff_max_min_luma_coding_block_size =
        bits.read_ue("log2_diff_max_min_luma_coding_block_size")?;
    let log2_min_luma_transform_block_size_minus2 =
        bits.read_ue("log2_min_luma_transform_block_size_minus2")?;
    let log2_diff_max_min_luma_transform_block_size =
        bits.read_ue("log2_diff_max_min_luma_transform_block_size")?;
    let max_transform_hierarchy_depth_inter =
        bits.read_ue("max_transform_hierarchy_depth_inter")?;
    let max_transform_hierarchy_depth_intra =
        bits.read_ue("max_transform_hierarchy_depth_intra")?;
    let scaling_list_enabled_flag = bits.read_bool("scaling_list_enabled_flag")?;
    let sps_scaling_list_data_present_flag =
        scaling_list_enabled_flag && bits.read_bool("sps_scaling_list_data_present_flag")?;
    if sps_scaling_list_data_present_flag {
        native_vulkan_h265_skip_scaling_list_data(&mut bits)?;
    }
    let amp_enabled_flag = bits.read_bool("amp_enabled_flag")?;
    let sample_adaptive_offset_enabled_flag =
        bits.read_bool("sample_adaptive_offset_enabled_flag")?;
    let pcm_enabled_flag = bits.read_bool("pcm_enabled_flag")?;
    let mut pcm_loop_filter_disabled_flag = false;
    if pcm_enabled_flag {
        bits.skip_bits(4, "pcm_sample_bit_depth_luma_minus1")?;
        bits.skip_bits(4, "pcm_sample_bit_depth_chroma_minus1")?;
        bits.read_ue("log2_min_pcm_luma_coding_block_size_minus3")?;
        bits.read_ue("log2_diff_max_min_pcm_luma_coding_block_size")?;
        pcm_loop_filter_disabled_flag = bits.read_bool("pcm_loop_filter_disabled_flag")?;
    }
    let num_short_term_ref_pic_sets = bits.read_ue("num_short_term_ref_pic_sets")?;
    let mut short_term_ref_pic_sets = Vec::with_capacity(num_short_term_ref_pic_sets as usize);
    for st_rps_idx in 0..num_short_term_ref_pic_sets {
        let short_term_ref_pic_set = native_vulkan_h265_read_short_term_ref_pic_set(
            &mut bits,
            st_rps_idx,
            num_short_term_ref_pic_sets,
            &short_term_ref_pic_sets,
        )?;
        short_term_ref_pic_sets.push(short_term_ref_pic_set);
    }
    let long_term_ref_pics_present_flag = bits.read_bool("long_term_ref_pics_present_flag")?;
    let mut long_term_ref_pics_sps = Vec::new();
    if long_term_ref_pics_present_flag {
        let num_long_term_ref_pics_sps = bits.read_ue("num_long_term_ref_pics_sps")?;
        if num_long_term_ref_pics_sps > 32 {
            return Err(format!(
                "H.265 SPS has {num_long_term_ref_pics_sps} long-term refs; maximum supported is 32"
            ));
        }
        long_term_ref_pics_sps.reserve(num_long_term_ref_pics_sps as usize);
        for _ in 0..num_long_term_ref_pics_sps {
            let lt_ref_pic_poc_lsb_sps = bits.read_bits(
                log2_max_pic_order_cnt_lsb_minus4 + 4,
                "lt_ref_pic_poc_lsb_sps",
            )?;
            let used_by_curr_pic_lt_sps_flag = bits.read_bool("used_by_curr_pic_lt_sps_flag")?;
            long_term_ref_pics_sps.push(NativeVulkanH265LongTermRefPicSpsSnapshot {
                lt_ref_pic_poc_lsb_sps,
                used_by_curr_pic_lt_sps_flag,
            });
        }
    }
    let temporal_mvp_enabled_flag = bits.read_bool("sps_temporal_mvp_enabled_flag")?;
    let strong_intra_smoothing_enabled_flag =
        bits.read_bool("strong_intra_smoothing_enabled_flag")?;
    let vui_parameters_present_flag = bits.read_bool("vui_parameters_present_flag")?;
    let vui = if vui_parameters_present_flag {
        Some(native_vulkan_parse_h265_vui_parameters(
            &mut bits,
            max_sub_layers_minus1,
        )?)
    } else {
        None
    };
    let sps_extension_present_flag = bits.read_bool("sps_extension_present_flag")?;

    Ok(NativeVulkanH265ParsedSps {
        id,
        vps_id,
        max_sub_layers_minus1,
        temporal_id_nesting_flag,
        dec_pic_buf_mgr,
        profile,
        chroma_format_idc,
        separate_colour_plane_flag,
        width,
        height,
        conformance_window_flag,
        conf_win_left_offset,
        conf_win_right_offset,
        conf_win_top_offset,
        conf_win_bottom_offset,
        bit_depth_luma_minus8,
        bit_depth_chroma_minus8,
        log2_max_pic_order_cnt_lsb_minus4,
        log2_min_luma_coding_block_size_minus3,
        log2_diff_max_min_luma_coding_block_size,
        log2_min_luma_transform_block_size_minus2,
        log2_diff_max_min_luma_transform_block_size,
        max_transform_hierarchy_depth_inter,
        max_transform_hierarchy_depth_intra,
        scaling_list_enabled_flag,
        sps_scaling_list_data_present_flag,
        amp_enabled_flag,
        sample_adaptive_offset_enabled_flag,
        pcm_enabled_flag,
        pcm_loop_filter_disabled_flag,
        num_short_term_ref_pic_sets,
        short_term_ref_pic_sets,
        long_term_ref_pics_present_flag,
        long_term_ref_pics_sps,
        temporal_mvp_enabled_flag,
        strong_intra_smoothing_enabled_flag,
        vui_parameters_present_flag,
        vui,
        sps_extension_present_flag,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h265_pps(payload: &[u8]) -> Result<NativeVulkanH265ParsedPps, String> {
    let rbsp = native_vulkan_h265_rbsp(payload)?;
    let mut bits = NativeVulkanH265BitReader::new(&rbsp);
    bits.skip_bits(16, "pps nal header")?;
    let id = bits.read_ue("pps_pic_parameter_set_id")?;
    let sps_id = bits.read_ue("pps_seq_parameter_set_id")?;
    let dependent_slice_segments_enabled_flag =
        bits.read_bool("dependent_slice_segments_enabled_flag")?;
    let output_flag_present_flag = bits.read_bool("output_flag_present_flag")?;
    let num_extra_slice_header_bits = bits.read_bits(3, "num_extra_slice_header_bits")? as u8;
    let sign_data_hiding_enabled_flag = bits.read_bool("sign_data_hiding_enabled_flag")?;
    let cabac_init_present_flag = bits.read_bool("cabac_init_present_flag")?;
    let num_ref_idx_l0_default_active_minus1 =
        bits.read_ue("num_ref_idx_l0_default_active_minus1")?;
    let num_ref_idx_l1_default_active_minus1 =
        bits.read_ue("num_ref_idx_l1_default_active_minus1")?;
    let init_qp_minus26 = bits.read_se("init_qp_minus26")?;
    let constrained_intra_pred_flag = bits.read_bool("constrained_intra_pred_flag")?;
    let transform_skip_enabled_flag = bits.read_bool("transform_skip_enabled_flag")?;
    let cu_qp_delta_enabled_flag = bits.read_bool("cu_qp_delta_enabled_flag")?;
    let diff_cu_qp_delta_depth = if cu_qp_delta_enabled_flag {
        Some(bits.read_ue("diff_cu_qp_delta_depth")?)
    } else {
        None
    };
    let cb_qp_offset = bits.read_se("pps_cb_qp_offset")?;
    let cr_qp_offset = bits.read_se("pps_cr_qp_offset")?;
    let slice_chroma_qp_offsets_present_flag =
        bits.read_bool("pps_slice_chroma_qp_offsets_present_flag")?;
    let weighted_pred_flag = bits.read_bool("weighted_pred_flag")?;
    let weighted_bipred_flag = bits.read_bool("weighted_bipred_flag")?;
    let transquant_bypass_enabled_flag = bits.read_bool("transquant_bypass_enabled_flag")?;
    let tiles_enabled_flag = bits.read_bool("tiles_enabled_flag")?;
    let entropy_coding_sync_enabled_flag = bits.read_bool("entropy_coding_sync_enabled_flag")?;
    let mut num_tile_columns_minus1 = 0;
    let mut num_tile_rows_minus1 = 0;
    let mut loop_filter_across_tiles_enabled_flag = None;
    let mut uniform_spacing_flag = false;
    if tiles_enabled_flag {
        num_tile_columns_minus1 = bits.read_ue("num_tile_columns_minus1")?;
        num_tile_rows_minus1 = bits.read_ue("num_tile_rows_minus1")?;
        uniform_spacing_flag = bits.read_bool("uniform_spacing_flag")?;
        if !uniform_spacing_flag {
            for _ in 0..num_tile_columns_minus1 {
                bits.read_ue("column_width_minus1")?;
            }
            for _ in 0..num_tile_rows_minus1 {
                bits.read_ue("row_height_minus1")?;
            }
        }
        loop_filter_across_tiles_enabled_flag =
            Some(bits.read_bool("loop_filter_across_tiles_enabled_flag")?);
    }
    let loop_filter_across_slices_enabled_flag =
        bits.read_bool("pps_loop_filter_across_slices_enabled_flag")?;
    let deblocking_filter_control_present_flag =
        bits.read_bool("deblocking_filter_control_present_flag")?;
    let mut deblocking_filter_override_enabled_flag = None;
    let mut pps_deblocking_filter_disabled_flag = None;
    let mut pps_beta_offset_div2 = 0;
    let mut pps_tc_offset_div2 = 0;
    if deblocking_filter_control_present_flag {
        deblocking_filter_override_enabled_flag =
            Some(bits.read_bool("deblocking_filter_override_enabled_flag")?);
        let disabled = bits.read_bool("pps_deblocking_filter_disabled_flag")?;
        pps_deblocking_filter_disabled_flag = Some(disabled);
        if !disabled {
            pps_beta_offset_div2 = bits.read_se("pps_beta_offset_div2")?;
            pps_tc_offset_div2 = bits.read_se("pps_tc_offset_div2")?;
        }
    }
    let pps_scaling_list_data_present_flag =
        bits.read_bool("pps_scaling_list_data_present_flag")?;
    if pps_scaling_list_data_present_flag {
        native_vulkan_h265_skip_scaling_list_data(&mut bits)?;
    }
    let lists_modification_present_flag = bits.read_bool("lists_modification_present_flag")?;
    let log2_parallel_merge_level_minus2 = bits.read_ue("log2_parallel_merge_level_minus2")?;
    let slice_segment_header_extension_present_flag =
        bits.read_bool("slice_segment_header_extension_present_flag")?;
    let pps_extension_present_flag = bits.read_bool("pps_extension_present_flag")?;

    Ok(NativeVulkanH265ParsedPps {
        id,
        sps_id,
        dependent_slice_segments_enabled_flag,
        output_flag_present_flag,
        num_extra_slice_header_bits,
        sign_data_hiding_enabled_flag,
        cabac_init_present_flag,
        num_ref_idx_l0_default_active_minus1,
        num_ref_idx_l1_default_active_minus1,
        init_qp_minus26,
        constrained_intra_pred_flag,
        transform_skip_enabled_flag,
        cu_qp_delta_enabled_flag,
        diff_cu_qp_delta_depth,
        cb_qp_offset,
        cr_qp_offset,
        slice_chroma_qp_offsets_present_flag,
        weighted_pred_flag,
        weighted_bipred_flag,
        transquant_bypass_enabled_flag,
        tiles_enabled_flag,
        entropy_coding_sync_enabled_flag,
        uniform_spacing_flag,
        num_tile_columns_minus1,
        num_tile_rows_minus1,
        loop_filter_across_tiles_enabled_flag,
        loop_filter_across_slices_enabled_flag,
        deblocking_filter_control_present_flag,
        deblocking_filter_override_enabled_flag,
        pps_deblocking_filter_disabled_flag,
        pps_beta_offset_div2,
        pps_tc_offset_div2,
        pps_scaling_list_data_present_flag,
        lists_modification_present_flag,
        log2_parallel_merge_level_minus2,
        slice_segment_header_extension_present_flag,
        pps_extension_present_flag,
    })
}

include!("h264_h265_slice_parsers/h265_vui.rs");

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_parse_h265_vui_parameters(
    bits: &mut NativeVulkanH265BitReader<'_>,
    max_sub_layers_minus1: u8,
) -> Result<NativeVulkanH265ParsedVui, String> {
    let aspect_ratio_info_present_flag = bits.read_bool("aspect_ratio_info_present_flag")?;
    let mut aspect_ratio_idc = 0u32;
    let mut sar_width = 0u16;
    let mut sar_height = 0u16;
    if aspect_ratio_info_present_flag {
        aspect_ratio_idc = bits.read_bits(8, "aspect_ratio_idc")?;
        if aspect_ratio_idc == 255 {
            sar_width = native_vulkan_h265_u16(bits.read_bits(16, "sar_width")?, "sar_width")?;
            sar_height = native_vulkan_h265_u16(bits.read_bits(16, "sar_height")?, "sar_height")?;
        }
    }

    let overscan_info_present_flag = bits.read_bool("overscan_info_present_flag")?;
    let overscan_appropriate_flag =
        overscan_info_present_flag && bits.read_bool("overscan_appropriate_flag")?;

    let video_signal_type_present_flag = bits.read_bool("video_signal_type_present_flag")?;
    let mut video_format = 5u8;
    let mut video_full_range_flag = false;
    let mut colour_description_present_flag = false;
    let mut colour_primaries = 2u8;
    let mut transfer_characteristics = 2u8;
    let mut matrix_coeffs = 2u8;
    if video_signal_type_present_flag {
        video_format = native_vulkan_h265_u8(bits.read_bits(3, "video_format")?, "video_format")?;
        video_full_range_flag = bits.read_bool("video_full_range_flag")?;
        colour_description_present_flag = bits.read_bool("colour_description_present_flag")?;
        if colour_description_present_flag {
            colour_primaries =
                native_vulkan_h265_u8(bits.read_bits(8, "colour_primaries")?, "colour_primaries")?;
            transfer_characteristics = native_vulkan_h265_u8(
                bits.read_bits(8, "transfer_characteristics")?,
                "transfer_characteristics",
            )?;
            matrix_coeffs =
                native_vulkan_h265_u8(bits.read_bits(8, "matrix_coeffs")?, "matrix_coeffs")?;
        }
    }

    let chroma_loc_info_present_flag = bits.read_bool("chroma_loc_info_present_flag")?;
    let mut chroma_sample_loc_type_top_field = 0u8;
    let mut chroma_sample_loc_type_bottom_field = 0u8;
    if chroma_loc_info_present_flag {
        chroma_sample_loc_type_top_field = native_vulkan_h265_u8(
            bits.read_ue("chroma_sample_loc_type_top_field")?,
            "chroma_sample_loc_type_top_field",
        )?;
        chroma_sample_loc_type_bottom_field = native_vulkan_h265_u8(
            bits.read_ue("chroma_sample_loc_type_bottom_field")?,
            "chroma_sample_loc_type_bottom_field",
        )?;
    }

    let neutral_chroma_indication_flag = bits.read_bool("neutral_chroma_indication_flag")?;
    let field_seq_flag = bits.read_bool("field_seq_flag")?;
    let frame_field_info_present_flag = bits.read_bool("frame_field_info_present_flag")?;
    let default_display_window_flag = bits.read_bool("default_display_window_flag")?;
    let mut def_disp_win_left_offset = 0u16;
    let mut def_disp_win_right_offset = 0u16;
    let mut def_disp_win_top_offset = 0u16;
    let mut def_disp_win_bottom_offset = 0u16;
    if default_display_window_flag {
        def_disp_win_left_offset = native_vulkan_h265_u16(
            bits.read_ue("def_disp_win_left_offset")?,
            "def_disp_win_left_offset",
        )?;
        def_disp_win_right_offset = native_vulkan_h265_u16(
            bits.read_ue("def_disp_win_right_offset")?,
            "def_disp_win_right_offset",
        )?;
        def_disp_win_top_offset = native_vulkan_h265_u16(
            bits.read_ue("def_disp_win_top_offset")?,
            "def_disp_win_top_offset",
        )?;
        def_disp_win_bottom_offset = native_vulkan_h265_u16(
            bits.read_ue("def_disp_win_bottom_offset")?,
            "def_disp_win_bottom_offset",
        )?;
    }

    let vui_timing_info_present_flag = bits.read_bool("vui_timing_info_present_flag")?;
    let mut vui_num_units_in_tick = 0u32;
    let mut vui_time_scale = 0u32;
    let mut vui_poc_proportional_to_timing_flag = false;
    let mut vui_num_ticks_poc_diff_one_minus1 = 0u32;
    let mut vui_hrd_parameters_present_flag = false;
    if vui_timing_info_present_flag {
        vui_num_units_in_tick = bits.read_bits(32, "vui_num_units_in_tick")?;
        vui_time_scale = bits.read_bits(32, "vui_time_scale")?;
        vui_poc_proportional_to_timing_flag =
            bits.read_bool("vui_poc_proportional_to_timing_flag")?;
        if vui_poc_proportional_to_timing_flag {
            vui_num_ticks_poc_diff_one_minus1 =
                bits.read_ue("vui_num_ticks_poc_diff_one_minus1")?;
        }
        vui_hrd_parameters_present_flag = bits.read_bool("vui_hrd_parameters_present_flag")?;
        if vui_hrd_parameters_present_flag {
            native_vulkan_h265_skip_hrd_parameters(bits, true, max_sub_layers_minus1)?;
        }
    }

    let bitstream_restriction_flag = bits.read_bool("bitstream_restriction_flag")?;
    let mut tiles_fixed_structure_flag = false;
    let mut motion_vectors_over_pic_boundaries_flag = false;
    let mut restricted_ref_pic_lists_flag = false;
    let mut min_spatial_segmentation_idc = 0u16;
    let mut max_bytes_per_pic_denom = 0u8;
    let mut max_bits_per_min_cu_denom = 0u8;
    let mut log2_max_mv_length_horizontal = 0u8;
    let mut log2_max_mv_length_vertical = 0u8;
    if bitstream_restriction_flag {
        tiles_fixed_structure_flag = bits.read_bool("tiles_fixed_structure_flag")?;
        motion_vectors_over_pic_boundaries_flag =
            bits.read_bool("motion_vectors_over_pic_boundaries_flag")?;
        restricted_ref_pic_lists_flag = bits.read_bool("restricted_ref_pic_lists_flag")?;
        min_spatial_segmentation_idc = native_vulkan_h265_u16(
            bits.read_ue("min_spatial_segmentation_idc")?,
            "min_spatial_segmentation_idc",
        )?;
        max_bytes_per_pic_denom = native_vulkan_h265_u8(
            bits.read_ue("max_bytes_per_pic_denom")?,
            "max_bytes_per_pic_denom",
        )?;
        max_bits_per_min_cu_denom = native_vulkan_h265_u8(
            bits.read_ue("max_bits_per_min_cu_denom")?,
            "max_bits_per_min_cu_denom",
        )?;
        log2_max_mv_length_horizontal = native_vulkan_h265_u8(
            bits.read_ue("log2_max_mv_length_horizontal")?,
            "log2_max_mv_length_horizontal",
        )?;
        log2_max_mv_length_vertical = native_vulkan_h265_u8(
            bits.read_ue("log2_max_mv_length_vertical")?,
            "log2_max_mv_length_vertical",
        )?;
    }

    Ok(NativeVulkanH265ParsedVui {
        aspect_ratio_info_present_flag,
        aspect_ratio_idc,
        sar_width,
        sar_height,
        overscan_info_present_flag,
        overscan_appropriate_flag,
        video_signal_type_present_flag,
        video_format,
        video_full_range_flag,
        colour_description_present_flag,
        colour_primaries,
        transfer_characteristics,
        matrix_coeffs,
        chroma_loc_info_present_flag,
        chroma_sample_loc_type_top_field,
        chroma_sample_loc_type_bottom_field,
        neutral_chroma_indication_flag,
        field_seq_flag,
        frame_field_info_present_flag,
        default_display_window_flag,
        def_disp_win_left_offset,
        def_disp_win_right_offset,
        def_disp_win_top_offset,
        def_disp_win_bottom_offset,
        vui_timing_info_present_flag,
        vui_num_units_in_tick,
        vui_time_scale,
        vui_poc_proportional_to_timing_flag,
        vui_num_ticks_poc_diff_one_minus1,
        vui_hrd_parameters_present_flag,
        bitstream_restriction_flag,
        tiles_fixed_structure_flag,
        motion_vectors_over_pic_boundaries_flag,
        restricted_ref_pic_lists_flag,
        min_spatial_segmentation_idc,
        max_bytes_per_pic_denom,
        max_bits_per_min_cu_denom,
        log2_max_mv_length_horizontal,
        log2_max_mv_length_vertical,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_h265_skip_hrd_parameters(
    bits: &mut NativeVulkanH265BitReader<'_>,
    common_inf_present_flag: bool,
    max_sub_layers_minus1: u8,
) -> Result<(), String> {
    let mut nal_hrd_parameters_present_flag = false;
    let mut vcl_hrd_parameters_present_flag = false;
    let mut sub_pic_hrd_params_present_flag = false;
    if common_inf_present_flag {
        nal_hrd_parameters_present_flag = bits.read_bool("nal_hrd_parameters_present_flag")?;
        vcl_hrd_parameters_present_flag = bits.read_bool("vcl_hrd_parameters_present_flag")?;
        if nal_hrd_parameters_present_flag || vcl_hrd_parameters_present_flag {
            sub_pic_hrd_params_present_flag = bits.read_bool("sub_pic_hrd_params_present_flag")?;
            if sub_pic_hrd_params_present_flag {
                bits.skip_bits(8, "tick_divisor_minus2")?;
                bits.skip_bits(5, "du_cpb_removal_delay_increment_length_minus1")?;
                bits.read_bool("sub_pic_cpb_params_in_pic_timing_sei_flag")?;
                bits.skip_bits(5, "dpb_output_delay_du_length_minus1")?;
            }
            bits.skip_bits(4, "bit_rate_scale")?;
            bits.skip_bits(4, "cpb_size_scale")?;
            if sub_pic_hrd_params_present_flag {
                bits.skip_bits(4, "cpb_size_du_scale")?;
            }
            bits.skip_bits(5, "initial_cpb_removal_delay_length_minus1")?;
            bits.skip_bits(5, "au_cpb_removal_delay_length_minus1")?;
            bits.skip_bits(5, "dpb_output_delay_length_minus1")?;
        }
    }

    for _ in 0..=max_sub_layers_minus1 {
        let fixed_pic_rate_general_flag = bits.read_bool("fixed_pic_rate_general_flag")?;
        let fixed_pic_rate_within_cvs_flag = if fixed_pic_rate_general_flag {
            true
        } else {
            bits.read_bool("fixed_pic_rate_within_cvs_flag")?
        };
        let mut low_delay_hrd_flag = false;
        if fixed_pic_rate_within_cvs_flag {
            bits.read_ue("elemental_duration_in_tc_minus1")?;
        } else {
            low_delay_hrd_flag = bits.read_bool("low_delay_hrd_flag")?;
        }
        let cpb_cnt_minus1 = if low_delay_hrd_flag {
            0
        } else {
            bits.read_ue("cpb_cnt_minus1")?
        };
        if nal_hrd_parameters_present_flag {
            native_vulkan_h265_skip_sub_layer_hrd_parameters(
                bits,
                cpb_cnt_minus1,
                sub_pic_hrd_params_present_flag,
            )?;
        }
        if vcl_hrd_parameters_present_flag {
            native_vulkan_h265_skip_sub_layer_hrd_parameters(
                bits,
                cpb_cnt_minus1,
                sub_pic_hrd_params_present_flag,
            )?;
        }
    }
    Ok(())
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_h265_skip_sub_layer_hrd_parameters(
    bits: &mut NativeVulkanH265BitReader<'_>,
    cpb_cnt_minus1: u32,
    sub_pic_hrd_params_present_flag: bool,
) -> Result<(), String> {
    for _ in 0..=cpb_cnt_minus1 {
        bits.read_ue("bit_rate_value_minus1")?;
        bits.read_ue("cpb_size_value_minus1")?;
        if sub_pic_hrd_params_present_flag {
            bits.read_ue("cpb_size_du_value_minus1")?;
            bits.read_ue("bit_rate_du_value_minus1")?;
        }
        bits.read_bool("cbr_flag")?;
    }
    Ok(())
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_parse_h265_profile_tier_level(
    bits: &mut NativeVulkanH265BitReader<'_>,
    max_sub_layers_minus1: u8,
) -> Result<NativeVulkanH265ParsedProfileTierLevel, String> {
    bits.skip_bits(2, "general_profile_space")?;
    let tier_flag = bits.read_bool("general_tier_flag")?;
    let profile_idc = bits.read_bits(5, "general_profile_idc")? as u8;
    let mut profile_compatibility_flags = [false; 32];
    for flag in profile_compatibility_flags.iter_mut() {
        *flag = bits.read_bool("general_profile_compatibility_flag")?;
    }
    let progressive_source_flag = bits.read_bool("general_progressive_source_flag")?;
    let interlaced_source_flag = bits.read_bool("general_interlaced_source_flag")?;
    let non_packed_constraint_flag = bits.read_bool("general_non_packed_constraint_flag")?;
    let frame_only_constraint_flag = bits.read_bool("general_frame_only_constraint_flag")?;
    bits.skip_bits(44, "general_constraint_indicator_flags")?;
    let level_idc = bits.read_bits(8, "general_level_idc")? as u8;
    let mut sub_layer_profile_present_flags = [false; 8];
    let mut sub_layer_level_present_flags = [false; 8];
    for index in 0..usize::from(max_sub_layers_minus1) {
        sub_layer_profile_present_flags[index] =
            bits.read_bool("sub_layer_profile_present_flag")?;
        sub_layer_level_present_flags[index] = bits.read_bool("sub_layer_level_present_flag")?;
    }
    if max_sub_layers_minus1 > 0 {
        for _ in max_sub_layers_minus1..8 {
            bits.skip_bits(2, "reserved_zero_2bits")?;
        }
    }
    for index in 0..usize::from(max_sub_layers_minus1) {
        if sub_layer_profile_present_flags[index] {
            bits.skip_bits(2, "sub_layer_profile_space")?;
            bits.skip_bits(1, "sub_layer_tier_flag")?;
            bits.skip_bits(5, "sub_layer_profile_idc")?;
            bits.skip_bits(32, "sub_layer_profile_compatibility_flags")?;
            bits.skip_bits(4, "sub_layer_source_constraint_flags")?;
            bits.skip_bits(44, "sub_layer_constraint_indicator_flags")?;
        }
        if sub_layer_level_present_flags[index] {
            bits.skip_bits(8, "sub_layer_level_idc")?;
        }
    }

    Ok(NativeVulkanH265ParsedProfileTierLevel {
        profile_idc,
        tier_flag,
        progressive_source_flag,
        interlaced_source_flag,
        non_packed_constraint_flag,
        frame_only_constraint_flag,
        profile_compatibility_flags,
        level_idc,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_h265_skip_scaling_list_data(
    bits: &mut NativeVulkanH265BitReader<'_>,
) -> Result<(), String> {
    for size_id in 0..4u32 {
        let step = if size_id == 3 { 3 } else { 1 };
        let mut matrix_id = 0u32;
        while matrix_id < 6 {
            let pred_mode_flag = bits.read_bool("scaling_list_pred_mode_flag")?;
            if !pred_mode_flag {
                bits.read_ue("scaling_list_pred_matrix_id_delta")?;
            } else {
                let coef_num = 64u32.min(1u32 << (4 + (size_id << 1)));
                if size_id > 1 {
                    bits.read_se("scaling_list_dc_coef_minus8")?;
                }
                for _ in 0..coef_num {
                    bits.read_se("scaling_list_delta_coef")?;
                }
            }
            matrix_id += step;
        }
    }
    Ok(())
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_h265_read_short_term_ref_pic_set(
    bits: &mut NativeVulkanH265BitReader<'_>,
    st_rps_idx: u32,
    num_short_term_ref_pic_sets: u32,
    previous_ref_pic_sets: &[NativeVulkanH265ShortTermRefPicSetSnapshot],
) -> Result<NativeVulkanH265ShortTermRefPicSetSnapshot, String> {
    let inter_ref_pic_set_prediction_flag =
        st_rps_idx != 0 && bits.read_bool("inter_ref_pic_set_prediction_flag")?;
    if inter_ref_pic_set_prediction_flag {
        let delta_idx_minus1 = if st_rps_idx == num_short_term_ref_pic_sets {
            bits.read_ue("delta_idx_minus1")?
        } else {
            0
        };
        let ref_rps_idx = st_rps_idx
            .checked_sub(delta_idx_minus1.saturating_add(1))
            .ok_or_else(|| {
                format!(
                    "H.265 predicted RPS delta_idx_minus1 {delta_idx_minus1} underflows stRpsIdx {st_rps_idx}"
                )
            })?;
        let ref_pic_set = previous_ref_pic_sets
            .get(ref_rps_idx as usize)
            .ok_or_else(|| {
                format!(
                    "H.265 predicted RPS RefRpsIdx {ref_rps_idx} exceeds previous RPS count {}",
                    previous_ref_pic_sets.len()
                )
            })?;
        let ref_num_delta_pocs = ref_pic_set
            .num_negative_pics
            .checked_add(ref_pic_set.num_positive_pics)
            .ok_or_else(|| "H.265 predicted RPS reference delta POC count overflow".to_owned())?;
        let delta_rps_sign = bits.read_bool("delta_rps_sign")?;
        let abs_delta_rps_minus1 = bits.read_ue("abs_delta_rps_minus1")?;
        let delta_rps_magnitude = i32::try_from(abs_delta_rps_minus1.saturating_add(1))
            .map_err(|_| "H.265 predicted RPS abs_delta_rps_minus1 exceeds i32 range".to_owned())?;
        let delta_rps = if delta_rps_sign {
            -delta_rps_magnitude
        } else {
            delta_rps_magnitude
        };
        let flag_count = ref_num_delta_pocs.saturating_add(1);
        if flag_count > 16 {
            return Err(format!(
                "H.265 predicted RPS has {flag_count} use-delta flags; maximum supported is 16"
            ));
        }
        let mut used_by_current_flags = Vec::with_capacity(flag_count as usize);
        let mut use_delta_flags = Vec::with_capacity(flag_count as usize);
        for flag_index in 0..flag_count {
            let used_by_current = bits.read_bool("used_by_curr_pic_flag")?;
            let use_delta = if used_by_current {
                true
            } else {
                bits.read_bool("use_delta_flag")?
            };
            used_by_current_flags.push(used_by_current);
            use_delta_flags.push(use_delta);
            if flag_index == ref_num_delta_pocs && !use_delta {
                continue;
            }
        }

        let mut negative_entries = Vec::<(i32, bool)>::new();
        let mut positive_entries = Vec::<(i32, bool)>::new();
        let ref_negative_count = ref_pic_set.negative_delta_pocs.len();
        let ref_positive_count = ref_pic_set.positive_delta_pocs.len();
        for index in (0..ref_positive_count).rev() {
            let flag_index = ref_negative_count + index;
            let delta_poc = ref_pic_set.positive_delta_pocs[index]
                .checked_add(delta_rps)
                .ok_or_else(|| "H.265 predicted positive RPS delta overflow".to_owned())?;
            if delta_poc < 0 && use_delta_flags.get(flag_index).copied().unwrap_or(false) {
                negative_entries.push((
                    delta_poc,
                    used_by_current_flags
                        .get(flag_index)
                        .copied()
                        .unwrap_or(false),
                ));
            }
        }
        let delta_rps_flag_index = ref_num_delta_pocs as usize;
        if delta_rps < 0
            && use_delta_flags
                .get(delta_rps_flag_index)
                .copied()
                .unwrap_or(false)
        {
            negative_entries.push((
                delta_rps,
                used_by_current_flags
                    .get(delta_rps_flag_index)
                    .copied()
                    .unwrap_or(false),
            ));
        }
        for index in 0..ref_negative_count {
            let delta_poc = ref_pic_set.negative_delta_pocs[index]
                .checked_add(delta_rps)
                .ok_or_else(|| "H.265 predicted negative RPS delta overflow".to_owned())?;
            if delta_poc < 0 && use_delta_flags.get(index).copied().unwrap_or(false) {
                negative_entries.push((
                    delta_poc,
                    used_by_current_flags.get(index).copied().unwrap_or(false),
                ));
            }
        }

        for index in (0..ref_negative_count).rev() {
            let delta_poc = ref_pic_set.negative_delta_pocs[index]
                .checked_add(delta_rps)
                .ok_or_else(|| "H.265 predicted negative RPS delta overflow".to_owned())?;
            if delta_poc > 0 && use_delta_flags.get(index).copied().unwrap_or(false) {
                positive_entries.push((
                    delta_poc,
                    used_by_current_flags.get(index).copied().unwrap_or(false),
                ));
            }
        }
        if delta_rps > 0
            && use_delta_flags
                .get(delta_rps_flag_index)
                .copied()
                .unwrap_or(false)
        {
            positive_entries.push((
                delta_rps,
                used_by_current_flags
                    .get(delta_rps_flag_index)
                    .copied()
                    .unwrap_or(false),
            ));
        }
        for index in 0..ref_positive_count {
            let flag_index = ref_negative_count + index;
            let delta_poc = ref_pic_set.positive_delta_pocs[index]
                .checked_add(delta_rps)
                .ok_or_else(|| "H.265 predicted positive RPS delta overflow".to_owned())?;
            if delta_poc > 0 && use_delta_flags.get(flag_index).copied().unwrap_or(false) {
                positive_entries.push((
                    delta_poc,
                    used_by_current_flags
                        .get(flag_index)
                        .copied()
                        .unwrap_or(false),
                ));
            }
        }

        let negative_delta_pocs = negative_entries
            .iter()
            .map(|(delta_poc, _)| *delta_poc)
            .collect::<Vec<_>>();
        let negative_used_by_curr_pic = negative_entries
            .iter()
            .map(|(_, used)| *used)
            .collect::<Vec<_>>();
        let positive_delta_pocs = positive_entries
            .iter()
            .map(|(delta_poc, _)| *delta_poc)
            .collect::<Vec<_>>();
        let positive_used_by_curr_pic = positive_entries
            .iter()
            .map(|(_, used)| *used)
            .collect::<Vec<_>>();
        return Ok(native_vulkan_h265_short_term_ref_pic_set_snapshot(
            true,
            Some(delta_idx_minus1),
            Some(delta_rps_sign),
            Some(abs_delta_rps_minus1),
            ref_num_delta_pocs,
            use_delta_flags,
            used_by_current_flags,
            negative_delta_pocs,
            negative_used_by_curr_pic,
            positive_delta_pocs,
            positive_used_by_curr_pic,
        ));
    }

    let num_negative_pics = bits.read_ue("num_negative_pics")?;
    let num_positive_pics = bits.read_ue("num_positive_pics")?;
    let mut negative_delta_pocs = Vec::with_capacity(num_negative_pics as usize);
    let mut negative_used_by_curr_pic = Vec::with_capacity(num_negative_pics as usize);
    let mut previous_delta_poc = 0i32;
    for _ in 0..num_negative_pics {
        let delta_poc_s0_minus1 = bits.read_ue("delta_poc_s0_minus1")?;
        let delta_poc = previous_delta_poc
            .checked_sub(
                i32::try_from(delta_poc_s0_minus1)
                    .map_err(|_| "delta_poc_s0_minus1 exceeds i32 range".to_owned())?
                    + 1,
            )
            .ok_or_else(|| "negative short-term delta POC underflow".to_owned())?;
        previous_delta_poc = delta_poc;
        negative_delta_pocs.push(delta_poc);
        negative_used_by_curr_pic.push(bits.read_bool("used_by_curr_pic_s0_flag")?);
    }

    let mut positive_delta_pocs = Vec::with_capacity(num_positive_pics as usize);
    let mut positive_used_by_curr_pic = Vec::with_capacity(num_positive_pics as usize);
    let mut previous_delta_poc = 0i32;
    for _ in 0..num_positive_pics {
        let delta_poc_s1_minus1 = bits.read_ue("delta_poc_s1_minus1")?;
        let delta_poc = previous_delta_poc
            .checked_add(
                i32::try_from(delta_poc_s1_minus1)
                    .map_err(|_| "delta_poc_s1_minus1 exceeds i32 range".to_owned())?
                    + 1,
            )
            .ok_or_else(|| "positive short-term delta POC overflow".to_owned())?;
        previous_delta_poc = delta_poc;
        positive_delta_pocs.push(delta_poc);
        positive_used_by_curr_pic.push(bits.read_bool("used_by_curr_pic_s1_flag")?);
    }
    Ok(native_vulkan_h265_short_term_ref_pic_set_snapshot(
        false,
        None,
        None,
        None,
        0,
        Vec::new(),
        Vec::new(),
        negative_delta_pocs,
        negative_used_by_curr_pic,
        positive_delta_pocs,
        positive_used_by_curr_pic,
    ))
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_h265_short_term_ref_pic_set_snapshot(
    inter_ref_pic_set_prediction_flag: bool,
    delta_idx_minus1: Option<u32>,
    delta_rps_sign: Option<bool>,
    abs_delta_rps_minus1: Option<u32>,
    num_delta_pocs_of_ref_rps_idx: u32,
    use_delta_flags: Vec<bool>,
    used_by_current_flags: Vec<bool>,
    negative_delta_pocs: Vec<i32>,
    negative_used_by_curr_pic: Vec<bool>,
    positive_delta_pocs: Vec<i32>,
    positive_used_by_curr_pic: Vec<bool>,
) -> NativeVulkanH265ShortTermRefPicSetSnapshot {
    let used_by_current_count = negative_used_by_curr_pic
        .iter()
        .chain(positive_used_by_curr_pic.iter())
        .filter(|used| **used)
        .count() as u32;
    let used_negative_delta_pocs = negative_delta_pocs
        .iter()
        .copied()
        .zip(negative_used_by_curr_pic.iter().copied())
        .filter_map(|(delta_poc, used)| used.then_some(delta_poc))
        .collect::<Vec<_>>();
    let used_positive_delta_pocs = positive_delta_pocs
        .iter()
        .copied()
        .zip(positive_used_by_curr_pic.iter().copied())
        .filter_map(|(delta_poc, used)| used.then_some(delta_poc))
        .collect::<Vec<_>>();
    NativeVulkanH265ShortTermRefPicSetSnapshot {
        inter_ref_pic_set_prediction_flag,
        delta_idx_minus1,
        delta_rps_sign,
        abs_delta_rps_minus1,
        num_delta_pocs_of_ref_rps_idx,
        use_delta_flags,
        used_by_current_flags,
        num_negative_pics: negative_delta_pocs.len() as u32,
        num_positive_pics: positive_delta_pocs.len() as u32,
        negative_delta_pocs,
        negative_used_by_curr_pic,
        used_negative_delta_pocs,
        positive_delta_pocs,
        positive_used_by_curr_pic,
        used_positive_delta_pocs,
        used_by_current_count,
    }
}

pub(super) fn native_vulkan_h265_u8(value: u32, label: &'static str) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("{label}={value} exceeds u8 range"))
}

pub(super) fn native_vulkan_h265_i8(value: i32, label: &'static str) -> Result<i8, String> {
    i8::try_from(value).map_err(|_| format!("{label}={value} exceeds i8 range"))
}

pub(super) fn native_vulkan_h265_u16(value: u32, label: &'static str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("{label}={value} exceeds u16 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_h265_rbsp(payload: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    if payload.len() < 2 {
        return Err("H.265 NAL payload is too short".to_owned());
    }
    Ok(native_vulkan_rbsp_unescape(payload))
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_h265_slice_header_rbsp(payload: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    const H265_SLICE_HEADER_PROBE_BYTES: usize = 4096;
    if payload.len() < 2 {
        return Err("H.265 NAL payload is too short".to_owned());
    }
    let probe_len = payload.len().min(H265_SLICE_HEADER_PROBE_BYTES);
    Ok(native_vulkan_rbsp_unescape(&payload[..probe_len]))
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) struct NativeVulkanH265BitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl<'a> NativeVulkanH265BitReader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bit_offset: 0,
        }
    }

    pub(super) fn bit_offset(&self) -> usize {
        self.bit_offset
    }

    pub(super) fn read_bool(&mut self, label: &'static str) -> Result<bool, String> {
        Ok(self.read_bits(1, label)? != 0)
    }

    pub(super) fn skip_bits(&mut self, count: u32, label: &'static str) -> Result<(), String> {
        let mut remaining = count;
        while remaining > 0 {
            let chunk = remaining.min(32);
            self.read_bits(chunk, label)?;
            remaining -= chunk;
        }
        Ok(())
    }

    pub(super) fn read_bits(&mut self, count: u32, label: &'static str) -> Result<u32, String> {
        native_vulkan_read_bits_be(
            self.bytes,
            &mut self.bit_offset,
            count,
            label,
            "H.265 RBSP length",
        )
    }

    pub(super) fn read_ue(&mut self, label: &'static str) -> Result<u32, String> {
        let mut leading_zero_bits = 0u32;
        while !self.read_bool(label)? {
            leading_zero_bits += 1;
            if leading_zero_bits > 31 {
                return Err(format!("{label} Exp-Golomb code is too large"));
            }
        }
        if leading_zero_bits == 0 {
            return Ok(0);
        }
        let suffix = self.read_bits(leading_zero_bits, label)?;
        Ok((1u32 << leading_zero_bits) - 1 + suffix)
    }

    pub(super) fn read_se(&mut self, label: &'static str) -> Result<i32, String> {
        let value = self.read_ue(label)?;
        let signed = value.div_ceil(2) as i32;
        if value % 2 == 0 {
            Ok(-signed)
        } else {
            Ok(signed)
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_h265_profile_idc_label(profile_idc: u8) -> &'static str {
    match profile_idc {
        1 => "main",
        2 => "main-10",
        3 => "main-still-picture",
        4 => "format-range-extensions",
        5 => "high-throughput",
        6 => "multiview-main",
        7 => "scalable-main",
        8 => "3d-main",
        9 => "screen-content-coding",
        10 => "scalable-format-range-extensions",
        11 => "high-throughput-screen-content-coding",
        _ => "unknown",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_h265_chroma_format_label(chroma_format_idc: u32) -> &'static str {
    match chroma_format_idc {
        0 => "monochrome",
        1 => "4:2:0",
        2 => "4:2:2",
        3 => "4:4:4",
        _ => "unknown",
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_h265_level_idc_byte_label(level_idc: u8) -> Option<&'static str> {
    match level_idc {
        30 => Some("1.0"),
        60 => Some("2.0"),
        63 => Some("2.1"),
        90 => Some("3.0"),
        93 => Some("3.1"),
        120 => Some("4.0"),
        123 => Some("4.1"),
        150 => Some("5.0"),
        153 => Some("5.1"),
        156 => Some("5.2"),
        180 => Some("6.0"),
        183 => Some("6.1"),
        186 => Some("6.2"),
        _ => None,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct NativeVulkanH265NalStats {
    bytes: u64,
    has_annex_b_start_codes: bool,
    vps_count: u32,
    sps_count: u32,
    pps_count: u32,
    idr_count: u32,
    slice_count: u32,
    first_slice: Option<NativeVulkanH265SlicePayloadSummary>,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanH265NalStats {
    pub(super) fn parameter_sets_present(&self) -> bool {
        self.vps_count > 0 && self.sps_count > 0 && self.pps_count > 0
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanH265SlicePayloadSummary {
    nal_type: u8,
    slice_segment_offset: u32,
    payload_start: usize,
    payload_end: usize,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct NativeVulkanH264NalStats {
    bytes: u64,
    has_annex_b_start_codes: bool,
    sps_count: u32,
    pps_count: u32,
    idr_count: u32,
    slice_count: u32,
    first_slice: Option<NativeVulkanH264SlicePayloadSummary>,
    slice_offsets: NativeVulkanH264SliceOffsets,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanH264NalStats {
    pub(super) fn parameter_sets_present(&self) -> bool {
        self.sps_count > 0 && self.pps_count > 0
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanH264SlicePayloadSummary {
    nal_type: u8,
    nal_ref_idc: u8,
    payload_start: usize,
    payload_end: usize,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct NativeVulkanAv1ObuStats {
    bytes: u64,
    obu_count: u32,
    sequence_header_count: u32,
    temporal_delimiter_count: u32,
    frame_header_count: u32,
    tile_group_count: u32,
    frame_count: u32,
    tile_payload_bytes: u64,
    frame_payload_bytes: u64,
    first_frame_header_obu_offset: Option<u64>,
    first_tile_group_obu_offset: Option<u64>,
    sequence_header: Option<NativeVulkanAv1SequenceHeaderSnapshot>,
    first_frame_submit: Option<NativeVulkanAv1FrameSubmitSnapshot>,
    obus: Vec<NativeVulkanAv1ObuSnapshot>,
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanAv1ObuStats {
    pub(super) fn sequence_header_present(&self) -> bool {
        self.sequence_header_count > 0
    }

    pub(super) fn decode_candidate(&self) -> bool {
        self.sequence_header_present()
            && (self.frame_count > 0 || (self.frame_header_count > 0 && self.tile_group_count > 0))
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_av1_obu_stats(bytes: &[u8]) -> Result<NativeVulkanAv1ObuStats, String> {
    let mut stats = NativeVulkanAv1ObuStats {
        bytes: bytes.len() as u64,
        ..Default::default()
    };
    let mut offset = 0usize;
    while offset < bytes.len() {
        let header_offset = offset;
        let header = bytes[offset];
        if header & 0x80 != 0 {
            return Err(format!(
                "AV1 OBU forbidden bit set at byte offset {header_offset}"
            ));
        }
        let obu_type = (header >> 3) & 0x0f;
        let has_extension = header & 0x04 != 0;
        let has_size_field = header & 0x02 != 0;
        if header & 0x01 != 0 {
            return Err(format!(
                "AV1 OBU reserved bit set at byte offset {header_offset}"
            ));
        }
        offset += 1;
        if has_extension {
            if offset >= bytes.len() {
                return Err("AV1 OBU extension flag set without extension byte".to_owned());
            }
            offset += 1;
        }
        if !has_size_field {
            return Err(format!(
                "AV1 OBU at byte offset {header_offset} has no size field; annexb AV1 extraction is not supported yet"
            ));
        }
        let (payload_size, leb_size) = native_vulkan_av1_read_leb128(&bytes[offset..])?;
        offset = offset
            .checked_add(leb_size)
            .ok_or_else(|| "AV1 OBU offset overflow after LEB128".to_owned())?;
        let payload_offset = offset;
        let payload_size_usize = usize::try_from(payload_size)
            .map_err(|_| format!("AV1 OBU payload size {payload_size} exceeds usize"))?;
        let payload_end = payload_offset
            .checked_add(payload_size_usize)
            .ok_or_else(|| "AV1 OBU payload end overflow".to_owned())?;
        if payload_end > bytes.len() {
            return Err(format!(
                "AV1 OBU payload at byte offset {payload_offset} extends past sample end"
            ));
        }

        stats.obu_count = stats.obu_count.saturating_add(1);
        match obu_type {
            1 => {
                stats.sequence_header_count = stats.sequence_header_count.saturating_add(1);
                if stats.sequence_header.is_none() {
                    stats.sequence_header = Some(native_vulkan_parse_av1_sequence_header(
                        &bytes[payload_offset..payload_end],
                    )?);
                }
            }
            2 => stats.temporal_delimiter_count = stats.temporal_delimiter_count.saturating_add(1),
            3 => {
                stats.frame_header_count = stats.frame_header_count.saturating_add(1);
                stats
                    .first_frame_header_obu_offset
                    .get_or_insert(header_offset as u64);
            }
            4 => {
                stats.tile_group_count = stats.tile_group_count.saturating_add(1);
                stats.tile_payload_bytes = stats.tile_payload_bytes.saturating_add(payload_size);
                stats
                    .first_tile_group_obu_offset
                    .get_or_insert(header_offset as u64);
            }
            6 => {
                stats.frame_count = stats.frame_count.saturating_add(1);
                stats.frame_payload_bytes = stats.frame_payload_bytes.saturating_add(payload_size);
                stats
                    .first_frame_header_obu_offset
                    .get_or_insert(header_offset as u64);
            }
            _ => {}
        }
        if stats.obus.len() < 32 {
            stats.obus.push(NativeVulkanAv1ObuSnapshot {
                offset: header_offset as u64,
                header_size: (payload_offset - header_offset) as u64,
                payload_offset: payload_offset as u64,
                payload_size,
                obu_type,
                obu_type_label: native_vulkan_av1_obu_type_label(obu_type),
                has_extension,
                has_size_field,
            });
        }
        offset = payload_end;
    }
    if let Some(sequence_header) = stats.sequence_header.as_ref() {
        stats.first_frame_submit =
            native_vulkan_av1_first_frame_submit_snapshot(bytes, &stats.obus, sequence_header);
    }
    Ok(stats)
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanAv1ObuRange {
    offset: usize,
    end: usize,
    obu_type: u8,
}

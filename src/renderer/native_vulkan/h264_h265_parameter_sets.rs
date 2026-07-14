
#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_first_slice_probe_snapshot_from_stats(
    access_unit: &[u8],
    stats: &NativeVulkanH265NalStats,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<NativeVulkanH265AccessUnitSliceSnapshot, String> {
    let first_slice = stats
        .first_slice
        .ok_or_else(|| "H.265 access unit has no slice NAL".to_owned())?;
    if first_slice.payload_start >= first_slice.payload_end
        || first_slice.payload_end > access_unit.len()
    {
        return Err("H.265 first slice payload range exceeds access-unit bounds".to_owned());
    }
    native_vulkan_h265_slice_probe_snapshot_from_summary(
        first_slice,
        &access_unit[first_slice.payload_start..first_slice.payload_end],
        parameter_sets,
    )
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_slice_probe_snapshot_from_summary(
    slice: NativeVulkanH265SlicePayloadSummary,
    payload: &[u8],
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<NativeVulkanH265AccessUnitSliceSnapshot, String> {
    let idr = matches!(slice.nal_type, 19 | 20);
    let irap = (16..=23).contains(&slice.nal_type);
    let rbsp = native_vulkan_h265_slice_header_rbsp(payload)?;
    if rbsp.len() < 3 {
        return Err("H.265 slice NAL is too short".to_owned());
    }
    let mut bits = NativeVulkanH265BitReader::new(&rbsp);
    bits.skip_bits(16, "h265_nal_unit_header")?;
    let first_slice_segment_in_pic_flag = bits.read_bool("first_slice_segment_in_pic_flag")?;
    if irap {
        bits.read_bool("no_output_of_prior_pics_flag")?;
    }
    let pps_id = bits.read_ue("slice_pic_parameter_set_id")?;
    if pps_id != parameter_sets.pps.id {
        return Err(format!(
            "H.265 first slice PPS id {pps_id} does not match session PPS id {}",
            parameter_sets.pps.id
        ));
    }
    if !first_slice_segment_in_pic_flag {
        return Err(
            "H.265 access unit first slice is not the first slice segment in picture".to_owned(),
        );
    }
    for _ in 0..parameter_sets.pps.num_extra_slice_header_bits {
        bits.skip_bits(1, "slice_reserved_flag")?;
    }
    let slice_type = bits.read_ue("slice_type")?;
    if parameter_sets.pps.output_flag_present_flag {
        bits.read_bool("pic_output_flag")?;
    }
    if parameter_sets.sps.separate_colour_plane_flag {
        bits.skip_bits(2, "colour_plane_id")?;
    }
    let mut short_term_ref_pic_set_sps_flag = false;
    let mut short_term_ref_pic_set_idx = None::<u32>;
    let mut num_delta_pocs_of_ref_rps_idx = 0u8;
    let mut num_bits_for_st_ref_pic_set_in_slice = 0u16;
    let (pic_order_cnt_lsb, short_term_reference_delta_pocs, long_term_references) = if idr {
        (None, NativeVulkanH265ReferenceDeltas::new(), Vec::new())
    } else {
        let pic_order_cnt_lsb = bits.read_bits(
            parameter_sets.sps.log2_max_pic_order_cnt_lsb_minus4 + 4,
            "slice_pic_order_cnt_lsb",
        )?;
        short_term_ref_pic_set_sps_flag = bits.read_bool("short_term_ref_pic_set_sps_flag")?;
        let mut short_term_reference_delta_pocs = NativeVulkanH265ReferenceDeltas::new();
        if short_term_ref_pic_set_sps_flag {
            if parameter_sets.sps.num_short_term_ref_pic_sets == 0 {
                return Err("H.265 slice references SPS short-term RPS but SPS has none".to_owned());
            }
            let selected_ref_pic_set_idx = if parameter_sets.sps.num_short_term_ref_pic_sets > 1 {
                let bits_for_idx =
                    32 - (parameter_sets.sps.num_short_term_ref_pic_sets - 1).leading_zeros();
                bits.read_bits(bits_for_idx, "short_term_ref_pic_set_idx")?
            } else {
                0
            };
            short_term_ref_pic_set_idx = Some(selected_ref_pic_set_idx);
            let short_term_ref_pic_set = parameter_sets
                .sps
                .short_term_ref_pic_sets
                .get(selected_ref_pic_set_idx as usize)
                .ok_or_else(|| {
                    format!(
                        "H.265 slice short_term_ref_pic_set_idx {selected_ref_pic_set_idx} exceeds SPS RPS count {}",
                        parameter_sets.sps.short_term_ref_pic_sets.len()
                    )
                })?;
            short_term_reference_delta_pocs.extend_used_ref_pic_set(short_term_ref_pic_set);
        } else {
            let rps_bit_start = bits.bit_offset();
            let short_term_ref_pic_set = native_vulkan_h265_read_short_term_ref_pic_set(
                &mut bits,
                parameter_sets.sps.num_short_term_ref_pic_sets,
                parameter_sets.sps.num_short_term_ref_pic_sets,
                &parameter_sets.sps.short_term_ref_pic_sets,
            )?;
            let rps_bit_count = bits
                .bit_offset()
                .checked_sub(rps_bit_start)
                .ok_or_else(|| "H.265 short-term RPS bit position underflow".to_owned())?;
            num_bits_for_st_ref_pic_set_in_slice = u16::try_from(rps_bit_count)
                .map_err(|_| "H.265 short-term RPS bit count exceeds u16 range".to_owned())?;
            if short_term_ref_pic_set.inter_ref_pic_set_prediction_flag {
                num_delta_pocs_of_ref_rps_idx = native_vulkan_h265_u8(
                    short_term_ref_pic_set.num_delta_pocs_of_ref_rps_idx,
                    "NumDeltaPocsOfRefRpsIdx",
                )?;
            }
            short_term_reference_delta_pocs.extend_used_ref_pic_set(&short_term_ref_pic_set);
        }
        let long_term_references =
            native_vulkan_h265_read_long_term_references(&mut bits, &parameter_sets.sps)?;
        (
            Some(pic_order_cnt_lsb),
            short_term_reference_delta_pocs,
            long_term_references,
        )
    };

    Ok(NativeVulkanH265AccessUnitSliceSnapshot {
        nal_type: slice.nal_type,
        nal_type_label: native_vulkan_h265_nal_type_label(slice.nal_type),
        slice_segment_offset: slice.slice_segment_offset,
        first_slice_segment_in_pic_flag,
        slice_type,
        pps_id,
        pic_order_cnt_lsb,
        short_term_ref_pic_set_sps_flag,
        short_term_ref_pic_set_idx,
        num_delta_pocs_of_ref_rps_idx,
        num_bits_for_st_ref_pic_set_in_slice,
        short_term_reference_delta_pocs,
        long_term_references,
        idr,
        irap,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_read_long_term_references(
    bits: &mut NativeVulkanH265BitReader<'_>,
    sps: &NativeVulkanH265SpsSnapshot,
) -> Result<Vec<NativeVulkanH265LongTermReferenceSnapshot>, String> {
    if !sps.long_term_ref_pics_present_flag {
        return Ok(Vec::new());
    }

    let sps_ref_count = sps.long_term_ref_pics_sps.len() as u32;
    let num_long_term_sps = if sps_ref_count > 0 {
        bits.read_ue("num_long_term_sps")?
    } else {
        0
    };
    let num_long_term_pics = bits.read_ue("num_long_term_pics")?;
    if num_long_term_sps > sps_ref_count {
        return Err(format!(
            "H.265 slice requests {num_long_term_sps} SPS long-term refs but SPS has {sps_ref_count}"
        ));
    }
    let total = num_long_term_sps
        .checked_add(num_long_term_pics)
        .ok_or_else(|| "H.265 long-term reference count overflow".to_owned())?;
    if total > 32 {
        return Err(format!(
            "H.265 slice has {total} long-term refs; maximum supported is 32"
        ));
    }

    let mut references = Vec::with_capacity(total as usize);
    let lt_idx_sps_bits = native_vulkan_h265_ceil_log2(sps_ref_count);
    let poc_lsb_bits = sps
        .log2_max_pic_order_cnt_lsb_minus4
        .checked_add(4)
        .ok_or_else(|| "H.265 long-term POC LSB bit count overflow".to_owned())?;
    let mut previous_delta_poc_msb_cycle_lt = None::<u32>;
    for index in 0..total {
        let (from_sps, lt_idx_sps, poc_lsb, used_by_current) = if index < num_long_term_sps {
            let lt_idx_sps = if sps_ref_count > 1 {
                bits.read_bits(lt_idx_sps_bits, "lt_idx_sps")?
            } else {
                0
            };
            let entry = sps
                .long_term_ref_pics_sps
                .get(lt_idx_sps as usize)
                .ok_or_else(|| {
                    format!(
                        "H.265 slice lt_idx_sps {lt_idx_sps} exceeds SPS long-term ref count {sps_ref_count}"
                    )
                })?;
            (
                true,
                Some(lt_idx_sps),
                entry.lt_ref_pic_poc_lsb_sps,
                entry.used_by_curr_pic_lt_sps_flag,
            )
        } else {
            let poc_lsb = bits.read_bits(poc_lsb_bits, "poc_lsb_lt")?;
            let used_by_current = bits.read_bool("used_by_curr_pic_lt_flag")?;
            (false, None, poc_lsb, used_by_current)
        };
        let delta_poc_msb_present_flag = bits.read_bool("delta_poc_msb_present_flag")?;
        let delta_poc_msb_cycle_lt = if delta_poc_msb_present_flag {
            let value = bits.read_ue("delta_poc_msb_cycle_lt")?;
            let derived = if index == 0 || index == num_long_term_sps {
                value
            } else {
                previous_delta_poc_msb_cycle_lt
                    .unwrap_or(0)
                    .saturating_add(value)
            };
            previous_delta_poc_msb_cycle_lt = Some(derived);
            Some(derived)
        } else {
            None
        };
        references.push(NativeVulkanH265LongTermReferenceSnapshot {
            from_sps,
            lt_idx_sps,
            poc_lsb,
            used_by_current,
            delta_poc_msb_present_flag,
            delta_poc_msb_cycle_lt,
        });
    }

    Ok(references)
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_ceil_log2(value: u32) -> u32 {
    if value <= 1 {
        0
    } else {
        u32::BITS - (value - 1).leading_zeros()
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h265_parameter_sets(
    access_unit: &[u8],
) -> Result<NativeVulkanH265ParameterSetSnapshot, String> {
    let nal_units = native_vulkan_h265_nal_payloads(access_unit);
    let vps_payload = nal_units
        .iter()
        .find(|unit| unit.nal_type == 32)
        .ok_or_else(|| "H.265 access unit has no VPS NAL".to_owned())?;
    let sps_payload = nal_units
        .iter()
        .find(|unit| unit.nal_type == 33)
        .ok_or_else(|| "H.265 access unit has no SPS NAL".to_owned())?;
    let pps_payload = nal_units
        .iter()
        .find(|unit| unit.nal_type == 34)
        .ok_or_else(|| "H.265 access unit has no PPS NAL".to_owned())?;

    let vps = native_vulkan_parse_h265_vps(vps_payload.payload)?;
    let sps = native_vulkan_parse_h265_sps(sps_payload.payload)?;
    let pps = native_vulkan_parse_h265_pps(pps_payload.payload)?;
    let h265_main8_compatible = sps.bit_depth_luma_minus8 == 0
        && sps.bit_depth_chroma_minus8 == 0
        && sps.profile.main_compatible();
    let h265_main10_compatible = sps.bit_depth_luma_minus8 == 2
        && sps.bit_depth_chroma_minus8 == 2
        && sps.profile.main10_compatible();
    let requested_profile_compatible = sps.chroma_format_idc == 1
        && !sps.separate_colour_plane_flag
        && (h265_main8_compatible || h265_main10_compatible);
    let vulkan_std_session_parameters_ready = requested_profile_compatible
        && vps.id == sps.vps_id
        && sps.id == pps.sps_id
        && native_vulkan_h265_sps_short_term_ref_pic_sets_supported(&sps.short_term_ref_pic_sets)
        && native_vulkan_h265_sps_long_term_ref_pics_supported(&sps.long_term_ref_pics_sps)
        && !sps.scaling_list_enabled_flag
        && !sps.sps_scaling_list_data_present_flag
        && !sps.pcm_enabled_flag
        && !sps.sps_extension_present_flag
        && !sps
            .vui
            .as_ref()
            .is_some_and(|vui| vui.vui_hrd_parameters_present_flag)
        && !pps.tiles_enabled_flag
        && !pps.pps_scaling_list_data_present_flag
        && !pps.pps_extension_present_flag;

    Ok(NativeVulkanH265ParameterSetSnapshot {
        parser: "native-rust-h265-vps-sps-pps",
        vps: NativeVulkanH265VpsSnapshot {
            id: vps.id,
            max_layers_minus1: vps.max_layers_minus1,
            max_sub_layers_minus1: vps.max_sub_layers_minus1,
            temporal_id_nesting_flag: vps.temporal_id_nesting_flag,
            sub_layer_ordering_info_present_flag: vps
                .dec_pic_buf_mgr
                .sub_layer_ordering_info_present_flag,
            profile_idc: vps.profile.profile_idc,
            profile_label: native_vulkan_h265_profile_idc_label(vps.profile.profile_idc),
            tier_flag: vps.profile.tier_flag,
            progressive_source_flag: vps.profile.progressive_source_flag,
            interlaced_source_flag: vps.profile.interlaced_source_flag,
            non_packed_constraint_flag: vps.profile.non_packed_constraint_flag,
            frame_only_constraint_flag: vps.profile.frame_only_constraint_flag,
            level_idc: vps.profile.level_idc,
            level_label: native_vulkan_h265_level_idc_byte_label(vps.profile.level_idc),
            dec_pic_buf_mgr: native_vulkan_h265_dec_pic_buf_mgr_snapshot(&vps.dec_pic_buf_mgr),
            timing_info_present_flag: vps.timing_info_present_flag,
            poc_proportional_to_timing_flag: vps.poc_proportional_to_timing_flag,
            num_units_in_tick: vps.num_units_in_tick,
            time_scale: vps.time_scale,
            num_ticks_poc_diff_one_minus1: vps.num_ticks_poc_diff_one_minus1,
        },
        sps: NativeVulkanH265SpsSnapshot {
            id: sps.id,
            vps_id: sps.vps_id,
            max_sub_layers_minus1: sps.max_sub_layers_minus1,
            temporal_id_nesting_flag: sps.temporal_id_nesting_flag,
            sub_layer_ordering_info_present_flag: sps
                .dec_pic_buf_mgr
                .sub_layer_ordering_info_present_flag,
            profile_idc: sps.profile.profile_idc,
            profile_label: native_vulkan_h265_profile_idc_label(sps.profile.profile_idc),
            tier_flag: sps.profile.tier_flag,
            progressive_source_flag: sps.profile.progressive_source_flag,
            interlaced_source_flag: sps.profile.interlaced_source_flag,
            non_packed_constraint_flag: sps.profile.non_packed_constraint_flag,
            frame_only_constraint_flag: sps.profile.frame_only_constraint_flag,
            level_idc: sps.profile.level_idc,
            level_label: native_vulkan_h265_level_idc_byte_label(sps.profile.level_idc),
            dec_pic_buf_mgr: native_vulkan_h265_dec_pic_buf_mgr_snapshot(&sps.dec_pic_buf_mgr),
            chroma_format_idc: sps.chroma_format_idc,
            chroma_format_label: native_vulkan_h265_chroma_format_label(sps.chroma_format_idc),
            separate_colour_plane_flag: sps.separate_colour_plane_flag,
            width: sps.width,
            height: sps.height,
            conformance_window_flag: sps.conformance_window_flag,
            conf_win_left_offset: sps.conf_win_left_offset,
            conf_win_right_offset: sps.conf_win_right_offset,
            conf_win_top_offset: sps.conf_win_top_offset,
            conf_win_bottom_offset: sps.conf_win_bottom_offset,
            bit_depth_luma_minus8: sps.bit_depth_luma_minus8,
            bit_depth_chroma_minus8: sps.bit_depth_chroma_minus8,
            log2_max_pic_order_cnt_lsb_minus4: sps.log2_max_pic_order_cnt_lsb_minus4,
            log2_min_luma_coding_block_size_minus3: sps.log2_min_luma_coding_block_size_minus3,
            log2_diff_max_min_luma_coding_block_size: sps.log2_diff_max_min_luma_coding_block_size,
            log2_min_luma_transform_block_size_minus2: sps
                .log2_min_luma_transform_block_size_minus2,
            log2_diff_max_min_luma_transform_block_size: sps
                .log2_diff_max_min_luma_transform_block_size,
            max_transform_hierarchy_depth_inter: sps.max_transform_hierarchy_depth_inter,
            max_transform_hierarchy_depth_intra: sps.max_transform_hierarchy_depth_intra,
            scaling_list_enabled_flag: sps.scaling_list_enabled_flag,
            sps_scaling_list_data_present_flag: sps.sps_scaling_list_data_present_flag,
            amp_enabled_flag: sps.amp_enabled_flag,
            sample_adaptive_offset_enabled_flag: sps.sample_adaptive_offset_enabled_flag,
            pcm_enabled_flag: sps.pcm_enabled_flag,
            pcm_loop_filter_disabled_flag: sps.pcm_loop_filter_disabled_flag,
            num_short_term_ref_pic_sets: sps.num_short_term_ref_pic_sets,
            short_term_ref_pic_sets: sps.short_term_ref_pic_sets.clone(),
            long_term_ref_pics_present_flag: sps.long_term_ref_pics_present_flag,
            long_term_ref_pics_sps: sps.long_term_ref_pics_sps.clone(),
            temporal_mvp_enabled_flag: sps.temporal_mvp_enabled_flag,
            strong_intra_smoothing_enabled_flag: sps.strong_intra_smoothing_enabled_flag,
            vui_parameters_present_flag: sps.vui_parameters_present_flag,
            vui: sps.vui.as_ref().map(native_vulkan_h265_vui_snapshot),
            sps_extension_present_flag: sps.sps_extension_present_flag,
        },
        pps: NativeVulkanH265PpsSnapshot {
            id: pps.id,
            sps_id: pps.sps_id,
            dependent_slice_segments_enabled_flag: pps.dependent_slice_segments_enabled_flag,
            output_flag_present_flag: pps.output_flag_present_flag,
            num_extra_slice_header_bits: pps.num_extra_slice_header_bits,
            sign_data_hiding_enabled_flag: pps.sign_data_hiding_enabled_flag,
            cabac_init_present_flag: pps.cabac_init_present_flag,
            num_ref_idx_l0_default_active_minus1: pps.num_ref_idx_l0_default_active_minus1,
            num_ref_idx_l1_default_active_minus1: pps.num_ref_idx_l1_default_active_minus1,
            init_qp_minus26: pps.init_qp_minus26,
            constrained_intra_pred_flag: pps.constrained_intra_pred_flag,
            transform_skip_enabled_flag: pps.transform_skip_enabled_flag,
            cu_qp_delta_enabled_flag: pps.cu_qp_delta_enabled_flag,
            diff_cu_qp_delta_depth: pps.diff_cu_qp_delta_depth,
            cb_qp_offset: pps.cb_qp_offset,
            cr_qp_offset: pps.cr_qp_offset,
            slice_chroma_qp_offsets_present_flag: pps.slice_chroma_qp_offsets_present_flag,
            weighted_pred_flag: pps.weighted_pred_flag,
            weighted_bipred_flag: pps.weighted_bipred_flag,
            transquant_bypass_enabled_flag: pps.transquant_bypass_enabled_flag,
            tiles_enabled_flag: pps.tiles_enabled_flag,
            entropy_coding_sync_enabled_flag: pps.entropy_coding_sync_enabled_flag,
            uniform_spacing_flag: pps.uniform_spacing_flag,
            num_tile_columns_minus1: pps.num_tile_columns_minus1,
            num_tile_rows_minus1: pps.num_tile_rows_minus1,
            loop_filter_across_tiles_enabled_flag: pps.loop_filter_across_tiles_enabled_flag,
            loop_filter_across_slices_enabled_flag: pps.loop_filter_across_slices_enabled_flag,
            deblocking_filter_control_present_flag: pps.deblocking_filter_control_present_flag,
            deblocking_filter_override_enabled_flag: pps.deblocking_filter_override_enabled_flag,
            pps_deblocking_filter_disabled_flag: pps.pps_deblocking_filter_disabled_flag,
            pps_beta_offset_div2: pps.pps_beta_offset_div2,
            pps_tc_offset_div2: pps.pps_tc_offset_div2,
            pps_scaling_list_data_present_flag: pps.pps_scaling_list_data_present_flag,
            lists_modification_present_flag: pps.lists_modification_present_flag,
            log2_parallel_merge_level_minus2: pps.log2_parallel_merge_level_minus2,
            slice_segment_header_extension_present_flag: pps
                .slice_segment_header_extension_present_flag,
            pps_extension_present_flag: pps.pps_extension_present_flag,
        },
        requested_profile_compatible,
        vulkan_std_session_parameters_ready,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_dec_pic_buf_mgr_snapshot(
    dec_pic_buf_mgr: &NativeVulkanH265ParsedDecPicBufMgr,
) -> NativeVulkanH265DecPicBufMgrSnapshot {
    NativeVulkanH265DecPicBufMgrSnapshot {
        max_latency_increase_plus1: dec_pic_buf_mgr.max_latency_increase_plus1,
        max_dec_pic_buffering_minus1: dec_pic_buf_mgr.max_dec_pic_buffering_minus1,
        max_num_reorder_pics: dec_pic_buf_mgr.max_num_reorder_pics,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h265_vui_snapshot(vui: &NativeVulkanH265ParsedVui) -> NativeVulkanH265VuiSnapshot {
    NativeVulkanH265VuiSnapshot {
        aspect_ratio_info_present_flag: vui.aspect_ratio_info_present_flag,
        aspect_ratio_idc: vui.aspect_ratio_idc,
        sar_width: vui.sar_width,
        sar_height: vui.sar_height,
        overscan_info_present_flag: vui.overscan_info_present_flag,
        overscan_appropriate_flag: vui.overscan_appropriate_flag,
        video_signal_type_present_flag: vui.video_signal_type_present_flag,
        video_format: vui.video_format,
        video_full_range_flag: vui.video_full_range_flag,
        colour_description_present_flag: vui.colour_description_present_flag,
        colour_primaries: vui.colour_primaries,
        transfer_characteristics: vui.transfer_characteristics,
        matrix_coeffs: vui.matrix_coeffs,
        chroma_loc_info_present_flag: vui.chroma_loc_info_present_flag,
        chroma_sample_loc_type_top_field: vui.chroma_sample_loc_type_top_field,
        chroma_sample_loc_type_bottom_field: vui.chroma_sample_loc_type_bottom_field,
        neutral_chroma_indication_flag: vui.neutral_chroma_indication_flag,
        field_seq_flag: vui.field_seq_flag,
        frame_field_info_present_flag: vui.frame_field_info_present_flag,
        default_display_window_flag: vui.default_display_window_flag,
        def_disp_win_left_offset: vui.def_disp_win_left_offset,
        def_disp_win_right_offset: vui.def_disp_win_right_offset,
        def_disp_win_top_offset: vui.def_disp_win_top_offset,
        def_disp_win_bottom_offset: vui.def_disp_win_bottom_offset,
        vui_timing_info_present_flag: vui.vui_timing_info_present_flag,
        vui_num_units_in_tick: vui.vui_num_units_in_tick,
        vui_time_scale: vui.vui_time_scale,
        vui_poc_proportional_to_timing_flag: vui.vui_poc_proportional_to_timing_flag,
        vui_num_ticks_poc_diff_one_minus1: vui.vui_num_ticks_poc_diff_one_minus1,
        vui_hrd_parameters_present_flag: vui.vui_hrd_parameters_present_flag,
        bitstream_restriction_flag: vui.bitstream_restriction_flag,
        tiles_fixed_structure_flag: vui.tiles_fixed_structure_flag,
        motion_vectors_over_pic_boundaries_flag: vui.motion_vectors_over_pic_boundaries_flag,
        restricted_ref_pic_lists_flag: vui.restricted_ref_pic_lists_flag,
        min_spatial_segmentation_idc: vui.min_spatial_segmentation_idc,
        max_bytes_per_pic_denom: vui.max_bytes_per_pic_denom,
        max_bits_per_min_cu_denom: vui.max_bits_per_min_cu_denom,
        log2_max_mv_length_horizontal: vui.log2_max_mv_length_horizontal,
        log2_max_mv_length_vertical: vui.log2_max_mv_length_vertical,
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h264_parameter_sets(
    access_unit: &[u8],
) -> Result<NativeVulkanH264ParameterSetSnapshot, String> {
    let nal_units = native_vulkan_h264_nal_payloads(access_unit);
    let sps_payload = nal_units
        .iter()
        .find(|unit| unit.nal_type == 7)
        .ok_or_else(|| "H.264 access unit has no SPS NAL".to_owned())?;
    let pps_payload = nal_units
        .iter()
        .find(|unit| unit.nal_type == 8)
        .ok_or_else(|| "H.264 access unit has no PPS NAL".to_owned())?;

    let sps = native_vulkan_parse_h264_sps(sps_payload.payload)?;
    let pps = native_vulkan_parse_h264_pps(pps_payload.payload, &sps)?;
    let requested_profile_compatible =
        h264::native_vulkan_h264_profile_is_8bit_420_decode_candidate(sps.profile_idc)
            && sps.chroma_format_idc == 1
            && !sps.separate_colour_plane_flag
            && sps.bit_depth_luma_minus8 == 0
            && sps.bit_depth_chroma_minus8 == 0;
    let vulkan_std_session_parameters_ready = requested_profile_compatible
        && sps.id == pps.sps_id
        && pps.num_slice_groups_minus1 == 0
        && sps.pic_order_cnt_type <= 2
        && sps.offset_for_ref_frame.len() <= u8::MAX as usize
        && !sps.seq_scaling_matrix_present_flag
        && !pps.pic_scaling_matrix_present_flag;

    Ok(NativeVulkanH264ParameterSetSnapshot {
        parser: "native-rust-h264-sps-pps",
        sps,
        pps,
        requested_profile_compatible,
        vulkan_std_session_parameters_ready,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h264_sps(payload: &[u8]) -> Result<NativeVulkanH264SpsSnapshot, String> {
    let rbsp = native_vulkan_h264_rbsp(payload)?;
    if rbsp.len() < 4 {
        return Err("H.264 SPS NAL is too short".to_owned());
    }
    let mut bits = NativeVulkanH264BitReader::new(&rbsp[1..]);
    let profile_idc = native_vulkan_h264_u8(bits.read_bits(8, "profile_idc")?, "profile_idc")?;
    let constraint_flags = native_vulkan_h264_u8(
        bits.read_bits(8, "constraint_set_flags")?,
        "constraint_set_flags",
    )?;
    let constraint_set0_flag = constraint_flags & 0x80 != 0;
    let constraint_set1_flag = constraint_flags & 0x40 != 0;
    let constraint_set2_flag = constraint_flags & 0x20 != 0;
    let constraint_set3_flag = constraint_flags & 0x10 != 0;
    let constraint_set4_flag = constraint_flags & 0x08 != 0;
    let constraint_set5_flag = constraint_flags & 0x04 != 0;
    let level_idc = native_vulkan_h264_u8(bits.read_bits(8, "level_idc")?, "level_idc")?;
    let id = bits.read_ue("seq_parameter_set_id")?;

    let mut chroma_format_idc = 1;
    let mut separate_colour_plane_flag = false;
    let mut bit_depth_luma_minus8 = 0;
    let mut bit_depth_chroma_minus8 = 0;
    let mut qpprime_y_zero_transform_bypass_flag = false;
    let mut seq_scaling_matrix_present_flag = false;
    if h264::native_vulkan_h264_profile_has_high_syntax(profile_idc) {
        chroma_format_idc = bits.read_ue("chroma_format_idc")?;
        if chroma_format_idc > 3 {
            return Err(format!(
                "H.264 chroma_format_idc {chroma_format_idc} is not supported"
            ));
        }
        if chroma_format_idc == 3 {
            separate_colour_plane_flag = bits.read_bool("separate_colour_plane_flag")?;
        }
        bit_depth_luma_minus8 = bits.read_ue("bit_depth_luma_minus8")?;
        bit_depth_chroma_minus8 = bits.read_ue("bit_depth_chroma_minus8")?;
        qpprime_y_zero_transform_bypass_flag =
            bits.read_bool("qpprime_y_zero_transform_bypass_flag")?;
        seq_scaling_matrix_present_flag = bits.read_bool("seq_scaling_matrix_present_flag")?;
        if seq_scaling_matrix_present_flag {
            let scaling_list_count = if chroma_format_idc != 3 { 8 } else { 12 };
            for index in 0..scaling_list_count {
                if bits.read_bool("seq_scaling_list_present_flag")? {
                    let size = if index < 6 { 16 } else { 64 };
                    native_vulkan_h264_skip_scaling_list(&mut bits, size)?;
                }
            }
        }
    }

    let log2_max_frame_num_minus4 = bits.read_ue("log2_max_frame_num_minus4")?;
    let pic_order_cnt_type = bits.read_ue("pic_order_cnt_type")?;
    let mut log2_max_pic_order_cnt_lsb_minus4 = 0;
    let mut delta_pic_order_always_zero_flag = false;
    let mut offset_for_non_ref_pic = 0;
    let mut offset_for_top_to_bottom_field = 0;
    let mut offset_for_ref_frame = Vec::new();
    match pic_order_cnt_type {
        0 => {
            log2_max_pic_order_cnt_lsb_minus4 =
                bits.read_ue("log2_max_pic_order_cnt_lsb_minus4")?;
        }
        1 => {
            delta_pic_order_always_zero_flag =
                bits.read_bool("delta_pic_order_always_zero_flag")?;
            offset_for_non_ref_pic = bits.read_se("offset_for_non_ref_pic")?;
            offset_for_top_to_bottom_field = bits.read_se("offset_for_top_to_bottom_field")?;
            let num_ref_frames_in_pic_order_cnt_cycle =
                bits.read_ue("num_ref_frames_in_pic_order_cnt_cycle")?;
            if num_ref_frames_in_pic_order_cnt_cycle > u8::MAX as u32 {
                return Err(format!(
                    "H.264 num_ref_frames_in_pic_order_cnt_cycle {num_ref_frames_in_pic_order_cnt_cycle} exceeds u8 range"
                ));
            }
            for _ in 0..num_ref_frames_in_pic_order_cnt_cycle {
                offset_for_ref_frame.push(bits.read_se("offset_for_ref_frame")?);
            }
        }
        2 => {}
        _ => {
            return Err(format!(
                "H.264 pic_order_cnt_type {pic_order_cnt_type} is not supported"
            ));
        }
    }

    let max_num_ref_frames = bits.read_ue("max_num_ref_frames")?;
    let gaps_in_frame_num_value_allowed_flag =
        bits.read_bool("gaps_in_frame_num_value_allowed_flag")?;
    let pic_width_in_mbs_minus1 = bits.read_ue("pic_width_in_mbs_minus1")?;
    let pic_height_in_map_units_minus1 = bits.read_ue("pic_height_in_map_units_minus1")?;
    let frame_mbs_only_flag = bits.read_bool("frame_mbs_only_flag")?;
    let mb_adaptive_frame_field_flag = if frame_mbs_only_flag {
        false
    } else {
        bits.read_bool("mb_adaptive_frame_field_flag")?
    };
    let direct_8x8_inference_flag = bits.read_bool("direct_8x8_inference_flag")?;
    let frame_cropping_flag = bits.read_bool("frame_cropping_flag")?;
    let (
        frame_crop_left_offset,
        frame_crop_right_offset,
        frame_crop_top_offset,
        frame_crop_bottom_offset,
    ) = if frame_cropping_flag {
        (
            bits.read_ue("frame_crop_left_offset")?,
            bits.read_ue("frame_crop_right_offset")?,
            bits.read_ue("frame_crop_top_offset")?,
            bits.read_ue("frame_crop_bottom_offset")?,
        )
    } else {
        (0, 0, 0, 0)
    };
    let vui_parameters_present_flag = bits.read_bool("vui_parameters_present_flag")?;
    let vui = if vui_parameters_present_flag {
        Some(native_vulkan_parse_h264_vui_parameters(
            &mut bits,
            &rbsp[1..],
        )?)
    } else {
        None
    };
    let (width, height) = native_vulkan_h264_sps_dimensions(
        chroma_format_idc,
        separate_colour_plane_flag,
        pic_width_in_mbs_minus1,
        pic_height_in_map_units_minus1,
        frame_mbs_only_flag,
        frame_crop_left_offset,
        frame_crop_right_offset,
        frame_crop_top_offset,
        frame_crop_bottom_offset,
    )?;

    Ok(NativeVulkanH264SpsSnapshot {
        id,
        profile_idc,
        profile_label: h264::native_vulkan_h264_profile_idc_label(profile_idc),
        constraint_set0_flag,
        constraint_set1_flag,
        constraint_set2_flag,
        constraint_set3_flag,
        constraint_set4_flag,
        constraint_set5_flag,
        level_idc,
        level_label: native_vulkan_h264_level_idc_byte_label(level_idc),
        chroma_format_idc,
        chroma_format_label: native_vulkan_h264_chroma_format_label(chroma_format_idc),
        separate_colour_plane_flag,
        bit_depth_luma_minus8,
        bit_depth_chroma_minus8,
        qpprime_y_zero_transform_bypass_flag,
        seq_scaling_matrix_present_flag,
        log2_max_frame_num_minus4,
        pic_order_cnt_type,
        log2_max_pic_order_cnt_lsb_minus4,
        delta_pic_order_always_zero_flag,
        offset_for_non_ref_pic,
        offset_for_top_to_bottom_field,
        offset_for_ref_frame,
        max_num_ref_frames,
        gaps_in_frame_num_value_allowed_flag,
        pic_width_in_mbs_minus1,
        pic_height_in_map_units_minus1,
        frame_mbs_only_flag,
        mb_adaptive_frame_field_flag,
        direct_8x8_inference_flag,
        frame_cropping_flag,
        frame_crop_left_offset,
        frame_crop_right_offset,
        frame_crop_top_offset,
        frame_crop_bottom_offset,
        vui_parameters_present_flag,
        vui,
        width,
        height,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_parse_h264_pps(
    payload: &[u8],
    sps: &NativeVulkanH264SpsSnapshot,
) -> Result<NativeVulkanH264PpsSnapshot, String> {
    let rbsp = native_vulkan_h264_rbsp(payload)?;
    if rbsp.len() < 2 {
        return Err("H.264 PPS NAL is too short".to_owned());
    }
    let mut bits = NativeVulkanH264BitReader::new(&rbsp[1..]);
    let id = bits.read_ue("pic_parameter_set_id")?;
    let sps_id = bits.read_ue("seq_parameter_set_id")?;
    let entropy_coding_mode_flag = bits.read_bool("entropy_coding_mode_flag")?;
    let bottom_field_pic_order_in_frame_present_flag =
        bits.read_bool("bottom_field_pic_order_in_frame_present_flag")?;
    let num_slice_groups_minus1 = bits.read_ue("num_slice_groups_minus1")?;
    if num_slice_groups_minus1 > 0 {
        return Err(format!(
            "H.264 num_slice_groups_minus1 {num_slice_groups_minus1} is not supported"
        ));
    }
    let num_ref_idx_l0_default_active_minus1 =
        bits.read_ue("num_ref_idx_l0_default_active_minus1")?;
    let num_ref_idx_l1_default_active_minus1 =
        bits.read_ue("num_ref_idx_l1_default_active_minus1")?;
    let weighted_pred_flag = bits.read_bool("weighted_pred_flag")?;
    let weighted_bipred_idc = bits.read_bits(2, "weighted_bipred_idc")?;
    let pic_init_qp_minus26 = bits.read_se("pic_init_qp_minus26")?;
    let pic_init_qs_minus26 = bits.read_se("pic_init_qs_minus26")?;
    let chroma_qp_index_offset = bits.read_se("chroma_qp_index_offset")?;
    let deblocking_filter_control_present_flag =
        bits.read_bool("deblocking_filter_control_present_flag")?;
    let constrained_intra_pred_flag = bits.read_bool("constrained_intra_pred_flag")?;
    let redundant_pic_cnt_present_flag = bits.read_bool("redundant_pic_cnt_present_flag")?;
    let mut transform_8x8_mode_flag = false;
    let mut pic_scaling_matrix_present_flag = false;
    let mut second_chroma_qp_index_offset = chroma_qp_index_offset;
    if native_vulkan_rbsp_more_data(&rbsp[1..], bits.bit_offset()) {
        transform_8x8_mode_flag = bits.read_bool("transform_8x8_mode_flag")?;
        pic_scaling_matrix_present_flag = bits.read_bool("pic_scaling_matrix_present_flag")?;
        if pic_scaling_matrix_present_flag {
            let scaling_list_count = 6 + if transform_8x8_mode_flag {
                if sps.chroma_format_idc != 3 { 2 } else { 6 }
            } else {
                0
            };
            for index in 0..scaling_list_count {
                if bits.read_bool("pic_scaling_list_present_flag")? {
                    let size = if index < 6 { 16 } else { 64 };
                    native_vulkan_h264_skip_scaling_list(&mut bits, size)?;
                }
            }
        }
        if native_vulkan_rbsp_more_data(&rbsp[1..], bits.bit_offset()) {
            second_chroma_qp_index_offset = bits.read_se("second_chroma_qp_index_offset")?;
        }
    }

    Ok(NativeVulkanH264PpsSnapshot {
        id,
        sps_id,
        entropy_coding_mode_flag,
        bottom_field_pic_order_in_frame_present_flag,
        num_slice_groups_minus1,
        num_ref_idx_l0_default_active_minus1,
        num_ref_idx_l1_default_active_minus1,
        weighted_pred_flag,
        weighted_bipred_idc,
        pic_init_qp_minus26,
        pic_init_qs_minus26,
        chroma_qp_index_offset,
        deblocking_filter_control_present_flag,
        constrained_intra_pred_flag,
        redundant_pic_cnt_present_flag,
        transform_8x8_mode_flag,
        pic_scaling_matrix_present_flag,
        second_chroma_qp_index_offset,
    })
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_sps_dimensions(
    chroma_format_idc: u32,
    separate_colour_plane_flag: bool,
    pic_width_in_mbs_minus1: u32,
    pic_height_in_map_units_minus1: u32,
    frame_mbs_only_flag: bool,
    frame_crop_left_offset: u32,
    frame_crop_right_offset: u32,
    frame_crop_top_offset: u32,
    frame_crop_bottom_offset: u32,
) -> Result<(u32, u32), String> {
    let chroma_array_type = if separate_colour_plane_flag {
        0
    } else {
        chroma_format_idc
    };
    let (sub_width_c, sub_height_c): (u32, u32) = match chroma_format_idc {
        0 => (1, 1),
        1 => (2, 2),
        2 => (2, 1),
        3 => (1, 1),
        _ => {
            return Err(format!(
                "H.264 chroma_format_idc {chroma_format_idc} is not supported"
            ));
        }
    };
    let frame_height_in_mbs_factor = if frame_mbs_only_flag { 1 } else { 2 };
    let (crop_unit_x, crop_unit_y) = if chroma_array_type == 0 {
        (1, frame_height_in_mbs_factor)
    } else {
        (
            sub_width_c,
            sub_height_c.saturating_mul(frame_height_in_mbs_factor),
        )
    };
    let coded_width = pic_width_in_mbs_minus1
        .checked_add(1)
        .and_then(|mbs| mbs.checked_mul(16))
        .ok_or_else(|| "H.264 SPS width overflow".to_owned())?;
    let coded_height = pic_height_in_map_units_minus1
        .checked_add(1)
        .and_then(|map_units| map_units.checked_mul(frame_height_in_mbs_factor))
        .and_then(|mbs| mbs.checked_mul(16))
        .ok_or_else(|| "H.264 SPS height overflow".to_owned())?;
    let crop_width = frame_crop_left_offset
        .checked_add(frame_crop_right_offset)
        .and_then(|crop| crop.checked_mul(crop_unit_x))
        .ok_or_else(|| "H.264 SPS crop width overflow".to_owned())?;
    let crop_height = frame_crop_top_offset
        .checked_add(frame_crop_bottom_offset)
        .and_then(|crop| crop.checked_mul(crop_unit_y))
        .ok_or_else(|| "H.264 SPS crop height overflow".to_owned())?;
    Ok((
        coded_width.saturating_sub(crop_width),
        coded_height.saturating_sub(crop_height),
    ))
}

#[cfg(any(feature = "native-vulkan-video", test))]
fn native_vulkan_h264_skip_scaling_list(
    bits: &mut NativeVulkanH264BitReader<'_>,
    size: u32,
) -> Result<(), String> {
    let mut last_scale = 8i32;
    let mut next_scale = 8i32;
    for _ in 0..size {
        if next_scale != 0 {
            let delta_scale = bits.read_se("delta_scale")?;
            next_scale = (last_scale + delta_scale + 256) % 256;
        }
        if next_scale != 0 {
            last_scale = next_scale;
        }
    }
    Ok(())
}

include!("h264_h265_parameter_sets/h264_bitstream.rs");

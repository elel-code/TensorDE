
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn av1_planner_uses_transient_output_slot_when_reference_map_is_full() {
        let mut temporal_units = (0..8)
            .map(|index| test_av1_temporal_unit(index, 1u8 << index))
            .collect::<Vec<_>>();
        temporal_units.push(test_av1_temporal_unit(8, 0));

        let (dpb_slots, plan) = native_vulkan_av1_min_decodable_dpb_plan(&temporal_units, 16);
        let transient = plan.last().expect("AV1 transient frame is planned");

        assert_eq!(dpb_slots, 9);
        assert!(transient.ready_for_decode_submit);
        assert_eq!(transient.output_slot, Some(8));
        assert_eq!(transient.displayed_slot, Some(8));
        assert!(transient.refreshed_reference_names.is_empty());
        assert_eq!(
            transient.map_slot_indices_after,
            vec![0, 1, 2, 3, 4, 5, 6, 7]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn h264_streaming_dpb_budget_keeps_output_slot_separate_from_single_reference() {
        let access_units = vec![
            test_h264_access_unit(0, true, 0),
            test_h264_access_unit(1, false, 1),
            test_h264_access_unit(2, false, 1),
        ];

        let (one_slot_count, one_slot_plan) =
            native_vulkan_h264_min_decodable_dpb_plan_with_gaps(&access_units, 1, 1, 16, false);
        assert_eq!(one_slot_count, 1);
        assert!(!one_slot_plan[1].ready_for_decode_submit);
        assert_eq!(one_slot_plan[1].missing_reference_count, 1);

        let budget = native_vulkan_h264_streaming_dpb_slot_budget(1, 1);
        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan_with_gaps(
            &access_units,
            budget,
            1,
            16,
            false,
        );
        assert_eq!(dpb_slots, 2);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].references[0].source_access_unit_index, Some(0));
        assert_ne!(
            plan[1].references[0].dpb_slot,
            Some(plan[1].planned_output_slot)
        );
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
        assert_ne!(
            plan[2].references[0].dpb_slot,
            Some(plan[2].planned_output_slot)
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn h264_min_decodable_plan_uses_transient_slot_for_three_reference_p_slice() {
        let access_units = vec![
            test_h264_access_unit(0, true, 0),
            test_h264_access_unit(1, false, 1),
            test_h264_access_unit(2, false, 2),
            test_h264_access_unit(3, false, 3),
        ];

        let (three_slots, three_slot_plan) =
            native_vulkan_h264_min_decodable_dpb_plan_with_gaps(&access_units, 3, 3, 16, false);
        assert_eq!(three_slots, 3);
        assert!(!three_slot_plan[3].ready_for_decode_submit);
        assert_eq!(three_slot_plan[3].requested_reference_count, 3);
        assert_eq!(three_slot_plan[3].available_reference_count, 2);
        assert_eq!(three_slot_plan[3].missing_reference_count, 1);

        let budget = native_vulkan_h264_streaming_dpb_slot_budget(3, 3);
        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan_with_gaps(
            &access_units,
            budget,
            3,
            16,
            false,
        );
        assert_eq!(dpb_slots, 4);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[3].requested_reference_count, 3);
        assert_eq!(plan[3].available_reference_count, 3);
        assert_eq!(plan[3].missing_reference_count, 0);
    }

    #[cfg(feature = "native-vulkan-video")]
    fn test_h264_access_unit(
        index: u32,
        idr: bool,
        l0_reference_count: u32,
    ) -> NativeVulkanH264AccessUnitSnapshot {
        NativeVulkanH264AccessUnitSnapshot {
            index,
            bytes: 1,
            byte_hash: index as u64,
            pts_ns: None,
            duration_ns: None,
            pts_ms: Some(u64::from(index) * 16),
            duration_ms: Some(16),
            has_annex_b_start_codes: true,
            has_parameter_sets: idr,
            h264_sps_count: u32::from(idr),
            h264_pps_count: u32::from(idr),
            h264_idr_count: u32::from(idr),
            h264_slice_count: 1,
            first_slice: Some(test_h264_slice(index as u16, idr, l0_reference_count)),
            first_slice_parse_error: None,
            idr_decode_ready: idr,
            decode_ready: true,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn test_h264_slice(
        frame_num: u16,
        idr: bool,
        l0_reference_count: u32,
    ) -> NativeVulkanH264AccessUnitSliceSnapshot {
        NativeVulkanH264AccessUnitSliceSnapshot {
            nal_type: if idr { 5 } else { 1 },
            nal_type_label: if idr { "idr" } else { "non-idr" },
            nal_ref_idc: 3,
            first_mb_in_slice: 0,
            first_slice_segment_in_pic_flag: true,
            slice_type: if idr { 2 } else { 0 },
            slice_type_normalized: if idr { 2 } else { 0 },
            pps_id: 0,
            frame_num,
            idr_pic_id: if idr { frame_num } else { 0 },
            num_ref_idx_l0_active_minus1: (!idr).then_some(l0_reference_count.saturating_sub(1)),
            num_ref_idx_l1_active_minus1: None,
            ref_pic_list_modification_l0: false,
            ref_pic_list_modifications_l0: Vec::new(),
            ref_pic_list_modification_l1: false,
            ref_pic_list_modifications_l1: Vec::new(),
            adaptive_ref_pic_marking_mode_flag: false,
            memory_management_control_operations: Vec::new(),
            field_pic_flag: false,
            bottom_field_flag: false,
            is_reference: true,
            is_intra: idr,
            is_p: !idr,
            is_b: false,
            long_term_reference_flag: false,
            pic_order_cnt: [i32::from(frame_num) * 2; 2],
            slice_offsets: NativeVulkanH264SliceOffsets::single(0),
            idr,
            irap: idr,
        }
    }

    fn test_av1_temporal_unit(
        index: u32,
        refresh_frame_flags: u8,
    ) -> NativeVulkanAv1TemporalUnitSnapshot {
        NativeVulkanAv1TemporalUnitSnapshot {
            index,
            bytes: 0,
            byte_hash: index as u64,
            pts_ns: None,
            duration_ns: None,
            pts_ms: None,
            duration_ms: None,
            obu_count: 1,
            sequence_header_count: 0,
            temporal_delimiter_count: 0,
            frame_header_count: 1,
            tile_group_count: 1,
            frame_count: 1,
            decode_candidate: true,
            tile_payload_bytes: 1,
            frame_payload_bytes: 1,
            first_frame_header_obu_offset: Some(0),
            first_tile_group_obu_offset: Some(0),
            sequence_header_present: false,
            sequence_header: None,
            first_frame_submit: Some(test_av1_frame_submit(index, refresh_frame_flags)),
            obus: Vec::new(),
        }
    }

    fn test_av1_frame_submit(
        index: u32,
        refresh_frame_flags: u8,
    ) -> NativeVulkanAv1FrameSubmitSnapshot {
        NativeVulkanAv1FrameSubmitSnapshot {
            parser: "test",
            frame_header_obu_offset: 0,
            frame_header_payload_offset: 0,
            frame_header_payload_size: 1,
            frame_header_offset_for_vulkan: 0,
            tile_count: 1,
            tile_columns: 1,
            tile_rows: 1,
            tile_size_bytes: 1,
            tile_offsets: vec![0],
            tile_sizes: vec![1],
            tile_payload_total_bytes: 1,
            frame_obu_payload_bytes: 1,
            frame_type: 1,
            frame_type_label: "inter",
            show_existing_frame: false,
            frame_to_show_map_idx: None,
            display_frame_id: None,
            current_frame_id: Some(index),
            expected_frame_ids: Vec::new(),
            show_frame: true,
            showable_frame: true,
            error_resilient_mode: false,
            disable_cdf_update: false,
            allow_screen_content_tools: 0,
            force_integer_mv: 0,
            allow_high_precision_mv: false,
            interpolation_filter: 0,
            interpolation_filter_label: "eighttap",
            is_filter_switchable: false,
            is_motion_mode_switchable: false,
            use_ref_frame_mvs: false,
            reference_select: false,
            skip_mode_present: false,
            allow_warped_motion: false,
            order_hint: Some(index as u8),
            primary_ref_frame: Some(0),
            refresh_frame_flags,
            reference_order_hints: Vec::new(),
            frame_refs_short_signaling: false,
            last_frame_idx: None,
            gold_frame_idx: None,
            ref_frame_indices: Vec::new(),
            render_and_frame_size_different: Some(false),
            frame_width: Some(640),
            frame_height: Some(368),
            render_width: Some(640),
            render_height: Some(368),
            found_frame_header: true,
            found_tile_payload: true,
            vulkan_submit_candidate: true,
            unsupported_reason: None,
        }
    }
}

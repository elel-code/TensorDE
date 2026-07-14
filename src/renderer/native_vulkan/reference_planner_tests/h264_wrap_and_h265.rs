    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_mmco1_short_term_unused_across_frame_num_wrap() {
        let mut access_units = vec![
            h264_test_access_unit(0, 11, true),
            h264_test_access_unit(1, 12, false),
            h264_test_access_unit(2, 13, false),
            h264_test_access_unit(3, 14, false),
            h264_test_access_unit(4, 15, false),
            h264_test_access_unit(5, 0, false),
        ];
        let mmco_slice = access_units[5].first_slice.as_mut().unwrap();
        mmco_slice.adaptive_ref_pic_marking_mode_flag = true;
        mmco_slice.memory_management_control_operations =
            vec![h264_test_mmco(1, Some(4), None, None, None)];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 8, 8, 16);

        assert!(
            plan.iter().all(|entry| entry.ready_for_decode_submit),
            "{plan:#?}"
        );
        assert_eq!(plan[5].dropped_reference_frame_nums, vec![11]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_field_mmco1_drops_only_target_field() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 1, false),
            h264_test_access_unit(3, 2, false),
        ];
        let top_field = access_units[1].first_slice.as_mut().unwrap();
        top_field.field_pic_flag = true;
        top_field.bottom_field_flag = false;
        top_field.pic_order_cnt = [2, 0];
        let bottom_field = access_units[2].first_slice.as_mut().unwrap();
        bottom_field.field_pic_flag = true;
        bottom_field.bottom_field_flag = true;
        bottom_field.pic_order_cnt = [2, 3];
        bottom_field.adaptive_ref_pic_marking_mode_flag = true;
        bottom_field.memory_management_control_operations =
            vec![h264_test_mmco(1, Some(0), None, None, None)];
        let next_frame = access_units[3].first_slice.as_mut().unwrap();
        next_frame.num_ref_idx_l0_active_minus1 = Some(1);
        next_frame.pic_order_cnt = [4, 4];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 4, 3, 16, false);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].dropped_reference_frame_nums, vec![1]);
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| (
                    reference.frame_num,
                    reference.field_pic_flag,
                    reference.bottom_field_flag,
                ))
                .collect::<Vec<_>>(),
            vec![(1, true, true), (0, false, false)]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_long_term_reference_marking_and_ref_list_modification() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 2, false),
        ];
        let mark_long_term = access_units[1].first_slice.as_mut().unwrap();
        mark_long_term.adaptive_ref_pic_marking_mode_flag = true;
        mark_long_term.memory_management_control_operations =
            vec![NativeVulkanH264MemoryManagementControlOperationSnapshot {
                memory_management_control_operation: 3,
                difference_of_pic_nums_minus1: Some(0),
                long_term_pic_num: None,
                long_term_frame_idx: Some(0),
                max_long_term_frame_idx_plus1: None,
            }];
        let long_term_reference = access_units[2].first_slice.as_mut().unwrap();
        long_term_reference.ref_pic_list_modification_l0 = true;
        long_term_reference.ref_pic_list_modifications_l0 =
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 2,
                abs_diff_pic_num_minus1: None,
                long_term_pic_num: Some(0),
            }];
        let drop_long_term = access_units[3].first_slice.as_mut().unwrap();
        drop_long_term.adaptive_ref_pic_marking_mode_flag = true;
        drop_long_term.memory_management_control_operations =
            vec![NativeVulkanH264MemoryManagementControlOperationSnapshot {
                memory_management_control_operation: 2,
                difference_of_pic_nums_minus1: None,
                long_term_pic_num: Some(0),
                long_term_frame_idx: None,
                max_long_term_frame_idx_plus1: None,
            }];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 4, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert!(plan[1].dropped_reference_frame_nums.is_empty());
        assert_eq!(plan[2].references[0].frame_num, 0);
        assert!(plan[2].references[0].used_for_long_term_reference);
        assert_eq!(plan[2].references[0].long_term_frame_idx, Some(0));
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(0));
        assert_eq!(plan[3].dropped_long_term_frame_indices, vec![0]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_field_long_term_ref_list_and_mmco2_by_long_term_pic_num() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 1, false),
            h264_test_access_unit(3, 2, false),
            h264_test_access_unit(4, 3, false),
        ];
        let top_field = access_units[1].first_slice.as_mut().unwrap();
        top_field.field_pic_flag = true;
        top_field.bottom_field_flag = false;
        top_field.pic_order_cnt = [2, 0];
        top_field.adaptive_ref_pic_marking_mode_flag = true;
        top_field.memory_management_control_operations =
            vec![h264_test_mmco(6, None, None, Some(2), None)];

        let bottom_field = access_units[2].first_slice.as_mut().unwrap();
        bottom_field.field_pic_flag = true;
        bottom_field.bottom_field_flag = true;
        bottom_field.pic_order_cnt = [2, 3];

        let ref_top_from_bottom = access_units[3].first_slice.as_mut().unwrap();
        ref_top_from_bottom.field_pic_flag = true;
        ref_top_from_bottom.bottom_field_flag = true;
        ref_top_from_bottom.pic_order_cnt = [4, 5];
        ref_top_from_bottom.ref_pic_list_modification_l0 = true;
        ref_top_from_bottom.ref_pic_list_modifications_l0 =
            vec![h264_test_long_term_l0_modification(4)];

        let drop_top_from_bottom = access_units[4].first_slice.as_mut().unwrap();
        drop_top_from_bottom.field_pic_flag = true;
        drop_top_from_bottom.bottom_field_flag = true;
        drop_top_from_bottom.pic_order_cnt = [6, 7];
        drop_top_from_bottom.adaptive_ref_pic_marking_mode_flag = true;
        drop_top_from_bottom.memory_management_control_operations =
            vec![h264_test_mmco(2, None, Some(4), None, None)];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 6, 4, 16, false);

        assert!(
            plan.iter().all(|entry| entry.ready_for_decode_submit),
            "{plan:#?}"
        );
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(1));
        assert!(plan[3].references[0].used_for_long_term_reference);
        assert_eq!(plan[3].references[0].long_term_frame_idx, Some(2));
        assert_eq!(plan[3].references[0].long_term_pic_num, Some(4));
        assert!(plan[3].references[0].field_pic_flag);
        assert!(!plan[3].references[0].bottom_field_flag);
        assert_eq!(plan[4].dropped_long_term_frame_indices, vec![2]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_idr_long_term_reference_flag() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
        ];
        access_units[0]
            .first_slice
            .as_mut()
            .unwrap()
            .long_term_reference_flag = true;
        let p_slice = access_units[1].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 = vec![h264_test_long_term_l0_modification(0)];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 2, 1, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[0].current_long_term_frame_idx, Some(0));
        assert_eq!(plan[1].references[0].frame_num, 0);
        assert!(plan[1].references[0].used_for_long_term_reference);
        assert_eq!(plan[1].references[0].long_term_frame_idx, Some(0));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn slides_h264_short_term_window_with_existing_long_term_reference() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        access_units[0]
            .first_slice
            .as_mut()
            .unwrap()
            .long_term_reference_flag = true;
        access_units[3]
            .first_slice
            .as_mut()
            .unwrap()
            .num_ref_idx_l0_active_minus1 = Some(1);

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 3, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].dropped_reference_frame_nums, vec![1]);
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| {
                    (
                        reference.frame_num,
                        reference.used_for_long_term_reference,
                        reference.source_access_unit_index,
                    )
                })
                .collect::<Vec<_>>(),
            vec![(2, false, Some(2)), (0, true, Some(0))]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_mmco6_current_picture_as_long_term_reference() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
        ];
        let mark_current_long_term = access_units[1].first_slice.as_mut().unwrap();
        mark_current_long_term.adaptive_ref_pic_marking_mode_flag = true;
        mark_current_long_term.memory_management_control_operations =
            vec![h264_test_mmco(6, None, None, Some(1), None)];
        let p_slice = access_units[2].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 = vec![h264_test_long_term_l0_modification(1)];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 3, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].current_long_term_frame_idx, Some(1));
        assert_eq!(plan[2].references[0].frame_num, 1);
        assert!(plan[2].references[0].used_for_long_term_reference);
        assert_eq!(plan[2].references[0].long_term_frame_idx, Some(1));
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_mmco4_drops_long_term_references_above_limit() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 2, false),
        ];
        let convert_idr_to_long_term = access_units[1].first_slice.as_mut().unwrap();
        convert_idr_to_long_term.adaptive_ref_pic_marking_mode_flag = true;
        convert_idr_to_long_term.memory_management_control_operations =
            vec![h264_test_mmco(3, Some(0), None, Some(0), None)];
        let convert_previous_to_long_term = access_units[2].first_slice.as_mut().unwrap();
        convert_previous_to_long_term.adaptive_ref_pic_marking_mode_flag = true;
        convert_previous_to_long_term.memory_management_control_operations =
            vec![h264_test_mmco(3, Some(0), None, Some(2), None)];
        let trim_long_terms = access_units[3].first_slice.as_mut().unwrap();
        trim_long_terms.adaptive_ref_pic_marking_mode_flag = true;
        trim_long_terms.memory_management_control_operations =
            vec![h264_test_mmco(4, None, None, None, Some(1))];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 4, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].long_term_reference_conversions[0].frame_num, 0);
        assert_eq!(
            plan[2].long_term_reference_conversions[0].long_term_frame_idx,
            2
        );
        assert_eq!(plan[3].dropped_long_term_frame_indices, vec![2]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_mmco5_clears_all_existing_references_before_current_picture() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        let clear_refs = access_units[2].first_slice.as_mut().unwrap();
        clear_refs.adaptive_ref_pic_marking_mode_flag = true;
        clear_refs.memory_management_control_operations =
            vec![h264_test_mmco(5, None, None, None, None)];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 4, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].dropped_reference_frame_nums, vec![0, 1]);
        assert_eq!(plan[3].references.len(), 1);
        assert_eq!(plan[3].references[0].frame_num, 2);
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(2));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_long_term_index_replacement() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        let convert_idr_to_long_term = access_units[1].first_slice.as_mut().unwrap();
        convert_idr_to_long_term.adaptive_ref_pic_marking_mode_flag = true;
        convert_idr_to_long_term.memory_management_control_operations =
            vec![h264_test_mmco(3, Some(0), None, Some(0), None)];
        let replace_long_term = access_units[2].first_slice.as_mut().unwrap();
        replace_long_term.adaptive_ref_pic_marking_mode_flag = true;
        replace_long_term.memory_management_control_operations =
            vec![h264_test_mmco(3, Some(0), None, Some(0), None)];
        let p_slice = access_units[3].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 = vec![h264_test_long_term_l0_modification(0)];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 4, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].dropped_long_term_frame_indices, vec![0]);
        assert_eq!(plan[2].long_term_reference_conversions[0].frame_num, 1);
        assert_eq!(plan[3].references[0].frame_num, 1);
        assert!(plan[3].references[0].used_for_long_term_reference);
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_non_reference_pictures_as_scratch_outputs() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 2, false),
        ];
        let non_reference = access_units[2].first_slice.as_mut().unwrap();
        non_reference.nal_ref_idc = 0;
        non_reference.is_reference = false;

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 2, 1, 16);

        assert_eq!(dpb_slots, 2);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan.iter()
                .map(|entry| entry.planned_output_slot)
                .collect::<Vec<_>>(),
            vec![0, 1, 0, 0]
        );
        assert_eq!(
            plan.iter()
                .map(|entry| entry.setup_slot_index)
                .collect::<Vec<_>>(),
            vec![Some(0), Some(1), None, Some(0)]
        );
        assert_eq!(plan[2].references[0].frame_num, 1);
        assert_eq!(plan[3].references[0].frame_num, 1);
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_default_b_slice_short_term_references() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 2, false),
        ];
        let future_ref = access_units[1].first_slice.as_mut().unwrap();
        future_ref.pic_order_cnt = [2, 2];
        let b_slice = access_units[2].first_slice.as_mut().unwrap();
        b_slice.nal_ref_idc = 0;
        b_slice.slice_type = 6;
        b_slice.slice_type_normalized = vk::video::STD_VIDEO_H264_SLICE_TYPE_B.0 as u32;
        b_slice.num_ref_idx_l0_active_minus1 = Some(0);
        b_slice.num_ref_idx_l1_active_minus1 = Some(0);
        b_slice.is_reference = false;
        b_slice.is_p = false;
        b_slice.is_b = true;
        b_slice.pic_order_cnt = [1, 1];
        access_units[3].first_slice.as_mut().unwrap().pic_order_cnt = [3, 3];

        assert_eq!(
            native_vulkan_h264_access_units_max_active_references(&access_units),
            2
        );
        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 3, 2, 16);

        assert_eq!(dpb_slots, 3);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan.iter()
                .map(|entry| entry.planned_output_slot)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 2]
        );
        assert_eq!(plan[2].setup_slot_index, None);
        assert_eq!(
            plan[2]
                .references
                .iter()
                .map(|reference| reference.frame_num)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(plan[3].references[0].frame_num, 1);
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_b_slice_l1_short_term_ref_list_modification() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        access_units[1].first_slice.as_mut().unwrap().pic_order_cnt = [4, 4];
        access_units[2].first_slice.as_mut().unwrap().pic_order_cnt = [2, 2];
        let b_slice = access_units[3].first_slice.as_mut().unwrap();
        b_slice.nal_ref_idc = 0;
        b_slice.slice_type = 6;
        b_slice.slice_type_normalized = vk::video::STD_VIDEO_H264_SLICE_TYPE_B.0 as u32;
        b_slice.num_ref_idx_l0_active_minus1 = Some(0);
        b_slice.num_ref_idx_l1_active_minus1 = Some(0);
        b_slice.ref_pic_list_modification_l1 = true;
        b_slice.ref_pic_list_modifications_l1 =
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 0,
                abs_diff_pic_num_minus1: Some(2),
                long_term_pic_num: None,
            }];
        b_slice.is_reference = false;
        b_slice.is_p = false;
        b_slice.is_b = true;
        b_slice.pic_order_cnt = [3, 3];

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 4, 3, 16);

        assert_eq!(dpb_slots, 4);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| reference.frame_num)
                .collect::<Vec<_>>(),
            vec![2, 0]
        );
        assert_eq!(plan[3].references[0].source_access_unit_index, Some(2));
        assert_eq!(plan[3].references[1].source_access_unit_index, Some(0));
    }

    #[test]
    fn scans_h265_annex_b_parameter_sets_and_idr() {
        let bytes = [
            0, 0, 0, 1, 0x40, 0x01, 0xaa, 0xbb, // VPS, type 32
            0, 0, 1, 0x42, 0x01, 0xcc, // SPS, type 33
            0, 0, 0, 1, 0x44, 0x01, 0xdd, // PPS, type 34
            0, 0, 1, 0x26, 0x01, 0xee, // IDR_W_RADL, type 19
        ];

        let stats = native_vulkan_h265_nal_stats(&bytes);

        assert!(stats.has_annex_b_start_codes);
        assert!(stats.parameter_sets_present());
        assert_eq!(stats.vps_count, 1);
        assert_eq!(stats.sps_count, 1);
        assert_eq!(stats.pps_count, 1);
        assert_eq!(stats.idr_count, 1);
        assert_eq!(stats.slice_count, 1);
        let first_slice = stats.first_slice.expect("first slice summary");
        assert_eq!(
            native_vulkan_h265_nal_type_label(first_slice.nal_type),
            "idr-w-radl"
        );
        assert_eq!(first_slice.slice_segment_offset, 21);
        assert_eq!(first_slice.payload_start, 24);
        assert_eq!(first_slice.payload_end, bytes.len());
        let payloads = native_vulkan_h265_nal_payloads(&bytes);
        assert_eq!(payloads[3].nal_type, 19);
        assert_eq!(payloads[3].start_code_offset, 21);
        assert_eq!(payloads[3].slice_segment_offset, 21);
        assert_eq!(payloads[3].payload_offset, 24);
    }

    #[test]
    fn uses_three_byte_h265_start_code_for_slice_segment_offset() {
        let bytes = [
            0, 0, 0, 1, 0x02, 0x01, 0xaa, // TRAIL_R with four-byte Annex-B prefix
            0, 0, 1, 0x26, 0x01, 0xbb, // IDR_W_RADL with three-byte Annex-B prefix
        ];

        let payloads = native_vulkan_h265_nal_payloads(&bytes);

        assert_eq!(payloads[0].start_code_offset, 0);
        assert_eq!(payloads[0].slice_segment_offset, 1);
        assert_eq!(payloads[0].payload_offset, 4);
        assert_eq!(payloads[1].start_code_offset, 7);
        assert_eq!(payloads[1].slice_segment_offset, 7);
        assert_eq!(payloads[1].payload_offset, 10);
    }

    #[cfg(feature = "native-vulkan-video")]
    fn h265_test_access_unit(
        index: u32,
        poc: u32,
        idr: bool,
        used_delta_pocs: &[i32],
    ) -> NativeVulkanH265AccessUnitSnapshot {
        let mut short_term_reference_delta_pocs = NativeVulkanH265ReferenceDeltas::new();
        if !idr {
            for delta_poc in used_delta_pocs {
                short_term_reference_delta_pocs.push(*delta_poc);
            }
        }

        NativeVulkanH265AccessUnitSnapshot {
            index,
            bytes: 0,
            byte_hash: 0,
            pts_ns: Some(u64::from(index) * 4_166_667),
            duration_ns: Some(4_166_667),
            pts_ms: Some(u64::from(index) * 4),
            duration_ms: Some(4),
            has_annex_b_start_codes: true,
            has_parameter_sets: idr,
            h265_vps_count: u32::from(idr),
            h265_sps_count: u32::from(idr),
            h265_pps_count: u32::from(idr),
            h265_idr_count: u32::from(idr),
            h265_slice_count: 1,
            first_slice: Some(NativeVulkanH265AccessUnitSliceSnapshot {
                nal_type: if idr { 19 } else { 1 },
                nal_type_label: if idr { "idr-w-radl" } else { "trail-r" },
                slice_segment_offset: 0,
                first_slice_segment_in_pic_flag: true,
                slice_type: if idr { 2 } else { 1 },
                pps_id: 0,
                pic_order_cnt_lsb: (!idr).then_some(poc),
                short_term_ref_pic_set_sps_flag: false,
                short_term_ref_pic_set_idx: None,
                num_delta_pocs_of_ref_rps_idx: 0,
                num_bits_for_st_ref_pic_set_in_slice: 0,
                short_term_reference_delta_pocs,
                long_term_references: Vec::new(),
                idr,
                irap: idr,
            }),
            first_slice_parse_error: None,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn maps_h265_sps_long_term_refs_to_vulkan_std() {
        let std_refs = vulkan::native_vulkan_vulkanalia_h265_std_long_term_ref_pics_sps(&[
            NativeVulkanH265LongTermRefPicSpsSnapshot {
                lt_ref_pic_poc_lsb_sps: 4,
                used_by_curr_pic_lt_sps_flag: true,
            },
            NativeVulkanH265LongTermRefPicSpsSnapshot {
                lt_ref_pic_poc_lsb_sps: 9,
                used_by_curr_pic_lt_sps_flag: false,
            },
        ])
        .expect("H.265 SPS long-term refs should map")
        .expect("non-empty refs should produce STD payload");

        assert_eq!(std_refs.used_by_curr_pic_lt_sps_flag, 0b01);
        assert_eq!(std_refs.lt_ref_pic_poc_lsb_sps[0], 4);
        assert_eq!(std_refs.lt_ref_pic_poc_lsb_sps[1], 9);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h265_long_term_reference_by_poc_lsb() {
        let mut access_units = vec![
            h265_test_access_unit(0, 0, true, &[]),
            h265_test_access_unit(1, 4, false, &[]),
            h265_test_access_unit(2, 8, false, &[]),
        ];
        access_units[2]
            .first_slice
            .as_mut()
            .unwrap()
            .long_term_references = vec![NativeVulkanH265LongTermReferenceSnapshot {
            from_sps: false,
            lt_idx_sps: None,
            poc_lsb: 4,
            used_by_current: true,
            delta_poc_msb_present_flag: false,
            delta_poc_msb_cycle_lt: None,
        }];

        let plan = native_vulkan_h265_decode_reference_plan(&access_units, 3, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].references.len(), 1);
        assert_eq!(plan[2].references[0].poc, 4);
        assert_eq!(plan[2].references[0].delta_poc, -4);
        assert!(plan[2].references[0].used_for_long_term_reference);
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
        assert_eq!(plan[2].references[0].dpb_slot, Some(1));

        let available_references = plan[2].references.iter().collect::<Vec<_>>();
        let st_before = native_vulkan_h265_ref_pic_set_st_curr_before(2, &available_references)
            .expect("short-term before refs should map");
        let lt_curr = native_vulkan_h265_ref_pic_set_lt_curr(2, &available_references)
            .expect("long-term refs should map");
        assert_eq!(st_before, [0xff; 8]);
        assert_eq!(lt_curr[0], 1);
        assert_eq!(&lt_curr[1..], &[0xff; 7]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn counts_h265_mixed_short_and_long_term_active_references() {
        let mut access_units = vec![
            h265_test_access_unit(0, 0, true, &[]),
            h265_test_access_unit(1, 4, false, &[]),
            h265_test_access_unit(2, 8, false, &[-4]),
        ];
        access_units[2]
            .first_slice
            .as_mut()
            .unwrap()
            .long_term_references = vec![NativeVulkanH265LongTermReferenceSnapshot {
            from_sps: false,
            lt_idx_sps: None,
            poc_lsb: 0,
            used_by_current: true,
            delta_poc_msb_present_flag: false,
            delta_poc_msb_cycle_lt: None,
        }];

        assert_eq!(
            native_vulkan_h265_access_units_max_active_references(&access_units),
            2
        );

        let plan = native_vulkan_h265_decode_reference_plan(&access_units, 3, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan[2]
                .references
                .iter()
                .map(|reference| (reference.poc, reference.used_for_long_term_reference))
                .collect::<Vec<_>>(),
            vec![(4, false), (0, true)]
        );
        assert_eq!(plan[2].available_reference_count, 2);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn h265_begin_slots_preserve_current_long_term_reference_flags() {
        let active_refs = vec![
            None,
            Some(NativeVulkanH265ActiveDpbReference {
                poc: 4,
                used_for_long_term_reference: false,
            }),
        ];
        let references = vec![NativeVulkanH265DecodeReferenceSnapshot {
            delta_poc: -4,
            poc: 4,
            used_for_long_term_reference: true,
            available: true,
            source_access_unit_index: Some(1),
            dpb_slot: Some(1),
        }];
        let policy = NativeVulkanH265BeginSlotPolicy::default();

        let begin_refs =
            native_vulkan_h265_begin_slot_refs(&active_refs, &references, false, policy);
        let slot_1 = begin_refs
            .iter()
            .find(|(slot, _)| *slot == 1)
            .expect("active reference slot should be emitted");

        assert_eq!(
            slot_1.1,
            Some(NativeVulkanH265ActiveDpbReference {
                poc: 4,
                used_for_long_term_reference: true,
            })
        );

        let reset_begin_refs =
            native_vulkan_h265_begin_slot_refs(&active_refs, &references, true, policy);
        let reset_slot_1 = reset_begin_refs
            .iter()
            .find(|(slot, _)| *slot == 1)
            .expect("pre-reset active slot should remain visible as inactive");
        assert_eq!(reset_slot_1.1, None);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn parses_predicted_h265_short_term_ref_pic_set() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn push_ue(bits: &mut Vec<bool>, value: u32) {
            let code_num = value + 1;
            let bit_count = 32 - code_num.leading_zeros();
            for _ in 0..bit_count.saturating_sub(1) {
                bits.push(false);
            }
            push_bits(bits, code_num, bit_count);
        }
        fn pack_bits(mut bits: Vec<bool>) -> Vec<u8> {
            bits.push(true);
            while !bits.len().is_multiple_of(8) {
                bits.push(false);
            }
            let mut bytes = vec![0u8; bits.len() / 8];
            for (index, bit) in bits.into_iter().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }

        let reference_rps = native_vulkan_h265_short_term_ref_pic_set_snapshot(
            false,
            None,
            None,
            None,
            0,
            Vec::new(),
            Vec::new(),
            vec![-1, -3],
            vec![true, false],
            vec![2],
            vec![true],
        );
        let mut bits = Vec::new();
        bits.push(true); // inter_ref_pic_set_prediction_flag
        push_ue(&mut bits, 0); // delta_idx_minus1: RefRpsIdx = 0
        bits.push(true); // delta_rps_sign: negative DeltaRps
        push_ue(&mut bits, 0); // abs_delta_rps_minus1: |DeltaRps| = 1
        bits.push(true); // ref negative -1 is used by current and included
        bits.push(false); // ref negative -3 is not used by current
        bits.push(true); // ref negative -3 is still included
        bits.push(true); // ref positive +2 is used by current and included
        bits.push(true); // DeltaRps -1 is used by current and included
        let bytes = pack_bits(bits);
        let mut reader = NativeVulkanH265BitReader::new(&bytes);

        let rps =
            native_vulkan_h265_read_short_term_ref_pic_set(&mut reader, 1, 1, &[reference_rps])
                .expect("predicted RPS should parse");

        assert!(rps.inter_ref_pic_set_prediction_flag);
        assert_eq!(rps.delta_idx_minus1, Some(0));
        assert_eq!(rps.delta_rps_sign, Some(true));
        assert_eq!(rps.abs_delta_rps_minus1, Some(0));
        assert_eq!(rps.num_delta_pocs_of_ref_rps_idx, 3);
        assert_eq!(rps.use_delta_flags, vec![true, true, true, true]);
        assert_eq!(rps.used_by_current_flags, vec![true, false, true, true]);
        assert_eq!(rps.negative_delta_pocs, vec![-1, -2, -4]);
        assert_eq!(rps.negative_used_by_curr_pic, vec![true, true, false]);
        assert_eq!(rps.used_negative_delta_pocs, vec![-1, -2]);
        assert_eq!(rps.positive_delta_pocs, vec![1]);
        assert_eq!(rps.positive_used_by_curr_pic, vec![true]);
        assert_eq!(rps.used_positive_delta_pocs, vec![1]);
        assert_eq!(rps.used_by_current_count, 3);

        let std_rps = vulkan::native_vulkan_vulkanalia_h265_std_short_term_ref_pic_set(&rps)
            .expect("predicted RPS should map to Vulkan STD fields");
        assert_eq!(std_rps.flags.inter_ref_pic_set_prediction_flag(), 1);
        assert_eq!(std_rps.flags.delta_rps_sign(), 1);
        assert_eq!(std_rps.delta_idx_minus1, 0);
        assert_eq!(std_rps.abs_delta_rps_minus1, 0);
        assert_eq!(std_rps.use_delta_flag, 0b1111);
        assert_eq!(std_rps.used_by_curr_pic_flag, 0b1101);
        assert_eq!(std_rps.num_negative_pics, 3);
        assert_eq!(std_rps.num_positive_pics, 1);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn marks_h265_self_evicted_reference_unready() {
        let access_units = vec![
            h265_test_access_unit(0, 0, true, &[]),
            h265_test_access_unit(1, 1, false, &[-1]),
        ];

        let plan = native_vulkan_h265_decode_reference_plan(&access_units, 1, 16);

        assert!(plan[0].ready_for_decode_submit);
        assert!(!plan[1].ready_for_decode_submit);
        assert_eq!(plan[1].planned_output_slot, 0);
        assert_eq!(plan[1].evicted_poc, Some(0));
        assert_eq!(plan[1].missing_reference_pocs, vec![0]);
        assert_eq!(plan[1].references[0].dpb_slot, Some(0));
        assert!(!plan[1].references[0].available);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn keeps_h265_b_frame_references_when_reusing_full_dpb() {
        let access_units = vec![
            h265_test_access_unit(0, 0, true, &[]),
            h265_test_access_unit(1, 3, false, &[-3]),
            h265_test_access_unit(2, 2, false, &[-2, 1]),
            h265_test_access_unit(3, 1, false, &[-1, 1, 2]),
            h265_test_access_unit(4, 6, false, &[-3, -4, -6]),
            h265_test_access_unit(5, 5, false, &[-2, -3, -5, 1]),
        ];

        let plan = native_vulkan_h265_decode_reference_plan(&access_units, 5, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[5].current_poc, Some(5));
        assert_eq!(plan[5].planned_output_slot, 3);
        assert_eq!(plan[5].evicted_poc, Some(1));
        assert_eq!(plan[5].missing_reference_pocs, Vec::<i32>::new());
        assert_eq!(
            plan[5]
                .references
                .iter()
                .map(|reference| reference.poc)
                .collect::<Vec<_>>(),
            vec![3, 2, 0, 6]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn chooses_minimum_h265_dpb_slots_by_reference_distance() {
        let access_units = vec![
            h265_test_access_unit(0, 0, true, &[]),
            h265_test_access_unit(1, 1, false, &[-1]),
            h265_test_access_unit(2, 2, false, &[-2]),
        ];

        assert_eq!(
            native_vulkan_h265_access_units_max_active_references(&access_units),
            1
        );
        let (dpb_slots, plan) = native_vulkan_h265_min_decodable_dpb_plan(&access_units, 3, 16);

        assert_eq!(dpb_slots, 2);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan.iter()
                .map(|entry| entry.planned_output_slot)
                .collect::<Vec<_>>(),
            vec![0, 1, 1]
        );
    }

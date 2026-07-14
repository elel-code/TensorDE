
    #[test]
    fn parses_av1_uniform_multi_tile_key_frame_submit_candidate() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }
        fn push_obu(bytes: &mut Vec<u8>, obu_type: u8, payload: &[u8]) {
            bytes.push((obu_type << 3) | 0x02);
            bytes.push(payload.len() as u8);
            bytes.extend_from_slice(payload);
        }

        let mut sequence_bits = Vec::new();
        push_bits(&mut sequence_bits, 0, 3); // seq_profile Main
        push_bits(&mut sequence_bits, 0, 1); // still_picture
        push_bits(&mut sequence_bits, 0, 1); // reduced_still_picture_header
        push_bits(&mut sequence_bits, 0, 1); // timing_info_present_flag
        push_bits(&mut sequence_bits, 0, 1); // initial_display_delay_present_flag
        push_bits(&mut sequence_bits, 0, 5); // operating_points_cnt_minus_1
        push_bits(&mut sequence_bits, 0, 12); // operating_point_idc
        push_bits(&mut sequence_bits, 4, 5); // seq_level_idx 3.0
        push_bits(&mut sequence_bits, 9, 4); // frame_width_bits_minus_1
        push_bits(&mut sequence_bits, 8, 4); // frame_height_bits_minus_1
        push_bits(&mut sequence_bits, 639, 10); // max_frame_width_minus_1
        push_bits(&mut sequence_bits, 367, 9); // max_frame_height_minus_1
        push_bits(&mut sequence_bits, 0, 1); // frame_id_numbers_present_flag
        push_bits(&mut sequence_bits, 0, 1); // use_128x128_superblock
        push_bits(&mut sequence_bits, 1, 1); // enable_filter_intra
        push_bits(&mut sequence_bits, 1, 1); // enable_intra_edge_filter
        push_bits(&mut sequence_bits, 0, 1); // enable_interintra_compound
        push_bits(&mut sequence_bits, 0, 1); // enable_masked_compound
        push_bits(&mut sequence_bits, 0, 1); // enable_warped_motion
        push_bits(&mut sequence_bits, 0, 1); // enable_dual_filter
        push_bits(&mut sequence_bits, 0, 1); // enable_order_hint
        push_bits(&mut sequence_bits, 0, 1); // seq_choose_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // seq_force_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // enable_superres
        push_bits(&mut sequence_bits, 0, 1); // enable_cdef
        push_bits(&mut sequence_bits, 0, 1); // enable_restoration
        push_bits(&mut sequence_bits, 0, 1); // high_bitdepth
        push_bits(&mut sequence_bits, 0, 1); // mono_chrome
        push_bits(&mut sequence_bits, 0, 1); // color_description_present_flag
        push_bits(&mut sequence_bits, 0, 1); // color_range
        push_bits(&mut sequence_bits, 0, 2); // chroma_sample_position
        push_bits(&mut sequence_bits, 0, 1); // separate_uv_delta_q
        push_bits(&mut sequence_bits, 0, 1); // film_grain_params_present

        let mut frame_bits = Vec::new();
        push_bits(&mut frame_bits, 0, 1); // show_existing_frame
        push_bits(&mut frame_bits, 0, 2); // frame_type key
        push_bits(&mut frame_bits, 1, 1); // show_frame
        push_bits(&mut frame_bits, 1, 1); // disable_cdf_update
        push_bits(&mut frame_bits, 0, 1); // frame_size_override_flag
        push_bits(&mut frame_bits, 0, 1); // render_and_frame_size_different
        push_bits(&mut frame_bits, 1, 1); // uniform_tile_spacing_flag
        push_bits(&mut frame_bits, 1, 1); // increment_tile_cols_log2 -> 1
        push_bits(&mut frame_bits, 0, 1); // stop tile column increments
        push_bits(&mut frame_bits, 1, 1); // increment_tile_rows_log2 -> 1
        push_bits(&mut frame_bits, 0, 1); // stop tile row increments
        push_bits(&mut frame_bits, 0, 2); // context_update_tile_id
        push_bits(&mut frame_bits, 0, 2); // tile_size_bytes_minus_1
        push_bits(&mut frame_bits, 1, 8); // base_q_idx
        push_bits(&mut frame_bits, 0, 1); // delta_q_y_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_ac
        push_bits(&mut frame_bits, 0, 1); // using_qmatrix
        push_bits(&mut frame_bits, 0, 1); // segmentation_enabled
        push_bits(&mut frame_bits, 0, 1); // delta_q_present
        push_bits(&mut frame_bits, 0, 6); // loop_filter_level_0
        push_bits(&mut frame_bits, 0, 6); // loop_filter_level_1
        push_bits(&mut frame_bits, 0, 3); // loop_filter_sharpness
        push_bits(&mut frame_bits, 0, 1); // loop_filter_delta_enabled
        push_bits(&mut frame_bits, 0, 1); // tx_mode_select
        push_bits(&mut frame_bits, 0, 1); // reduced_tx_set
        while !frame_bits.len().is_multiple_of(8) {
            frame_bits.push(false);
        }

        let mut frame_payload = pack_bits(&frame_bits);
        let frame_header_len = frame_payload.len() as u32;
        frame_payload.push(0); // tile_start_and_end_present_flag + alignment padding
        frame_payload.extend_from_slice(&[1, 0xaa, 0xab]);
        frame_payload.extend_from_slice(&[2, 0xba, 0xbb, 0xbc]);
        frame_payload.extend_from_slice(&[0, 0xca]);
        frame_payload.extend_from_slice(&[0xda, 0xdb, 0xdc, 0xdd]);

        let mut obu = Vec::new();
        push_obu(&mut obu, 1, &pack_bits(&sequence_bits));
        let frame_obu_payload_offset = obu.len() as u32 + 2;
        push_obu(&mut obu, 6, &frame_payload);

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let submit = stats.first_frame_submit.as_ref().unwrap();

        assert!(submit.vulkan_submit_candidate, "{submit:?}");
        assert_eq!(submit.tile_count, 4);
        assert_eq!(submit.tile_columns, 2);
        assert_eq!(submit.tile_rows, 2);
        assert_eq!(submit.tile_size_bytes, 1);
        assert_eq!(
            submit.tile_offsets,
            vec![
                frame_obu_payload_offset + frame_header_len + 2,
                frame_obu_payload_offset + frame_header_len + 5,
                frame_obu_payload_offset + frame_header_len + 9,
                frame_obu_payload_offset + frame_header_len + 10,
            ]
        );
        assert_eq!(submit.tile_sizes, vec![2, 3, 1, 4]);
        assert_eq!(submit.tile_payload_total_bytes, 10);
    }

    #[test]
    fn parses_av1_main10_sequence_header_obu_for_vulkan_std_subset() {
        fn push_bits(bits: &mut Vec<bool>, value: u32, count: u32) {
            for shift in (0..count).rev() {
                bits.push(((value >> shift) & 1) != 0);
            }
        }
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }

        let mut bits = Vec::new();
        push_bits(&mut bits, 0, 3); // seq_profile Main
        push_bits(&mut bits, 0, 1); // still_picture
        push_bits(&mut bits, 0, 1); // reduced_still_picture_header
        push_bits(&mut bits, 0, 1); // timing_info_present_flag
        push_bits(&mut bits, 0, 1); // initial_display_delay_present_flag
        push_bits(&mut bits, 0, 5); // operating_points_cnt_minus_1
        push_bits(&mut bits, 0, 12); // operating_point_idc
        push_bits(&mut bits, 4, 5); // seq_level_idx 3.0
        push_bits(&mut bits, 9, 4); // frame_width_bits_minus_1
        push_bits(&mut bits, 8, 4); // frame_height_bits_minus_1
        push_bits(&mut bits, 639, 10); // max_frame_width_minus_1
        push_bits(&mut bits, 367, 9); // max_frame_height_minus_1
        push_bits(&mut bits, 0, 1); // frame_id_numbers_present_flag
        push_bits(&mut bits, 0, 1); // use_128x128_superblock
        push_bits(&mut bits, 1, 1); // enable_filter_intra
        push_bits(&mut bits, 1, 1); // enable_intra_edge_filter
        push_bits(&mut bits, 1, 1); // enable_interintra_compound
        push_bits(&mut bits, 1, 1); // enable_masked_compound
        push_bits(&mut bits, 1, 1); // enable_warped_motion
        push_bits(&mut bits, 1, 1); // enable_dual_filter
        push_bits(&mut bits, 1, 1); // enable_order_hint
        push_bits(&mut bits, 1, 1); // enable_jnt_comp
        push_bits(&mut bits, 1, 1); // enable_ref_frame_mvs
        push_bits(&mut bits, 1, 1); // seq_choose_screen_content_tools
        push_bits(&mut bits, 1, 1); // seq_choose_integer_mv
        push_bits(&mut bits, 6, 3); // order_hint_bits_minus_1
        push_bits(&mut bits, 0, 1); // enable_superres
        push_bits(&mut bits, 1, 1); // enable_cdef
        push_bits(&mut bits, 1, 1); // enable_restoration
        push_bits(&mut bits, 1, 1); // high_bitdepth
        push_bits(&mut bits, 0, 1); // mono_chrome
        push_bits(&mut bits, 0, 1); // color_description_present_flag
        push_bits(&mut bits, 0, 1); // color_range
        push_bits(&mut bits, 0, 2); // chroma_sample_position
        push_bits(&mut bits, 0, 1); // separate_uv_delta_q
        push_bits(&mut bits, 0, 1); // film_grain_params_present

        let payload = pack_bits(&bits);
        let mut obu = Vec::with_capacity(payload.len() + 2);
        obu.push(0x0a);
        obu.push(payload.len() as u8);
        obu.extend_from_slice(&payload);

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let sequence_header = stats.sequence_header.as_ref().unwrap();

        assert_eq!(stats.sequence_header_count, 1);
        assert_eq!(sequence_header.seq_profile_label, "main");
        assert_eq!(sequence_header.max_frame_width, 640);
        assert_eq!(sequence_header.max_frame_height, 368);
        assert_eq!(sequence_header.color_config.bit_depth, 10);
        assert!(sequence_header.color_config.subsampling_x);
        assert!(sequence_header.color_config.subsampling_y);
        assert!(sequence_header.vulkan_std_session_parameters_ready);
    }

    #[cfg(feature = "native-vulkan-video")]
    fn h264_test_access_unit(
        index: u32,
        frame_num: u16,
        idr: bool,
    ) -> NativeVulkanH264AccessUnitSnapshot {
        let is_p = !idr;
        NativeVulkanH264AccessUnitSnapshot {
            index,
            bytes: 0,
            byte_hash: 0,
            pts_ns: Some(u64::from(index) * 4_166_667),
            duration_ns: Some(4_166_667),
            pts_ms: Some(u64::from(index) * 4),
            duration_ms: Some(4),
            has_annex_b_start_codes: true,
            has_parameter_sets: idr,
            h264_sps_count: u32::from(idr),
            h264_pps_count: u32::from(idr),
            h264_idr_count: u32::from(idr),
            h264_slice_count: 1,
            first_slice: Some(NativeVulkanH264AccessUnitSliceSnapshot {
                nal_type: if idr { 5 } else { 1 },
                nal_type_label: if idr { "idr" } else { "non-idr-slice" },
                nal_ref_idc: 3,
                first_mb_in_slice: 0,
                first_slice_segment_in_pic_flag: true,
                slice_type: if idr { 7 } else { 5 },
                slice_type_normalized: if idr { 2 } else { 0 },
                pps_id: 0,
                frame_num,
                idr_pic_id: if idr { 0 } else { 0 },
                num_ref_idx_l0_active_minus1: is_p.then_some(0),
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
                is_p,
                is_b: false,
                long_term_reference_flag: false,
                pic_order_cnt: [i32::from(frame_num); 2],
                slice_offsets: NativeVulkanH264SliceOffsets::single(0),
                idr,
                irap: idr,
            }),
            first_slice_parse_error: None,
            idr_decode_ready: idr,
            decode_ready: true,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn h264_test_sps(frame_mbs_only_flag: bool) -> NativeVulkanH264SpsSnapshot {
        NativeVulkanH264SpsSnapshot {
            id: 0,
            profile_idc: 100,
            profile_label: "high",
            constraint_set0_flag: false,
            constraint_set1_flag: false,
            constraint_set2_flag: false,
            constraint_set3_flag: false,
            constraint_set4_flag: false,
            constraint_set5_flag: false,
            level_idc: 52,
            level_label: Some("5.2"),
            chroma_format_idc: 1,
            chroma_format_label: "4:2:0",
            separate_colour_plane_flag: false,
            bit_depth_luma_minus8: 0,
            bit_depth_chroma_minus8: 0,
            qpprime_y_zero_transform_bypass_flag: false,
            seq_scaling_matrix_present_flag: false,
            log2_max_frame_num_minus4: 0,
            pic_order_cnt_type: 0,
            log2_max_pic_order_cnt_lsb_minus4: 0,
            delta_pic_order_always_zero_flag: false,
            offset_for_non_ref_pic: 0,
            offset_for_top_to_bottom_field: 0,
            offset_for_ref_frame: Vec::new(),
            max_num_ref_frames: 2,
            gaps_in_frame_num_value_allowed_flag: false,
            pic_width_in_mbs_minus1: 119,
            pic_height_in_map_units_minus1: 67,
            frame_mbs_only_flag,
            mb_adaptive_frame_field_flag: !frame_mbs_only_flag,
            direct_8x8_inference_flag: true,
            frame_cropping_flag: false,
            frame_crop_left_offset: 0,
            frame_crop_right_offset: 0,
            frame_crop_top_offset: 0,
            frame_crop_bottom_offset: 0,
            vui_parameters_present_flag: false,
            vui: None,
            width: 1920,
            height: 1080,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn h264_test_mmco(
        memory_management_control_operation: u32,
        difference_of_pic_nums_minus1: Option<u32>,
        long_term_pic_num: Option<u32>,
        long_term_frame_idx: Option<u32>,
        max_long_term_frame_idx_plus1: Option<u32>,
    ) -> NativeVulkanH264MemoryManagementControlOperationSnapshot {
        NativeVulkanH264MemoryManagementControlOperationSnapshot {
            memory_management_control_operation,
            difference_of_pic_nums_minus1,
            long_term_pic_num,
            long_term_frame_idx,
            max_long_term_frame_idx_plus1,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn h264_test_long_term_l0_modification(
        long_term_pic_num: u32,
    ) -> NativeVulkanH264RefPicListModificationSnapshot {
        NativeVulkanH264RefPicListModificationSnapshot {
            modification_of_pic_nums_idc: 2,
            abs_diff_pic_num_minus1: None,
            long_term_pic_num: Some(long_term_pic_num),
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn keys_h264_short_term_field_pictures_by_frame_num_and_field_kind() {
        let top_key = NativeVulkanH264ShortTermPictureKey {
            frame_num: 7,
            field_kind: NativeVulkanH264PictureFieldKind::TopField,
        };
        let bottom_key = NativeVulkanH264ShortTermPictureKey {
            frame_num: 7,
            field_kind: NativeVulkanH264PictureFieldKind::BottomField,
        };
        let mut references = BTreeMap::new();
        references.insert(
            top_key,
            NativeVulkanH264DpbReferenceState {
                source_access_unit_index: Some(10),
                dpb_slot: 0,
                pic_order_cnt_val: 14,
                pic_order_cnt: [14, 15],
                frame_num: 7,
                field_kind: top_key.field_kind,
                non_existing: false,
            },
        );
        references.insert(
            bottom_key,
            NativeVulkanH264DpbReferenceState {
                source_access_unit_index: Some(11),
                dpb_slot: 1,
                pic_order_cnt_val: 15,
                pic_order_cnt: [14, 15],
                frame_num: 7,
                field_kind: bottom_key.field_kind,
                non_existing: false,
            },
        );

        let keys = references
            .keys()
            .filter(|key| key.frame_num == 7)
            .copied()
            .collect::<Vec<_>>();

        assert_eq!(references.len(), 2);
        assert_eq!(keys, vec![top_key, bottom_key]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn sets_h264_reference_info_field_flags() {
        let frame = native_vulkan_h264_reference_info_flags(false, false, false, false);
        let top = native_vulkan_h264_reference_info_flags(true, false, false, false);
        let bottom = native_vulkan_h264_reference_info_flags(true, true, true, true);

        assert_eq!(frame.top_field_flag(), 0);
        assert_eq!(frame.bottom_field_flag(), 0);
        assert_eq!(top.top_field_flag(), 1);
        assert_eq!(top.bottom_field_flag(), 0);
        assert_eq!(bottom.top_field_flag(), 0);
        assert_eq!(bottom.bottom_field_flag(), 1);
        assert_eq!(bottom.used_for_long_term_reference(), 1);
        assert_eq!(bottom.is_non_existing(), 1);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_complementary_field_pair_without_frame_num_gap() {
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
        let next_frame = access_units[3].first_slice.as_mut().unwrap();
        next_frame.num_ref_idx_l0_active_minus1 = Some(1);
        next_frame.pic_order_cnt = [4, 4];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 4, 3, 16, false);

        assert!(
            plan.iter().all(|entry| entry.ready_for_decode_submit),
            "{plan:#?}"
        );
        assert_eq!(plan[1].current_pic_order_cnt_val, Some(2));
        assert_eq!(plan[2].current_pic_order_cnt_val, Some(3));
        assert_eq!(plan[2].references[0].frame_num, 1);
        assert!(plan[2].references[0].field_pic_flag);
        assert!(!plan[2].references[0].bottom_field_flag);
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| (
                    reference.frame_num,
                    reference.field_pic_flag,
                    reference.bottom_field_flag
                ))
                .collect::<Vec<_>>(),
            vec![(1, true, true), (1, true, false)]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn chooses_h264_picture_layout_candidates_from_sps_and_field_window() {
        assert_eq!(
            native_vulkan_h264_picture_layout_candidates(&h264_test_sps(true), false),
            vec![vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE]
        );
        assert_eq!(
            native_vulkan_h264_picture_layout_candidates(&h264_test_sps(false), false),
            vec![
                vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES,
                vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES,
                vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE,
            ]
        );
        assert_eq!(
            native_vulkan_h264_picture_layout_candidates(&h264_test_sps(false), true),
            vec![
                vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES,
                vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES,
            ]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn finds_h264_recovery_offset_after_non_idr_prefix() {
        let access_units = vec![
            h264_test_access_unit(0, 21, false),
            h264_test_access_unit(1, 22, false),
            h264_test_access_unit(2, 0, true),
        ];

        assert!(!native_vulkan_h264_access_unit_starts_recovery(
            &access_units[0]
        ));
        assert!(native_vulkan_h264_access_unit_starts_recovery(
            &access_units[2]
        ));
        assert_eq!(
            native_vulkan_h264_first_recovery_access_unit_offset(&access_units),
            Some(2)
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn chooses_minimum_h264_dpb_slots_for_ippp_ready_prefix() {
        let access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
        ];

        let one_slot_plan = native_vulkan_h264_decode_reference_plan(&access_units, 1, 1, 16);
        assert!(one_slot_plan[0].ready_for_decode_submit);
        assert!(!one_slot_plan[1].ready_for_decode_submit);
        assert_eq!(one_slot_plan[1].missing_reference_count, 1);

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 2, 1, 16);

        assert_eq!(dpb_slots, 2);
        assert!(
            plan.iter().all(|entry| entry.ready_for_decode_submit),
            "{plan:#?}"
        );
        assert_eq!(
            plan.iter()
                .map(|entry| entry.planned_output_slot)
                .collect::<Vec<_>>(),
            vec![0, 1, 0]
        );
        assert_eq!(
            plan.iter()
                .map(|entry| entry.available_reference_count)
                .collect::<Vec<_>>(),
            vec![0, 1, 1]
        );
        assert_eq!(plan[1].references[0].source_access_unit_index, Some(0));
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_multi_reference_ippp_ready_prefix() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        access_units[2]
            .first_slice
            .as_mut()
            .unwrap()
            .num_ref_idx_l0_active_minus1 = Some(1);
        access_units[3]
            .first_slice
            .as_mut()
            .unwrap()
            .num_ref_idx_l0_active_minus1 = Some(1);

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 3, 2, 16);

        assert_eq!(dpb_slots, 3);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan.iter()
                .map(|entry| entry.requested_reference_count)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 2]
        );
        assert_eq!(
            plan.iter()
                .map(|entry| entry.available_reference_count)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 2]
        );
        assert_eq!(
            plan[2]
                .references
                .iter()
                .map(|reference| reference.source_access_unit_index)
                .collect::<Vec<_>>(),
            vec![Some(1), Some(0)]
        );
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| reference.source_access_unit_index)
                .collect::<Vec<_>>(),
            vec![Some(2), Some(1)]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_references_with_full_pic_order_count_pair() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
        ];
        access_units[0].first_slice.as_mut().unwrap().pic_order_cnt = [0, 2];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 2, 1, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[0].current_pic_order_cnt, Some([0, 2]));
        assert_eq!(plan[1].references[0].pic_order_cnt_val, 0);
        assert_eq!(plan[1].references[0].pic_order_cnt, [0, 2]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_short_term_ref_list_modification_p_slice() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
        ];
        let p_slice = access_units[2].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 =
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 0,
                abs_diff_pic_num_minus1: Some(1),
                long_term_pic_num: None,
            }];

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 3, 2, 16);

        assert_eq!(dpb_slots, 3);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].references[0].frame_num, 0);
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(0));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn computes_h264_short_term_pic_num_across_frame_num_wrap() {
        assert_eq!(native_vulkan_h264_short_term_pic_num(15, 0, 16), -1);
        assert_eq!(native_vulkan_h264_short_term_pic_num(14, 0, 16), -2);
        assert_eq!(native_vulkan_h264_short_term_pic_num(0, 1, 16), 0);
        assert_eq!(native_vulkan_h264_short_term_pic_num(1, 1, 16), 1);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn computes_h264_field_pic_num_for_same_and_opposite_fields() {
        let top_key = NativeVulkanH264ShortTermPictureKey {
            frame_num: 7,
            field_kind: NativeVulkanH264PictureFieldKind::TopField,
        };
        let bottom_key = NativeVulkanH264ShortTermPictureKey {
            frame_num: 7,
            field_kind: NativeVulkanH264PictureFieldKind::BottomField,
        };

        assert_eq!(
            native_vulkan_h264_current_pic_num(8, NativeVulkanH264PictureFieldKind::TopField),
            17
        );
        assert_eq!(
            native_vulkan_h264_short_term_pic_num_for_key(
                top_key,
                8,
                NativeVulkanH264PictureFieldKind::TopField,
                16,
            ),
            15
        );
        assert_eq!(
            native_vulkan_h264_short_term_pic_num_for_key(
                bottom_key,
                8,
                NativeVulkanH264PictureFieldKind::TopField,
                16,
            ),
            14
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn computes_h264_field_long_term_pic_num_for_same_and_opposite_fields() {
        let top_key = NativeVulkanH264LongTermPictureKey {
            frame_idx: 3,
            field_kind: NativeVulkanH264PictureFieldKind::TopField,
        };
        let bottom_key = NativeVulkanH264LongTermPictureKey {
            frame_idx: 3,
            field_kind: NativeVulkanH264PictureFieldKind::BottomField,
        };

        assert_eq!(
            native_vulkan_h264_long_term_pic_num_for_key(
                top_key,
                NativeVulkanH264PictureFieldKind::TopField,
            ),
            7
        );
        assert_eq!(
            native_vulkan_h264_long_term_pic_num_for_key(
                bottom_key,
                NativeVulkanH264PictureFieldKind::TopField,
            ),
            6
        );
        assert_eq!(
            native_vulkan_h264_long_term_key_from_pic_num(
                7,
                NativeVulkanH264PictureFieldKind::TopField,
            )
            .unwrap(),
            top_key
        );
        assert_eq!(
            native_vulkan_h264_long_term_key_from_pic_num(
                6,
                NativeVulkanH264PictureFieldKind::TopField,
            )
            .unwrap(),
            bottom_key
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_short_term_default_list_by_pic_num_across_wrap() {
        let mut access_units = vec![
            h264_test_access_unit(0, 14, true),
            h264_test_access_unit(1, 15, false),
            h264_test_access_unit(2, 0, false),
        ];
        access_units[2]
            .first_slice
            .as_mut()
            .unwrap()
            .num_ref_idx_l0_active_minus1 = Some(1);

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 3, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(
            plan[2]
                .references
                .iter()
                .map(|reference| reference.frame_num)
                .collect::<Vec<_>>(),
            vec![15, 14]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_short_term_ref_list_modification_by_pic_num_across_wrap() {
        let mut access_units = vec![
            h264_test_access_unit(0, 14, true),
            h264_test_access_unit(1, 15, false),
            h264_test_access_unit(2, 0, false),
        ];
        let p_slice = access_units[2].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 =
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 0,
                abs_diff_pic_num_minus1: Some(0),
                long_term_pic_num: None,
            }];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 3, 2, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].references[0].frame_num, 15);
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_short_term_ref_list_increment_modification_by_pic_num_across_wrap() {
        let mut access_units = vec![
            h264_test_access_unit(0, 15, true),
            h264_test_access_unit(1, 0, false),
            h264_test_access_unit(2, 15, false),
        ];
        let p_slice = access_units[2].first_slice.as_mut().unwrap();
        p_slice.ref_pic_list_modification_l0 = true;
        p_slice.ref_pic_list_modifications_l0 =
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 1,
                abs_diff_pic_num_minus1: Some(0),
                long_term_pic_num: None,
            }];

        let plan = native_vulkan_h264_decode_reference_plan(&access_units, 17, 16, 16);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].references[0].frame_num, 0);
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn rejects_h264_frame_num_gap_when_sps_disallows_gaps() {
        let access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 2, false),
        ];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 3, 1, 16, false);

        assert!(plan[0].ready_for_decode_submit);
        assert!(!plan[1].ready_for_decode_submit);
        assert!(
            plan[1]
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("gaps_in_frame_num_value_allowed_flag is false")
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn infers_h264_non_existing_short_term_reference_for_allowed_gap() {
        let access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 2, false),
        ];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 3, 1, 16, true);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].inferred_non_existing_frame_nums, vec![1]);
        assert_eq!(plan[1].inferred_non_existing_references.len(), 1);
        assert_eq!(plan[1].inferred_non_existing_references[0].frame_num, 1);
        assert_eq!(plan[1].references[0].frame_num, 1);
        assert!(plan[1].references[0].non_existing);
        assert_eq!(plan[1].references[0].source_access_unit_index, None);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn infers_h264_non_existing_short_term_reference_across_u16_frame_num_wrap() {
        let access_units = vec![
            h264_test_access_unit(0, u16::MAX - 1, true),
            h264_test_access_unit(1, 0, false),
        ];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 3, 1, 65_536, true);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].inferred_non_existing_frame_nums, vec![u16::MAX]);
        assert_eq!(plan[1].references[0].frame_num, u16::MAX);
        assert!(plan[1].references[0].non_existing);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn slides_h264_inferred_non_existing_references_through_short_term_window() {
        let access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 3, false),
        ];

        let plan =
            native_vulkan_h264_decode_reference_plan_with_gaps(&access_units, 4, 2, 16, true);

        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].inferred_non_existing_frame_nums, vec![1, 2]);
        assert_eq!(plan[1].inferred_dropped_reference_frame_nums, vec![0]);
        assert_eq!(
            plan[1]
                .inferred_non_existing_references
                .iter()
                .map(|reference| reference.frame_num)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(plan[1].references[0].frame_num, 2);
        assert!(plan[1].references[0].non_existing);
        assert_eq!(plan[1].dropped_reference_frame_nums, vec![1]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_h264_adaptive_marking_short_term_unused_for_reference() {
        let mut access_units = vec![
            h264_test_access_unit(0, 0, true),
            h264_test_access_unit(1, 1, false),
            h264_test_access_unit(2, 2, false),
            h264_test_access_unit(3, 3, false),
        ];
        let mmco_slice = access_units[2].first_slice.as_mut().unwrap();
        mmco_slice.adaptive_ref_pic_marking_mode_flag = true;
        mmco_slice.memory_management_control_operations =
            vec![NativeVulkanH264MemoryManagementControlOperationSnapshot {
                memory_management_control_operation: 1,
                difference_of_pic_nums_minus1: Some(1),
                long_term_pic_num: None,
                long_term_frame_idx: None,
                max_long_term_frame_idx_plus1: None,
            }];
        access_units[3]
            .first_slice
            .as_mut()
            .unwrap()
            .num_ref_idx_l0_active_minus1 = Some(1);

        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan(&access_units, 3, 2, 16);

        assert_eq!(dpb_slots, 3);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[2].dropped_reference_frame_nums, vec![0]);
        assert_eq!(plan[2].dropped_reference_slots, vec![0]);
        assert_eq!(
            plan[3]
                .references
                .iter()
                .map(|reference| reference.frame_num)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
    }

include!("reference_planner_tests/h264_wrap_and_h265.rs");

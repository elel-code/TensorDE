    #[test]
    fn splits_av1_ffmpeg_packet_into_frame_units() {
        fn push_obu(bytes: &mut Vec<u8>, obu_type: u8, payload: &[u8]) {
            bytes.push((obu_type << 3) | 0x02);
            bytes.push(payload.len() as u8);
            bytes.extend_from_slice(payload);
        }

        let mut packet = Vec::new();
        push_obu(&mut packet, 1, &[0xaa]); // sequence header prefixes next frame
        push_obu(&mut packet, 6, &[0x80, 0x01]); // complete frame OBU
        push_obu(&mut packet, 3, &[0xc8]); // show-existing style frame header
        push_obu(&mut packet, 3, &[0x40]); // split frame header
        push_obu(&mut packet, 4, &[0x11, 0x22]); // tile group for split header

        let ranges = native_vulkan_av1_split_ffmpeg_packet_frame_ranges(&packet).unwrap();
        assert_eq!(ranges.len(), 3);

        let unit_obu_types = ranges
            .iter()
            .map(|range| {
                native_vulkan_av1_obu_ranges(&packet[range.clone()])
                    .unwrap()
                    .into_iter()
                    .map(|range| range.obu_type)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(unit_obu_types[0], vec![1, 6]);
        assert_eq!(unit_obu_types[1], vec![3]);
        assert_eq!(unit_obu_types[2], vec![3, 4]);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn plans_av1_reference_map_for_inter_and_show_existing_frames() {
        fn submit(
            frame_type: u8,
            frame_type_label: &'static str,
            show_existing_frame: bool,
            frame_to_show_map_idx: Option<u8>,
            show_frame: bool,
            order_hint: Option<u8>,
            refresh_frame_flags: u8,
            ref_frame_indices: Vec<i8>,
            submit_ready: bool,
        ) -> NativeVulkanAv1FrameSubmitSnapshot {
            NativeVulkanAv1FrameSubmitSnapshot {
                parser: "test",
                frame_header_obu_offset: 0,
                frame_header_payload_offset: 0,
                frame_header_payload_size: 0,
                frame_header_offset_for_vulkan: 0,
                tile_count: u32::from(submit_ready),
                tile_columns: u32::from(submit_ready),
                tile_rows: u32::from(submit_ready),
                tile_size_bytes: 0,
                tile_offsets: if submit_ready { vec![0] } else { Vec::new() },
                tile_sizes: if submit_ready { vec![1] } else { Vec::new() },
                tile_payload_total_bytes: u64::from(submit_ready),
                frame_obu_payload_bytes: u64::from(submit_ready),
                frame_type,
                frame_type_label,
                show_existing_frame,
                frame_to_show_map_idx,
                display_frame_id: None,
                current_frame_id: None,
                expected_frame_ids: Vec::new(),
                show_frame,
                showable_frame: false,
                error_resilient_mode: frame_type == 0,
                disable_cdf_update: true,
                allow_screen_content_tools: 0,
                force_integer_mv: 2,
                allow_high_precision_mv: false,
                interpolation_filter: vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP.0
                    as u32,
                interpolation_filter_label: "eighttap",
                is_filter_switchable: false,
                is_motion_mode_switchable: false,
                use_ref_frame_mvs: false,
                reference_select: false,
                skip_mode_present: false,
                allow_warped_motion: false,
                order_hint,
                primary_ref_frame: None,
                refresh_frame_flags,
                reference_order_hints: Vec::new(),
                frame_refs_short_signaling: false,
                last_frame_idx: None,
                gold_frame_idx: None,
                ref_frame_indices,
                render_and_frame_size_different: None,
                frame_width: Some(640),
                frame_height: Some(368),
                render_width: Some(640),
                render_height: Some(368),
                found_frame_header: true,
                found_tile_payload: submit_ready,
                vulkan_submit_candidate: submit_ready,
                unsupported_reason: (!submit_ready && !show_existing_frame)
                    .then(|| "AV1 inter frame reference indices parsed; inter submit fields are not ready".to_owned()),
            }
        }

        fn temporal_unit(
            index: u32,
            first_frame_submit: NativeVulkanAv1FrameSubmitSnapshot,
        ) -> NativeVulkanAv1TemporalUnitSnapshot {
            NativeVulkanAv1TemporalUnitSnapshot {
                index,
                bytes: 0,
                byte_hash: 0,
                pts_ns: None,
                duration_ns: None,
                pts_ms: None,
                duration_ms: None,
                obu_count: 1,
                sequence_header_count: u32::from(index == 0),
                temporal_delimiter_count: 0,
                frame_header_count: 0,
                tile_group_count: 0,
                frame_count: u32::from(!first_frame_submit.show_existing_frame),
                decode_candidate: true,
                tile_payload_bytes: 0,
                frame_payload_bytes: 0,
                first_frame_header_obu_offset: Some(0),
                first_tile_group_obu_offset: None,
                sequence_header_present: index == 0,
                sequence_header: None,
                first_frame_submit: Some(first_frame_submit),
                obus: Vec::new(),
            }
        }

        let units = vec![
            temporal_unit(
                0,
                submit(0, "key", false, None, true, Some(0), 0xff, Vec::new(), true),
            ),
            temporal_unit(
                1,
                submit(
                    1,
                    "inter",
                    false,
                    None,
                    false,
                    Some(7),
                    0x02,
                    vec![0, 0, 0, 0, 0, 0, 0],
                    false,
                ),
            ),
            temporal_unit(
                2,
                submit(
                    1,
                    "inter",
                    false,
                    None,
                    true,
                    Some(2),
                    0x10,
                    vec![3, 0, 0, 0, 2, 0, 1],
                    false,
                ),
            ),
            temporal_unit(
                3,
                submit(
                    u8::MAX,
                    "unknown",
                    true,
                    Some(2),
                    true,
                    None,
                    0,
                    Vec::new(),
                    false,
                ),
            ),
        ];

        let plan = native_vulkan_av1_decode_reference_plan(&units, 8);

        assert_eq!(plan.len(), 4);
        assert!(plan[0].ready_for_decode_submit);
        assert_eq!(plan[0].output_slot, Some(0));
        assert_eq!(plan[0].map_slot_indices_after, vec![0, 0, 0, 0, 0, 0, 0, 0]);

        assert!(plan[1].references_resolved);
        assert!(!plan[1].submit_fields_ready);
        assert!(!plan[1].ready_for_decode_submit);
        assert_eq!(plan[1].output_slot, Some(1));
        assert_eq!(plan[1].decode_reference_slots, vec![0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            plan[1].reference_name_order_hints,
            vec![
                None,
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0)
            ]
        );
        assert_eq!(plan[1].refreshed_reference_names, vec![1]);
        assert_eq!(plan[1].map_slot_indices_after, vec![0, 1, 0, 0, 0, 0, 0, 0]);

        assert!(plan[2].references_resolved);
        assert_eq!(plan[2].output_slot, Some(2));
        assert_eq!(plan[2].displayed_slot, Some(2));
        assert_eq!(plan[2].decode_reference_slots, vec![0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(
            plan[2].reference_name_order_hints,
            vec![
                None,
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(7)
            ]
        );
        assert_eq!(plan[2].refreshed_reference_names, vec![4]);
        assert_eq!(plan[2].map_slot_indices_after, vec![0, 1, 0, 0, 2, 0, 0, 0]);

        assert!(plan[3].show_existing_frame);
        assert!(plan[3].ready_for_display_handoff);
        assert_eq!(plan[3].frame_to_show_map_idx, Some(2));
        assert_eq!(plan[3].displayed_slot, Some(0));
        assert_eq!(plan[3].missing_reference_count, 0);

        let ready_units = vec![
            temporal_unit(
                0,
                submit(0, "key", false, None, true, Some(0), 0xff, Vec::new(), true),
            ),
            temporal_unit(
                1,
                submit(
                    1,
                    "inter",
                    false,
                    None,
                    true,
                    Some(1),
                    0x02,
                    vec![0, 0, 0, 0, 0, 0, 0],
                    true,
                ),
            ),
        ];
        let one_slot_plan = native_vulkan_av1_decode_reference_plan(&ready_units, 1);
        assert!(!one_slot_plan[1].ready_for_decode_submit);
        assert!(
            one_slot_plan[1]
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("no free DPB output slot")
        );
        let (min_slots, min_plan) = native_vulkan_av1_min_decodable_dpb_plan(&ready_units, 8);
        assert_eq!(min_slots, 2);
        assert!(min_plan[1].ready_for_decode_submit);
        assert_eq!(min_plan[1].output_slot, Some(1));
        assert_eq!(
            min_plan[1].decode_reference_slots,
            vec![0, 0, 0, 0, 0, 0, 0]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn updates_av1_active_dpb_refs_for_show_existing_key_handoff() {
        let segmentation = NativeVulkanAv1ParsedSegmentation {
            enabled: false,
            update_map: false,
            temporal_update: false,
            update_data: false,
            feature_enabled: [0; 8],
            feature_data: [[0; 8]; 8],
        };
        let displayed = NativeVulkanAv1ActiveDpbReference {
            frame_type: 0,
            order_hint: 11,
            ref_frame_sign_bias: 0,
            saved_order_hints: [0, 7, 8, 9, 10, 0, 0, 0],
            frame_width: 3840,
            frame_height: 2160,
            render_width: 3840,
            render_height: 2160,
            disable_frame_end_update_cdf: true,
            segmentation_enabled: false,
            segmentation,
            loop_filter_ref_deltas: [1, 0, 0, 0, -1, 0, -1, -1],
            loop_filter_mode_deltas: [0, 0],
        };
        let stale = NativeVulkanAv1ActiveDpbReference {
            order_hint: 99,
            frame_type: 1,
            ..displayed
        };
        let mut active_dpb_refs = vec![Some(displayed), Some(stale), Some(stale)];
        let entry = NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
            temporal_unit_index: 6,
            frame_type_label: "key",
            show_existing_frame: true,
            frame_to_show_map_idx: Some(2),
            show_frame: true,
            order_hint: Some(11),
            current_frame_id: None,
            expected_frame_ids: Vec::new(),
            refresh_frame_flags: 0xff,
            output_slot: None,
            displayed_slot: Some(0),
            reference_name_slot_indices: vec![0, 1, 2, -1, -1, -1, -1, -1],
            reference_name_order_hints: vec![None; 8],
            map_order_hints: vec![Some(11), Some(99), Some(99), None, None, None, None, None],
            ref_frame_indices: Vec::new(),
            decode_reference_slots: Vec::new(),
            refreshed_reference_names: (0..8).collect(),
            missing_reference_names: Vec::new(),
            missing_reference_count: 0,
            references_resolved: true,
            submit_fields_ready: false,
            ready_for_decode_submit: false,
            ready_for_display_handoff: true,
            unsupported_reason: None,
            map_slot_indices_after: vec![0; 8],
            map_order_hints_after: vec![Some(11); 8],
        };

        native_vulkan_av1_update_active_dpb_refs_after_display_handoff(
            &mut active_dpb_refs,
            &entry,
        )
        .expect("show-existing handoff updates active refs");

        assert_eq!(
            active_dpb_refs[0].map(|reference| reference.order_hint),
            Some(11)
        );
        assert!(active_dpb_refs[1].is_none());
        assert!(active_dpb_refs[2].is_none());
    }

    #[test]
    fn computes_av1_reference_sign_bias_from_order_hint_distance() {
        assert_eq!(
            native_vulkan_av1_relative_dist_from_order_hint_bits(true, Some(3), 0, 15),
            1
        );
        assert_eq!(
            native_vulkan_av1_relative_dist_from_order_hint_bits(true, Some(3), 15, 0),
            -1
        );
        assert_eq!(
            native_vulkan_av1_relative_dist_from_order_hint_bits(false, Some(3), 15, 0),
            0
        );

        let sequence_header = NativeVulkanAv1SequenceHeaderSnapshot {
            parser: "test",
            seq_profile: 0,
            seq_profile_label: "main",
            still_picture: false,
            reduced_still_picture_header: false,
            timing_info_present_flag: false,
            timing_info: None,
            decoder_model_info_present_flag: false,
            buffer_delay_length_minus_1: 0,
            frame_presentation_time_length_minus_1: 0,
            initial_display_delay_present_flag: false,
            operating_points_cnt_minus_1: 0,
            operating_points: vec![NativeVulkanAv1OperatingPointSnapshot {
                index: 0,
                idc: 0,
                seq_level_idx: 4,
                seq_level_label: Some("3.0"),
                seq_tier: false,
                decoder_model_present_for_this_op: false,
                initial_display_delay_present_for_this_op: false,
                initial_display_delay_minus_1: None,
            }],
            frame_width_bits_minus_1: 9,
            frame_height_bits_minus_1: 8,
            max_frame_width_minus_1: 639,
            max_frame_height_minus_1: 367,
            max_frame_width: 640,
            max_frame_height: 368,
            frame_id_numbers_present_flag: false,
            delta_frame_id_length_minus_2: None,
            additional_frame_id_length_minus_1: None,
            use_128x128_superblock: false,
            enable_filter_intra: false,
            enable_intra_edge_filter: false,
            enable_interintra_compound: false,
            enable_masked_compound: false,
            enable_warped_motion: false,
            enable_dual_filter: false,
            enable_order_hint: true,
            enable_jnt_comp: false,
            enable_ref_frame_mvs: false,
            seq_force_screen_content_tools: 0,
            seq_force_integer_mv: 2,
            order_hint_bits_minus_1: Some(6),
            enable_superres: false,
            enable_cdef: false,
            enable_restoration: false,
            film_grain_params_present: false,
            color_config: NativeVulkanAv1ColorConfigSnapshot {
                high_bitdepth: false,
                twelve_bit: false,
                mono_chrome: false,
                color_description_present_flag: false,
                color_primaries: 2,
                transfer_characteristics: 2,
                matrix_coefficients: 2,
                color_range: false,
                subsampling_x: true,
                subsampling_y: true,
                chroma_sample_position: 0,
                separate_uv_delta_q: false,
                bit_depth: 8,
                num_planes: 3,
            },
            requested_profile_compatible: true,
            vulkan_std_session_parameters_ready: true,
        };
        assert_eq!(
            native_vulkan_av1_ref_frame_sign_bias_from_order_hints(
                &sequence_header,
                8,
                [0, 8, 9, 7, 8, 0, 0, 0],
            ),
            0b0000_0100
        );
    }

    #[test]
    fn trims_av1_single_tile_inter_leading_zero_for_tile_payload_window() {
        fn header(frame_type: u8) -> NativeVulkanAv1ParsedFrameHeader {
            let bits = NativeVulkanAv1BitReader::new(&[]);
            let prefix = NativeVulkanAv1ParsedFrameHeaderPrefix {
                frame_type,
                show_existing_frame: false,
                frame_to_show_map_idx: None,
                display_frame_id: None,
                current_frame_id: None,
                show_frame: true,
                showable_frame: false,
                error_resilient_mode: frame_type == 0,
                disable_cdf_update: true,
                disable_frame_end_update_cdf: true,
                allow_screen_content_tools: 0,
                force_integer_mv: 2,
                allow_high_precision_mv: false,
                interpolation_filter: vk::video::STD_VIDEO_AV1_INTERPOLATION_FILTER_EIGHTTAP,
                is_filter_switchable: false,
                is_motion_mode_switchable: false,
                use_ref_frame_mvs: false,
                reference_select: false,
                skip_mode_present: false,
                allow_warped_motion: false,
                frame_size_override_flag: false,
                order_hint: Some(0),
                primary_ref_frame: None,
                refresh_frame_flags: 0,
            };
            let mut header = native_vulkan_av1_partial_frame_header(
                &bits,
                prefix,
                Vec::new(),
                Vec::new(),
                false,
                None,
                None,
                Vec::new(),
                "test".to_owned(),
            );
            header.tile_count = 1;
            header.tile_columns = 1;
            header.tile_rows = 1;
            header
        }

        let inter_header = header(1);
        let (offsets, sizes) = native_vulkan_av1_tile_group_offsets_from_payload(
            100,
            20,
            &[0x00, 0xff, 0xaa],
            &inter_header,
        )
        .unwrap();
        assert_eq!(offsets, vec![121]);
        assert_eq!(sizes, vec![2]);

        let (offsets, sizes) = native_vulkan_av1_tile_group_offsets_from_payload(
            100,
            20,
            &[0xff, 0xaa],
            &inter_header,
        )
        .unwrap();
        assert_eq!(offsets, vec![120]);
        assert_eq!(sizes, vec![2]);

        let key_header = header(0);
        let (offsets, sizes) = native_vulkan_av1_tile_group_offsets_from_payload(
            100,
            20,
            &[0x00, 0xff, 0xaa],
            &key_header,
        )
        .unwrap();
        assert_eq!(offsets, vec![120]);
        assert_eq!(sizes, vec![3]);
    }

    #[test]
    fn submits_av1_picture_order_hints_by_reference_name() {
        let reference_name_order_hints = [0, 0, 0, 0, 0, 29, 0, 0];

        assert_eq!(
            native_vulkan_av1_picture_order_hints_for_submit(reference_name_order_hints, false),
            reference_name_order_hints
        );
        assert_eq!(
            native_vulkan_av1_picture_order_hints_for_submit(reference_name_order_hints, true),
            [0, 0, 0, 0, 29, 0, 0, 0]
        );
    }

    #[test]
    fn treats_av1_primary_ref_frame_7_as_none_for_segmentation() {
        fn pack_bits(bits: &[bool]) -> Vec<u8> {
            let mut bytes = vec![0u8; bits.len().div_ceil(8)];
            for (index, bit) in bits.iter().copied().enumerate() {
                if bit {
                    bytes[index / 8] |= 1 << (7 - (index % 8));
                }
            }
            bytes
        }

        assert!(native_vulkan_av1_primary_ref_none(None));
        assert!(native_vulkan_av1_primary_ref_none(Some(7)));
        assert!(!native_vulkan_av1_primary_ref_none(Some(0)));

        let mut bits = vec![true]; // segmentation_enabled
        bits.resize(65, false); // update_data=true feature flags
        let bytes = pack_bits(&bits);
        let mut reader = NativeVulkanAv1BitReader::new(&bytes);
        let segmentation =
            native_vulkan_parse_av1_segmentation_params(&mut reader, Some(7), None).unwrap();
        assert!(segmentation.enabled);
        assert!(segmentation.update_map);
        assert!(!segmentation.temporal_update);
        assert!(segmentation.update_data);
        assert_eq!(reader.bit_offset, 65);
    }

    #[test]
    fn inherits_av1_segmentation_from_primary_reference_when_update_data_is_clear() {
        let mut history_segmentation = NativeVulkanAv1ParsedSegmentation {
            enabled: true,
            update_map: true,
            temporal_update: false,
            update_data: true,
            feature_enabled: [0; 8],
            feature_data: [[0; 8]; 8],
        };
        history_segmentation.feature_enabled[3] = 0b0000_0101;
        history_segmentation.feature_data[3][0] = -7;
        history_segmentation.feature_data[3][2] = 12;
        let history = NativeVulkanAv1ReferenceHistory {
            frame_width: 640,
            frame_height: 368,
            render_width: 640,
            render_height: 368,
            segmentation: history_segmentation,
            loop_filter_ref_deltas: [2, 1, 0, -1, -2, -3, -4, -5],
            loop_filter_mode_deltas: [3, -3],
        };

        let bytes = [0b1000_0000u8]; // enabled=1, update_map=0, update_data=0
        let mut reader = NativeVulkanAv1BitReader::new(&bytes);
        let segmentation =
            native_vulkan_parse_av1_segmentation_params(&mut reader, Some(0), Some(history))
                .unwrap();
        assert!(segmentation.enabled);
        assert!(!segmentation.update_map);
        assert!(!segmentation.update_data);
        assert_eq!(
            segmentation.feature_enabled,
            history_segmentation.feature_enabled
        );
        assert_eq!(segmentation.feature_data, history_segmentation.feature_data);
        assert_eq!(reader.bit_offset, 3);
    }

    #[test]
    fn parses_av1_single_tile_key_frame_submit_candidate() {
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
        push_bits(&mut frame_bits, 0, 1); // disable_cdf_update
        push_bits(&mut frame_bits, 0, 1); // frame_size_override_flag
        push_bits(&mut frame_bits, 0, 1); // render_and_frame_size_different
        push_bits(&mut frame_bits, 0, 1); // disable_frame_end_update_cdf
        push_bits(&mut frame_bits, 1, 1); // uniform_tile_spacing_flag
        push_bits(&mut frame_bits, 0, 1); // stop tile column increments
        push_bits(&mut frame_bits, 0, 1); // stop tile row increments
        push_bits(&mut frame_bits, 1, 8); // base_q_idx
        push_bits(&mut frame_bits, 0, 1); // delta_q_y_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_dc
        push_bits(&mut frame_bits, 0, 1); // delta_q_u_ac
        push_bits(&mut frame_bits, 0, 1); // using_qmatrix
        push_bits(&mut frame_bits, 1, 1); // segmentation_enabled
        push_bits(&mut frame_bits, 1, 1); // segmentation_feature_enabled
        push_bits(&mut frame_bits, 0, 8); // segmentation_feature_value
        for _ in 1..64 {
            push_bits(&mut frame_bits, 0, 1); // segmentation_feature_enabled
        }
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
        let expected_tile_offset_in_payload = frame_payload.len() as u32;
        frame_payload.extend_from_slice(&[0xaa, 0xbb, 0xcc]);

        let mut obu = Vec::new();
        push_obu(&mut obu, 1, &pack_bits(&sequence_bits));
        let frame_obu_payload_offset = obu.len() as u32 + 2;
        push_obu(&mut obu, 6, &frame_payload);

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let submit = stats.first_frame_submit.as_ref().unwrap();

        assert!(submit.vulkan_submit_candidate, "{submit:?}");
        assert_eq!(submit.frame_type_label, "key");
        assert_eq!(submit.tile_count, 1);
        assert_eq!(
            submit.tile_offsets,
            vec![frame_obu_payload_offset + expected_tile_offset_in_payload]
        );
        assert_eq!(submit.tile_sizes, vec![3]);
        assert_eq!(submit.frame_width, Some(640));
        assert_eq!(submit.frame_height, Some(368));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn av1_temporal_unit_snapshot_uses_bootstrap_sequence_header_for_frame_only_tu() {
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

        let sequence_header = NativeVulkanAv1SequenceHeaderSnapshot {
            parser: "test",
            seq_profile: 0,
            seq_profile_label: "main",
            still_picture: false,
            reduced_still_picture_header: false,
            timing_info_present_flag: false,
            timing_info: None,
            decoder_model_info_present_flag: false,
            buffer_delay_length_minus_1: 0,
            frame_presentation_time_length_minus_1: 0,
            initial_display_delay_present_flag: false,
            operating_points_cnt_minus_1: 0,
            operating_points: vec![NativeVulkanAv1OperatingPointSnapshot {
                index: 0,
                idc: 0,
                seq_level_idx: 4,
                seq_level_label: Some("3.0"),
                seq_tier: false,
                decoder_model_present_for_this_op: false,
                initial_display_delay_present_for_this_op: false,
                initial_display_delay_minus_1: None,
            }],
            frame_width_bits_minus_1: 9,
            frame_height_bits_minus_1: 8,
            max_frame_width_minus_1: 639,
            max_frame_height_minus_1: 367,
            max_frame_width: 640,
            max_frame_height: 368,
            frame_id_numbers_present_flag: false,
            delta_frame_id_length_minus_2: None,
            additional_frame_id_length_minus_1: None,
            use_128x128_superblock: false,
            enable_filter_intra: true,
            enable_intra_edge_filter: true,
            enable_interintra_compound: false,
            enable_masked_compound: false,
            enable_warped_motion: false,
            enable_dual_filter: false,
            enable_order_hint: false,
            enable_jnt_comp: false,
            enable_ref_frame_mvs: false,
            seq_force_screen_content_tools: 0,
            seq_force_integer_mv: 2,
            order_hint_bits_minus_1: None,
            enable_superres: false,
            enable_cdef: false,
            enable_restoration: false,
            film_grain_params_present: false,
            color_config: NativeVulkanAv1ColorConfigSnapshot {
                high_bitdepth: false,
                twelve_bit: false,
                mono_chrome: false,
                color_description_present_flag: false,
                color_primaries: 2,
                transfer_characteristics: 2,
                matrix_coefficients: 2,
                color_range: false,
                subsampling_x: true,
                subsampling_y: true,
                chroma_sample_position: 0,
                separate_uv_delta_q: false,
                bit_depth: 8,
                num_planes: 3,
            },
            requested_profile_compatible: true,
            vulkan_std_session_parameters_ready: true,
        };

        let mut frame_bits = Vec::new();
        push_bits(&mut frame_bits, 0, 1); // show_existing_frame
        push_bits(&mut frame_bits, 0, 2); // frame_type key
        push_bits(&mut frame_bits, 1, 1); // show_frame
        push_bits(&mut frame_bits, 0, 1); // disable_cdf_update
        push_bits(&mut frame_bits, 0, 1); // frame_size_override_flag
        push_bits(&mut frame_bits, 0, 1); // render_and_frame_size_different
        push_bits(&mut frame_bits, 0, 1); // disable_frame_end_update_cdf
        push_bits(&mut frame_bits, 1, 1); // uniform_tile_spacing_flag
        push_bits(&mut frame_bits, 0, 1); // stop tile column increments
        push_bits(&mut frame_bits, 0, 1); // stop tile row increments
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
        let expected_tile_offset_in_payload = frame_payload.len() as u32;
        frame_payload.extend_from_slice(&[0xaa, 0xbb, 0xcc]);

        let mut frame_only_obu = Vec::new();
        let frame_obu_payload_offset = 2u32;
        push_obu(&mut frame_only_obu, 6, &frame_payload);
        let frame_only_stats = native_vulkan_av1_obu_stats(&frame_only_obu).unwrap();
        assert_eq!(frame_only_stats.sequence_header_count, 0);
        assert!(frame_only_stats.first_frame_submit.is_none());

        let temporal_unit = NativeVulkanAv1TemporalUnitExtract {
            payload: NativeVulkanEncodedAccessUnitPayload::owned(frame_only_obu),
            pts_ns: Some(4_000_000),
            duration_ns: Some(4_000_000),
            pts_ms: Some(4),
            duration_ms: Some(4),
            stats: frame_only_stats,
        };

        let snapshot =
            native_vulkan_av1_temporal_unit_snapshot(1, &temporal_unit, Some(&sequence_header));
        let submit = snapshot.first_frame_submit.as_ref().unwrap();

        assert!(!snapshot.sequence_header_present);
        assert!(submit.vulkan_submit_candidate, "{submit:?}");
        assert_eq!(snapshot.pts_ms, Some(4));
        assert_eq!(snapshot.duration_ms, Some(4));
        assert_eq!(
            submit.tile_offsets,
            vec![frame_obu_payload_offset + expected_tile_offset_in_payload]
        );
        assert_eq!(submit.tile_sizes, vec![3]);
    }

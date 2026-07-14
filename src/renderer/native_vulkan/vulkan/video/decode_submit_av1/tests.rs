    use super::*;

    #[test]
    fn av1_decode_submit_plan_matches_ffmpeg_slot_shape() {
        let entry = test_av1_entry();
        let frame = test_av1_frame();
        let mut reference_infos = Vec::new();

        let plan = native_vulkan_vulkanalia_av1_decode_submit_plan(
            vk::Extent2D {
                width: 1920,
                height: 1080,
            },
            NativeVulkanVideoSessionCodec::Av1Main10,
            &entry,
            frame,
            4096,
            8192,
            false,
            &mut reference_infos,
        )
        .unwrap();

        assert_eq!(plan.common.src_buffer_offset, 4096);
        assert_eq!(plan.common.src_buffer_range, 8192);
        assert_eq!(plan.common.setup_reference_slot.slot_index, 2);
        assert_eq!(plan.common.decode_reference_slot_count, 1);
        assert_eq!(plan.common.begin_reference_slot_count, 2);
        assert_eq!(plan.picture.references[0].slot_index, 1);
        assert_eq!(plan.picture.ffmpeg_reference, FFMPEG_AV1_PICTURE_REFERENCE);
        assert_eq!(plan.picture.frame_type, 1);
        assert_eq!(plan.picture.order_hint, 6);
        assert_eq!(plan.picture.tile_offsets, vec![128]);
        assert_eq!(plan.picture.reference_name_slot_indices[0], 1);
        assert!(!plan.common.reset_control_recorded);
        assert!(plan.common.command_order.contains(&"cmd_decode_video_khr"));
    }

    #[test]
    fn av1_decode_submit_plan_lowers_to_vulkanalia_decode_info() {
        let mut reference_infos = Vec::new();
        let plan = native_vulkan_vulkanalia_av1_decode_submit_plan(
            vk::Extent2D {
                width: 1280,
                height: 720,
            },
            NativeVulkanVideoSessionCodec::Av1Main8,
            &test_av1_entry(),
            test_av1_frame(),
            2048,
            4096,
            true,
            &mut reference_infos,
        )
        .unwrap();
        let image_views = NativeVulkanVulkanaliaDecodeImageViewBindings::repeated(
            vk::ImageView::default(),
            plan.common.begin_reference_slot_count,
            plan.common.decode_reference_slot_count,
        );

        native_vulkan_vulkanalia_av1_with_vk_submit_info(
            &plan,
            vk::VideoSessionKHR::default(),
            vk::VideoDecodeAV1InlineSessionParametersInfoKHR::builder().build(),
            vk::Buffer::default(),
            &image_views,
            |vk_info| {
                assert_eq!(
                    vk_info.begin_info.video_session_parameters,
                    vk::VideoSessionParametersKHR::default()
                );
                assert_eq!(vk_info.begin_info.reference_slot_count, 2);
                assert_eq!(vk_info.decode_info.src_buffer_offset, 2048);
                assert_eq!(vk_info.decode_info.src_buffer_range, 4096);
                assert_eq!(vk_info.decode_info.reference_slot_count, 1);
                assert!(!vk_info.decode_info.next.is_null());
                assert!(
                    vk_info
                        .inline_session_parameters_info
                        .std_sequence_header
                        .is_null()
                );
                assert_eq!(vk_info.av1_picture_info.tile_count, 1);
                assert_eq!(vk_info.av1_picture_info.frame_header_offset, 64);
                assert_eq!(vk_info.av1_picture_info.reference_name_slot_indices[0], 1);
                assert_eq!(
                    vk_info.std_picture_info.frame_type,
                    vk::video::STD_VIDEO_AV1_FRAME_TYPE_INTER
                );
                assert_eq!(vk_info.std_picture_info.OrderHint, 6);
                assert_eq!(vk_info.std_picture_info.primary_ref_frame, 0);
                assert_eq!(vk_info.setup_reference_slot.slot_index, 2);
                assert!(!vk_info.setup_reference_slot.next.is_null());
                assert_eq!(vk_info.decode_reference_slots[0].slot_index, 1);
                assert!(!vk_info.decode_reference_slots[0].next.is_null());
                assert_eq!(vk_info.begin_reference_slots[0].slot_index, 1);
                assert!(!vk_info.begin_reference_slots[0].next.is_null());
                assert_eq!(vk_info.begin_reference_slots[1].slot_index, -1);
                assert!(!vk_info.begin_reference_slots[1].next.is_null());
            },
        )
        .unwrap();
    }

    fn test_av1_entry() -> NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
        NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
            temporal_unit_index: 4,
            frame_type_label: "inter",
            show_existing_frame: false,
            frame_to_show_map_idx: None,
            show_frame: true,
            order_hint: Some(6),
            current_frame_id: Some(9),
            expected_frame_ids: vec![0; 8],
            refresh_frame_flags: 0x04,
            output_slot: Some(2),
            displayed_slot: Some(2),
            reference_name_slot_indices: vec![1, -1, -1, -1, -1, -1, -1],
            reference_name_order_hints: vec![None, Some(4), None, None, None, None, None, None],
            map_order_hints: vec![None; 8],
            ref_frame_indices: vec![0],
            decode_reference_slots: vec![1, -1, -1, -1, -1, -1, -1],
            refreshed_reference_names: vec![2],
            missing_reference_names: Vec::new(),
            missing_reference_count: 0,
            references_resolved: true,
            submit_fields_ready: true,
            ready_for_decode_submit: true,
            ready_for_display_handoff: true,
            unsupported_reason: None,
            map_slot_indices_after: vec![-1, 1, 2, -1, -1, -1, -1, -1],
            map_order_hints_after: vec![None, Some(4), Some(6), None, None, None, None, None],
        }
    }

    fn test_av1_frame() -> NativeVulkanVulkanaliaAv1FrameSubmitInput {
        NativeVulkanVulkanaliaAv1FrameSubmitInput {
            temporal_unit_index: 4,
            frame_header_offset_for_vulkan: 64,
            tile_offsets: vec![128],
            tile_sizes: vec![2048],
            tile_info: NativeVulkanVulkanaliaAv1TileInfoPlan {
                uniform_tile_spacing_flag: true,
                tile_columns: 1,
                tile_rows: 1,
                context_update_tile_id: 0,
                tile_size_bytes_minus_1: 0,
                mi_col_starts: vec![0],
                mi_row_starts: vec![0],
                width_in_sbs_minus_1: vec![119],
                height_in_sbs_minus_1: vec![67],
            },
            frame_type: 1,
            show_existing_frame: false,
            show_frame: true,
            error_resilient_mode: false,
            disable_cdf_update: false,
            disable_frame_end_update_cdf: false,
            use_superres: false,
            render_and_frame_size_different: false,
            allow_screen_content_tools: true,
            is_filter_switchable: true,
            force_integer_mv: false,
            frame_size_override_flag: false,
            allow_intrabc: false,
            frame_refs_short_signaling: false,
            allow_high_precision_mv: true,
            is_motion_mode_switchable: true,
            use_ref_frame_mvs: true,
            allow_warped_motion: false,
            reduced_tx_set: false,
            reference_select: true,
            skip_mode_present: false,
            delta_q_present: false,
            delta_lf_present: false,
            delta_lf_multi: false,
            apply_grain: false,
            current_frame_id: Some(9),
            order_hint: Some(6),
            primary_ref_frame: Some(0),
            refresh_frame_flags: 0x04,
            interpolation_filter: 4,
            tx_mode_select: true,
            delta_q_res: 0,
            delta_lf_res: 0,
            skip_mode_frame: [0; 2],
            coded_denom: 8,
            picture_order_hints: [0, 4, 0, 0, 0, 0, 0, 0],
            expected_frame_ids: vec![0; 8],
            reference_name_slot_indices: vec![1, -1, -1, -1, -1, -1, -1],
            quantization: NativeVulkanVulkanaliaAv1QuantizationPlan {
                using_qmatrix: false,
                diff_uv_delta: false,
                base_q_idx: 120,
                delta_q_y_dc: 0,
                delta_q_u_dc: 0,
                delta_q_u_ac: 0,
                delta_q_v_dc: 0,
                delta_q_v_ac: 0,
                qm_y: 0,
                qm_u: 0,
                qm_v: 0,
            },
            segmentation: NativeVulkanVulkanaliaAv1SegmentationPlan {
                enabled: false,
                update_map: false,
                temporal_update: false,
                update_data: false,
                feature_enabled: [0; 8],
                feature_data: [[0; 8]; 8],
            },
            loop_filter: NativeVulkanVulkanaliaAv1LoopFilterPlan {
                delta_enabled: false,
                delta_update: false,
                level: [8, 8, 4, 4],
                sharpness: 0,
                update_ref_delta: 0,
                ref_deltas: [1, 0, 0, 0, -1, 0, -1, -1],
                update_mode_delta: 0,
                mode_deltas: [0, 0],
            },
            cdef: NativeVulkanVulkanaliaAv1CdefPlan {
                damping_minus_3: 3,
                bits: 2,
                y_pri_strength: [0; 8],
                y_sec_strength: [0; 8],
                uv_pri_strength: [0; 8],
                uv_sec_strength: [0; 8],
            },
            loop_restoration: NativeVulkanVulkanaliaAv1LoopRestorationPlan {
                frame_restoration_type: [0; 3],
                loop_restoration_size: [0; 3],
                uses_lr: false,
                uses_chroma_lr: false,
            },
            global_motion: NativeVulkanVulkanaliaAv1GlobalMotionPlan {
                gm_type: [0; 8],
                gm_params: [[0; 6]; 8],
            },
            setup_reference: NativeVulkanVulkanaliaAv1ReferenceInfoPlan {
                slot_index: 2,
                frame_type: 1,
                ref_frame_sign_bias: 0,
                order_hint: 6,
                saved_order_hints: [0, 4, 6, 0, 0, 0, 0, 0],
                disable_frame_end_update_cdf: false,
                segmentation_enabled: false,
            },
            references: vec![NativeVulkanVulkanaliaAv1ReferenceInfoPlan {
                slot_index: 1,
                frame_type: 0,
                ref_frame_sign_bias: 0,
                order_hint: 4,
                saved_order_hints: [0, 4, 0, 0, 0, 0, 0, 0],
                disable_frame_end_update_cdf: false,
                segmentation_enabled: false,
            }],
        }
    }

    use super::*;
    use crate::renderer::{
        SceneDisplayPlan, SceneRenderLayer, SceneWallpaperPlan, StaticRenderSyncPlan,
    };
    use std::path::PathBuf;

    #[cfg(feature = "native-vulkan-video")]
    struct NativeVulkanTestDecodeReadbackLayout {
        format: &'static str,
        y_plane_bytes: u64,
        uv_plane_bytes: u64,
        size: u64,
    }

    #[cfg(feature = "native-vulkan-video")]
    struct NativeVulkanTestDecodedPlaneFormats {
        y_view_format: vk::Format,
        uv_view_format: vk::Format,
    }

    #[cfg(feature = "native-vulkan-video")]
    fn native_vulkan_video_decode_readback_layout(
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> Option<NativeVulkanTestDecodeReadbackLayout> {
        let pixels = u64::from(extent.width).checked_mul(u64::from(extent.height))?;
        match format {
            vk::Format::G8_B8R8_2PLANE_420_UNORM => Some(NativeVulkanTestDecodeReadbackLayout {
                format: "G8_B8R8_2PLANE_420_UNORM",
                y_plane_bytes: pixels,
                uv_plane_bytes: pixels / 2,
                size: pixels * 3 / 2,
            }),
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16 => {
                Some(NativeVulkanTestDecodeReadbackLayout {
                    format: "G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16",
                    y_plane_bytes: pixels * 2,
                    uv_plane_bytes: pixels,
                    size: pixels * 3,
                })
            }
            _ => None,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn native_vulkan_decoded_video_plane_formats(
        format: vk::Format,
    ) -> Option<NativeVulkanTestDecodedPlaneFormats> {
        match format {
            vk::Format::G8_B8R8_2PLANE_420_UNORM => Some(NativeVulkanTestDecodedPlaneFormats {
                y_view_format: vk::Format::R8_UNORM,
                uv_view_format: vk::Format::R8G8_UNORM,
            }),
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16 => {
                Some(NativeVulkanTestDecodedPlaneFormats {
                    y_view_format: vk::Format::R16_UNORM,
                    uv_view_format: vk::Format::R16G16_UNORM,
                })
            }
            _ => None,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn native_vulkan_h264_reference_info_flags(
        field_pic_flag: bool,
        bottom_field_flag: bool,
        used_for_long_term_reference: bool,
        non_existing: bool,
    ) -> vk::video::StdVideoDecodeH264ReferenceInfoFlags {
        let top_field_flag = field_pic_flag && !bottom_field_flag;
        let bottom_field_flag = field_pic_flag && bottom_field_flag;
        vk::video::StdVideoDecodeH264ReferenceInfoFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::video::StdVideoDecodeH264ReferenceInfoFlags::new_bitfield_1(
                native_vulkan_bool_u32(top_field_flag),
                native_vulkan_bool_u32(bottom_field_flag),
                native_vulkan_bool_u32(used_for_long_term_reference),
                native_vulkan_bool_u32(non_existing),
            ),
            __bindgen_padding_0: [0; 3],
        }
    }

    #[test]
    fn reports_vulkan_spike_as_built_but_not_default() {
        let capabilities = capabilities();

        assert!(capabilities.built);
        assert!(capabilities.experimental);
        assert!(!capabilities.default_enabled);
        assert!(capabilities.reuses_native_wayland_host);
        assert!(capabilities.owns_vulkan_instance_now);
        assert!(capabilities.owns_wayland_vulkan_surface_now);
        assert!(capabilities.owns_vulkan_device_now);
        assert!(capabilities.owns_swapchain_now);
        assert!(capabilities.renders_frames_now);
        assert!(!capabilities.consumes_render_sync);
        assert!(capabilities.direct_video_memory_status.contains("DMABuf"));
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn sizes_video_decode_readback_layouts_by_output_format() {
        let extent = vk::Extent2D {
            width: 3840,
            height: 2160,
        };

        let nv12 = native_vulkan_video_decode_readback_layout(
            vk::Format::G8_B8R8_2PLANE_420_UNORM,
            extent,
        )
        .expect("NV12 readback layout should be supported");
        assert_eq!(nv12.format, "G8_B8R8_2PLANE_420_UNORM");
        assert_eq!(nv12.y_plane_bytes, 3840 * 2160);
        assert_eq!(nv12.uv_plane_bytes, 3840 * 2160 / 2);
        assert_eq!(nv12.size, 3840 * 2160 * 3 / 2);

        let p010 = native_vulkan_video_decode_readback_layout(
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
            extent,
        )
        .expect("P010 readback layout should be supported");
        assert_eq!(p010.format, "G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16");
        assert_eq!(p010.y_plane_bytes, 3840 * 2160 * 2);
        assert_eq!(p010.uv_plane_bytes, 3840 * 2160);
        assert_eq!(p010.size, 3840 * 2160 * 3);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn chooses_decoded_plane_view_formats_by_output_format() {
        let nv12 = native_vulkan_decoded_video_plane_formats(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .expect("NV12 plane view formats should be supported");
        assert_eq!(nv12.y_view_format, vk::Format::R8_UNORM);
        assert_eq!(nv12.uv_view_format, vk::Format::R8G8_UNORM);

        let p010 = native_vulkan_decoded_video_plane_formats(
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
        )
        .expect("P010 plane view formats should be supported");
        assert_eq!(p010.y_view_format, vk::Format::R16_UNORM);
        assert_eq!(p010.uv_view_format, vk::Format::R16G16_UNORM);
    }

    #[test]
    fn parses_native_vulkan_video_session_main10_codecs() {
        assert_eq!(
            "h264".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::H264High8)
        );
        assert_eq!(
            "h264-high-8".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::H264High8)
        );
        assert_eq!(
            "h265-main-10".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::H265Main10)
        );
        assert_eq!(
            "hevc-main-10".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::H265Main10)
        );
        assert_eq!(
            "av1-main-10".parse::<NativeVulkanVideoSessionCodec>(),
            Ok(NativeVulkanVideoSessionCodec::Av1Main10)
        );
        assert_eq!(
            NativeVulkanVideoSessionCodec::H265Main10.label(),
            "h265-main-10"
        );
        assert_eq!(
            NativeVulkanVideoSessionCodec::H264High8.label(),
            "h264-high-8"
        );
        assert_eq!(
            NativeVulkanVideoSessionCodec::H264High8.profile_label(),
            "high-8"
        );
        assert_eq!(
            NativeVulkanVideoSessionCodec::Av1Main10.profile_label(),
            "main-10"
        );
    }

    #[test]
    fn scans_h264_annex_b_parameter_sets_and_idr() {
        let bytes = [
            0, 0, 0, 1, 0x67, 0x64, 0x00, 0x2a, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80, 0, 0, 1, 0x65,
            0x88, 0x84, 0, 0, 1, 0x41, 0x9a,
        ];

        let stats = native_vulkan_h264_nal_stats(&bytes);

        assert_eq!(stats.bytes, bytes.len() as u64);
        assert!(stats.has_annex_b_start_codes);
        assert_eq!(stats.sps_count, 1);
        assert_eq!(stats.pps_count, 1);
        assert_eq!(stats.idr_count, 1);
        assert_eq!(stats.slice_count, 2);
        assert!(stats.parameter_sets_present());
    }

    #[test]
    fn parses_h264_high_sps_pps_for_vulkan_std_subset() {
        let bytes = [
            0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x2a, 0xac, 0xb4, 0x02, 0x80, 0x2d, 0xd8,
            0x08, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00, 0x00, 0x3c, 0x47, 0x8c, 0x19, 0x50,
            0x00, 0x00, 0x00, 0x01, 0x68, 0xef, 0x0f, 0xcb,
        ];

        let parameter_sets = native_vulkan_parse_h264_parameter_sets(&bytes).unwrap();

        assert_eq!(parameter_sets.parser, "native-rust-h264-sps-pps");
        assert_eq!(parameter_sets.sps.profile_idc, 100);
        assert_eq!(parameter_sets.sps.profile_label, "high");
        assert_eq!(parameter_sets.sps.level_idc, 42);
        assert_eq!(parameter_sets.sps.width, 1280);
        assert_eq!(parameter_sets.sps.height, 720);
        assert_eq!(parameter_sets.sps.chroma_format_idc, 1);
        assert_eq!(parameter_sets.sps.pic_order_cnt_type, 2);
        assert_eq!(parameter_sets.pps.id, 0);
        assert_eq!(parameter_sets.pps.sps_id, 0);
        assert!(parameter_sets.pps.entropy_coding_mode_flag);
        assert!(parameter_sets.pps.transform_8x8_mode_flag);
        assert!(parameter_sets.pps.weighted_pred_flag);
        assert!(parameter_sets.requested_profile_compatible);
        assert!(parameter_sets.vulkan_std_session_parameters_ready);
    }

    #[test]
    fn parses_h264_first_idr_slice_for_direct_decode() {
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
        fn pack_rbsp(mut bits: Vec<bool>) -> Vec<u8> {
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

        let mut access_unit = vec![
            0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x2a, 0xac, 0xb4, 0x02, 0x80, 0x2d, 0xd8,
            0x08, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00, 0x00, 0x3c, 0x47, 0x8c, 0x19, 0x50,
            0x00, 0x00, 0x00, 0x01, 0x68, 0xef, 0x0f, 0xcb,
        ];
        let parameter_sets = native_vulkan_parse_h264_parameter_sets(&access_unit).unwrap();
        let slice_start_code_offset = access_unit.len();
        let mut slice_bits = Vec::new();
        push_ue(&mut slice_bits, 0); // first_mb_in_slice
        push_ue(&mut slice_bits, 2); // I-slice
        push_ue(&mut slice_bits, parameter_sets.pps.id);
        push_bits(
            &mut slice_bits,
            0,
            parameter_sets.sps.log2_max_frame_num_minus4 + 4,
        );
        push_ue(&mut slice_bits, 0); // idr_pic_id
        slice_bits.push(false); // no_output_of_prior_pics_flag
        slice_bits.push(false); // long_term_reference_flag
        access_unit.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x65]);
        access_unit.extend_from_slice(&pack_rbsp(slice_bits));

        let first_frame =
            native_vulkan_h264_first_frame_decode_info(&access_unit, &parameter_sets).unwrap();

        assert_eq!(first_frame.nal_type_label, "idr");
        assert!(first_frame.idr);
        assert!(first_frame.irap);
        assert!(first_frame.is_intra);
        assert!(first_frame.is_reference);
        assert_eq!(first_frame.pps_id, parameter_sets.pps.id);
        assert_eq!(first_frame.frame_num, 0);
        assert_eq!(first_frame.idr_pic_id, 0);
        assert_eq!(first_frame.pic_order_cnt, [0, 0]);
        assert_eq!(
            first_frame.slice_offsets,
            vec![(slice_start_code_offset + 1) as u32]
        );
    }

    #[test]
    fn parses_h264_weighted_p_slice_header_for_vulkanalia_extractor() {
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
        fn push_se(bits: &mut Vec<bool>, value: i32) {
            let code_num = if value <= 0 {
                value.unsigned_abs() * 2
            } else {
                value as u32 * 2 - 1
            };
            push_ue(bits, code_num);
        }
        fn pack_rbsp(mut bits: Vec<bool>) -> Vec<u8> {
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

        let mut access_unit = vec![
            0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x2a, 0xac, 0xb4, 0x02, 0x80, 0x2d, 0xd8,
            0x08, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00, 0x00, 0x3c, 0x47, 0x8c, 0x19, 0x50,
            0x00, 0x00, 0x00, 0x01, 0x68, 0xef, 0x0f, 0xcb,
        ];
        let parameter_sets = native_vulkan_parse_h264_parameter_sets(&access_unit).unwrap();
        assert!(parameter_sets.pps.weighted_pred_flag);
        assert_eq!(parameter_sets.sps.pic_order_cnt_type, 2);

        let mut slice_bits = Vec::new();
        push_ue(&mut slice_bits, 0); // first_mb_in_slice
        push_ue(&mut slice_bits, 0); // P-slice
        push_ue(&mut slice_bits, parameter_sets.pps.id);
        push_bits(
            &mut slice_bits,
            1,
            parameter_sets.sps.log2_max_frame_num_minus4 + 4,
        );
        if parameter_sets.pps.redundant_pic_cnt_present_flag {
            push_ue(&mut slice_bits, 0);
        }
        slice_bits.push(true); // num_ref_idx_active_override_flag
        push_ue(&mut slice_bits, 0); // num_ref_idx_l0_active_minus1
        slice_bits.push(false); // ref_pic_list_modification_flag_l0
        push_ue(&mut slice_bits, 0); // luma_log2_weight_denom
        if native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0 {
            push_ue(&mut slice_bits, 0); // chroma_log2_weight_denom
        }
        slice_bits.push(true); // luma_weight_l0_flag
        push_se(&mut slice_bits, 1); // luma_weight_l0[0]
        push_se(&mut slice_bits, 0); // luma_offset_l0[0]
        if native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0 {
            slice_bits.push(true); // chroma_weight_l0_flag
            push_se(&mut slice_bits, 1); // chroma_weight_l0[0][0]
            push_se(&mut slice_bits, 0); // chroma_offset_l0[0][0]
            push_se(&mut slice_bits, 1); // chroma_weight_l0[0][1]
            push_se(&mut slice_bits, 0); // chroma_offset_l0[0][1]
        }
        slice_bits.push(false); // adaptive_ref_pic_marking_mode_flag
        access_unit.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x61]);
        access_unit.extend_from_slice(&pack_rbsp(slice_bits));

        let picture = native_vulkan_h264_picture_decode_info(&access_unit, &parameter_sets, 1)
            .expect("weighted P-slice header should parse");

        assert!(picture.is_p);
        assert_eq!(picture.frame_num, 1);
        assert_eq!(picture.num_ref_idx_l0_active_minus1, Some(0));
        assert!(!picture.ref_pic_list_modification_l0);
        assert!(!picture.adaptive_ref_pic_marking_mode_flag);
        assert_eq!(picture.pic_order_cnt, [1, 1]);
    }

    #[test]
    fn parses_h264_b_slice_l1_ref_list_modification_for_vulkanalia_extractor() {
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
        fn pack_rbsp(mut bits: Vec<bool>) -> Vec<u8> {
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

        let mut access_unit = vec![
            0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x2a, 0xac, 0xb4, 0x02, 0x80, 0x2d, 0xd8,
            0x08, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00, 0x00, 0x3c, 0x47, 0x8c, 0x19, 0x50,
            0x00, 0x00, 0x00, 0x01, 0x68, 0xef, 0x0f, 0xcb,
        ];
        let parameter_sets = native_vulkan_parse_h264_parameter_sets(&access_unit).unwrap();
        assert_eq!(parameter_sets.sps.pic_order_cnt_type, 2);

        let mut slice_bits = Vec::new();
        push_ue(&mut slice_bits, 0); // first_mb_in_slice
        push_ue(&mut slice_bits, 1); // B-slice
        push_ue(&mut slice_bits, parameter_sets.pps.id);
        push_bits(
            &mut slice_bits,
            3,
            parameter_sets.sps.log2_max_frame_num_minus4 + 4,
        );
        if parameter_sets.pps.redundant_pic_cnt_present_flag {
            push_ue(&mut slice_bits, 0);
        }
        slice_bits.push(false); // direct_spatial_mv_pred_flag
        slice_bits.push(true); // num_ref_idx_active_override_flag
        push_ue(&mut slice_bits, 0); // num_ref_idx_l0_active_minus1
        push_ue(&mut slice_bits, 0); // num_ref_idx_l1_active_minus1
        slice_bits.push(false); // ref_pic_list_modification_flag_l0
        slice_bits.push(true); // ref_pic_list_modification_flag_l1
        push_ue(&mut slice_bits, 0); // modification_of_pic_nums_idc: short-term subtract
        push_ue(&mut slice_bits, 2); // abs_diff_pic_num_minus1
        push_ue(&mut slice_bits, 3); // end
        if parameter_sets.pps.weighted_bipred_idc == 1 {
            push_ue(&mut slice_bits, 0); // luma_log2_weight_denom
            if native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0 {
                push_ue(&mut slice_bits, 0); // chroma_log2_weight_denom
            }
            slice_bits.push(false); // luma_weight_l0_flag
            if native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0 {
                slice_bits.push(false); // chroma_weight_l0_flag
            }
            slice_bits.push(false); // luma_weight_l1_flag
            if native_vulkan_h264_chroma_array_type(&parameter_sets.sps) != 0 {
                slice_bits.push(false); // chroma_weight_l1_flag
            }
        }
        slice_bits.push(false); // adaptive_ref_pic_marking_mode_flag
        access_unit.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x61]);
        access_unit.extend_from_slice(&pack_rbsp(slice_bits));

        let picture = native_vulkan_h264_picture_decode_info(&access_unit, &parameter_sets, 1)
            .expect("B-slice L1 modification header should parse");

        assert!(picture.is_b);
        assert_eq!(picture.frame_num, 3);
        assert_eq!(picture.num_ref_idx_l0_active_minus1, Some(0));
        assert_eq!(picture.num_ref_idx_l1_active_minus1, Some(0));
        assert!(!picture.ref_pic_list_modification_l0);
        assert!(picture.ref_pic_list_modification_l1);
        assert_eq!(
            picture.ref_pic_list_modifications_l1,
            vec![NativeVulkanH264RefPicListModificationSnapshot {
                modification_of_pic_nums_idc: 0,
                abs_diff_pic_num_minus1: Some(2),
                long_term_pic_num: None,
            }]
        );
    }

    #[test]
    fn parses_av1_sequence_header_obu_for_vulkan_std_subset() {
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
        push_bits(&mut bits, 0, 1); // high_bitdepth
        push_bits(&mut bits, 0, 1); // mono_chrome
        push_bits(&mut bits, 0, 1); // color_description_present_flag
        push_bits(&mut bits, 0, 1); // color_range
        push_bits(&mut bits, 0, 2); // chroma_sample_position
        push_bits(&mut bits, 0, 1); // separate_uv_delta_q
        push_bits(&mut bits, 0, 1); // film_grain_params_present

        let payload = pack_bits(&bits);
        let mut obu = Vec::with_capacity(payload.len() + 2);
        obu.push(0x0a); // sequence-header OBU with size field
        obu.push(payload.len() as u8);
        obu.extend_from_slice(&payload);

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let sequence_header = stats.sequence_header.as_ref().unwrap();

        assert_eq!(stats.sequence_header_count, 1);
        assert_eq!(sequence_header.seq_profile_label, "main");
        assert_eq!(sequence_header.max_frame_width, 640);
        assert_eq!(sequence_header.max_frame_height, 368);
        assert_eq!(sequence_header.color_config.bit_depth, 8);
        assert!(sequence_header.color_config.subsampling_x);
        assert!(sequence_header.color_config.subsampling_y);
        assert!(sequence_header.vulkan_std_session_parameters_ready);
        assert!(!stats.decode_candidate());

        let mut obu_with_frame = obu.clone();
        let frame_obu_offset = obu_with_frame.len() as u64;
        obu_with_frame.push(0x32); // frame OBU with size field
        obu_with_frame.push(1);
        obu_with_frame.push(0);
        let stats_with_frame = native_vulkan_av1_obu_stats(&obu_with_frame).unwrap();
        assert_eq!(stats_with_frame.frame_count, 1);
        assert_eq!(stats_with_frame.frame_payload_bytes, 1);
        assert_eq!(
            stats_with_frame.first_frame_header_obu_offset,
            Some(frame_obu_offset)
        );
        assert!(stats_with_frame.decode_candidate());
        assert!(stats_with_frame.first_frame_submit.is_some());
        assert!(
            !stats_with_frame
                .first_frame_submit
                .as_ref()
                .unwrap()
                .vulkan_submit_candidate
        );
    }

    #[test]
    fn parses_av1_inter_frame_reference_indices_for_planning() {
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
        push_bits(&mut sequence_bits, 1, 1); // enable_order_hint
        push_bits(&mut sequence_bits, 0, 1); // enable_jnt_comp
        push_bits(&mut sequence_bits, 0, 1); // enable_ref_frame_mvs
        push_bits(&mut sequence_bits, 0, 1); // seq_choose_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // seq_force_screen_content_tools
        push_bits(&mut sequence_bits, 6, 3); // order_hint_bits_minus_1
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
        push_bits(&mut frame_bits, 1, 2); // frame_type inter
        push_bits(&mut frame_bits, 1, 1); // show_frame
        push_bits(&mut frame_bits, 1, 1); // error_resilient_mode
        push_bits(&mut frame_bits, 1, 1); // disable_cdf_update
        push_bits(&mut frame_bits, 0, 1); // frame_size_override_flag
        push_bits(&mut frame_bits, 5, 7); // order_hint
        push_bits(&mut frame_bits, 0x01, 8); // refresh_frame_flags
        for value in 0..8 {
            push_bits(&mut frame_bits, value, 7); // ref_order_hint
        }
        push_bits(&mut frame_bits, 0, 1); // frame_refs_short_signaling
        for value in 0..7 {
            push_bits(&mut frame_bits, value, 3); // ref_frame_idx
        }

        let mut obu = Vec::new();
        push_obu(&mut obu, 1, &pack_bits(&sequence_bits));
        push_obu(&mut obu, 6, &pack_bits(&frame_bits));

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let submit = stats.first_frame_submit.as_ref().unwrap();

        assert_eq!(submit.frame_type_label, "inter");
        assert!(submit.found_frame_header);
        assert!(!submit.vulkan_submit_candidate);
        assert_eq!(submit.order_hint, Some(5));
        assert_eq!(submit.refresh_frame_flags, 0x01);
        assert_eq!(submit.reference_order_hints, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        assert!(!submit.frame_refs_short_signaling);
        assert_eq!(submit.ref_frame_indices, vec![0, 1, 2, 3, 4, 5, 6]);
        assert!(
            submit
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("reference indices parsed")
        );
    }

    #[test]
    fn parses_av1_allow_warped_motion_before_reduced_tx_set() {
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
        push_bits(&mut sequence_bits, 1, 1); // enable_warped_motion
        push_bits(&mut sequence_bits, 0, 1); // enable_dual_filter
        push_bits(&mut sequence_bits, 1, 1); // enable_order_hint
        push_bits(&mut sequence_bits, 0, 1); // enable_jnt_comp
        push_bits(&mut sequence_bits, 0, 1); // enable_ref_frame_mvs
        push_bits(&mut sequence_bits, 0, 1); // seq_choose_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // seq_force_screen_content_tools
        push_bits(&mut sequence_bits, 6, 3); // order_hint_bits_minus_1
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
        push_bits(&mut frame_bits, 1, 2); // frame_type inter
        push_bits(&mut frame_bits, 1, 1); // show_frame
        push_bits(&mut frame_bits, 0, 1); // error_resilient_mode
        push_bits(&mut frame_bits, 1, 1); // disable_cdf_update
        push_bits(&mut frame_bits, 0, 1); // frame_size_override_flag
        push_bits(&mut frame_bits, 5, 7); // order_hint
        push_bits(&mut frame_bits, 7, 3); // primary_ref_frame none
        push_bits(&mut frame_bits, 0x01, 8); // refresh_frame_flags
        push_bits(&mut frame_bits, 0, 1); // frame_refs_short_signaling
        for value in 0..7 {
            push_bits(&mut frame_bits, value, 3); // ref_frame_idx
        }
        push_bits(&mut frame_bits, 0, 1); // render_and_frame_size_different
        push_bits(&mut frame_bits, 0, 1); // allow_high_precision_mv
        push_bits(&mut frame_bits, 0, 1); // is_filter_switchable
        push_bits(&mut frame_bits, 0, 2); // interpolation_filter eighttap
        push_bits(&mut frame_bits, 1, 1); // is_motion_mode_switchable
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
        push_bits(&mut frame_bits, 0, 1); // reference_select
        push_bits(&mut frame_bits, 0, 1); // skip_mode_present
        push_bits(&mut frame_bits, 0, 1); // allow_warped_motion
        push_bits(&mut frame_bits, 1, 1); // reduced_tx_set
        while !frame_bits.len().is_multiple_of(8) {
            frame_bits.push(false);
        }

        let sequence_payload = pack_bits(&sequence_bits);
        let frame_payload = pack_bits(&frame_bits);
        let sequence_header = native_vulkan_parse_av1_sequence_header(&sequence_payload).unwrap();
        let header =
            native_vulkan_parse_av1_frame_header_for_submit(&frame_payload, &sequence_header)
                .unwrap();

        assert!(!header.allow_warped_motion);
        assert!(header.reduced_tx_set);

        let mut obu = Vec::new();
        push_obu(&mut obu, 1, &sequence_payload);
        push_obu(&mut obu, 6, &frame_payload);

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let submit = stats.first_frame_submit.as_ref().unwrap();

        assert_eq!(submit.frame_type_label, "inter");
        assert!(submit.is_motion_mode_switchable);
        assert!(!submit.allow_warped_motion);
        assert!(
            submit
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("AV1 first frame has no tile payload bytes")
        );
    }

    #[test]
    fn parses_av1_show_existing_frame_for_display_planning() {
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
        push_bits(&mut sequence_bits, 1, 1); // enable_order_hint
        push_bits(&mut sequence_bits, 0, 1); // enable_jnt_comp
        push_bits(&mut sequence_bits, 0, 1); // enable_ref_frame_mvs
        push_bits(&mut sequence_bits, 0, 1); // seq_choose_screen_content_tools
        push_bits(&mut sequence_bits, 0, 1); // seq_force_screen_content_tools
        push_bits(&mut sequence_bits, 6, 3); // order_hint_bits_minus_1
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
        push_bits(&mut frame_bits, 1, 1); // show_existing_frame
        push_bits(&mut frame_bits, 5, 3); // frame_to_show_map_idx

        let mut obu = Vec::new();
        push_obu(&mut obu, 1, &pack_bits(&sequence_bits));
        push_obu(&mut obu, 6, &pack_bits(&frame_bits));

        let stats = native_vulkan_av1_obu_stats(&obu).unwrap();
        let submit = stats.first_frame_submit.as_ref().unwrap();

        assert!(submit.show_existing_frame);
        assert_eq!(submit.frame_to_show_map_idx, Some(5));
        assert_eq!(submit.frame_type_label, "unknown");
        assert!(submit.show_frame);
        assert!(!submit.vulkan_submit_candidate);
        assert!(
            submit
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("show_existing_frame map index parsed")
        );

        let mut split_obu = Vec::new();
        push_obu(&mut split_obu, 1, &pack_bits(&sequence_bits));
        push_obu(&mut split_obu, 3, &pack_bits(&frame_bits));

        let split_stats = native_vulkan_av1_obu_stats(&split_obu).unwrap();
        let split_submit = split_stats.first_frame_submit.as_ref().unwrap();

        assert!(split_submit.show_existing_frame);
        assert_eq!(split_submit.frame_to_show_map_idx, Some(5));
        assert!(!split_submit.vulkan_submit_candidate);
        assert!(
            split_submit
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("show_existing_frame map index parsed")
        );
        assert!(
            !split_submit
                .unsupported_reason
                .as_deref()
                .unwrap_or_default()
                .contains("no following tile-group")
        );
    }

include!("codec_parser_tests/av1_submit.rs");

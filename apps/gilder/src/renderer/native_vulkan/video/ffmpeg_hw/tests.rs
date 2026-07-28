    use super::*;

    #[test]
    fn ffmpeg_hw_decode_contract_is_mainline_and_rejects_software_fallback() {
        let contract = native_vulkan_ffmpeg_hw_decode_backend_contract();

        assert!(contract.mainline);
        assert_eq!(contract.binding, "ffmpeg-vulkan-hwdecode");
        assert_eq!(
            contract.device_policy,
            NativeVulkanFfmpegHwDecodeDevicePolicy::VulkanaliaProvidedDevice
        );
        assert_eq!(
            contract.output_frame_contract.required_avframe_format,
            "AV_PIX_FMT_VULKAN"
        );
        assert!(
            contract
                .required_telemetry
                .contains(&"av_hwframe_transfer_data_calls=0")
        );
        assert!(
            contract
                .output_frame_contract
                .forbidden_operations
                .contains(&"FFmpeg-created private Vulkan device on the mainline")
        );
    }

    #[test]
    fn ffmpeg_hw_decode_contract_covers_current_video_codecs() {
        let codecs = native_vulkan_ffmpeg_hw_decode_codec_contracts()
            .into_iter()
            .map(|contract| contract.codec)
            .collect::<Vec<_>>();

        assert_eq!(codecs.len(), 5);
        assert!(codecs.contains(&NativeVulkanVideoSessionCodec::H264High8));
        assert!(codecs.contains(&NativeVulkanVideoSessionCodec::H265Main8));
        assert!(codecs.contains(&NativeVulkanVideoSessionCodec::H265Main10));
        assert!(codecs.contains(&NativeVulkanVideoSessionCodec::Av1Main8));
        assert!(codecs.contains(&NativeVulkanVideoSessionCodec::Av1Main10));
    }

    #[test]
    fn ffmpeg_hw_decode_constants_cover_descriptor_source_formats() {
        assert!(native_vulkan_ffmpeg_hwdecode_constant_names_ready());

        let nv12 = unsafe { gilder_av_pix_fmt_nv12() };
        let p010 = unsafe { gilder_av_pix_fmt_p010le() };

        assert_eq!(
            native_vulkan_ffmpeg_sw_format_to_picture_format(nv12).unwrap(),
            ("AV_PIX_FMT_NV12", vk::Format::G8_B8R8_2PLANE_420_UNORM)
        );
        assert_eq!(
            native_vulkan_ffmpeg_sw_format_to_picture_format(p010).unwrap(),
            (
                "AV_PIX_FMT_P010LE",
                vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
            )
        );
    }

    #[test]
    fn ffmpeg_hw_decode_codec_operations_name_current_mainline() {
        let labels = native_vulkan_ffmpeg_video_codec_operation_labels(
            vk::VideoCodecOperationFlagsKHR::DECODE_H264
                | vk::VideoCodecOperationFlagsKHR::DECODE_H265
                | vk::VideoCodecOperationFlagsKHR::DECODE_AV1,
        );

        assert_eq!(labels, vec!["decode-h264", "decode-h265", "decode-av1"]);
    }

    #[test]
    fn ffmpeg_hw_decode_host_memory_inference_matches_4k_shape() {
        let h264_per_picture = native_vulkan_ffmpeg_infer_h264_refstruct_picture_bytes(
            NativeVulkanVideoSessionCodec::H264High8,
            3840,
            2160,
        );
        assert_eq!(h264_per_picture.saturating_mul(3), 13_730_766);

        let hevc_per_picture = native_vulkan_ffmpeg_infer_hevc_refstruct_picture_bytes(
            NativeVulkanVideoSessionCodec::H265Main8,
            3840,
            2160,
        );
        let hevc_layer_tables = native_vulkan_ffmpeg_infer_hevc_layer_table_bytes(
            NativeVulkanVideoSessionCodec::H265Main8,
            3840,
            2160,
        );
        assert_eq!(hevc_per_picture, 7_297_920);
        assert_eq!(hevc_layer_tables, 3_178_023);
        assert_eq!(
            native_vulkan_ffmpeg_infer_codec_resolution_scaled_host_bytes(
                NativeVulkanVideoSessionCodec::H265Main8,
                0,
                hevc_per_picture.saturating_mul(3),
                hevc_layer_tables,
            ),
            25_071_783
        );
        assert_eq!(
            native_vulkan_ffmpeg_infer_codec_resolution_scaled_host_bytes(
                NativeVulkanVideoSessionCodec::Av1Main8,
                0,
                0,
                0,
            ),
            0
        );
    }

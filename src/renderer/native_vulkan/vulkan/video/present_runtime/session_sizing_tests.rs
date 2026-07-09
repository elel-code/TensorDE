
fn native_vulkan_vulkanalia_select_stream_session_dpb_slots(
    required_dpb_slots: u32,
    driver_session_max_dpb_slots: u32,
) -> Result<u32, String> {
    let required_dpb_slots = required_dpb_slots.max(1);
    if driver_session_max_dpb_slots != 0 && required_dpb_slots > driver_session_max_dpb_slots {
        return Err(format!(
            "streaming decode requires {required_dpb_slots} DPB slot(s), but the selected Vulkan video session exposes only {driver_session_max_dpb_slots}"
        ));
    }
    // Keep the session sized to the stream, not the driver's advertised ceiling.
    // FFmpeg adds a small fixed output-frame reserve after
    // avcodec_get_hw_frames_parameters()
    // (references/ffmpeg/libavcodec/decode.c:1088-1095). This runtime owns one
    // coincident decoded-image array for DPB/output/sampling, so retaining the
    // driver's full maxDpbSlots here only pins unused image/session memory.
    Ok(required_dpb_slots)
}

fn native_vulkan_vulkanalia_select_stream_resource_image_array_layers(
    required_dpb_slots: u32,
    session_max_dpb_slots: u32,
) -> Result<u32, String> {
    let required_dpb_slots = required_dpb_slots.max(1);
    if session_max_dpb_slots != 0 && required_dpb_slots > session_max_dpb_slots {
        return Err(format!(
            "streaming decode requires {required_dpb_slots} resource image layer(s), but the selected Vulkan video session exposes only {session_max_dpb_slots}"
        ));
    }
    // FFmpeg's separate layered DPB path may allocate caps.maxDpbSlots
    // (references/ffmpeg/libavcodec/vulkan_decode.c:1388-1431). This runtime
    // deliberately uses one coincident sampled image array, so layer count must
    // track the stream-required DPB/output ring instead of the driver ceiling.
    Ok(required_dpb_slots)
}

fn native_vulkan_vulkanalia_select_stream_session_active_reference_pictures(
    required_active_reference_pictures: u32,
    driver_session_max_active_reference_pictures: u32,
    session_max_dpb_slots: u32,
) -> Result<u32, String> {
    if session_max_dpb_slots == 0 {
        return Ok(0);
    }
    let required_active_reference_pictures = required_active_reference_pictures
        .max(1)
        .min(session_max_dpb_slots);
    if driver_session_max_active_reference_pictures != 0
        && required_active_reference_pictures > driver_session_max_active_reference_pictures
    {
        return Err(format!(
            "streaming decode requires {required_active_reference_pictures} active reference picture(s), but the selected Vulkan video session exposes only {driver_session_max_active_reference_pictures}"
        ));
    }
    Ok(required_active_reference_pictures)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_session_sizing_uses_stream_required_dpb_and_driver_as_ceiling() {
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_session_dpb_slots(3, 16).unwrap(),
            3
        );
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_session_dpb_slots(3, 5).unwrap(),
            3
        );
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_resource_image_array_layers(3, 16).unwrap(),
            3
        );
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_resource_image_array_layers(3, 5).unwrap(),
            3
        );
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_session_active_reference_pictures(3, 16, 3)
                .unwrap(),
            3
        );
    }

    #[test]
    fn av1_stream_resource_layers_follow_stream_dpb_slots() {
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_session_dpb_slots(9, 16).unwrap(),
            9
        );
        assert_eq!(
            native_vulkan_vulkanalia_select_stream_resource_image_array_layers(9, 9).unwrap(),
            9
        );
    }

    #[test]
    fn ffmpeg_decode_async_exec_depth_matches_single_decode_thread_formula() {
        assert_eq!(
            native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(1),
            2
        );
        assert_eq!(
            native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(4),
            2
        );
        assert_eq!(
            native_vulkan_vulkanalia_ffmpeg_decode_async_exec_depth(0),
            2
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn ffmpeg_hwdecode_codec_operations_match_selected_codec() {
        assert_eq!(
            native_vulkan_ffmpeg_vulkan_hw_codec_operations(
                NativeVulkanVideoSessionCodec::H264High8
            ),
            vk::VideoCodecOperationFlagsKHR::DECODE_H264
        );
        assert_eq!(
            native_vulkan_ffmpeg_vulkan_hw_codec_operations(
                NativeVulkanVideoSessionCodec::H265Main10
            ),
            vk::VideoCodecOperationFlagsKHR::DECODE_H265
        );
        assert_eq!(
            native_vulkan_ffmpeg_vulkan_hw_codec_operations(
                NativeVulkanVideoSessionCodec::Av1Main10
            ),
            vk::VideoCodecOperationFlagsKHR::DECODE_AV1
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn ffmpeg_hwdecode_time_base_rescales_pts_to_nanoseconds() {
        assert_eq!(
            native_vulkan_ffmpeg_time_base_timestamp_ns(Some(48), (1, 24)),
            Some(2_000_000_000)
        );
        assert_eq!(
            native_vulkan_ffmpeg_time_base_timestamp_ns(Some(90_000), (1, 90_000)),
            Some(1_000_000_000)
        );
        assert_eq!(
            native_vulkan_ffmpeg_time_base_timestamp_ns(None, (1, 90_000)),
            None
        );
    }

    #[test]
    fn present_frame_timer_uses_ffmpeg_pts_delta_before_duration_fallback() {
        let mut timer = NativeVulkanVulkanaliaPresentFrameTimer::new(
            Some(240),
            NativeVulkanVulkanaliaVideoPresentAudioMasterClock::DISABLED,
        );
        timer.last_pts_ns = Some(1_000_000_000);
        timer.last_duration_ns = Some(4_166_667);

        assert_eq!(
            timer.next_delay(Some(1_004_000_000), Some(5_000_000)),
            (
                Duration::from_nanos(4_000_000),
                "ffmpeg-frame-timer-pts-delta-sleep"
            )
        );
        assert_eq!(
            timer.next_delay(None, Some(5_000_000)),
            (
                Duration::from_nanos(4_166_667),
                "ffmpeg-frame-timer-last-duration-sleep"
            )
        );
    }

    #[test]
    fn present_frame_timer_falls_back_to_target_fps_without_pts_or_duration() {
        let timer = NativeVulkanVulkanaliaPresentFrameTimer::new(
            Some(240),
            NativeVulkanVulkanaliaVideoPresentAudioMasterClock::DISABLED,
        );

        assert_eq!(
            timer.next_delay(None, None),
            (
                Duration::from_nanos(4_166_666),
                "ffmpeg-frame-timer-target-fps-sleep"
            )
        );
    }

    #[test]
    fn present_frame_timer_audio_master_uses_rebased_video_pts() {
        let mut timer = NativeVulkanVulkanaliaPresentFrameTimer::new(
            Some(240),
            NativeVulkanVulkanaliaVideoPresentAudioMasterClock::clock_only(None),
        );
        let started_at = Instant::now();
        timer.reset(started_at);

        assert_eq!(
            timer.audio_master_delay_for_frame(
                started_at + Duration::from_micros(1_000),
                1,
                Some(4_166_666),
                None,
            ),
            Some((
                Duration::from_nanos(3_166_666),
                "audio-clock-master-pts-sync-sleep"
            ))
        );
        assert_eq!(
            timer.audio_master_delay_for_frame(
                started_at + Duration::from_micros(5_000),
                1,
                Some(4_166_666),
                None,
            ),
            Some((Duration::ZERO, "audio-clock-master-video-late-no-sleep"))
        );
    }

    #[test]
    fn present_frame_timer_audio_master_starts_from_audio_clock_sample() {
        let mut timer = NativeVulkanVulkanaliaPresentFrameTimer::new(
            Some(240),
            NativeVulkanVulkanaliaVideoPresentAudioMasterClock::clock_only(Some(2_000_000)),
        );
        let started_at = Instant::now();
        timer.reset(started_at);

        assert_eq!(
            timer.audio_master_delay_for_frame(
                started_at + Duration::from_micros(1_000),
                1,
                Some(5_000_000),
                None,
            ),
            Some((
                Duration::from_nanos(2_000_000),
                "audio-clock-master-pts-sync-sleep"
            ))
        );
    }

    #[test]
    fn decoded_present_startup_preroll_is_first_frame_driven() {
        assert_eq!(DECODED_IMAGE_PRESENT_STARTUP_PREROLL_FRAMES, 1);
        assert!(DECODED_IMAGE_PRESENT_STARTUP_PREROLL_FRAMES <= FFMPEG_VIDEO_PICTURE_QUEUE_SIZE);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn streaming_pts_state_rebases_each_source_loop_to_segment_start() {
        let mut pts = NativeVulkanVulkanaliaStreamingPtsState::new(0);

        assert_eq!(
            pts.adjusted_pts_ns(Some(650_000_000), Some(650), Some(4_166_667), Some(4)),
            Some(0)
        );
        assert_eq!(
            pts.adjusted_pts_ns(Some(654_166_667), Some(654), Some(4_166_667), Some(4)),
            Some(4_166_667)
        );

        assert!(pts.sync_loop(1));
        assert_eq!(
            pts.adjusted_pts_ns(Some(650_000_000), Some(650), Some(4_166_667), Some(4)),
            Some(8_333_334)
        );
        assert_eq!(
            pts.adjusted_pts_ns(Some(654_166_667), Some(654), Some(4_166_667), Some(4)),
            Some(12_500_001)
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn multi_source_present_frame_count_uses_longest_source_budget() {
        let sources = vec![
            NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions {
                source: PathBuf::from("/tmp/sky.mp4"),
                codec: NativeVulkanVideoSessionCodec::H264High8,
                width: 1920,
                height: 1080,
                queue_capacity: 3,
                playback_frame_count: 120,
            },
            NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions {
                source: PathBuf::from("/tmp/character.mp4"),
                codec: NativeVulkanVideoSessionCodec::H264High8,
                width: 640,
                height: 1080,
                queue_capacity: 3,
                playback_frame_count: 60,
            },
            NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions {
                source: PathBuf::from("/tmp/effects.mp4"),
                codec: NativeVulkanVideoSessionCodec::H264High8,
                width: 1920,
                height: 1080,
                queue_capacity: 3,
                playback_frame_count: 240,
            },
        ];

        let plan = NativeVulkanVulkanaliaMultiVideoDecodePlan::from_sources(&sources).unwrap();
        assert_eq!(plan.source_count, 3);
        assert_eq!(plan.requested_present_frame_count, 240);
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn multi_source_codec_set_preserves_distinct_codec_order() {
        let sources = vec![
            NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions {
                source: PathBuf::from("/tmp/sky-h264.mp4"),
                codec: NativeVulkanVideoSessionCodec::H264High8,
                width: 1920,
                height: 1080,
                queue_capacity: 3,
                playback_frame_count: 120,
            },
            NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions {
                source: PathBuf::from("/tmp/fx-h265-main10.mp4"),
                codec: NativeVulkanVideoSessionCodec::H265Main10,
                width: 1920,
                height: 1080,
                queue_capacity: 3,
                playback_frame_count: 120,
            },
            NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions {
                source: PathBuf::from("/tmp/overlay-av1.mp4"),
                codec: NativeVulkanVideoSessionCodec::Av1Main8,
                width: 640,
                height: 360,
                queue_capacity: 3,
                playback_frame_count: 120,
            },
            NativeVulkanVulkanaliaStreamingVideoPresentDecodeSourceOptions {
                source: PathBuf::from("/tmp/reuse-h265-main8.mp4"),
                codec: NativeVulkanVideoSessionCodec::H265Main8,
                width: 640,
                height: 360,
                queue_capacity: 3,
                playback_frame_count: 120,
            },
        ];

        let plan = NativeVulkanVulkanaliaMultiVideoDecodePlan::from_sources(&sources).unwrap();
        assert_eq!(plan.source_count, 4);
        assert_eq!(
            plan.codecs(),
            &[
                NativeVulkanVideoSessionCodec::H264High8,
                NativeVulkanVideoSessionCodec::H265Main10,
                NativeVulkanVideoSessionCodec::Av1Main8,
                NativeVulkanVideoSessionCodec::H265Main8,
            ]
        );
    }

    #[test]
    fn stream_session_sizing_rejects_driver_capability_overflow() {
        let dpb_err = native_vulkan_vulkanalia_select_stream_session_dpb_slots(4, 3)
            .expect_err("driver max must bound DPB sizing");
        assert!(dpb_err.contains("requires 4 DPB slot"));

        let refs_err =
            native_vulkan_vulkanalia_select_stream_session_active_reference_pictures(4, 3, 4)
                .expect_err("driver max must bound active reference sizing");
        assert!(refs_err.contains("requires 4 active reference picture"));
    }
}

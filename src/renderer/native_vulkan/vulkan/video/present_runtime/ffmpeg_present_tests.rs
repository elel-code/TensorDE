#[test]
fn ffmpeg_time_base_rescales_pts_to_nanoseconds() {
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
fn ffmpeg_codec_operations_match_selected_codec() {
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
            NativeVulkanVideoSessionCodec::Av1Main8
        ),
        vk::VideoCodecOperationFlagsKHR::DECODE_AV1
    );
}

#[test]
fn unbounded_present_timer_does_not_invent_a_delay() {
    let timer = NativeVulkanVulkanaliaPresentFrameTimer::new(
        None,
        NativeVulkanVulkanaliaVideoPresentAudioMasterClock::DISABLED,
    );

    assert_eq!(timer.next_delay(None, None), (Duration::ZERO, "unpaced-no-video-clock"));
}

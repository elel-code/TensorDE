#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanVideoAudioRuntimeTelemetry {
    pub(super) audio_provider: &'static str,
    pub(super) reached_clocked_playback: bool,
    pub(super) audio_buffer_count: u32,
    pub(super) audio_output_sink_count: usize,
    pub(super) audio_loop_seek_count: u32,
    pub(super) audio_loop_seek_error_count: u32,
    pub(super) audio_loop_restart_count: u32,
    pub(super) audio_last_loop_seek_position_ms: Option<u64>,
    pub(super) audio_clock_serial: u32,
    pub(super) audio_segment_start_position_ns: Option<u64>,
    pub(super) audio_segment_elapsed_ns: Option<u64>,
    pub(super) audio_position_stale_count: u32,
    pub(super) audio_sample_stale_count: u32,
    pub(super) audio_master_clock_estimate_ns: Option<u64>,
    pub(super) sampled_video_frame_count: u32,
    pub(super) audio_position_query_count: u32,
    pub(super) audio_position_query_hit_count: u32,
    pub(super) audio_video_clock_drift_latest_ns: Option<i64>,
    pub(super) audio_video_master_clock_drift_latest_ns: Option<i64>,
    pub(super) audio_video_master_clock_drift_abs_max_ns: Option<u64>,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanVideoAudioRuntimeTelemetry {
    pub(super) fn from_audio_clock_runtime(
        audio_provider: &'static str,
        value: super::audio_clock::NativeVulkanAudioClockRuntimeTelemetry,
    ) -> Self {
        Self {
            audio_provider,
            reached_clocked_playback: value.reached_clocked_playback,
            audio_buffer_count: value.audio_buffer_count,
            audio_output_sink_count: value.audio_output_sink_count,
            audio_loop_seek_count: value.audio_loop_seek_count,
            audio_loop_seek_error_count: value.audio_loop_seek_error_count,
            audio_loop_restart_count: value.audio_loop_restart_count,
            audio_last_loop_seek_position_ms: value.audio_last_loop_seek_position_ms,
            audio_clock_serial: value.audio_clock_serial,
            audio_segment_start_position_ns: value.audio_segment_start_position_ns,
            audio_segment_elapsed_ns: value.audio_segment_elapsed_ns,
            audio_position_stale_count: value.audio_position_stale_count,
            audio_sample_stale_count: value.audio_sample_stale_count,
            audio_master_clock_estimate_ns: value.audio_master_clock_estimate_ns,
            sampled_video_frame_count: value.sampled_video_frame_count,
            audio_position_query_count: value.audio_position_query_count,
            audio_position_query_hit_count: value.audio_position_query_hit_count,
            audio_video_clock_drift_latest_ns: value.audio_video_clock_drift_latest_ns,
            audio_video_master_clock_drift_latest_ns: value
                .audio_video_master_clock_drift_latest_ns,
            audio_video_master_clock_drift_abs_max_ns: value
                .audio_video_master_clock_drift_abs_max_ns,
        }
    }
}

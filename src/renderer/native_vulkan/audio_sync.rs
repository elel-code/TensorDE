use super::NativeVulkanError;
use super::audio_frontend::NativeVulkanAudioClockRuntimeFrontend;
use super::pacing;

pub(super) fn native_vulkan_sample_audio_clock_for_video_frame(
    audio_clock_probe: &mut NativeVulkanAudioClockRuntimeFrontend,
    video_clock_frame_index: u32,
    target_max_fps: Option<u32>,
    frame_pts_ms: Option<u64>,
    video_clock_segment_start_pts_ms: Option<u64>,
    loop_boundary_reset: bool,
) -> Result<(), NativeVulkanError> {
    if loop_boundary_reset {
        audio_clock_probe.seek_for_video_loop(frame_pts_ms.unwrap_or(0))?;
    }
    audio_clock_probe.sample_video_pts_ms(
        frame_pts_ms,
        pacing::native_vulkan_audio_probe_video_clock_ns(
            video_clock_frame_index,
            target_max_fps,
            frame_pts_ms,
            video_clock_segment_start_pts_ms,
        ),
    )
}

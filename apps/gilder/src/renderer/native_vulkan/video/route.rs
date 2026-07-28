//! Playback-frame planning for the single FFmpeg Vulkan video route.

use std::time::Duration;

pub fn native_vulkan_video_duration_playback_frames(
    duration: Duration,
    target_max_fps: Option<u32>,
) -> Option<u32> {
    let fps = u128::from(target_max_fps?);
    if fps == 0 {
        return None;
    }
    let nanos = duration.as_nanos();
    if nanos == 0 {
        return Some(1);
    }
    let frames = nanos.saturating_mul(fps).saturating_add(999_999_999) / 1_000_000_000;
    Some(u32::try_from(frames).unwrap_or(u32::MAX).max(1))
}

pub fn native_vulkan_video_playback_frame_count(
    requested_playback_frames: u32,
    duration_playback_frames: Option<u32>,
) -> u32 {
    if requested_playback_frames > 0 {
        requested_playback_frames
    } else {
        duration_playback_frames.unwrap_or(1).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_uses_the_selected_present_rate() {
        assert_eq!(
            native_vulkan_video_duration_playback_frames(Duration::from_secs(10), Some(240)),
            Some(2_400)
        );
    }

    #[test]
    fn explicit_frame_budget_overrides_duration() {
        assert_eq!(
            native_vulkan_video_playback_frame_count(96, Some(2_400)),
            96
        );
    }

    #[test]
    fn unbounded_duration_still_requests_a_frame() {
        assert_eq!(native_vulkan_video_playback_frame_count(0, None), 1);
    }
}

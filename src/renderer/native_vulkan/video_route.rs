use std::time::Duration;

use serde::Serialize;

use super::{NativeVulkanVideoSessionCodec, NativeVulkanVideoSessionSmokeOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanVideoRunRouteKind {
    LegacyVideo,
    VulkanaliaReadyPrefix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoReadyPrefixCounts {
    pub h264: u32,
    pub h265: u32,
    pub av1: u32,
}

impl NativeVulkanVideoReadyPrefixCounts {
    pub fn from_smoke_options(
        options: &NativeVulkanVideoSessionSmokeOptions,
        av1_ready_prefix_frames: u32,
    ) -> Self {
        Self {
            h264: options.decode_h264_ready_prefix_frames,
            h265: options.decode_h265_ready_prefix_frames,
            av1: av1_ready_prefix_frames,
        }
    }

    pub fn matching(self, codec: NativeVulkanVideoSessionCodec) -> u32 {
        match codec {
            NativeVulkanVideoSessionCodec::H264High8 => self.h264,
            NativeVulkanVideoSessionCodec::H265Main8
            | NativeVulkanVideoSessionCodec::H265Main10 => self.h265,
            NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
                self.av1
            }
        }
    }

    pub fn any(self) -> bool {
        self.h264 > 0 || self.h265 > 0 || self.av1 > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoRunRouteDecision {
    pub kind: NativeVulkanVideoRunRouteKind,
    pub status: &'static str,
    pub fallback_allowed: bool,
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
    pub ready_prefix_frames: u32,
    pub playback_frames: u32,
}

impl NativeVulkanVideoRunRouteDecision {
    pub fn is_vulkanalia_ready_prefix(self) -> bool {
        self.kind == NativeVulkanVideoRunRouteKind::VulkanaliaReadyPrefix
    }
}

pub fn native_vulkan_video_default_ready_prefix_frames(
    _codec: NativeVulkanVideoSessionCodec,
) -> u32 {
    16
}

pub fn native_vulkan_video_run_route(
    options: &NativeVulkanVideoSessionSmokeOptions,
    av1_ready_prefix_frames: u32,
    requested_playback_frames: u32,
    duration_playback_frames: Option<u32>,
) -> NativeVulkanVideoRunRouteDecision {
    let counts =
        NativeVulkanVideoReadyPrefixCounts::from_smoke_options(options, av1_ready_prefix_frames);
    let requested_ready_prefix_frames = counts.matching(options.codec);
    let default_ready_prefix_frames =
        native_vulkan_video_default_ready_prefix_frames(options.codec);
    let ready_prefix_frames = if requested_ready_prefix_frames == 0 && !counts.any() {
        default_ready_prefix_frames
    } else {
        requested_ready_prefix_frames
    };
    if ready_prefix_frames == 0 {
        return NativeVulkanVideoRunRouteDecision {
            kind: NativeVulkanVideoRunRouteKind::LegacyVideo,
            status: if counts.any() {
                "ready-prefix-count-does-not-match-video-codec"
            } else {
                "no-ready-prefix-requested"
            },
            fallback_allowed: false,
            codec: options.codec,
            width: options.width,
            height: options.height,
            ready_prefix_frames: 0,
            playback_frames: 0,
        };
    }

    let playback_frames = native_vulkan_video_playback_frame_count(
        ready_prefix_frames,
        requested_playback_frames,
        duration_playback_frames,
    );

    if options.width == 0 || options.height == 0 {
        return NativeVulkanVideoRunRouteDecision {
            kind: NativeVulkanVideoRunRouteKind::LegacyVideo,
            status: "vulkanalia-ready-prefix-requires-non-zero-extent",
            fallback_allowed: false,
            codec: options.codec,
            width: options.width,
            height: options.height,
            ready_prefix_frames,
            playback_frames,
        };
    }

    NativeVulkanVideoRunRouteDecision {
        kind: NativeVulkanVideoRunRouteKind::VulkanaliaReadyPrefix,
        status: if counts.any() {
            "vulkanalia-ready-prefix"
        } else {
            "vulkanalia-ready-prefix-default"
        },
        fallback_allowed: false,
        codec: options.codec,
        width: options.width,
        height: options.height,
        ready_prefix_frames,
        playback_frames,
    }
}

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
    ready_prefix_frames: u32,
    requested_playback_frames: u32,
    duration_playback_frames: Option<u32>,
) -> u32 {
    let requested = if requested_playback_frames > 0 {
        requested_playback_frames
    } else {
        duration_playback_frames.unwrap_or(ready_prefix_frames)
    };
    requested.max(ready_prefix_frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(codec: NativeVulkanVideoSessionCodec) -> NativeVulkanVideoSessionSmokeOptions {
        NativeVulkanVideoSessionSmokeOptions {
            codec,
            ..Default::default()
        }
    }

    #[test]
    fn h264_ready_prefix_routes_to_vulkanalia() {
        let mut options = options(NativeVulkanVideoSessionCodec::H264High8);
        options.decode_h264_ready_prefix_frames = 5;

        let route = native_vulkan_video_run_route(&options, 0, 0, None);

        assert!(route.is_vulkanalia_ready_prefix());
        assert_eq!(route.ready_prefix_frames, 5);
        assert_eq!(route.playback_frames, 5);
    }

    #[test]
    fn h265_main10_uses_h265_ready_prefix_count() {
        let mut options = options(NativeVulkanVideoSessionCodec::H265Main10);
        options.decode_h265_ready_prefix_frames = 7;

        let route = native_vulkan_video_run_route(&options, 0, 3, None);

        assert_eq!(
            route.kind,
            NativeVulkanVideoRunRouteKind::VulkanaliaReadyPrefix
        );
        assert_eq!(route.ready_prefix_frames, 7);
        assert_eq!(route.playback_frames, 7);
    }

    #[test]
    fn av1_main10_uses_av1_ready_prefix_count() {
        let options = options(NativeVulkanVideoSessionCodec::Av1Main10);

        let route = native_vulkan_video_run_route(&options, 9, 0, None);

        assert_eq!(
            route.kind,
            NativeVulkanVideoRunRouteKind::VulkanaliaReadyPrefix
        );
        assert_eq!(route.ready_prefix_frames, 9);
        assert_eq!(route.playback_frames, 9);
    }

    #[test]
    fn no_ready_prefix_defaults_to_vulkanalia_ready_prefix() {
        let options = options(NativeVulkanVideoSessionCodec::H265Main8);

        let route = native_vulkan_video_run_route(&options, 0, 0, None);

        assert_eq!(
            route.kind,
            NativeVulkanVideoRunRouteKind::VulkanaliaReadyPrefix
        );
        assert_eq!(route.status, "vulkanalia-ready-prefix-default");
        assert_eq!(
            route.ready_prefix_frames,
            native_vulkan_video_default_ready_prefix_frames(options.codec)
        );
        assert!(!route.fallback_allowed);
    }

    #[test]
    fn mismatched_ready_prefix_does_not_silently_fallback() {
        let mut options = options(NativeVulkanVideoSessionCodec::H265Main8);
        options.decode_h264_ready_prefix_frames = 4;

        let route = native_vulkan_video_run_route(&options, 0, 0, None);

        assert_eq!(route.kind, NativeVulkanVideoRunRouteKind::LegacyVideo);
        assert_eq!(
            route.status,
            "ready-prefix-count-does-not-match-video-codec"
        );
        assert!(!route.fallback_allowed);
    }

    #[test]
    fn zero_extent_ready_prefix_does_not_silently_fallback() {
        let mut options = options(NativeVulkanVideoSessionCodec::H264High8);
        options.width = 0;
        options.decode_h264_ready_prefix_frames = 2;

        let route = native_vulkan_video_run_route(&options, 0, 0, None);

        assert_eq!(route.kind, NativeVulkanVideoRunRouteKind::LegacyVideo);
        assert_eq!(
            route.status,
            "vulkanalia-ready-prefix-requires-non-zero-extent"
        );
        assert!(!route.fallback_allowed);
    }

    #[test]
    fn duration_target_fps_drives_ready_prefix_playback_when_no_explicit_count() {
        let mut options = options(NativeVulkanVideoSessionCodec::H265Main8);
        options.decode_h265_ready_prefix_frames = 16;

        let route = native_vulkan_video_run_route(&options, 0, 0, Some(2400));

        assert_eq!(
            route.kind,
            NativeVulkanVideoRunRouteKind::VulkanaliaReadyPrefix
        );
        assert_eq!(route.ready_prefix_frames, 16);
        assert_eq!(route.playback_frames, 2400);
    }

    #[test]
    fn explicit_playback_count_wins_over_duration_count() {
        let mut options = options(NativeVulkanVideoSessionCodec::H265Main8);
        options.decode_h265_ready_prefix_frames = 16;

        let route = native_vulkan_video_run_route(&options, 0, 96, Some(2400));

        assert_eq!(route.playback_frames, 96);
    }

    #[test]
    fn playback_count_never_shrinks_below_ready_prefix_window() {
        assert_eq!(native_vulkan_video_playback_frame_count(16, 4, None), 16);
        assert_eq!(native_vulkan_video_playback_frame_count(16, 0, Some(4)), 16);
    }

    #[test]
    fn duration_playback_frames_ceil_to_target_fps() {
        assert_eq!(
            native_vulkan_video_duration_playback_frames(Duration::from_secs(10), Some(240)),
            Some(2400)
        );
        assert_eq!(
            native_vulkan_video_duration_playback_frames(Duration::from_millis(1), Some(240)),
            Some(1)
        );
        assert_eq!(
            native_vulkan_video_duration_playback_frames(Duration::from_secs(10), None),
            None
        );
    }
}

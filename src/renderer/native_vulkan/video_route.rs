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

pub fn native_vulkan_video_run_route(
    options: &NativeVulkanVideoSessionSmokeOptions,
    av1_ready_prefix_frames: u32,
    requested_playback_frames: u32,
) -> NativeVulkanVideoRunRouteDecision {
    let counts =
        NativeVulkanVideoReadyPrefixCounts::from_smoke_options(options, av1_ready_prefix_frames);
    let ready_prefix_frames = counts.matching(options.codec);
    if ready_prefix_frames == 0 {
        return NativeVulkanVideoRunRouteDecision {
            kind: NativeVulkanVideoRunRouteKind::LegacyVideo,
            status: if counts.any() {
                "ready-prefix-count-does-not-match-video-codec"
            } else {
                "no-ready-prefix-requested"
            },
            fallback_allowed: !counts.any(),
            codec: options.codec,
            width: options.width,
            height: options.height,
            ready_prefix_frames: 0,
            playback_frames: 0,
        };
    }

    let playback_frames = if requested_playback_frames == 0 {
        ready_prefix_frames
    } else {
        requested_playback_frames
    };

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
        status: "vulkanalia-ready-prefix",
        fallback_allowed: false,
        codec: options.codec,
        width: options.width,
        height: options.height,
        ready_prefix_frames,
        playback_frames,
    }
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

        let route = native_vulkan_video_run_route(&options, 0, 0);

        assert!(route.is_vulkanalia_ready_prefix());
        assert_eq!(route.ready_prefix_frames, 5);
        assert_eq!(route.playback_frames, 5);
    }

    #[test]
    fn h265_main10_uses_h265_ready_prefix_count() {
        let mut options = options(NativeVulkanVideoSessionCodec::H265Main10);
        options.decode_h265_ready_prefix_frames = 7;

        let route = native_vulkan_video_run_route(&options, 0, 3);

        assert_eq!(
            route.kind,
            NativeVulkanVideoRunRouteKind::VulkanaliaReadyPrefix
        );
        assert_eq!(route.ready_prefix_frames, 7);
        assert_eq!(route.playback_frames, 3);
    }

    #[test]
    fn av1_main10_uses_av1_ready_prefix_count() {
        let options = options(NativeVulkanVideoSessionCodec::Av1Main10);

        let route = native_vulkan_video_run_route(&options, 9, 0);

        assert_eq!(
            route.kind,
            NativeVulkanVideoRunRouteKind::VulkanaliaReadyPrefix
        );
        assert_eq!(route.ready_prefix_frames, 9);
        assert_eq!(route.playback_frames, 9);
    }

    #[test]
    fn no_ready_prefix_allows_legacy_video_fallback() {
        let options = options(NativeVulkanVideoSessionCodec::H265Main8);

        let route = native_vulkan_video_run_route(&options, 0, 0);

        assert_eq!(route.kind, NativeVulkanVideoRunRouteKind::LegacyVideo);
        assert_eq!(route.status, "no-ready-prefix-requested");
        assert!(route.fallback_allowed);
    }

    #[test]
    fn mismatched_ready_prefix_does_not_silently_fallback() {
        let mut options = options(NativeVulkanVideoSessionCodec::H265Main8);
        options.decode_h264_ready_prefix_frames = 4;

        let route = native_vulkan_video_run_route(&options, 0, 0);

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

        let route = native_vulkan_video_run_route(&options, 0, 0);

        assert_eq!(route.kind, NativeVulkanVideoRunRouteKind::LegacyVideo);
        assert_eq!(
            route.status,
            "vulkanalia-ready-prefix-requires-non-zero-extent"
        );
        assert!(!route.fallback_allowed);
    }
}

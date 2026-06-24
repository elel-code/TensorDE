use super::NativeVulkanRenderItem;
use super::video_runtime::NativeVulkanVideoAudioRuntimeTelemetry;
#[cfg(feature = "native-vulkan-gst-video")]
use super::{NativeVulkanAudioOutputMode, NativeVulkanAudioOutputPolicy};

#[derive(Default)]
pub(super) struct NativeVulkanPlanAudioRuntime {
    #[cfg(feature = "native-vulkan-gst-video")]
    probe: Option<super::audio_clock::NativeVulkanAudioClockRuntimeProbe>,
    #[cfg(feature = "native-vulkan-gst-video")]
    last_error: Option<String>,
}

impl NativeVulkanPlanAudioRuntime {
    pub(super) fn start_for_render_item(item: &NativeVulkanRenderItem) -> Self {
        #[cfg(feature = "native-vulkan-gst-video")]
        {
            return Self::start_for_render_item_gst(item);
        }
        #[cfg(not(feature = "native-vulkan-gst-video"))]
        {
            let _ = item;
            Self::default()
        }
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn start_for_render_item_gst(item: &NativeVulkanRenderItem) -> Self {
        let NativeVulkanRenderItem::Video { source, muted, .. } = item else {
            return Self::default();
        };
        let output_mode = NativeVulkanAudioOutputPolicy::Plan.resolve(*muted);
        if output_mode != NativeVulkanAudioOutputMode::Auto {
            return Self::default();
        }
        match super::audio_clock::NativeVulkanAudioClockRuntimeProbe::start(source, output_mode) {
            Ok(probe) => Self {
                probe: Some(probe),
                last_error: None,
            },
            Err(err) => Self {
                probe: None,
                last_error: Some(err.to_string()),
            },
        }
    }

    pub(super) fn poll_video_clock(&mut self, video_clock_ns: u64) {
        #[cfg(feature = "native-vulkan-gst-video")]
        if let Some(probe) = self.probe.as_mut() {
            if let Err(err) = probe.sample_video_pts_ms(None, Some(video_clock_ns)) {
                self.last_error = Some(err.to_string());
                self.probe = None;
            }
        }
        #[cfg(not(feature = "native-vulkan-gst-video"))]
        {
            let _ = video_clock_ns;
        }
    }

    pub(super) fn telemetry(&self) -> Option<NativeVulkanVideoAudioRuntimeTelemetry> {
        #[cfg(feature = "native-vulkan-gst-video")]
        {
            return self.probe.as_ref().map(|probe| probe.telemetry().into());
        }
        #[cfg(not(feature = "native-vulkan-gst-video"))]
        {
            None
        }
    }

    pub(super) fn last_error(&self) -> Option<String> {
        #[cfg(feature = "native-vulkan-gst-video")]
        {
            return self.last_error.clone();
        }
        #[cfg(not(feature = "native-vulkan-gst-video"))]
        {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::config::VideoDecoderPolicy;
    use crate::core::FitMode;

    use super::*;

    fn video_item(muted: bool) -> NativeVulkanRenderItem {
        NativeVulkanRenderItem::Video {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/gilder-missing-audio-runtime-test.mp4"),
            poster: None,
            fit: FitMode::Cover,
            loop_playback: true,
            muted,
            manifest_max_fps: None,
            target_max_fps: Some(60),
            decoder_policy: VideoDecoderPolicy::HardwarePreferred,
            start_offset_ms: 0,
            renderer_status: "vulkan-lifecycle-video-placeholder",
        }
    }

    #[test]
    fn muted_plan_does_not_start_audio_runtime() {
        let runtime = NativeVulkanPlanAudioRuntime::start_for_render_item(&video_item(true));

        assert_eq!(runtime.telemetry(), None);
        assert_eq!(runtime.last_error(), None);
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    #[test]
    fn unmuted_missing_source_records_audio_runtime_error() {
        let runtime = NativeVulkanPlanAudioRuntime::start_for_render_item(&video_item(false));

        assert_eq!(runtime.telemetry(), None);
        assert!(
            runtime
                .last_error()
                .expect("audio runtime error")
                .contains("audio clock runtime source does not exist")
        );
    }
}

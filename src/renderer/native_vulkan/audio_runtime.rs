#[cfg(feature = "native-vulkan-gst-video")]
use super::NativeVulkanAudioOutputPolicy;
use super::NativeVulkanRenderItem;
use super::audio_telemetry::NativeVulkanVideoAudioRuntimeTelemetry;
#[cfg(feature = "native-vulkan-gst-video")]
use super::audio_worker::{
    NativeVulkanPlanAudioRuntimeSharedState, NativeVulkanPlanAudioRuntimeWorker,
    native_vulkan_audio_runtime_state, native_vulkan_audio_runtime_state_with_error,
};

#[derive(Default)]
pub(super) struct NativeVulkanPlanAudioRuntime {
    #[cfg(feature = "native-vulkan-gst-video")]
    worker: Option<NativeVulkanPlanAudioRuntimeWorker>,
    #[cfg(feature = "native-vulkan-gst-video")]
    state: NativeVulkanPlanAudioRuntimeSharedState,
    #[cfg(feature = "native-vulkan-gst-video")]
    video_clock_serial: u32,
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
        let state = NativeVulkanPlanAudioRuntimeSharedState::default();
        let NativeVulkanRenderItem::Video { source, muted, .. } = item else {
            return Self {
                worker: None,
                state,
                video_clock_serial: 0,
            };
        };
        let output_mode = NativeVulkanAudioOutputPolicy::Plan.resolve(*muted);
        match super::audio_frontend::NativeVulkanAudioClockRuntimeFrontend::start(
            source,
            output_mode,
        ) {
            Ok(probe) => {
                let worker = NativeVulkanPlanAudioRuntimeWorker::start(probe, state.clone());
                Self {
                    worker: Some(worker),
                    state,
                    video_clock_serial: 0,
                }
            }
            Err(err) => Self {
                worker: None,
                state: native_vulkan_audio_runtime_state_with_error(state, err.to_string()),
                video_clock_serial: 0,
            },
        }
    }

    pub(super) fn poll_video_clock(&mut self, video_clock_ns: u64) {
        #[cfg(feature = "native-vulkan-gst-video")]
        if let Some(worker) = self.worker.as_ref() {
            if let Err(err) = worker.send_video_clock(video_clock_ns, self.video_clock_serial) {
                native_vulkan_audio_runtime_state(&self.state).last_error = Some(err);
                self.worker = None;
            }
        }
        #[cfg(not(feature = "native-vulkan-gst-video"))]
        {
            let _ = video_clock_ns;
        }
    }

    #[cfg_attr(not(feature = "native-vulkan-gst-video"), allow(dead_code))]
    pub(super) fn seek_for_video_loop(&mut self, position_ms: u64) {
        #[cfg(feature = "native-vulkan-gst-video")]
        {
            self.video_clock_serial = self.video_clock_serial.saturating_add(1);
            if let Some(worker) = self.worker.as_ref() {
                if let Err(err) = worker.seek_for_video_loop(position_ms, self.video_clock_serial) {
                    native_vulkan_audio_runtime_state(&self.state).last_error = Some(err);
                    self.worker = None;
                }
            }
        }
        #[cfg(not(feature = "native-vulkan-gst-video"))]
        {
            let _ = position_ms;
        }
    }

    pub(super) fn telemetry(&self) -> Option<NativeVulkanVideoAudioRuntimeTelemetry> {
        #[cfg(feature = "native-vulkan-gst-video")]
        {
            return native_vulkan_audio_runtime_state(&self.state).telemetry;
        }
        #[cfg(not(feature = "native-vulkan-gst-video"))]
        {
            None
        }
    }

    pub(super) fn last_error(&self) -> Option<String> {
        #[cfg(feature = "native-vulkan-gst-video")]
        {
            return native_vulkan_audio_runtime_state(&self.state)
                .last_error
                .clone();
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
    #[cfg(not(feature = "native-vulkan-gst-video"))]
    fn muted_plan_does_not_start_audio_runtime() {
        let runtime = NativeVulkanPlanAudioRuntime::start_for_render_item(&video_item(true));

        assert_eq!(runtime.telemetry(), None);
        assert_eq!(runtime.last_error(), None);
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    #[test]
    fn muted_missing_source_records_clock_only_audio_runtime_error() {
        let runtime = NativeVulkanPlanAudioRuntime::start_for_render_item(&video_item(true));

        assert_eq!(runtime.telemetry(), None);
        assert!(
            runtime
                .last_error()
                .expect("clock-only audio runtime error")
                .contains("audio clock runtime source does not exist")
        );
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

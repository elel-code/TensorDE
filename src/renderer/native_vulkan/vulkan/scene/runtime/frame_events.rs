//! Per-frame publication of platform and media providers into one event snapshot.

use crate::engine::scene::{SceneEvent, SceneEventQueue, SceneFrameEvents, SceneStorage};
use crate::renderer::native_vulkan::audio::event_source::{
    NativeVulkanAudioEventSource, audio_state_summary,
};
use crate::renderer::native_vulkan::audio::system_monitor::NativeVulkanSystemAudioMonitor;
use crate::renderer::native_wayland::NativeWaylandHost;

use super::material_uniform::scene_uses_audio_spectrum;

pub(super) struct SceneRuntimeEventSources {
    audio_monitor: NativeVulkanSystemAudioMonitor,
    audio: NativeVulkanAudioEventSource,
    queue: SceneEventQueue,
    frame: SceneFrameEvents,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneRuntimeAudioSummary {
    pub model: &'static str,
    pub ready: bool,
    pub peak: f32,
    pub active_band_count: u32,
}

impl SceneRuntimeEventSources {
    pub(super) fn new(storage: &SceneStorage) -> Self {
        Self {
            audio_monitor: NativeVulkanSystemAudioMonitor::start_if_needed(
                scene_uses_audio_spectrum(storage),
            ),
            audio: NativeVulkanAudioEventSource,
            queue: SceneEventQueue::default(),
            frame: SceneFrameEvents::default(),
        }
    }

    pub(super) fn pump_platform(&mut self, host: &mut NativeWaylandHost) -> Result<bool, String> {
        host.pump_events().map_err(|err| err.to_string())?;
        host.publish_scene_events(&mut self.queue);
        self.audio_monitor.publish_latest();
        Ok(!host.is_closed())
    }

    pub(super) fn capture_frame(&mut self, sample_time_ns: u64) -> &SceneFrameEvents {
        self.queue
            .publish(SceneEvent::Audio(self.audio.capture(sample_time_ns)));
        self.frame = self.queue.finish_frame();
        &self.frame
    }

    pub(super) fn audio_summary(&mut self, frame_presented: bool) -> SceneRuntimeAudioSummary {
        if !frame_presented {
            self.audio_monitor.publish_latest();
            self.capture_frame(0);
        }
        let (model, ready) = self.audio.status();
        let (peak, active_band_count) = audio_state_summary(&self.frame.audio);
        SceneRuntimeAudioSummary {
            model,
            ready,
            peak,
            active_band_count,
        }
    }
}

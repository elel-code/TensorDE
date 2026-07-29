//! Per-frame publication of platform and media providers into one event snapshot.

mod local_time_source;

use crate::engine::scene::{
    SceneEventQueue, SceneFrameEvents, ScenePointerEvent, ScenePointerEventKind,
    ScenePointerSource, SceneStorage,
};
use crate::renderer::native_vulkan::audio::event_source::{
    NativeVulkanAudioEventSource, audio_state_summary,
};
use crate::renderer::native_vulkan::audio::system_monitor::NativeVulkanSystemAudioMonitor;
use crate::renderer::native_wayland::NativeWaylandHost;

use local_time_source::{SceneLocalTimeEventSource, SceneLocalTimePrecision};

pub(super) struct SceneRuntimeEventSources {
    audio_monitor: NativeVulkanSystemAudioMonitor,
    audio: NativeVulkanAudioEventSource,
    local_time: SceneLocalTimeEventSource,
    queue: SceneEventQueue,
    frame: SceneFrameEvents,
    pointer_replay_normalized: Option<[f64; 2]>,
    pointer_replay_entered: bool,
    pointer_replay_fallback_size: [u32; 2],
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneRuntimeAudioSummary {
    pub model: &'static str,
    pub ready: bool,
    pub peak: f32,
    pub active_band_count: u32,
}

impl SceneRuntimeEventSources {
    pub(super) fn new(
        storage: &SceneStorage,
        pointer_replay_normalized: Option<[f64; 2]>,
        audio_spectrum_required: bool,
    ) -> Self {
        Self {
            audio_monitor: NativeVulkanSystemAudioMonitor::start_if_needed(
                audio_spectrum_required,
            ),
            audio: NativeVulkanAudioEventSource::default(),
            local_time: SceneLocalTimeEventSource::new(local_time_precision(storage)),
            queue: SceneEventQueue::default(),
            frame: SceneFrameEvents::default(),
            pointer_replay_normalized,
            pointer_replay_entered: false,
            pointer_replay_fallback_size: [
                storage.project().logical_width.max(1),
                storage.project().logical_height.max(1),
            ],
        }
    }

    pub(super) fn pump_platform(&mut self, host: &mut NativeWaylandHost) -> Result<bool, String> {
        host.pump_events().map_err(|err| err.to_string())?;
        if self.pointer_replay_normalized.is_some() {
            host.discard_scene_events();
        } else {
            host.publish_scene_events(&mut self.queue);
        }
        self.audio_monitor.publish_latest();
        Ok(!host.is_closed())
    }

    pub(super) fn sample_frame_events(
        &mut self,
        sample_time_ns: u64,
        surface_size: Option<(u32, u32)>,
    ) -> &SceneFrameEvents {
        self.publish_pointer_replay(sample_time_ns, surface_size);
        self.queue.publish_audio(self.audio.capture(sample_time_ns));
        self.frame = self.queue.finish_frame();
        self.frame.local_time = self.local_time.capture();
        &self.frame
    }

    fn publish_pointer_replay(&mut self, sample_time_ns: u64, surface_size: Option<(u32, u32)>) {
        let Some(normalized) = self.pointer_replay_normalized else {
            return;
        };
        let surface_size = surface_size
            .map(|(width, height)| [width, height])
            .unwrap_or(self.pointer_replay_fallback_size);
        self.queue.publish_pointer(ScenePointerEvent {
            source: ScenePointerSource::Replay,
            surface_id: 0,
            time_millis: (sample_time_ns / 1_000_000).min(u64::from(u32::MAX)) as u32,
            position: [
                normalized[0] * f64::from(surface_size[0]),
                normalized[1] * f64::from(surface_size[1]),
            ],
            surface_size,
            kind: if self.pointer_replay_entered {
                ScenePointerEventKind::Motion
            } else {
                ScenePointerEventKind::Enter { serial: 0 }
            },
        });
        self.pointer_replay_entered = true;
    }

    pub(super) fn audio_summary(&mut self, frame_presented: bool) -> SceneRuntimeAudioSummary {
        if !frame_presented {
            self.audio_monitor.publish_latest();
            self.sample_frame_events(0, None);
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

fn local_time_precision(storage: &SceneStorage) -> Option<SceneLocalTimePrecision> {
    use crate::engine::scene::SceneScriptSubscriptions;

    let subscriptions = storage
        .script_programs()
        .iter()
        .fold(SceneScriptSubscriptions::NONE, |subscriptions, program| {
            subscriptions.union(program.subscriptions)
        });
    if subscriptions.intersects(SceneScriptSubscriptions::LOCAL_TIME_SECOND) {
        Some(SceneLocalTimePrecision::Second)
    } else if subscriptions.intersects(SceneScriptSubscriptions::LOCAL_TIME) {
        Some(SceneLocalTimePrecision::Minute)
    } else {
        None
    }
}

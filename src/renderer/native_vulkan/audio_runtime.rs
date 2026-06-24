use super::NativeVulkanRenderItem;
use super::video_runtime::NativeVulkanVideoAudioRuntimeTelemetry;
#[cfg(feature = "native-vulkan-gst-video")]
use super::{NativeVulkanAudioOutputMode, NativeVulkanAudioOutputPolicy};
#[cfg(feature = "native-vulkan-gst-video")]
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
#[cfg(feature = "native-vulkan-gst-video")]
use std::thread::{self, JoinHandle};

#[derive(Default)]
pub(super) struct NativeVulkanPlanAudioRuntime {
    #[cfg(feature = "native-vulkan-gst-video")]
    worker: Option<NativeVulkanPlanAudioRuntimeWorker>,
    #[cfg(feature = "native-vulkan-gst-video")]
    state: NativeVulkanPlanAudioRuntimeSharedState,
}

#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanPlanAudioRuntimeSharedState = Arc<Mutex<NativeVulkanPlanAudioRuntimeWorkerState>>;

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Default)]
struct NativeVulkanPlanAudioRuntimeWorkerState {
    telemetry: Option<NativeVulkanVideoAudioRuntimeTelemetry>,
    last_error: Option<String>,
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanPlanAudioRuntimeWorker {
    command_tx: mpsc::Sender<NativeVulkanPlanAudioRuntimeWorkerCommand>,
    join_handle: Option<JoinHandle<()>>,
}

#[cfg(feature = "native-vulkan-gst-video")]
enum NativeVulkanPlanAudioRuntimeWorkerCommand {
    SampleVideoClock { video_clock_ns: u64 },
    SeekForVideoLoop { position_ms: u64 },
    Stop,
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
            };
        };
        let output_mode = NativeVulkanAudioOutputPolicy::Plan.resolve(*muted);
        if output_mode != NativeVulkanAudioOutputMode::Auto {
            return Self {
                worker: None,
                state,
            };
        }
        match super::audio_frontend::NativeVulkanAudioClockRuntimeFrontend::start(
            source,
            output_mode,
        ) {
            Ok(probe) => {
                let worker = NativeVulkanPlanAudioRuntimeWorker::start(probe, Arc::clone(&state));
                Self {
                    worker: Some(worker),
                    state,
                }
            }
            Err(err) => Self {
                worker: None,
                state: native_vulkan_audio_runtime_state_with_error(state, err.to_string()),
            },
        }
    }

    pub(super) fn poll_video_clock(&mut self, video_clock_ns: u64) {
        #[cfg(feature = "native-vulkan-gst-video")]
        if let Some(worker) = self.worker.as_ref() {
            if let Err(err) = worker.send_video_clock(video_clock_ns) {
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
        if let Some(worker) = self.worker.as_ref() {
            if let Err(err) = worker.seek_for_video_loop(position_ms) {
                native_vulkan_audio_runtime_state(&self.state).last_error = Some(err);
                self.worker = None;
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

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanPlanAudioRuntimeWorker {
    fn start(
        probe: super::audio_frontend::NativeVulkanAudioClockRuntimeFrontend,
        state: NativeVulkanPlanAudioRuntimeSharedState,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let join_handle = thread::Builder::new()
            .name("gilder-vulkan-audio-runtime".to_owned())
            .spawn(move || native_vulkan_audio_runtime_worker_loop(probe, command_rx, state))
            .expect("spawn native Vulkan audio runtime worker");
        Self {
            command_tx,
            join_handle: Some(join_handle),
        }
    }

    fn send_video_clock(&self, video_clock_ns: u64) -> Result<(), String> {
        self.command_tx
            .send(NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { video_clock_ns })
            .map_err(|err| format!("audio runtime worker command channel closed: {err}"))
    }

    fn seek_for_video_loop(&self, position_ms: u64) -> Result<(), String> {
        self.command_tx
            .send(NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms })
            .map_err(|err| format!("audio runtime worker command channel closed: {err}"))
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
impl Drop for NativeVulkanPlanAudioRuntimeWorker {
    fn drop(&mut self) {
        let _ = self
            .command_tx
            .send(NativeVulkanPlanAudioRuntimeWorkerCommand::Stop);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_audio_runtime_worker_loop(
    mut probe: super::audio_frontend::NativeVulkanAudioClockRuntimeFrontend,
    command_rx: mpsc::Receiver<NativeVulkanPlanAudioRuntimeWorkerCommand>,
    state: NativeVulkanPlanAudioRuntimeSharedState,
) {
    while let Ok(command) = command_rx.recv() {
        match native_vulkan_audio_runtime_coalesced_command(command, &command_rx) {
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { video_clock_ns } => {
                match probe.sample_video_pts_ms(None, Some(video_clock_ns)) {
                    Ok(()) => {
                        let audio_provider = probe.provider().as_str();
                        native_vulkan_audio_runtime_state(&state).telemetry = Some(
                            NativeVulkanVideoAudioRuntimeTelemetry::from_audio_clock_runtime(
                                audio_provider,
                                probe.telemetry(),
                            ),
                        );
                    }
                    Err(err) => {
                        native_vulkan_audio_runtime_state(&state).last_error =
                            Some(err.to_string());
                        break;
                    }
                }
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms } => {
                match probe.seek_for_video_loop(position_ms) {
                    Ok(()) => {
                        let audio_provider = probe.provider().as_str();
                        native_vulkan_audio_runtime_state(&state).telemetry = Some(
                            NativeVulkanVideoAudioRuntimeTelemetry::from_audio_clock_runtime(
                                audio_provider,
                                probe.telemetry(),
                            ),
                        );
                    }
                    Err(err) => {
                        native_vulkan_audio_runtime_state(&state).last_error =
                            Some(err.to_string());
                        break;
                    }
                }
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::Stop => break,
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_audio_runtime_coalesced_command(
    mut command: NativeVulkanPlanAudioRuntimeWorkerCommand,
    command_rx: &mpsc::Receiver<NativeVulkanPlanAudioRuntimeWorkerCommand>,
) -> NativeVulkanPlanAudioRuntimeWorkerCommand {
    let mut latest_seek_position_ms = match command {
        NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms } => {
            Some(position_ms)
        }
        _ => None,
    };
    while let Ok(next_command) = command_rx.try_recv() {
        match next_command {
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { .. } => {
                if latest_seek_position_ms.is_none() {
                    command = next_command;
                }
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms } => {
                latest_seek_position_ms = Some(position_ms);
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::Stop => {
                return NativeVulkanPlanAudioRuntimeWorkerCommand::Stop;
            }
        }
    }
    latest_seek_position_ms
        .map(
            |position_ms| NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                position_ms,
            },
        )
        .unwrap_or(command)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_audio_runtime_state(
    state: &NativeVulkanPlanAudioRuntimeSharedState,
) -> MutexGuard<'_, NativeVulkanPlanAudioRuntimeWorkerState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_audio_runtime_state_with_error(
    state: NativeVulkanPlanAudioRuntimeSharedState,
    error: String,
) -> NativeVulkanPlanAudioRuntimeSharedState {
    native_vulkan_audio_runtime_state(&state).last_error = Some(error);
    state
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

    #[cfg(feature = "native-vulkan-gst-video")]
    #[test]
    fn worker_command_coalescing_keeps_latest_video_clock() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { video_clock_ns: 2 })
            .unwrap();
        tx.send(NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { video_clock_ns: 3 })
            .unwrap();

        let command = native_vulkan_audio_runtime_coalesced_command(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { video_clock_ns: 1 },
            &rx,
        );

        match command {
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { video_clock_ns } => {
                assert_eq!(video_clock_ns, 3);
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { .. } => {
                panic!("unexpected loop seek")
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::Stop => panic!("unexpected stop"),
        }
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    #[test]
    fn worker_command_coalescing_keeps_latest_loop_seek() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { video_clock_ns: 2 })
            .unwrap();
        tx.send(NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms: 125 })
            .unwrap();
        tx.send(NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms: 250 })
            .unwrap();

        let command = native_vulkan_audio_runtime_coalesced_command(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { video_clock_ns: 1 },
            &rx,
        );

        match command {
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms } => {
                assert_eq!(position_ms, 250);
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { .. } => {
                panic!("unexpected clock sample")
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::Stop => panic!("unexpected stop"),
        }
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    #[test]
    fn worker_command_coalescing_keeps_loop_seek_over_later_clock_sample() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { video_clock_ns: 2 })
            .unwrap();

        let command = native_vulkan_audio_runtime_coalesced_command(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms: 125 },
            &rx,
        );

        match command {
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms } => {
                assert_eq!(position_ms, 125);
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { .. } => {
                panic!("clock sample must not override loop seek")
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::Stop => panic!("unexpected stop"),
        }
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    #[test]
    fn worker_command_coalescing_prioritizes_stop() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms: 125 })
            .unwrap();
        tx.send(NativeVulkanPlanAudioRuntimeWorkerCommand::Stop)
            .unwrap();

        let command = native_vulkan_audio_runtime_coalesced_command(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { video_clock_ns: 1 },
            &rx,
        );

        assert!(matches!(
            command,
            NativeVulkanPlanAudioRuntimeWorkerCommand::Stop
        ));
    }
}

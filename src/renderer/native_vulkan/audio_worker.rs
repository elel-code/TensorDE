use super::audio_frontend::NativeVulkanAudioClockRuntimeFrontend;
use super::video_runtime::NativeVulkanVideoAudioRuntimeTelemetry;
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::thread::{self, JoinHandle};

pub(super) type NativeVulkanPlanAudioRuntimeSharedState =
    Arc<Mutex<NativeVulkanPlanAudioRuntimeWorkerState>>;

#[derive(Default)]
pub(super) struct NativeVulkanPlanAudioRuntimeWorkerState {
    pub(super) telemetry: Option<NativeVulkanVideoAudioRuntimeTelemetry>,
    pub(super) last_error: Option<String>,
}

pub(super) struct NativeVulkanPlanAudioRuntimeWorker {
    command_tx: mpsc::Sender<NativeVulkanPlanAudioRuntimeWorkerCommand>,
    join_handle: Option<JoinHandle<()>>,
}

enum NativeVulkanPlanAudioRuntimeWorkerCommand {
    SampleVideoClock { video_clock_ns: u64 },
    SeekForVideoLoop { position_ms: u64 },
    Stop,
}

impl NativeVulkanPlanAudioRuntimeWorker {
    pub(super) fn start(
        probe: NativeVulkanAudioClockRuntimeFrontend,
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

    pub(super) fn send_video_clock(&self, video_clock_ns: u64) -> Result<(), String> {
        self.command_tx
            .send(NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { video_clock_ns })
            .map_err(|err| format!("audio runtime worker command channel closed: {err}"))
    }

    pub(super) fn seek_for_video_loop(&self, position_ms: u64) -> Result<(), String> {
        self.command_tx
            .send(NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms })
            .map_err(|err| format!("audio runtime worker command channel closed: {err}"))
    }
}

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

pub(super) fn native_vulkan_audio_runtime_state(
    state: &NativeVulkanPlanAudioRuntimeSharedState,
) -> MutexGuard<'_, NativeVulkanPlanAudioRuntimeWorkerState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub(super) fn native_vulkan_audio_runtime_state_with_error(
    state: NativeVulkanPlanAudioRuntimeSharedState,
    error: String,
) -> NativeVulkanPlanAudioRuntimeSharedState {
    native_vulkan_audio_runtime_state(&state).last_error = Some(error);
    state
}

fn native_vulkan_audio_runtime_worker_loop(
    mut probe: NativeVulkanAudioClockRuntimeFrontend,
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

#[cfg(test)]
mod tests {
    use super::*;

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

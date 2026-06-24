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
    SampleVideoClock { video_clock_ns: u64, serial: u32 },
    SeekForVideoLoop { position_ms: u64, serial: u32 },
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

    pub(super) fn send_video_clock(&self, video_clock_ns: u64, serial: u32) -> Result<(), String> {
        self.command_tx
            .send(
                NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                    video_clock_ns,
                    serial,
                },
            )
            .map_err(|err| format!("audio runtime worker command channel closed: {err}"))
    }

    pub(super) fn seek_for_video_loop(&self, position_ms: u64, serial: u32) -> Result<(), String> {
        self.command_tx
            .send(
                NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                    position_ms,
                    serial,
                },
            )
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
    let mut active_serial = 0u32;
    while let Ok(command) = command_rx.recv() {
        let Some(command) = native_vulkan_audio_runtime_command_for_active_serial(
            &mut active_serial,
            native_vulkan_audio_runtime_coalesced_command(command, &command_rx),
        ) else {
            continue;
        };
        match command {
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                video_clock_ns, ..
            } => match probe.sample_video_pts_ms(None, Some(video_clock_ns)) {
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
                    native_vulkan_audio_runtime_state(&state).last_error = Some(err.to_string());
                    break;
                }
            },
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { position_ms, .. } => {
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

fn native_vulkan_audio_runtime_command_for_active_serial(
    active_serial: &mut u32,
    command: NativeVulkanPlanAudioRuntimeWorkerCommand,
) -> Option<NativeVulkanPlanAudioRuntimeWorkerCommand> {
    match command {
        NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { serial, .. } => {
            (serial == *active_serial).then_some(command)
        }
        NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop { serial, .. } => {
            if serial < *active_serial {
                None
            } else {
                *active_serial = serial;
                Some(command)
            }
        }
        NativeVulkanPlanAudioRuntimeWorkerCommand::Stop => Some(command),
    }
}

fn native_vulkan_audio_runtime_coalesced_command(
    mut command: NativeVulkanPlanAudioRuntimeWorkerCommand,
    command_rx: &mpsc::Receiver<NativeVulkanPlanAudioRuntimeWorkerCommand>,
) -> NativeVulkanPlanAudioRuntimeWorkerCommand {
    let mut latest_seek_position_ms = match command {
        NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
            position_ms,
            serial,
        } => Some((position_ms, serial)),
        _ => None,
    };
    while let Ok(next_command) = command_rx.try_recv() {
        match next_command {
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock { .. } => {
                if latest_seek_position_ms.is_none() {
                    command = next_command;
                }
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                position_ms,
                serial,
            } => {
                latest_seek_position_ms = Some((position_ms, serial));
            }
            NativeVulkanPlanAudioRuntimeWorkerCommand::Stop => {
                return NativeVulkanPlanAudioRuntimeWorkerCommand::Stop;
            }
        }
    }
    latest_seek_position_ms
        .map(
            |(position_ms, serial)| NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                position_ms,
                serial,
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
        tx.send(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                video_clock_ns: 2,
                serial: 0,
            },
        )
        .unwrap();
        tx.send(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                video_clock_ns: 3,
                serial: 0,
            },
        )
        .unwrap();

        let command = native_vulkan_audio_runtime_coalesced_command(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                video_clock_ns: 1,
                serial: 0,
            },
            &rx,
        );

        match command {
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                video_clock_ns, ..
            } => {
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
        tx.send(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                video_clock_ns: 2,
                serial: 0,
            },
        )
        .unwrap();
        tx.send(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                position_ms: 125,
                serial: 1,
            },
        )
        .unwrap();
        tx.send(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                position_ms: 250,
                serial: 2,
            },
        )
        .unwrap();

        let command = native_vulkan_audio_runtime_coalesced_command(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                video_clock_ns: 1,
                serial: 0,
            },
            &rx,
        );

        match command {
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                position_ms,
                serial,
            } => {
                assert_eq!(position_ms, 250);
                assert_eq!(serial, 2);
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
        tx.send(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                video_clock_ns: 2,
                serial: 1,
            },
        )
        .unwrap();

        let command = native_vulkan_audio_runtime_coalesced_command(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                position_ms: 125,
                serial: 1,
            },
            &rx,
        );

        match command {
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                position_ms,
                serial,
            } => {
                assert_eq!(position_ms, 125);
                assert_eq!(serial, 1);
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
        tx.send(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                position_ms: 125,
                serial: 1,
            },
        )
        .unwrap();
        tx.send(NativeVulkanPlanAudioRuntimeWorkerCommand::Stop)
            .unwrap();

        let command = native_vulkan_audio_runtime_coalesced_command(
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                video_clock_ns: 1,
                serial: 0,
            },
            &rx,
        );

        assert!(matches!(
            command,
            NativeVulkanPlanAudioRuntimeWorkerCommand::Stop
        ));
    }

    #[test]
    fn worker_command_serial_rejects_stale_video_clock_sample() {
        let mut active_serial = 1;

        let command = native_vulkan_audio_runtime_command_for_active_serial(
            &mut active_serial,
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                video_clock_ns: 1,
                serial: 0,
            },
        );

        assert!(command.is_none());
        assert_eq!(active_serial, 1);
    }

    #[test]
    fn worker_command_serial_accepts_current_video_clock_sample() {
        let mut active_serial = 1;

        let command = native_vulkan_audio_runtime_command_for_active_serial(
            &mut active_serial,
            NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                video_clock_ns: 1,
                serial: 1,
            },
        );

        assert!(matches!(
            command,
            Some(
                NativeVulkanPlanAudioRuntimeWorkerCommand::SampleVideoClock {
                    video_clock_ns: 1,
                    serial: 1,
                }
            )
        ));
        assert_eq!(active_serial, 1);
    }

    #[test]
    fn worker_command_serial_advances_on_loop_seek() {
        let mut active_serial = 0;

        let command = native_vulkan_audio_runtime_command_for_active_serial(
            &mut active_serial,
            NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                position_ms: 250,
                serial: 1,
            },
        );

        assert!(matches!(
            command,
            Some(
                NativeVulkanPlanAudioRuntimeWorkerCommand::SeekForVideoLoop {
                    position_ms: 250,
                    serial: 1,
                }
            )
        ));
        assert_eq!(active_serial, 1);
    }
}

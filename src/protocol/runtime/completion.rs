//! Compositor-thread Compio completion loop.

use std::{io, os::fd::AsFd, time::Duration};

use compio::runtime::{Runtime, fd::PollFd};
use tensor_runtime::{EventfdCompletion, EventfdWake, LocalCompletionQueue, RuntimeStop};

use super::{ProtocolError, WaylandRuntime};
use crate::protocol::state::RuntimeState;

const MAIN_COMPLETION_CAPACITY: usize = 2;

#[derive(Clone, Copy)]
enum SourceCommand {
    Rearm,
}

enum MainCompletion {
    Worker(io::Result<u64>),
    Legacy(io::Result<()>),
}

impl WaylandRuntime {
    /// Run the product completion loop on the compositor thread.
    ///
    /// The legacy source is only the transitional calloop/XWM aggregate. It is
    /// awaited as one submitted Compio operation and dispatched nonblocking
    /// after its CQE; worker wakes are completed eventfd reads.
    pub fn run_with_completions<C>(
        &mut self,
        wake: &EventfdWake,
        stop: &RuntimeStop,
        mut completion_handler: C,
    ) -> Result<(), ProtocolError>
    where
        C: FnMut(&mut RuntimeState),
    {
        if !self.prepared {
            return Err(ProtocolError::RuntimeNotPrepared);
        }
        let legacy_fd = rustix::io::fcntl_dupfd_cloexec(self.event_loop.as_fd(), 0)
            .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
        let runtime =
            Runtime::new().map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
        runtime.block_on(async {
            let worker_reader = wake
                .completion_reader()
                .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
            let legacy_reader = PollFd::new(legacy_fd)
                .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
            let completions = LocalCompletionQueue::bounded(MAIN_COMPLETION_CAPACITY);
            let worker_commands = LocalCompletionQueue::bounded(1);
            let legacy_commands = LocalCompletionQueue::bounded(1);
            let worker_task = compio::runtime::spawn(wait_for_worker_completions(
                worker_reader,
                completions.clone(),
                worker_commands.clone(),
            ));
            let legacy_task = compio::runtime::spawn(wait_for_legacy_completions(
                legacy_reader,
                completions.clone(),
                legacy_commands.clone(),
            ));

            self.state.on_loop_idle();
            while !stop.is_stopped() {
                let first = completions
                    .recv()
                    .await
                    .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
                let mut worker = None;
                let mut legacy = None;
                record_completion(first, &mut worker, &mut legacy)?;
                while let Some(completion) = completions
                    .try_recv()
                    .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?
                {
                    record_completion(completion, &mut worker, &mut legacy)?;
                }

                if let Some(result) = worker {
                    result.map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
                    completion_handler(&mut self.state);
                    if !stop.is_stopped() {
                        worker_commands
                            .try_send(SourceCommand::Rearm)
                            .map_err(|_| {
                                ProtocolError::MainCompletion(
                                    "worker completion rearm queue is full".to_owned(),
                                )
                            })?;
                    }
                }
                if let Some(result) = legacy {
                    result.map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
                    self.event_loop
                        .dispatch(Some(Duration::ZERO), &mut self.state)
                        .map_err(ProtocolError::Run)?;
                    if !stop.is_stopped() {
                        legacy_commands
                            .try_send(SourceCommand::Rearm)
                            .map_err(|_| {
                                ProtocolError::MainCompletion(
                                    "legacy completion rearm queue is full".to_owned(),
                                )
                            })?;
                    }
                }
                self.state.on_loop_idle();
            }

            drop(worker_task);
            drop(legacy_task);
            Ok(())
        })
    }
}

fn record_completion(
    completion: MainCompletion,
    worker: &mut Option<io::Result<u64>>,
    legacy: &mut Option<io::Result<()>>,
) -> Result<(), ProtocolError> {
    let duplicate = match completion {
        MainCompletion::Worker(result) => worker.replace(result).is_some(),
        MainCompletion::Legacy(result) => legacy.replace(result).is_some(),
    };
    if duplicate {
        return Err(ProtocolError::MainCompletion(
            "completion source published twice without a rearm".to_owned(),
        ));
    }
    Ok(())
}

async fn wait_for_worker_completions(
    mut reader: EventfdCompletion,
    completions: LocalCompletionQueue<MainCompletion>,
    commands: LocalCompletionQueue<SourceCommand>,
) {
    loop {
        let result = reader.completed().await;
        let failed = result.is_err();
        if completions
            .try_send(MainCompletion::Worker(result))
            .is_err()
            || failed
        {
            return;
        }
        if commands.recv().await.is_err() {
            return;
        }
    }
}

async fn wait_for_legacy_completions(
    reader: PollFd<std::os::fd::OwnedFd>,
    completions: LocalCompletionQueue<MainCompletion>,
    commands: LocalCompletionQueue<SourceCommand>,
) {
    loop {
        let result = reader.read_ready().await;
        let failed = result.is_err();
        if completions
            .try_send(MainCompletion::Legacy(result))
            .is_err()
            || failed
        {
            return;
        }
        if commands.recv().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, sync::Arc};

    use tensor_runtime::WakeSink;

    use super::*;
    use crate::{
        layout::{LayoutEngine, LayoutKind},
        scene::SceneAppearance,
    };

    #[test]
    fn worker_eventfd_completion_runs_exactly_one_turn_then_stops() {
        let mut runtime = WaylandRuntime::with_appearance(
            LayoutEngine::new(LayoutKind::Scrolling1D),
            SceneAppearance::default(),
        )
        .unwrap();
        runtime.prepared = true;
        let wake = Arc::new(EventfdWake::new().unwrap());
        let writer = std::thread::spawn({
            let wake = Arc::clone(&wake);
            move || {
                wake.wake();
                wake.wake();
            }
        });
        let stop = RuntimeStop::default();
        let turns = Cell::new(0);

        runtime
            .run_with_completions(&wake, &stop, |_| {
                turns.set(turns.get() + 1);
                stop.stop();
            })
            .unwrap();
        writer.join().unwrap();

        assert_eq!(turns.get(), 1);
        assert!(stop.is_stopped());
    }
}

//! Completion service for Vulkan-exported Linux sync-file fences.
//!
//! A sync-file exposes completion through the kernel poll ABI. Tensor submits
//! one io_uring `PollAdd` per fence and emits one value event when that request
//! completes. There is no periodic timer, retry loop, or fd readiness registry.

use std::{
    io,
    os::fd::OwnedFd,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

use compio::{
    BufResult,
    driver::op::{Interest, PollOnce},
    runtime::Runtime,
};
use tensor_host::ConnectorId;
use tensor_runtime::{
    EventfdWake, EventfdWakeError, TrySendError, WakeSink, WorkerBridge, WorkerRx, WorkerTx,
};
use thiserror::Error;

pub(crate) const MAX_PENDING_GPU_FENCES: usize = 64;

struct GpuFenceWait {
    output: ConnectorId,
    timeline_value: u64,
    fd: OwnedFd,
}

#[derive(Clone, Debug)]
pub(crate) struct GpuFenceSubmitter {
    waits: WorkerTx<GpuFenceWait>,
}

impl GpuFenceSubmitter {
    pub(crate) fn submit(
        &self,
        output: ConnectorId,
        timeline_value: u64,
        fd: OwnedFd,
    ) -> Result<(), GpuFenceSubmitError> {
        self.waits
            .try_send(GpuFenceWait {
                output,
                timeline_value,
                fd,
            })
            .map_err(GpuFenceSubmitError::from)
    }
}

#[derive(Debug)]
pub(crate) enum GpuFenceEvent {
    Signaled {
        output: ConnectorId,
        timeline_value: u64,
    },
    WaitFailed {
        output: ConnectorId,
        timeline_value: u64,
        message: String,
    },
    RuntimeFailed(String),
}

pub(crate) struct GpuFenceRuntime {
    submitter: GpuFenceSubmitter,
    input_wake: Arc<EventfdWake>,
    stopping: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl GpuFenceRuntime {
    pub(crate) fn start(events: WorkerTx<GpuFenceEvent>) -> Result<Self, GpuFenceRuntimeError> {
        let input_wake = Arc::new(EventfdWake::new()?);
        let (waits, pending) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_GPU_FENCES,
            input_wake.clone() as Arc<dyn WakeSink>,
        );
        let submitter = GpuFenceSubmitter { waits };
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_wake = Arc::clone(&input_wake);
        let thread_stopping = Arc::clone(&stopping);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("tensor-gpu-fence-completions".to_owned())
            .spawn(move || {
                let runtime = match Runtime::new() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(GpuFenceRuntimeError::Runtime(error)));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let mut input_completion = match thread_wake.completion_reader() {
                        Ok(completion) => completion,
                        Err(error) => {
                            let _ = ready_tx.send(Err(GpuFenceRuntimeError::AttachInput(error)));
                            return;
                        }
                    };
                    if ready_tx.send(Ok(())).is_err() {
                        return;
                    }
                    loop {
                        if let Err(error) = input_completion.completed().await {
                            let _ =
                                events.try_send(GpuFenceEvent::RuntimeFailed(error.to_string()));
                            return;
                        }
                        if thread_stopping.load(Ordering::Acquire) {
                            return;
                        }
                        spawn_pending_fence_waits(&pending, &events);
                    }
                });
            })
            .map_err(GpuFenceRuntimeError::RuntimeThread)?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                submitter,
                input_wake,
                stopping,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(GpuFenceRuntimeError::RuntimeStartupDisconnected)
            }
        }
    }

    pub(crate) fn submitter(&self) -> GpuFenceSubmitter {
        self.submitter.clone()
    }
}

impl Drop for GpuFenceRuntime {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        self.input_wake.wake();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn spawn_pending_fence_waits(pending: &WorkerRx<GpuFenceWait>, events: &WorkerTx<GpuFenceEvent>) {
    pending.drain(MAX_PENDING_GPU_FENCES, |wait| {
        compio::runtime::spawn(wait_for_fence(wait, events.clone())).detach();
    });
}

async fn wait_for_fence(wait: GpuFenceWait, events: WorkerTx<GpuFenceEvent>) {
    let GpuFenceWait {
        output,
        timeline_value,
        fd,
    } = wait;
    let BufResult(result, _operation) =
        compio::runtime::submit(PollOnce::new(fd, Interest::Readable)).await;
    let event = match result {
        Ok(_) => GpuFenceEvent::Signaled {
            output,
            timeline_value,
        },
        Err(error) => GpuFenceEvent::WaitFailed {
            output,
            timeline_value,
            message: error.to_string(),
        },
    };
    let _ = events.try_send(event);
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(crate) enum GpuFenceSubmitError {
    #[error("GPU fence completion queue is full")]
    Full,
    #[error("GPU fence completion runtime is disconnected")]
    Disconnected,
}

impl From<TrySendError> for GpuFenceSubmitError {
    fn from(error: TrySendError) -> Self {
        match error {
            TrySendError::Full => Self::Full,
            TrySendError::Disconnected => Self::Disconnected,
        }
    }
}

#[derive(Debug, Error)]
pub enum GpuFenceRuntimeError {
    #[error(transparent)]
    InputWake(#[from] EventfdWakeError),
    #[error("failed to spawn GPU fence completion runtime: {0}")]
    RuntimeThread(io::Error),
    #[error("failed to initialize GPU fence Compio runtime: {0}")]
    Runtime(io::Error),
    #[error("failed to attach GPU fence input eventfd to Compio: {0}")]
    AttachInput(io::Error),
    #[error("GPU fence completion runtime stopped during initialization")]
    RuntimeStartupDisconnected,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn submitted_sync_file_wait_emits_only_after_completion() {
        let fence = EventfdWake::new().unwrap();
        let fence_fd = rustix::io::dup(fence.as_fd()).unwrap();
        let (events, received) = WorkerBridge::bounded(4);
        let runtime = GpuFenceRuntime::start(events).unwrap();
        let output = ConnectorId::new(9, 4);

        runtime.submitter().submit(output, 17, fence_fd).unwrap();
        assert!(received.recv_timeout(Duration::from_millis(20)).is_err());
        fence.wake();

        match received.recv_timeout(Duration::from_secs(1)).unwrap() {
            GpuFenceEvent::Signaled {
                output: completed_output,
                timeline_value,
            } => {
                assert_eq!(completed_output, output);
                assert_eq!(timeline_value, 17);
            }
            event => panic!("unexpected GPU fence event: {event:?}"),
        }
    }

    #[test]
    fn runtime_drop_cancels_an_unsignaled_fence_wait() {
        let fence = EventfdWake::new().unwrap();
        let fence_fd = rustix::io::dup(fence.as_fd()).unwrap();
        let (events, _received) = WorkerBridge::bounded(4);
        let runtime = GpuFenceRuntime::start(events).unwrap();

        runtime
            .submitter()
            .submit(ConnectorId::new(3, 8), 21, fence_fd)
            .unwrap();
        drop(runtime);
    }
}

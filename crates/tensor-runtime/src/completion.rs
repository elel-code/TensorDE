//! Completion-to-callback relay for transitional event-loop adapters.
//!
//! Workers signal an eventfd after enqueueing value messages. A Compio runtime
//! submits the corresponding read and invokes a small callback only when that
//! operation completes. The callback may wake a transitional host loop, but it
//! never receives or owns Wayland, input, Vulkan, or DRM objects.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};

use thiserror::Error;

use crate::{EventfdWake, EventfdWakeError, WakeSink, io_uring_runtime};

/// Owns a submitted eventfd completion loop on a dedicated Compio runtime.
pub struct EventfdCompletionRelay {
    wake: Arc<EventfdWake>,
    stopping: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl EventfdCompletionRelay {
    /// Start a completion relay and wait until its first read can be submitted.
    ///
    /// `on_completion` runs on the Compio thread. It must remain bounded and
    /// should only wake the compositor; policy runs after the value bridge is
    /// drained on the compositor thread.
    pub fn start(
        name: impl Into<String>,
        mut on_completion: impl FnMut(u64) + Send + 'static,
    ) -> Result<Self, CompletionRelayError> {
        let wake = Arc::new(EventfdWake::new()?);
        let thread_wake = Arc::clone(&wake);
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_stopping = Arc::clone(&stopping);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let runtime = match io_uring_runtime(1) {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(CompletionRelayError::Runtime(error)));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let mut completion = match thread_wake.completion_reader() {
                        Ok(completion) => completion,
                        Err(error) => {
                            let _ = ready_tx.send(Err(CompletionRelayError::Attach(error)));
                            return;
                        }
                    };
                    if ready_tx.send(Ok(())).is_err() {
                        return;
                    }
                    loop {
                        let Ok(count) = completion.completed().await else {
                            break;
                        };
                        if thread_stopping.load(Ordering::Acquire) {
                            break;
                        }
                        on_completion(count);
                    }
                });
            })
            .map_err(CompletionRelayError::Spawn)?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                wake,
                stopping,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(CompletionRelayError::StartupDisconnected)
            }
        }
    }

    /// Wake handle cloned onto value-only worker senders.
    pub fn wake(&self) -> Arc<dyn WakeSink> {
        self.wake.clone()
    }
}

impl Drop for EventfdCompletionRelay {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        self.wake.wake();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Debug, Error)]
pub enum CompletionRelayError {
    #[error(transparent)]
    Eventfd(#[from] EventfdWakeError),
    #[error("failed to spawn the completion relay: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("failed to initialize the Compio runtime: {0}")]
    Runtime(#[source] std::io::Error),
    #[error("failed to attach eventfd to the Compio runtime: {0}")]
    Attach(#[source] std::io::Error),
    #[error("completion relay stopped during initialization")]
    StartupDisconnected,
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::Duration,
    };

    use super::*;
    use crate::WorkerBridge;

    #[test]
    fn successful_bridge_send_reaches_callback_after_completion() {
        let completed = Arc::new(AtomicU64::new(0));
        let callback_count = Arc::clone(&completed);
        let relay = EventfdCompletionRelay::start("tensor-completion-test", move |count| {
            callback_count.fetch_add(count, Ordering::Release);
        })
        .expect("completion relay");
        let (tx, rx) = WorkerBridge::bounded_with_wake(4, relay.wake());

        tx.try_send(17u32).expect("enqueue");
        for _ in 0..100 {
            if completed.load(Ordering::Acquire) > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        assert_eq!(completed.load(Ordering::Acquire), 1);
        assert_eq!(rx.try_recv(), Some(17));
    }
}

//! One-operation completion adapter for opaque, thread-affine fd sources.
//!
//! Some transitional libraries expose an aggregate or notifier fd but require
//! the owning thread to consume the underlying object. This adapter duplicates
//! only that fd and submits one Compio `PollOnce`. On Linux/io_uring it is one
//! `IORING_OP_POLL_ADD`; a CQE publishes one value-only completion. The owner
//! must explicitly rearm after consuming the source, so this never becomes a
//! readiness registry or a busy level-triggered loop.

use std::{
    io,
    os::fd::{AsFd, OwnedFd},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};

use compio::runtime::{Runtime, fd::PollFd};
use futures_util::future::{Either, select};
use rustix::io::fcntl_dupfd_cloexec;
use thiserror::Error;

use crate::{
    EventfdCompletion, EventfdWake, EventfdWakeError, TrySendError, WakeSink, WorkerBridge,
    WorkerRx, WorkerTx,
};

#[derive(Clone, Copy)]
enum CompletionCommand {
    Rearm,
    Finish,
}

/// One completed opaque-fd operation and its single-use disposition command.
pub struct OpaqueFdCompletion {
    commands: WorkerTx<CompletionCommand>,
}

impl OpaqueFdCompletion {
    /// Submit the next one-shot operation after the owner has drained its source.
    pub fn rearm(self) -> Result<(), TrySendError> {
        self.commands.try_send(CompletionCommand::Rearm)
    }

    /// Finish the completion service after a one-time source has been consumed.
    pub fn finish(self) -> Result<(), TrySendError> {
        self.commands.try_send(CompletionCommand::Finish)
    }
}

/// Owns one Compio thread and at most one submitted opaque-fd operation.
pub struct OpaqueFdCompletionRuntime {
    wake: Arc<EventfdWake>,
    stopping: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl OpaqueFdCompletionRuntime {
    /// Duplicate `source`, initialize Compio, and submit the first operation.
    pub fn start(
        name: impl Into<String>,
        source: impl AsFd,
        completions: WorkerTx<OpaqueFdCompletion>,
        failures: WorkerTx<String>,
    ) -> Result<Self, OpaqueFdCompletionError> {
        let poll_fd = fcntl_dupfd_cloexec(source.as_fd(), 0)
            .map_err(|error| OpaqueFdCompletionError::Duplicate(io::Error::from(error)))?;
        let wake = Arc::new(EventfdWake::new()?);
        let (commands, pending_commands) =
            WorkerBridge::bounded_with_wake(1, Arc::clone(&wake) as Arc<dyn WakeSink>);
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_wake = Arc::clone(&wake);
        let thread_stopping = Arc::clone(&stopping);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let runtime = match Runtime::new() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(OpaqueFdCompletionError::Runtime(error)));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let poll_fd = match PollFd::new(poll_fd) {
                        Ok(poll_fd) => poll_fd,
                        Err(error) => {
                            let _ =
                                ready_tx.send(Err(OpaqueFdCompletionError::AttachSource(error)));
                            return;
                        }
                    };
                    let mut command_completion = match thread_wake.completion_reader() {
                        Ok(completion) => completion,
                        Err(error) => {
                            let _ =
                                ready_tx.send(Err(OpaqueFdCompletionError::AttachCommand(error)));
                            return;
                        }
                    };
                    if ready_tx.send(Ok(())).is_err() {
                        return;
                    }
                    run_completion_loop(
                        &poll_fd,
                        &mut command_completion,
                        pending_commands,
                        commands,
                        completions,
                        failures,
                        thread_stopping,
                    )
                    .await;
                });
            })
            .map_err(OpaqueFdCompletionError::Spawn)?;

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
                Err(OpaqueFdCompletionError::StartupDisconnected)
            }
        }
    }
}

impl Drop for OpaqueFdCompletionRuntime {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        self.wake.wake();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

async fn run_completion_loop(
    poll_fd: &PollFd<OwnedFd>,
    command_completion: &mut EventfdCompletion,
    pending_commands: WorkerRx<CompletionCommand>,
    commands: WorkerTx<CompletionCommand>,
    completions: WorkerTx<OpaqueFdCompletion>,
    failures: WorkerTx<String>,
    stopping: Arc<AtomicBool>,
) {
    loop {
        let source_wait = Box::pin(poll_fd.read_ready());
        let command_wait = Box::pin(command_completion.completed());
        match select(source_wait, command_wait).await {
            Either::Right((result, _source_wait)) => {
                if let Err(error) = result {
                    emit_failure_unless_stopping(&failures, &stopping, error);
                    return;
                }
                if stopping.load(Ordering::Acquire) {
                    return;
                }
            }
            Either::Left((result, command_wait)) => {
                if let Err(error) = result {
                    emit_failure_unless_stopping(&failures, &stopping, error);
                    return;
                }
                if stopping.load(Ordering::Acquire) {
                    return;
                }
                if completions
                    .try_send(OpaqueFdCompletion {
                        commands: commands.clone(),
                    })
                    .is_err()
                {
                    emit_failure(&failures, "opaque fd completion bridge is unavailable");
                    return;
                }

                // Preserve this submitted read until its CQE. Cancelling and replacing it can
                // let the abandoned operation consume a stop write, leaving the replacement
                // pending forever.
                if let Err(error) = command_wait.await {
                    emit_failure_unless_stopping(&failures, &stopping, error);
                    return;
                }
                if stopping.load(Ordering::Acquire) {
                    return;
                }
                match pending_commands.try_recv() {
                    Some(CompletionCommand::Rearm) => {}
                    Some(CompletionCommand::Finish) => return,
                    None => {
                        emit_failure(&failures, "opaque fd completion command was not published");
                        return;
                    }
                }
            }
        }
    }
}

fn emit_failure_unless_stopping(
    failures: &WorkerTx<String>,
    stopping: &AtomicBool,
    error: impl std::fmt::Display,
) {
    if !stopping.load(Ordering::Acquire) {
        emit_failure(failures, error);
    }
}

fn emit_failure(failures: &WorkerTx<String>, error: impl std::fmt::Display) {
    let _ = failures.try_send(error.to_string());
}

#[derive(Debug, Error)]
pub enum OpaqueFdCompletionError {
    #[error(transparent)]
    Wake(#[from] EventfdWakeError),
    #[error("failed to duplicate the opaque source fd: {0}")]
    Duplicate(#[source] io::Error),
    #[error("failed to spawn the opaque fd completion thread: {0}")]
    Spawn(#[source] io::Error),
    #[error("failed to initialize the opaque fd Compio runtime: {0}")]
    Runtime(#[source] io::Error),
    #[error("failed to attach the opaque source fd to Compio: {0}")]
    AttachSource(#[source] io::Error),
    #[error("failed to attach the opaque fd command completion: {0}")]
    AttachCommand(#[source] io::Error),
    #[error("opaque fd completion runtime stopped during initialization")]
    StartupDisconnected,
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
        time::{Duration, Instant},
    };

    use super::*;

    #[test]
    fn cqe_is_published_once_and_requires_explicit_rearm() {
        let (mut source, mut writer) = UnixStream::pair().unwrap();
        let (completion_tx, completions) = WorkerBridge::bounded(1);
        let (failure_tx, failures) = WorkerBridge::bounded(1);
        let runtime = OpaqueFdCompletionRuntime::start(
            "tensor-opaque-fd-test",
            &source,
            completion_tx,
            failure_tx,
        )
        .unwrap();

        assert!(completions.recv_timeout(Duration::from_millis(30)).is_err());
        writer.write_all(&[1]).unwrap();
        let completion = completions.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(completions.recv_timeout(Duration::from_millis(30)).is_err());

        let mut byte = [0];
        source.read_exact(&mut byte).unwrap();
        completion.rearm().unwrap();
        writer.write_all(&[2]).unwrap();
        let completion = completions.recv_timeout(Duration::from_secs(2)).unwrap();
        completion.finish().unwrap();

        assert!(failures.try_recv().is_none());
        drop(runtime);
    }

    #[test]
    fn shutdown_preserves_the_command_read_after_source_completion() {
        let (source, mut writer) = UnixStream::pair().unwrap();
        let (completion_tx, completions) = WorkerBridge::bounded(1);
        let (failure_tx, failures) = WorkerBridge::bounded(1);
        let runtime = OpaqueFdCompletionRuntime::start(
            "tensor-opaque-fd-shutdown-test",
            &source,
            completion_tx,
            failure_tx,
        )
        .unwrap();

        writer.write_all(&[1]).unwrap();
        let completion = completions.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(completion);
        let started = Instant::now();
        drop(runtime);

        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(failures.try_recv().is_none());
    }
}

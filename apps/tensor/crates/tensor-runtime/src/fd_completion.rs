//! One-operation completion adapter for opaque, thread-affine fd sources.
//!
//! Some thread-affine native libraries expose an aggregate or notifier fd but require
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

use compio::runtime::{CancelToken, FutureExt as _, fd::PollFd};
use futures_util::{
    future::{Either, select},
    pin_mut,
};
use rustix::io::fcntl_dupfd_cloexec;
use thiserror::Error;

use crate::{
    EventfdCompletion, EventfdWake, EventfdWakeError, TrySendError, WakeSink, WorkerBridge,
    WorkerRx, WorkerTx, io_uring_runtime,
};

#[derive(Clone, Copy)]
enum CompletionCommand {
    Rearm,
    Refresh,
    Finish,
}

struct CompletionLoopState {
    pending_commands: WorkerRx<CompletionCommand>,
    commands: WorkerTx<CompletionCommand>,
    refresh_pending: Arc<AtomicBool>,
    completions: WorkerTx<OpaqueFdCompletion>,
    failures: WorkerTx<String>,
    stopping: Arc<AtomicBool>,
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
    commands: WorkerTx<CompletionCommand>,
    refresh_pending: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

/// Cloneable command handle that cancels and resubmits the current opaque-fd
/// operation after the source's internal membership changes.
#[derive(Clone)]
pub struct OpaqueFdRefresh {
    commands: WorkerTx<CompletionCommand>,
    pending: Arc<AtomicBool>,
}

impl OpaqueFdRefresh {
    pub fn refresh(&self) -> Result<(), TrySendError> {
        if self.pending.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        if let Err(error) = self.commands.try_send(CompletionCommand::Refresh) {
            self.pending.store(false, Ordering::Release);
            return Err(error);
        }
        Ok(())
    }
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
        // One disposition plus one concurrent membership refresh. Both are
        // fixed-capacity and coalesced by the eventfd completion.
        let (commands, pending_commands) =
            WorkerBridge::bounded_with_wake(2, Arc::clone(&wake) as Arc<dyn WakeSink>);
        let refresh_pending = Arc::new(AtomicBool::new(false));
        let stopping = Arc::new(AtomicBool::new(false));
        let runtime_commands = commands.clone();
        let runtime_refresh_pending = Arc::clone(&refresh_pending);
        let thread_wake = Arc::clone(&wake);
        let thread_stopping = Arc::clone(&stopping);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let runtime = match io_uring_runtime(2) {
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
                        CompletionLoopState {
                            pending_commands,
                            commands,
                            refresh_pending,
                            completions,
                            failures,
                            stopping: thread_stopping,
                        },
                    )
                    .await;
                });
            })
            .map_err(OpaqueFdCompletionError::Spawn)?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                wake,
                commands: runtime_commands,
                refresh_pending: runtime_refresh_pending,
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

    pub fn refresh_handle(&self) -> OpaqueFdRefresh {
        OpaqueFdRefresh {
            commands: self.commands.clone(),
            pending: Arc::clone(&self.refresh_pending),
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
    state: CompletionLoopState,
) {
    let CompletionLoopState {
        pending_commands,
        commands,
        refresh_pending,
        completions,
        failures,
        stopping,
    } = state;
    loop {
        // A replacement operation observes every membership change that
        // happened before submission, so consume already-coalesced refreshes
        // without spending queue capacity on an unnecessary cancel cycle.
        while let Some(command) = pending_commands.try_recv() {
            match command {
                CompletionCommand::Refresh => {}
                CompletionCommand::Rearm | CompletionCommand::Finish => {
                    emit_failure(
                        &failures,
                        "opaque fd disposition arrived without a published completion",
                    );
                    return;
                }
            }
        }
        refresh_pending.store(false, Ordering::Release);
        let source_completed = {
            let source_cancel = CancelToken::new();
            let source_wait = poll_fd.read_ready().with_cancel(source_cancel.clone());
            let command_wait = command_completion.completed();
            pin_mut!(source_wait, command_wait);
            match select(source_wait, command_wait).await {
                Either::Right((result, mut source_wait)) => {
                    if let Err(error) = result {
                        source_cancel.cancel();
                        let _ = source_wait.as_mut().await;
                        emit_failure_unless_stopping(&failures, &stopping, error);
                        return;
                    }
                    if stopping.load(Ordering::Acquire) {
                        source_cancel.cancel();
                        let _ = source_wait.as_mut().await;
                        return;
                    }
                    while let Some(command) = pending_commands.try_recv() {
                        match command {
                            CompletionCommand::Refresh => {}
                            CompletionCommand::Rearm | CompletionCommand::Finish => {
                                emit_failure(
                                    &failures,
                                    "opaque fd disposition arrived without a published completion",
                                );
                                return;
                            }
                        }
                    }
                    // The opaque source changed its internal fd membership.
                    // Cancellation is itself completed before submitting the
                    // replacement operation, so there is always at most one live
                    // io_uring poll operation for this source.
                    source_cancel.cancel();
                    let _ = source_wait.as_mut().await;
                    false
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

                    // Preserve the submitted command read until its CQE. Refresh
                    // may race the owner's disposition; only Rearm/Finish consumes
                    // the published completion, while Refresh is coalesced into
                    // the next submitted source operation.
                    if let Err(error) = command_wait.await {
                        emit_failure_unless_stopping(&failures, &stopping, error);
                        return;
                    }
                    true
                }
            }
        };
        if !source_completed {
            continue;
        }

        loop {
            if stopping.load(Ordering::Acquire) {
                return;
            }
            let mut disposition = None;
            while let Some(command) = pending_commands.try_recv() {
                match command {
                    CompletionCommand::Refresh => {}
                    CompletionCommand::Rearm => disposition = Some(false),
                    CompletionCommand::Finish => disposition = Some(true),
                }
            }
            match disposition {
                Some(true) => return,
                Some(false) => break,
                None => {
                    if let Err(error) = command_completion.completed().await {
                        emit_failure_unless_stopping(&failures, &stopping, error);
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
    fn refreshes_coalesce_across_cancel_completion_and_resubmit() {
        let (mut source, mut writer) = UnixStream::pair().unwrap();
        let (completion_tx, completions) = WorkerBridge::bounded(1);
        let (failure_tx, failures) = WorkerBridge::bounded(1);
        let runtime = OpaqueFdCompletionRuntime::start(
            "tensor-opaque-fd-refresh-test",
            &source,
            completion_tx,
            failure_tx,
        )
        .unwrap();
        let refresh = runtime.refresh_handle();

        for _ in 0..128 {
            refresh.refresh().unwrap();
        }
        writer.write_all(&[1]).unwrap();
        let completion = completions.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(completions.recv_timeout(Duration::from_millis(30)).is_err());

        // Exercise the race in which membership changes after the source CQE
        // is published but before the owner provides its disposition.
        for _ in 0..128 {
            refresh.refresh().unwrap();
        }
        let mut byte = [0];
        source.read_exact(&mut byte).unwrap();
        completion.rearm().unwrap();
        writer.write_all(&[2]).unwrap();
        let completion = completions.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(completions.recv_timeout(Duration::from_millis(30)).is_err());
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

    #[test]
    fn shutdown_completes_cancellation_of_a_pending_source_operation() {
        let (source, _writer) = UnixStream::pair().unwrap();
        let (completion_tx, _completions) = WorkerBridge::bounded(1);
        let (failure_tx, failures) = WorkerBridge::bounded(1);
        let runtime = OpaqueFdCompletionRuntime::start(
            "tensor-opaque-fd-pending-shutdown-test",
            &source,
            completion_tx,
            failure_tx,
        )
        .unwrap();

        let started = Instant::now();
        drop(runtime);

        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(failures.try_recv().is_none());
    }
}

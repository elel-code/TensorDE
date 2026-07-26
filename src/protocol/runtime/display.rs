//! Completion adapter for the `wayland-backend` aggregate fd.
//!
//! The Rust Wayland backend owns an opaque epoll instance containing its client
//! sockets. Tensor does not duplicate that registry. It submits one Compio
//! `PollOnce` operation for the aggregate fd; on Linux/io_uring this is one
//! `IORING_OP_POLL_ADD` whose CQE publishes a compositor-thread dispatch event.
//! Rearming is explicit and happens only after `dispatch_clients` drains the
//! backend, preventing a level-triggered completion loop.

use std::{
    io,
    os::fd::OwnedFd,
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
use tensor_runtime::{
    EventfdCompletion, EventfdWake, EventfdWakeError, TrySendError, WakeSink, WorkerBridge,
    WorkerRx, WorkerTx,
};
use thiserror::Error;
use wayland_server::Display;

use crate::protocol::state::RuntimeState;

pub(crate) const MAX_PENDING_WAYLAND_DISPLAY_EVENTS: usize = 1;
pub(crate) const MAX_PENDING_WAYLAND_DISPLAY_CONTROL_EVENTS: usize = 1;

pub(crate) enum WaylandDisplayEvent {
    Dispatch(WaylandDisplayRearm),
}

pub(crate) enum WaylandDisplayControlEvent {
    RuntimeFailed(String),
}

pub(crate) struct WaylandDisplayRearm {
    commands: WorkerTx<()>,
}

impl WaylandDisplayRearm {
    fn submit(self) -> Result<(), TrySendError> {
        self.commands.try_send(())
    }
}

/// Owns the Compio thread with one submitted display-fd operation at a time.
pub(crate) struct WaylandDisplayRuntime {
    wake: Arc<EventfdWake>,
    stopping: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl WaylandDisplayRuntime {
    pub(crate) fn start<State: 'static>(
        display: &Display<State>,
        events: WorkerTx<WaylandDisplayEvent>,
        control: WorkerTx<WaylandDisplayControlEvent>,
    ) -> Result<Self, WaylandDisplayRuntimeError> {
        let poll_fd = fcntl_dupfd_cloexec(display, 0)
            .map_err(|error| WaylandDisplayRuntimeError::Duplicate(io::Error::from(error)))?;
        let wake = Arc::new(EventfdWake::new()?);
        let (commands, pending_commands) =
            WorkerBridge::bounded_with_wake(1, Arc::clone(&wake) as Arc<dyn WakeSink>);
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_wake = Arc::clone(&wake);
        let thread_stopping = Arc::clone(&stopping);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("tensor-wayland-display-completions".to_owned())
            .spawn(move || {
                let runtime = match Runtime::new() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(WaylandDisplayRuntimeError::Runtime(error)));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let poll_fd = match PollFd::new(poll_fd) {
                        Ok(poll_fd) => poll_fd,
                        Err(error) => {
                            let _ = ready_tx
                                .send(Err(WaylandDisplayRuntimeError::AttachDisplay(error)));
                            return;
                        }
                    };
                    let mut command_completion = match thread_wake.completion_reader() {
                        Ok(completion) => completion,
                        Err(error) => {
                            let _ = ready_tx
                                .send(Err(WaylandDisplayRuntimeError::AttachCommand(error)));
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
                        events,
                        control,
                        thread_stopping,
                    )
                    .await;
                });
            })
            .map_err(WaylandDisplayRuntimeError::Spawn)?;

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
                Err(WaylandDisplayRuntimeError::StartupDisconnected)
            }
        }
    }
}

impl Drop for WaylandDisplayRuntime {
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
    pending_commands: WorkerRx<()>,
    commands: WorkerTx<()>,
    events: WorkerTx<WaylandDisplayEvent>,
    control: WorkerTx<WaylandDisplayControlEvent>,
    stopping: Arc<AtomicBool>,
) {
    loop {
        let display_wait = Box::pin(poll_fd.read_ready());
        let command_wait = Box::pin(command_completion.completed());
        match select(display_wait, command_wait).await {
            Either::Right((result, _display_wait)) => {
                if let Err(error) = result {
                    if !stopping.load(Ordering::Acquire) {
                        emit_failure(&control, error);
                    }
                    return;
                }
                if stopping.load(Ordering::Acquire) {
                    return;
                }
                while pending_commands.try_recv().is_some() {}
            }
            Either::Left((result, command_wait)) => {
                if let Err(error) = result {
                    if !stopping.load(Ordering::Acquire) {
                        emit_failure(&control, error);
                    }
                    return;
                }
                if stopping.load(Ordering::Acquire) {
                    return;
                }
                if events
                    .try_send(WaylandDisplayEvent::Dispatch(WaylandDisplayRearm {
                        commands: commands.clone(),
                    }))
                    .is_err()
                {
                    emit_failure(&control, "Wayland display event bridge is unavailable");
                    return;
                }
                // Keep the already-submitted eventfd read alive. Cancelling it here can race a
                // stop write: the abandoned read may consume the value while its replacement
                // remains pending forever.
                if let Err(error) = command_wait.await {
                    if !stopping.load(Ordering::Acquire) {
                        emit_failure(&control, error);
                    }
                    return;
                }
                if stopping.load(Ordering::Acquire) {
                    return;
                }
                while pending_commands.try_recv().is_some() {}
            }
        }
    }
}

fn emit_failure(control: &WorkerTx<WaylandDisplayControlEvent>, error: impl std::fmt::Display) {
    let _ = control.try_send(WaylandDisplayControlEvent::RuntimeFailed(error.to_string()));
}

pub(crate) fn drain_wayland_display_events(
    events: &WorkerRx<WaylandDisplayEvent>,
    control: &WorkerRx<WaylandDisplayControlEvent>,
    state: &mut RuntimeState,
) -> Result<(), String> {
    while let Some(WaylandDisplayEvent::Dispatch(rearm)) = events.try_recv() {
        state
            .dispatch_wayland_clients()
            .map_err(|error| error.to_string())?;
        state
            .display_handle
            .flush_clients()
            .map_err(|error| error.to_string())?;
        rearm
            .submit()
            .map_err(|error| format!("Wayland display rearm was rejected: {error:?}"))?;
    }
    if let Some(WaylandDisplayControlEvent::RuntimeFailed(message)) = control.try_recv() {
        return Err(message);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub(crate) enum WaylandDisplayRuntimeError {
    #[error(transparent)]
    Wake(#[from] EventfdWakeError),
    #[error("failed to duplicate the Wayland display fd: {0}")]
    Duplicate(#[source] io::Error),
    #[error("failed to spawn the Wayland display completion thread: {0}")]
    Spawn(#[source] io::Error),
    #[error("failed to initialize the Wayland display Compio runtime: {0}")]
    Runtime(#[source] io::Error),
    #[error("failed to attach the Wayland display fd to Compio: {0}")]
    AttachDisplay(#[source] io::Error),
    #[error("failed to attach the Wayland display command completion: {0}")]
    AttachCommand(#[source] io::Error),
    #[error("Wayland display completion runtime stopped during initialization")]
    StartupDisconnected,
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        os::unix::net::UnixStream,
        sync::Arc,
        time::{Duration, Instant},
    };

    use tensor_runtime::WorkerBridge;
    use wayland_server::backend::{ClientData, ClientId, DisconnectReason};

    use super::*;

    #[derive(Debug)]
    struct TestClientData;

    impl ClientData for TestClientData {
        fn initialized(&self, _: ClientId) {}

        fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
    }

    fn sync_request(stream: &mut UnixStream, callback_id: u32) {
        let mut message = [0u8; 12];
        message[0..4].copy_from_slice(&1u32.to_ne_bytes());
        message[4..8].copy_from_slice(&(12u32 << 16).to_ne_bytes());
        message[8..12].copy_from_slice(&callback_id.to_ne_bytes());
        stream.write_all(&message).unwrap();
    }

    #[test]
    fn dispatch_is_published_only_after_poll_completion_and_rearms_after_drain() {
        let mut display = Display::<()>::new().unwrap();
        let (server, mut client) = UnixStream::pair().unwrap();
        display
            .handle()
            .insert_client(server, Arc::new(TestClientData))
            .unwrap();
        let (event_tx, events) = WorkerBridge::bounded(1);
        let (control_tx, control) = WorkerBridge::bounded(1);
        let runtime = WaylandDisplayRuntime::start(&display, event_tx, control_tx).unwrap();

        assert!(events.recv_timeout(Duration::from_millis(30)).is_err());
        sync_request(&mut client, 2);
        let Some(WaylandDisplayEvent::Dispatch(rearm)) =
            events.recv_timeout(Duration::from_secs(2)).ok()
        else {
            panic!("expected a display completion");
        };
        display.dispatch_clients(&mut ()).unwrap();
        display.flush_clients().unwrap();
        rearm.submit().unwrap();

        sync_request(&mut client, 3);
        let WaylandDisplayEvent::Dispatch(pending_rearm) =
            events.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(pending_rearm);
        assert!(control.try_recv().is_none());
        let started = Instant::now();
        drop(runtime);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn shutdown_cancels_a_submitted_display_wait() {
        let display = Display::<()>::new().unwrap();
        let (event_tx, events) = WorkerBridge::bounded(1);
        let (control_tx, control) = WorkerBridge::bounded(1);
        let runtime = WaylandDisplayRuntime::start(&display, event_tx, control_tx).unwrap();

        let started = Instant::now();
        drop(runtime);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(events.try_recv().is_none());
        assert!(control.try_recv().is_none());
    }
}

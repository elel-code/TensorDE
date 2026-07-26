//! Compio-completed accepts for the primary Wayland listening socket.
//!
//! `wayland_server::ListeningSocket` stays on the compositor side so its lock
//! and filesystem cleanup semantics remain intact. A close-on-exec duplicate
//! is attached to Compio; only completed client streams cross the bounded
//! bridge back to the compositor thread.

use std::{
    io,
    os::unix::net::{UnixListener as StdUnixListener, UnixStream as StdUnixStream},
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

use compio::{
    net::{UnixListener, UnixStream},
    runtime::Runtime,
};
use rustix::{
    io::fcntl_dupfd_cloexec,
    net::{Shutdown, shutdown},
};
use tensor_runtime::{EventfdWake, EventfdWakeError, WakeSink, WorkerRx, WorkerTx};
use thiserror::Error;
use tracing::warn;
use wayland_server::ListeningSocket;

use crate::protocol::state::{RuntimeState, WaylandClientState};

pub(crate) const MAX_PENDING_WAYLAND_CLIENTS: usize = 64;
pub(crate) const MAX_PENDING_WAYLAND_SOCKET_CONTROL_EVENTS: usize = 1;

/// Critical listener state has a reserved slot separate from client load.
pub(crate) enum WaylandSocketControlEvent {
    RuntimeFailed(String),
}

/// Owns the Compio thread that keeps one accept operation submitted.
pub(crate) struct WaylandSocketRuntime {
    stop: Arc<EventfdWake>,
    join: Option<JoinHandle<()>>,
}

impl WaylandSocketRuntime {
    pub(crate) fn start(
        listener: &ListeningSocket,
        clients: WorkerTx<StdUnixStream>,
        control: WorkerTx<WaylandSocketControlEvent>,
    ) -> Result<Self, WaylandSocketRuntimeError> {
        let listener = fcntl_dupfd_cloexec(listener, 0)
            .map(StdUnixListener::from)
            .map_err(|error| WaylandSocketRuntimeError::Duplicate(io::Error::from(error)))?;
        let stop = Arc::new(EventfdWake::new()?);
        let thread_stop = Arc::clone(&stop);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("tensor-wayland-accept-completions".to_owned())
            .spawn(move || {
                let runtime = match Runtime::new() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(WaylandSocketRuntimeError::Runtime(error)));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let listener = match UnixListener::from_std(listener) {
                        Ok(listener) => listener,
                        Err(error) => {
                            let _ = ready_tx
                                .send(Err(WaylandSocketRuntimeError::AttachListener(error)));
                            return;
                        }
                    };
                    let mut stop_completion = match thread_stop.completion_reader() {
                        Ok(completion) => completion,
                        Err(error) => {
                            let _ =
                                ready_tx.send(Err(WaylandSocketRuntimeError::AttachStop(error)));
                            return;
                        }
                    };
                    let accepts = compio::runtime::spawn(accept_loop(
                        listener.clone(),
                        clients,
                        control.clone(),
                    ));
                    if ready_tx.send(Ok(())).is_err() {
                        let _ = accepts.cancel().await;
                        let _ = listener.close().await;
                        return;
                    }
                    if let Err(error) = stop_completion.completed().await {
                        let _ = control
                            .try_send(WaylandSocketControlEvent::RuntimeFailed(error.to_string()));
                    }
                    // Cancellation is itself awaited: shutdown never leaves an accept op in flight.
                    let _ = shutdown(&listener, Shutdown::Both);
                    let _ = accepts.cancel().await;
                    let _ = listener.close().await;
                });
            })
            .map_err(WaylandSocketRuntimeError::Spawn)?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                stop,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(WaylandSocketRuntimeError::StartupDisconnected)
            }
        }
    }
}

impl Drop for WaylandSocketRuntime {
    fn drop(&mut self) {
        self.stop.wake();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

async fn accept_loop(
    listener: UnixListener,
    clients: WorkerTx<StdUnixStream>,
    control: WorkerTx<WaylandSocketControlEvent>,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => match duplicate_stream(&stream) {
                Ok(stream) => {
                    if clients.try_send(stream).is_err() {
                        warn!("Wayland client completion queue is full; dropping connection");
                    }
                }
                Err(error) => {
                    let _ = control
                        .try_send(WaylandSocketControlEvent::RuntimeFailed(error.to_string()));
                    return;
                }
            },
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                let _ =
                    control.try_send(WaylandSocketControlEvent::RuntimeFailed(error.to_string()));
                return;
            }
        }
    }
}

fn duplicate_stream(stream: &UnixStream) -> io::Result<StdUnixStream> {
    fcntl_dupfd_cloexec(stream, 0)
        .map(StdUnixStream::from)
        .map_err(io::Error::from)
}

pub(crate) fn drain_wayland_socket_events(
    clients: &WorkerRx<StdUnixStream>,
    control: &WorkerRx<WaylandSocketControlEvent>,
    state: &mut RuntimeState,
) -> Result<(), String> {
    while let Some(stream) = clients.try_recv() {
        if let Err(error) = state
            .display_handle
            .insert_client(stream, Arc::new(WaylandClientState::default()))
        {
            warn!(%error, "failed to insert Wayland client");
        }
    }
    if let Some(WaylandSocketControlEvent::RuntimeFailed(message)) = control.try_recv() {
        return Err(message);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub(crate) enum WaylandSocketRuntimeError {
    #[error(transparent)]
    Wake(#[from] EventfdWakeError),
    #[error("failed to duplicate the Wayland listening socket: {0}")]
    Duplicate(#[source] io::Error),
    #[error("failed to spawn the Wayland accept completion thread: {0}")]
    Spawn(#[source] io::Error),
    #[error("failed to initialize the Wayland Compio runtime: {0}")]
    Runtime(#[source] io::Error),
    #[error("failed to attach the Wayland listener to Compio: {0}")]
    AttachListener(#[source] io::Error),
    #[error("failed to attach the Wayland shutdown completion: {0}")]
    AttachStop(#[source] io::Error),
    #[error("Wayland accept runtime stopped during initialization")]
    StartupDisconnected,
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };

    use tensor_runtime::WorkerBridge;

    use super::*;

    static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);

    fn test_listener() -> (ListeningSocket, PathBuf) {
        let ordinal = NEXT_SOCKET.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tensor-wayland-completion-{}-{ordinal}.sock",
            std::process::id()
        ));
        let listener = ListeningSocket::bind_absolute(path.clone()).unwrap();
        (listener, path)
    }

    #[test]
    fn client_is_published_only_after_accept_completion() {
        let (listener, path) = test_listener();
        let (client_tx, clients) = WorkerBridge::bounded(4);
        let (control_tx, control) = WorkerBridge::bounded(1);
        let runtime = WaylandSocketRuntime::start(&listener, client_tx, control_tx).unwrap();

        assert!(clients.recv_timeout(Duration::from_millis(30)).is_err());
        let _peer = StdUnixStream::connect(path).unwrap();
        let _accepted = clients.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(control.try_recv().is_none());

        drop(runtime);
    }

    #[test]
    fn shutdown_awaits_pending_accept_cancellation() {
        let (listener, path) = test_listener();
        let (client_tx, clients) = WorkerBridge::bounded(4);
        let (control_tx, control) = WorkerBridge::bounded(1);
        let runtime = WaylandSocketRuntime::start(&listener, client_tx, control_tx).unwrap();

        let started = Instant::now();
        drop(runtime);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(clients.try_recv().is_none());
        assert!(control.try_recv().is_none());

        drop(listener);
        assert!(StdUnixStream::connect(path).is_err());
    }
}

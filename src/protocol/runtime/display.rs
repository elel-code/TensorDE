//! Completion adapter for the `wayland-backend` aggregate fd.
//!
//! The Rust Wayland backend owns an opaque epoll instance containing its client
//! sockets. Tensor does not duplicate that registry. It submits one Compio
//! `PollOnce` operation for the aggregate fd; on Linux/io_uring this is one
//! `IORING_OP_POLL_ADD` whose CQE publishes a compositor-thread dispatch event.
//! Rearming is explicit and happens only after `dispatch_clients` drains the
//! backend, preventing a level-triggered completion loop.

use tensor_runtime::{
    OpaqueFdCompletion, OpaqueFdCompletionError, OpaqueFdCompletionRuntime, WorkerRx, WorkerTx,
};
use wayland_server::Display;

use crate::protocol::state::RuntimeState;

pub(crate) const MAX_PENDING_WAYLAND_DISPLAY_EVENTS: usize = 1;
pub(crate) const MAX_PENDING_WAYLAND_DISPLAY_CONTROL_EVENTS: usize = 1;

pub(crate) type WaylandDisplayEvent = OpaqueFdCompletion;
pub(crate) type WaylandDisplayControlEvent = String;

/// Owns the Compio thread with one submitted display-fd operation at a time.
pub(crate) struct WaylandDisplayRuntime {
    _runtime: OpaqueFdCompletionRuntime,
}

impl WaylandDisplayRuntime {
    pub(crate) fn start<State: 'static>(
        display: &Display<State>,
        events: WorkerTx<WaylandDisplayEvent>,
        control: WorkerTx<WaylandDisplayControlEvent>,
    ) -> Result<Self, OpaqueFdCompletionError> {
        OpaqueFdCompletionRuntime::start(
            "tensor-wayland-display-completions",
            display,
            events,
            control,
        )
        .map(|runtime| Self { _runtime: runtime })
    }
}

pub(crate) fn drain_wayland_display_events(
    events: &WorkerRx<WaylandDisplayEvent>,
    control: &WorkerRx<WaylandDisplayControlEvent>,
    state: &mut RuntimeState,
) -> Result<(), String> {
    while let Some(completion) = events.try_recv() {
        state
            .dispatch_wayland_clients()
            .map_err(|error| error.to_string())?;
        state
            .display_handle
            .flush_clients()
            .map_err(|error| error.to_string())?;
        completion
            .rearm()
            .map_err(|error| format!("Wayland display rearm was rejected: {error:?}"))?;
    }
    if let Some(message) = control.try_recv() {
        return Err(message);
    }
    Ok(())
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
        let Some(completion) = events.recv_timeout(Duration::from_secs(2)).ok() else {
            panic!("expected a display completion");
        };
        display.dispatch_clients(&mut ()).unwrap();
        display.flush_clients().unwrap();
        completion.rearm().unwrap();

        sync_request(&mut client, 3);
        let pending_completion = events.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(pending_completion);
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

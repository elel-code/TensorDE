//! Tensor event-layer ownership on [`RuntimeState`].
//!
//! The completion loop waits on fds; this module owns the **semantic** queue:
//! value events, coalescing, phase order, and a single end-of-turn redraw
//! latch. Heavy policy (seat, KMS submit) remains in existing handlers; they
//! post intents here so the reactor can later move without rewriting policy.
//!
//! # Performance
//!
//! - `push_event` is O(1) and never allocates (fixed rings in `tensor-event`).
//! - Pointer motion is coalesced in the queue (last sample wins).
//! - Redraw intents coalesce to one workspace repaint per dispatch turn.
//! - Worker bridge inject is capped (`INJECT_BURST`) so a flooded IPC/spawn
//!   queue cannot monopolize the compositor thread.

use tensor_event::{
    Event, EventQueue, OutputEvent, OutputId, PushResult, SurfaceId, ViewId as EventViewId,
};
use tensor_present::PresentQueue;
use tensor_runtime::{WorkerBridge, WorkerRx, WorkerTx, inject_events};
use tracing::trace;

use crate::ecs::ViewId;
use tensor_host::ConnectorId as BackendOutputId;

use super::RuntimeState;

/// Max worker messages moved into the event queue per idle turn.
const INJECT_BURST: usize = 64;
/// Max events drained from the queue per idle turn (prevents livelock).
const DRAIN_BURST: usize = 256;
/// Cross-thread bridge capacity (spawn / future IPC completions).
const WORKER_BRIDGE_CAPACITY: usize = 128;

/// Compositor-owned event bus + optional worker inject path.
pub(crate) struct EventLoopState {
    queue: EventQueue,
    /// Completions from Compio / spawn workers (value-only).
    worker_rx: WorkerRx<Event>,
    /// Cloned onto workers; kept so the compositor owns the send half.
    worker_tx: WorkerTx<Event>,
    /// Value-only present intents / slot readiness (KMS adapter drains this).
    present: PresentQueue,
    /// Coalesced redraw requested by drained events this turn.
    redraw_workspace: bool,
    redraw_all: bool,
    session_resume_repaint: bool,
}

impl EventLoopState {
    pub(crate) fn new() -> Self {
        let (worker_tx, worker_rx) = WorkerBridge::bounded(WORKER_BRIDGE_CAPACITY);
        Self {
            queue: EventQueue::new(),
            worker_rx,
            worker_tx,
            present: PresentQueue::new(),
            redraw_workspace: false,
            redraw_all: false,
            session_resume_repaint: false,
        }
    }

    #[inline]
    pub(crate) fn worker_tx(&self) -> WorkerTx<Event> {
        self.worker_tx.clone()
    }

    #[inline]
    pub(crate) fn push(&mut self, event: Event) -> PushResult {
        self.queue.push(event)
    }

    #[inline]
    pub(crate) fn present_queue(&mut self) -> &mut PresentQueue {
        &mut self.present
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn queue_stats(&self) -> tensor_event::QueueStats {
        self.queue.stats()
    }

    pub(crate) fn defer_session_resume_repaint(&mut self) {
        self.session_resume_repaint = true;
    }

    pub(crate) fn take_session_resume_repaint(&mut self) -> bool {
        std::mem::take(&mut self.session_resume_repaint)
    }
}

impl Default for EventLoopState {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeState {
    /// Cloneable sender for workers that must not touch `RuntimeState`.
    ///
    /// Used by spawn/IPC bridges in the next migration step; kept hot so
    /// workers never reach into `event_loop` fields directly.
    #[allow(dead_code)]
    pub(crate) fn event_worker_tx(&self) -> WorkerTx<Event> {
        self.event_loop.worker_tx()
    }

    /// Non-blocking push onto the Tensor event bus.
    #[inline]
    pub(crate) fn push_event(&mut self, event: Event) -> PushResult {
        let result = self.event_loop.push(event);
        if matches!(result, PushResult::Dropped) {
            trace!(?event, "tensor-event queue dropped event (phase full)");
        }
        result
    }

    /// Post a coalesced workspace redraw intent (does not submit immediately).
    #[allow(dead_code)] // Policy paths will switch off immediate redraw next.
    #[inline]
    pub(crate) fn queue_workspace_redraw_intent(&mut self) {
        let _ = self.push_event(Event::RedrawAll);
        // RedrawAll means "full" in the event enum; workspace vs all is decided
        // at drain time via force_full_redraw / explicit flag below.
        self.event_loop.redraw_workspace = true;
    }

    /// Map a backend output identity onto the event bus (stable bit packing).
    #[inline]
    pub(crate) fn event_output_id(id: BackendOutputId) -> OutputId {
        id.as_output_id()
    }

    #[inline]
    pub(crate) fn event_surface_id(protocol_id: u32) -> SurfaceId {
        SurfaceId::new(u64::from(protocol_id))
    }

    #[inline]
    pub(crate) fn event_view_id(view: ViewId) -> EventViewId {
        EventViewId::new(view.get())
    }

    /// Notify the bus of a committed surface (value-only).
    pub(crate) fn push_surface_committed(&mut self, protocol_id: u32, view: Option<ViewId>) {
        let _ = self.push_event(Event::SurfaceCommitted {
            surface: Self::event_surface_id(protocol_id),
            view: view.map(Self::event_view_id),
        });
        self.event_loop.redraw_workspace = true;
    }

    /// Notify the bus of a pointer sample (coalesced; seat already applied).
    ///
    /// Built through `tensor_input::Sample` so the bus never depends on
    /// libinput event types. `time_ns` is monotonic nanoseconds.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn push_pointer_motion_sample(&mut self, x: f64, y: f64, time_ns: u64) {
        let sample = tensor_input::Sample::pointer_motion(x, y, time_ns);
        let _ = self.push_event(sample.into_event());
    }

    /// Keyboard sample (Linux keycode, not keysym).
    pub(crate) fn push_key_sample(&mut self, sample: tensor_input::Sample) {
        let _ = self.push_event(sample.into_event());
    }

    /// Notify the bus of a completed page flip (after KMS bookkeeping).
    pub(crate) fn push_vblank(&mut self, output: BackendOutputId, sequence: u64) {
        let _ = self.push_event(Event::Output(OutputEvent::VBlank {
            output: Self::event_output_id(output),
            sequence,
        }));
    }

    /// Register a connector with the value-only present readiness table.
    #[cfg(feature = "tty")]
    pub(crate) fn register_present_output(&mut self, output: BackendOutputId) {
        self.event_loop.present_queue().register_output(output);
    }

    /// Drop present readiness when a connector leaves the plan.
    #[cfg(feature = "tty")]
    pub(crate) fn unregister_present_output(&mut self, output: BackendOutputId) {
        self.event_loop.present_queue().unregister_output(output);
    }

    /// Whether a present slot is free (policy-side; KMS still owns real FBs).
    #[cfg(feature = "tty")]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn present_slot_ready(
        &self,
        output: BackendOutputId,
        slot: tensor_host::PresentSlot,
    ) -> bool {
        self.event_loop
            .present
            .readiness(output)
            .is_some_and(|r| r.ready_for(slot))
    }

    /// One compositor turn over the Tensor event layer.
    ///
    /// Called after **Compio completions** on the io_uring driver. Order matches
    /// [`tensor_runtime::run_turn`] (inject → drain → idle) with **zero
    /// allocation**. Seat/KMS side effects still run in adapters.
    pub(crate) fn dispatch_event_turn(&mut self) {
        let inject = inject_events(
            &self.event_loop.worker_rx,
            &mut self.event_loop.queue,
            INJECT_BURST,
        );
        if inject.from_bridge > 0 {
            trace!(
                from_bridge = inject.from_bridge,
                queued = inject.queued,
                coalesced = inject.coalesced,
                queue_dropped = inject.queue_dropped,
                "injected worker events into tensor-event queue"
            );
        }

        let mut drained = 0;
        while drained < DRAIN_BURST {
            let Some(event) = self.event_loop.queue.pop() else {
                break;
            };
            drained += 1;
            self.handle_bus_event(event);
        }

        // Capture fill is budgeted software work — after event drain, before redraw.
        self.process_pending_captures();

        let redraw_all = self.event_loop.redraw_all;
        let redraw_workspace = self.event_loop.redraw_workspace;
        self.event_loop.redraw_all = false;
        self.event_loop.redraw_workspace = false;

        #[cfg(feature = "tty")]
        {
            if redraw_all {
                self.request_redraw_all();
            } else if redraw_workspace {
                self.request_redraw_workspace();
            }
        }
        #[cfg(not(feature = "tty"))]
        {
            let _ = (redraw_all, redraw_workspace);
        }
    }

    /// Completion-turn tail: event dispatch, then flush clients.
    pub(crate) fn on_loop_idle(&mut self) {
        self.dispatch_event_turn();
        // FIFO constraints that did not enter a KMS submission are off-screen
        // or otherwise unpresentable this turn; release them for forward progress.
        self.release_unlatched_fifo_barriers();
        self.flush_wayland_clients();
    }

    fn handle_bus_event(&mut self, event: Event) {
        match event {
            Event::Input(_) => {
                // Seat path already applied the sample; bus entry is for
                // coalescing / future pure-event input routing.
            }
            Event::SurfaceCommitted { .. } => {
                self.event_loop.redraw_workspace = true;
            }
            Event::RedrawAll => {
                #[cfg(feature = "tty")]
                if self.force_full_redraw {
                    self.event_loop.redraw_all = true;
                } else {
                    self.event_loop.redraw_workspace = true;
                }
                #[cfg(not(feature = "tty"))]
                {
                    self.event_loop.redraw_workspace = true;
                }
            }
            Event::Output(OutputEvent::VBlank { .. }) => {
                // Present completion already advanced redraw_states in
                // `dispatch_drm_vblank`; bus records the flip for observers.
            }
            Event::Output(OutputEvent::Connected(_))
            | Event::Output(OutputEvent::Changed(_))
            | Event::Output(OutputEvent::Disconnected(_)) => {
                self.event_loop.redraw_all = true;
            }
            Event::Gpu(_) => {
                // The completed sync-file already retired renderer resources;
                // the bus value preserves phase order for observers.
            }
            Event::Ipc(_) | Event::Timer(_) => {
                // Control-plane payloads resolved by their owners via IDs.
            }
            Event::Launch(outcome) => {
                // Spawn worker already logged; bus entry enables future
                // activation / IPC observers without re-entering protocol dispatch.
                trace!(?outcome, "launch outcome on tensor-event bus");
            }
            Event::Shutdown => {
                // Stop signal is owned by the reactor; ignore until main loop
                // is event-driven end-to-end.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutEngine, LayoutKind};
    use wayland_server::Display;

    fn state() -> RuntimeState {
        let display = Display::<RuntimeState>::new().unwrap();
        RuntimeState::with_appearance(
            display,
            LayoutEngine::new(LayoutKind::Scrolling1D),
            crate::scene::SceneAppearance::default(),
        )
    }

    #[test]
    fn pointer_motion_coalesces_on_the_bus() {
        let mut state = state();
        state.push_pointer_motion_sample(1.0, 2.0, 10);
        state.push_pointer_motion_sample(3.0, 4.0, 20);
        // Only the input phase ring holds motion; one slot after coalesce.
        assert_eq!(state.event_loop.queue.len(), 1);
        assert_eq!(state.event_loop.queue_stats().coalesced, 1);
    }

    #[test]
    fn redraw_intent_latches_once_per_turn() {
        let mut state = state();
        state.queue_workspace_redraw_intent();
        state.queue_workspace_redraw_intent();
        // RedrawAll coalesces in the scene phase.
        assert!(state.event_loop.queue.len() <= 1);
        assert!(state.event_loop.redraw_workspace);
        // Drain clears latches without panicking (tty may no-op redraw).
        state.dispatch_event_turn();
        assert!(!state.event_loop.redraw_workspace);
    }

    #[test]
    fn worker_bridge_injects_into_queue() {
        let mut state = state();
        let tx = state.event_worker_tx();
        tx.try_send(Event::Timer(tensor_event::TimerId(7))).unwrap();
        state.dispatch_event_turn();
        assert_eq!(state.event_loop.queue_stats().drained, 1);
    }

    #[test]
    fn session_resume_repaint_is_consumed_once_at_turn_end() {
        let mut event_loop = EventLoopState::new();
        event_loop.defer_session_resume_repaint();

        assert!(event_loop.take_session_resume_repaint());
        assert!(!event_loop.take_session_resume_repaint());
    }
}

//! Move bridge payloads into the compositor [`EventQueue`].

use tensor_event::{Event, EventQueue, PushResult};

use crate::bridge::WorkerRx;

/// Drain a worker bridge into the event queue (compositor-thread only).
///
/// Returns how many messages were taken from the bridge. Queue drops are
/// reflected in [`EventQueue::stats`], not here.
pub fn inject_events(rx: &WorkerRx<Event>, queue: &mut EventQueue, max: usize) -> InjectSummary {
    let mut summary = InjectSummary::default();
    summary.from_bridge = rx.drain(max, |event| match queue.push(event) {
        PushResult::Queued => summary.queued += 1,
        PushResult::Coalesced => summary.coalesced += 1,
        PushResult::Dropped => summary.queue_dropped += 1,
    });
    summary
}

/// Result of one injection burst.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InjectSummary {
    pub from_bridge: usize,
    pub queued: usize,
    pub coalesced: usize,
    pub queue_dropped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::WorkerBridge;
    use tensor_event::{Event, InputEvent};

    #[test]
    fn inject_coalesces_motion_across_bridge() {
        let (tx, rx) = WorkerBridge::bounded(16);
        let mut queue = EventQueue::with_phase_capacity(16);
        tx.try_send(Event::Input(InputEvent::PointerMotion {
            x: 0.0,
            y: 0.0,
            time_ns: 1,
        }))
        .unwrap();
        tx.try_send(Event::Input(InputEvent::PointerMotion {
            x: 4.0,
            y: 5.0,
            time_ns: 2,
        }))
        .unwrap();
        let summary = inject_events(&rx, &mut queue, 16);
        assert_eq!(summary.from_bridge, 2);
        assert_eq!(summary.queued + summary.coalesced, 2);
        assert_eq!(queue.len(), 1);
    }
}

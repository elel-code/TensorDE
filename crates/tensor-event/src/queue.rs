//! Fixed-capacity, phase-bucketed event ring.
//!
//! # Why not a single `VecDeque`?
//!
//! A monolithic queue forces either (a) sort-by-phase every drain or (b) unfair
//! FIFO that can starve present behind IPC. Per-phase rings keep **O(1) push**,
//! **O(1) pop**, and **phase order without sorting** — the same idea as
//! calloop processing sources then running idle work, made explicit.
//!
//! Capacity is fixed at construction. Overflow never blocks; producers see
//! [`PushResult::Dropped`] and can log via [`QueueStats`].

use crate::{
    coalesce::CoalesceStats,
    event::Event,
    phase::{PHASES, Phase},
};

/// Outcome of a non-blocking push.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PushResult {
    /// Stored as a new ring entry.
    Queued,
    /// Replaced a coalescable older event (no capacity growth).
    Coalesced,
    /// Ring full and no coalescing partner; event discarded.
    Dropped,
}

/// Cumulative queue counters (monotonic for the queue lifetime).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QueueStats {
    pub pushed: u64,
    pub coalesced: u64,
    pub dropped: u64,
    pub drained: u64,
    pub coalesce: CoalesceStats,
}

/// Default per-phase capacity tuned for a desktop compositor turn:
/// input can burst; present/gpu stay shallow; control is bounded.
pub const DEFAULT_PHASE_CAPACITY: usize = 256;

/// Compositor-owned event queue. Not thread-safe; the compositor thread owns it.
/// Cross-thread producers use `tensor-runtime` bridges that `try_send` into a
/// channel, then the compositor moves values in with [`EventQueue::push`].
pub struct EventQueue {
    phases: [PhaseRing; Phase::COUNT],
    stats: QueueStats,
}

struct PhaseRing {
    buf: Box<[Option<Event>]>,
    head: usize,
    len: usize,
}

impl PhaseRing {
    fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            buf: vec![None; capacity].into_boxed_slice(),
            head: 0,
            len: 0,
        }
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.buf.len()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.len == self.capacity()
    }

    /// Scan from newest to oldest for a coalescing partner (bounded by len).
    /// Returns the replaced event when coalescing succeeds.
    fn try_coalesce(&mut self, event: Event) -> Option<Event> {
        if self.len == 0 {
            return None;
        }
        // Newest is at (head + len - 1) % cap.
        let cap = self.capacity();
        for offset in 0..self.len {
            let idx = (self.head + self.len - 1 - offset) % cap;
            if let Some(existing) = self.buf[idx]
                && existing.coalesces_with(event)
            {
                self.buf[idx] = Some(event);
                return Some(existing);
            }
        }
        None
    }

    fn push_back(&mut self, event: Event) -> bool {
        if self.is_full() {
            return false;
        }
        let cap = self.capacity();
        let idx = (self.head + self.len) % cap;
        self.buf[idx] = Some(event);
        self.len += 1;
        true
    }

    fn pop_front(&mut self) -> Option<Event> {
        if self.len == 0 {
            return None;
        }
        let event = self.buf[self.head].take();
        self.head = (self.head + 1) % self.capacity();
        self.len -= 1;
        event
    }
}

impl EventQueue {
    /// Create a queue with the same capacity for every phase.
    pub fn with_phase_capacity(capacity: usize) -> Self {
        Self {
            phases: std::array::from_fn(|_| PhaseRing::with_capacity(capacity)),
            stats: QueueStats::default(),
        }
    }

    /// Default capacities suitable for interactive compositing.
    pub fn new() -> Self {
        Self::with_phase_capacity(DEFAULT_PHASE_CAPACITY)
    }

    #[inline]
    pub fn stats(&self) -> QueueStats {
        self.stats
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.phases.iter().all(PhaseRing::is_empty)
    }

    /// Total buffered events across all phases.
    pub fn len(&self) -> usize {
        self.phases.iter().map(|ring| ring.len).sum()
    }

    /// Non-blocking push with coalescing. Never allocates.
    pub fn push(&mut self, event: Event) -> PushResult {
        let phase = event.phase();
        let ring = &mut self.phases[phase.index()];
        if let Some(previous) = ring.try_coalesce(event) {
            self.stats.coalesced = self.stats.coalesced.saturating_add(1);
            self.stats.coalesce.record(previous, event);
            return PushResult::Coalesced;
        }
        if ring.push_back(event) {
            self.stats.pushed = self.stats.pushed.saturating_add(1);
            PushResult::Queued
        } else {
            self.stats.dropped = self.stats.dropped.saturating_add(1);
            PushResult::Dropped
        }
    }

    /// Pop the next event in phase order, or `None` if the queue is empty.
    pub fn pop(&mut self) -> Option<Event> {
        for phase in PHASES {
            if let Some(event) = self.phases[phase.index()].pop_front() {
                self.stats.drained = self.stats.drained.saturating_add(1);
                return Some(event);
            }
        }
        None
    }

    /// Drain up to `max` events, invoking `f` for each. Returns how many ran.
    pub fn drain(&mut self, max: usize, mut f: impl FnMut(Event)) -> usize {
        let mut n = 0;
        while n < max {
            let Some(event) = self.pop() else {
                break;
            };
            f(event);
            n += 1;
        }
        n
    }

    /// Drain only one phase (e.g. present-only after GPU work).
    pub fn drain_phase(&mut self, phase: Phase, max: usize, mut f: impl FnMut(Event)) -> usize {
        let ring = &mut self.phases[phase.index()];
        let mut n = 0;
        while n < max {
            let Some(event) = ring.pop_front() else {
                break;
            };
            self.stats.drained = self.stats.drained.saturating_add(1);
            f(event);
            n += 1;
        }
        n
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{InputEvent, OutputEvent},
        ids::OutputId,
    };

    #[test]
    fn push_pop_preserves_phase_order() {
        let mut q = EventQueue::with_phase_capacity(8);
        assert_eq!(
            q.push(Event::Ipc(crate::IpcCommandId(1))),
            PushResult::Queued
        );
        assert_eq!(
            q.push(Event::Input(InputEvent::Keyboard {
                key: 1,
                pressed: true,
                time_ns: 0,
            })),
            PushResult::Queued
        );
        assert_eq!(
            q.push(Event::Output(OutputEvent::VBlank {
                output: OutputId::new(1),
                sequence: 3,
            })),
            PushResult::Queued
        );
        // Input before Present before Control.
        assert!(matches!(q.pop(), Some(Event::Input(_))));
        assert!(matches!(
            q.pop(),
            Some(Event::Output(OutputEvent::VBlank { sequence: 3, .. }))
        ));
        assert!(matches!(q.pop(), Some(Event::Ipc(_))));
        assert!(q.pop().is_none());
    }

    #[test]
    fn pointer_motion_coalesces_to_latest() {
        let mut q = EventQueue::with_phase_capacity(8);
        assert_eq!(
            q.push(Event::Input(InputEvent::PointerMotion {
                x: 1.0,
                y: 2.0,
                time_ns: 1,
            })),
            PushResult::Queued
        );
        assert_eq!(
            q.push(Event::Input(InputEvent::PointerMotion {
                x: 9.0,
                y: 8.0,
                time_ns: 2,
            })),
            PushResult::Coalesced
        );
        assert_eq!(q.len(), 1);
        match q.pop() {
            Some(Event::Input(InputEvent::PointerMotion { x, y, time_ns })) => {
                assert_eq!((x, y, time_ns), (9.0, 8.0, 2));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn full_ring_drops_without_panic() {
        let mut q = EventQueue::with_phase_capacity(2);
        assert_eq!(q.push(Event::RedrawAll), PushResult::Queued);
        // Second redraw coalesces.
        assert_eq!(q.push(Event::RedrawAll), PushResult::Coalesced);
        // Non-coalescing control events fill then drop.
        assert_eq!(
            q.push(Event::Ipc(crate::IpcCommandId(1))),
            PushResult::Queued
        );
        assert_eq!(
            q.push(Event::Ipc(crate::IpcCommandId(2))),
            PushResult::Queued
        );
        assert_eq!(
            q.push(Event::Ipc(crate::IpcCommandId(3))),
            PushResult::Dropped
        );
        assert_eq!(q.stats().dropped, 1);
    }

    #[test]
    fn vblank_coalesces_per_output() {
        let mut q = EventQueue::with_phase_capacity(4);
        let a = OutputId::new(1);
        let b = OutputId::new(2);
        q.push(Event::Output(OutputEvent::VBlank {
            output: a,
            sequence: 1,
        }));
        q.push(Event::Output(OutputEvent::VBlank {
            output: b,
            sequence: 1,
        }));
        assert_eq!(
            q.push(Event::Output(OutputEvent::VBlank {
                output: a,
                sequence: 9,
            })),
            PushResult::Coalesced
        );
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn drain_respects_max() {
        let mut q = EventQueue::with_phase_capacity(8);
        for i in 0..5 {
            q.push(Event::Ipc(crate::IpcCommandId(i)));
        }
        let mut seen = 0;
        assert_eq!(q.drain(3, |_| seen += 1), 3);
        assert_eq!(seen, 3);
        assert_eq!(q.len(), 2);
    }
}

//! Fixed-capacity present intent queue (no allocation on the hot path).

use tensor_host::{ConnectorId, PresentIntent, PresentState};
use thiserror::Error;

use crate::readiness::OutputReadiness;

/// Max pending present intents (per compositor turn budget).
pub const PRESENT_QUEUE_CAP: usize = 16;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum QueueError {
    #[error("present queue is full")]
    Full,
    #[error("output is not ready for the requested slot")]
    NotReady,
    #[error("unknown output")]
    UnknownOutput,
}

/// Stats for diagnostics (mirrors event-queue counters).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PresentQueueStats {
    pub pushed: u64,
    pub dropped: u64,
    pub completed: u64,
}

/// Bounded queue of present intents + readiness table.
///
/// Push is O(1); never allocates. Adapters drain intents and perform KMS.
#[derive(Debug)]
pub struct PresentQueue {
    intents: [Option<PresentIntent>; PRESENT_QUEUE_CAP],
    head: usize,
    len: usize,
    readiness: Vec<OutputReadiness>,
    stats: PresentQueueStats,
}

impl PresentQueue {
    pub fn new() -> Self {
        Self {
            intents: [None; PRESENT_QUEUE_CAP],
            head: 0,
            len: 0,
            readiness: Vec::new(),
            stats: PresentQueueStats::default(),
        }
    }

    pub fn stats(&self) -> PresentQueueStats {
        self.stats
    }

    pub fn register_output(&mut self, output: ConnectorId) {
        if self.readiness.iter().any(|r| r.output == output) {
            return;
        }
        self.readiness.push(OutputReadiness::new(output));
    }

    pub fn unregister_output(&mut self, output: ConnectorId) {
        self.readiness.retain(|r| r.output != output);
        // Compact: drop intents for this output without counting as completed.
        let mut kept = [None; PRESENT_QUEUE_CAP];
        let mut n = 0;
        for _ in 0..self.len {
            if let Some(intent) = self.intents[self.head].take() {
                self.head = (self.head + 1) % PRESENT_QUEUE_CAP;
                if intent.output != output {
                    kept[n] = Some(intent);
                    n += 1;
                }
            } else {
                self.head = (self.head + 1) % PRESENT_QUEUE_CAP;
            }
        }
        self.head = 0;
        self.len = n;
        self.intents = kept;
    }

    pub fn readiness(&self, output: ConnectorId) -> Option<&OutputReadiness> {
        self.readiness.iter().find(|r| r.output == output)
    }

    pub fn readiness_mut(&mut self, output: ConnectorId) -> Option<&mut OutputReadiness> {
        self.readiness.iter_mut().find(|r| r.output == output)
    }

    /// Queue a present if the slot is ready. O(1).
    pub fn try_push(&mut self, intent: PresentIntent) -> Result<(), QueueError> {
        {
            let ready = self
                .readiness_mut(intent.output)
                .ok_or(QueueError::UnknownOutput)?;
            if !ready.mark_queued(intent.slot, intent.serial) {
                return Err(QueueError::NotReady);
            }
        }
        if self.len >= PRESENT_QUEUE_CAP {
            self.stats.dropped += 1;
            if let Some(ready) = self.readiness_mut(intent.output)
                && let Some(s) = ready.slot_mut(intent.slot)
            {
                s.state = PresentState::Idle;
            }
            return Err(QueueError::Full);
        }
        let idx = (self.head + self.len) % PRESENT_QUEUE_CAP;
        self.intents[idx] = Some(intent);
        self.len += 1;
        self.stats.pushed += 1;
        Ok(())
    }

    /// Pop the oldest intent, if any.
    pub fn try_pop(&mut self) -> Option<PresentIntent> {
        while self.len > 0 {
            let intent = self.intents[self.head].take();
            self.head = (self.head + 1) % PRESENT_QUEUE_CAP;
            self.len -= 1;
            if let Some(intent) = intent {
                self.stats.completed += 1;
                return Some(intent);
            }
        }
        None
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for PresentQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use tensor_host::{PresentIntent, PresentSlot};

    use super::*;

    #[test]
    fn queue_push_pop_fifo() {
        let mut q = PresentQueue::new();
        let out = ConnectorId::new(1, 1);
        q.register_output(out);
        let a = PresentIntent::new(out, PresentSlot(0), 1, 10);
        let b = PresentIntent::new(out, PresentSlot(1), 2, 20);
        q.try_push(a).unwrap();
        q.try_push(b).unwrap();
        assert_eq!(q.try_pop(), Some(a));
        assert_eq!(q.try_pop(), Some(b));
        assert!(q.is_empty());
    }

    #[test]
    fn not_ready_rejects_second_queue_same_slot() {
        let mut q = PresentQueue::new();
        let out = ConnectorId::new(1, 1);
        q.register_output(out);
        let a = PresentIntent::new(out, PresentSlot(0), 1, 10);
        q.try_push(a).unwrap();
        assert_eq!(
            q.try_push(PresentIntent::new(out, PresentSlot(0), 2, 11)),
            Err(QueueError::NotReady)
        );
    }

    #[test]
    fn unregister_drops_pending_for_output() {
        let mut q = PresentQueue::new();
        let a = ConnectorId::new(1, 1);
        let b = ConnectorId::new(1, 2);
        q.register_output(a);
        q.register_output(b);
        q.try_push(PresentIntent::new(a, PresentSlot(0), 1, 1))
            .unwrap();
        q.try_push(PresentIntent::new(b, PresentSlot(0), 2, 2))
            .unwrap();
        q.unregister_output(a);
        assert_eq!(q.len(), 1);
        assert_eq!(q.try_pop().unwrap().output, b);
    }
}

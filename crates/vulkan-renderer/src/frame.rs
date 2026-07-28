use std::any::Any;
use std::fmt;
use std::sync::Arc;

use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FrameToken(u64);

impl FrameToken {
    pub(crate) const fn from_value(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Type-erased host/resource ownership retained until a GPU timeline value
/// completes.
#[derive(Clone)]
pub struct SubmissionLease {
    _value: Arc<dyn Any + Send + Sync>,
}

impl SubmissionLease {
    pub fn new<T>(value: Arc<T>) -> Self
    where
        T: Any + Send + Sync,
    {
        Self { _value: value }
    }
}

impl fmt::Debug for SubmissionLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubmissionLease")
            .finish_non_exhaustive()
    }
}

/// A cloneable ownership handle that can be attached to command recording
/// without exposing its concrete Vulkan resource implementation.
pub trait SubmissionResource: Send + Sync {
    fn submission_lease(&self) -> SubmissionLease;
}

#[derive(Debug)]
pub struct FrameClock {
    next: u64,
    completed: u64,
}

impl Default for FrameClock {
    fn default() -> Self {
        Self {
            next: 1,
            completed: 0,
        }
    }
}

impl FrameClock {
    pub fn allocate(&mut self) -> Result<FrameToken> {
        let value = self.next;
        self.next = self.next.checked_add(1).ok_or(Error::TimelineExhausted)?;
        Ok(FrameToken(value))
    }

    pub fn retire(&mut self, completed: u64) {
        self.completed = self.completed.max(completed);
    }

    pub const fn completed(&self) -> u64 {
        self.completed
    }
}

#[derive(Debug)]
pub struct RetirementQueue<T> {
    entries: Vec<(u64, T)>,
}

impl<T> Default for RetirementQueue<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<T> RetirementQueue<T> {
    pub fn push(&mut self, retire_after: FrameToken, value: T) {
        self.entries.push((retire_after.value(), value));
    }

    pub fn retire_completed(&mut self, completed: u64) -> Vec<T> {
        let mut retired = Vec::new();
        let mut pending = Vec::with_capacity(self.entries.len());
        for (value, resource) in self.entries.drain(..) {
            if value <= completed {
                retired.push(resource);
            } else {
                pending.push((value, resource));
            }
        }
        self.entries = pending;
        retired
    }

    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn retirement_is_bounded_by_completed_timeline() {
        let mut clock = FrameClock::default();
        let first = clock.allocate().unwrap();
        let second = clock.allocate().unwrap();
        let mut queue = RetirementQueue::default();
        queue.push(second, "second");
        queue.push(first, "first");
        assert_eq!(queue.retire_completed(first.value()), vec!["first"]);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.retire_completed(second.value()), vec!["second"]);
    }

    #[test]
    fn submission_lease_drops_only_after_timeline_retirement() {
        struct Probe(Arc<AtomicUsize>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let dropped = Arc::new(AtomicUsize::new(0));
        let lease = SubmissionLease::new(Arc::new(Probe(Arc::clone(&dropped))));
        let frame = FrameToken::from_value(4);
        let mut queue = RetirementQueue::default();
        queue.push(frame, lease);
        assert!(queue.retire_completed(3).is_empty());
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
        drop(queue.retire_completed(4));
        assert_eq!(dropped.load(Ordering::Relaxed), 1);
    }
}

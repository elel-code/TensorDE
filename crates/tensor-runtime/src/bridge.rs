//! Bounded cross-thread channel (std-backed, calloop-channel idea).
//!
//! Same **bounded non-blocking** contract as calloop's channel. The compositor
//! injects on its turn after I/O **completions**. On the product path a worker
//! write to [`crate::EventfdWake`] completes a submitted Compio/io_uring read —
//! completion model, not readiness registration.

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{
            self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError as StdTrySend,
        },
    },
    time::Duration,
};

use crate::reactor::WakeSink;

/// Non-blocking send failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrySendError {
    Full,
    Disconnected,
}

/// Shared counters for bridge observability.
#[derive(Debug, Default)]
struct BridgeCounters {
    sent: AtomicU64,
    dropped_full: AtomicU64,
    received: AtomicU64,
}

/// Snapshot of bridge counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BridgeStats {
    pub sent: u64,
    pub dropped_full: u64,
    pub received: u64,
}

/// Sending end used by workers (cloneable).
pub struct WorkerTx<T> {
    tx: SyncSender<T>,
    counters: Arc<BridgeCounters>,
    wake: Option<Arc<dyn WakeSink>>,
}

impl<T> Clone for WorkerTx<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            counters: Arc::clone(&self.counters),
            wake: self.wake.clone(),
        }
    }
}

/// Receiving end owned by the compositor (or an adapter that injects events).
#[derive(Debug)]
pub struct WorkerRx<T> {
    rx: Receiver<T>,
    counters: Arc<BridgeCounters>,
}

/// Bounded worker → compositor bridge.
pub struct WorkerBridge;

impl WorkerBridge {
    /// Create a bridge with fixed capacity (`capacity` ≥ 1).
    pub fn bounded<T>(capacity: usize) -> (WorkerTx<T>, WorkerRx<T>) {
        let capacity = capacity.max(1);
        let (tx, rx) = mpsc::sync_channel(capacity);
        let counters = Arc::new(BridgeCounters::default());
        (
            WorkerTx {
                tx,
                counters: Arc::clone(&counters),
                wake: None,
            },
            WorkerRx { rx, counters },
        )
    }

    /// Create a bridge that signals `wake` after each successful enqueue.
    ///
    /// The wake is an operation trigger, not a readiness registration. The
    /// product path uses [`crate::EventfdWake`], whose submitted Compio read
    /// completes before the compositor drains this bridge.
    pub fn bounded_with_wake<T>(
        capacity: usize,
        wake: Arc<dyn WakeSink>,
    ) -> (WorkerTx<T>, WorkerRx<T>) {
        let (mut tx, rx) = Self::bounded(capacity);
        tx.wake = Some(wake);
        (tx, rx)
    }
}

impl<T> fmt::Debug for WorkerTx<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkerTx")
            .field("stats", &self.stats())
            .field("has_wake", &self.wake.is_some())
            .finish_non_exhaustive()
    }
}

impl<T> WorkerTx<T> {
    /// Non-blocking send. Never waits on the compositor.
    pub fn try_send(&self, value: T) -> Result<(), TrySendError> {
        match self.tx.try_send(value) {
            Ok(()) => {
                self.counters.sent.fetch_add(1, Ordering::Relaxed);
                if let Some(wake) = &self.wake {
                    wake.wake();
                }
                Ok(())
            }
            Err(StdTrySend::Full(_)) => {
                self.counters.dropped_full.fetch_add(1, Ordering::Relaxed);
                Err(TrySendError::Full)
            }
            Err(StdTrySend::Disconnected(_)) => Err(TrySendError::Disconnected),
        }
    }

    pub fn stats(&self) -> BridgeStats {
        stats(&self.counters)
    }
}

impl<T> WorkerRx<T> {
    /// Non-blocking receive.
    pub fn try_recv(&self) -> Option<T> {
        match self.rx.try_recv() {
            Ok(value) => {
                self.counters.received.fetch_add(1, Ordering::Relaxed);
                Some(value)
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }

    /// Drain up to `max` pending messages without blocking.
    pub fn drain(&self, max: usize, mut f: impl FnMut(T)) -> usize {
        let mut n = 0;
        while n < max {
            let Some(value) = self.try_recv() else {
                break;
            };
            f(value);
            n += 1;
        }
        n
    }

    /// Block until a message arrives or `timeout` elapses (tests / idle inject).
    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        let value = self.rx.recv_timeout(timeout)?;
        self.counters.received.fetch_add(1, Ordering::Relaxed);
        Ok(value)
    }

    pub fn stats(&self) -> BridgeStats {
        stats(&self.counters)
    }
}

fn stats(counters: &BridgeCounters) -> BridgeStats {
    BridgeStats {
        sent: counters.sent.load(Ordering::Relaxed),
        dropped_full: counters.dropped_full.load(Ordering::Relaxed),
        received: counters.received.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct CountingWake(AtomicU64);

    impl WakeSink for CountingWake {
        fn wake(&self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn bounded_drop_on_full() {
        let (tx, rx) = WorkerBridge::bounded::<u32>(1);
        assert!(tx.try_send(1).is_ok());
        assert_eq!(tx.try_send(2), Err(TrySendError::Full));
        assert_eq!(rx.try_recv(), Some(1));
        assert!(tx.try_send(3).is_ok());
        assert_eq!(tx.stats().dropped_full, 1);
    }

    #[test]
    fn drain_caps_work() {
        let (tx, rx) = WorkerBridge::bounded::<u32>(8);
        for i in 0..5 {
            tx.try_send(i).unwrap();
        }
        let mut sum = 0;
        assert_eq!(rx.drain(3, |v| sum += v), 3);
        assert_eq!(sum, 3);
        assert_eq!(rx.try_recv(), Some(3));
    }

    #[test]
    fn wake_runs_only_after_successful_enqueue() {
        let wake = Arc::new(CountingWake::default());
        let (tx, rx) = WorkerBridge::bounded_with_wake::<u32>(1, wake.clone());
        assert_eq!(tx.try_send(1), Ok(()));
        assert_eq!(tx.try_send(2), Err(TrySendError::Full));
        assert_eq!(wake.0.load(Ordering::Relaxed), 1);
        assert_eq!(rx.try_recv(), Some(1));
    }
}

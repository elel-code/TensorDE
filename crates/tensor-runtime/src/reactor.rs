//! Reactor contracts for the compositor I/O layer.
//!
//! # Compio is a **completion** model
//!
//! Compio does not own a readiness reactor that you “poll until readable.”
//! You **submit** operations; the driver (Linux product path: **io_uring**)
//! delivers **completions**. Tensor maps completions into value-only
//! [`tensor_event::Event`]s, then runs [`run_turn`].
//!
//! ```text
//! submit ops (read / accept / timer / wake) → completions → inject → drain → idle
//! ```
//!
//! Transitional calloop still uses readiness + callbacks for some Smithay-owned
//! fds. That is migration debt. The target is: same work as Compio-submitted
//! ops whose futures/CQEs complete on the compositor or worker thread.
//!
//! Compio's `polling` Cargo feature is disabled. Failure to create io_uring is
//! a runtime initialization error, never a switch to a readiness driver.
//!
//! Performance: zero-alloc at the turn call site; present/Vulkan stay on the
//! compositor thread. Workers never hold DRM/Wayland objects.

use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use compio::{io::AsyncReadExt, runtime::fd::AsyncFd};
use tensor_event::{Event, EventQueue};
use thiserror::Error;

use crate::bridge::WorkerRx;
use crate::inject::{InjectSummary, inject_events};

/// Caps for one compositor turn (mirrors compositor `EventLoopState` constants).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnBudget {
    /// Max worker messages moved into the event queue.
    pub inject: usize,
    /// Max events drained from the phase rings.
    pub drain: usize,
}

impl TurnBudget {
    pub const DEFAULT: Self = Self {
        inject: 64,
        drain: 256,
    };
}

/// Result of one semantic turn (no FDs, no Wayland objects).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TurnSummary {
    pub injected: InjectSummary,
    pub drained: usize,
    pub queue_len_after: usize,
}

/// Which completion driver Compio is expected to use underneath.
///
/// Product Tensor uses the io_uring completion driver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionDriver {
    /// Linux io_uring. Required for every Compio runtime in the product.
    IoUring,
}

impl CompletionDriver {
    /// Preferred driver for this build target.
    #[inline]
    pub const fn preferred() -> Self {
        Self::IoUring
    }

    #[inline]
    pub const fn is_io_uring(self) -> bool {
        matches!(self, Self::IoUring)
    }
}

/// Run inject + drain for one turn. Caller applies policy to each drained event.
///
/// Runs **after** one or more I/O completions (or a transitional idle slot).
/// `on_event` returns whether to continue draining (`false` stops early).
pub fn run_turn(
    worker_rx: &WorkerRx<Event>,
    queue: &mut EventQueue,
    budget: TurnBudget,
    mut on_event: impl FnMut(Event) -> bool,
) -> TurnSummary {
    let injected = inject_events(worker_rx, queue, budget.inject);
    let mut drained = 0;
    while drained < budget.drain {
        let Some(event) = queue.pop() else {
            break;
        };
        drained += 1;
        if !on_event(event) {
            break;
        }
    }
    TurnSummary {
        injected,
        drained,
        queue_len_after: queue.len(),
    }
}

/// Something that can wake the compositor after a worker send.
///
/// Product shape: write an eventfd; a **submitted** Compio/io_uring read (or
/// equivalent completion-bearing op) on that fd completes and schedules
/// [`run_turn`]. calloop's channel ping is only transitional.
pub trait WakeSink: Send + Sync {
    /// Non-blocking wake. May coalesce (eventfd counter is fine).
    fn wake(&self);
}

/// Cloneable stop flag checked between compositor completion turns.
#[derive(Clone, Debug, Default)]
pub struct RuntimeStop {
    stopped: Arc<AtomicBool>,
}

impl RuntimeStop {
    #[inline]
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
    }

    #[inline]
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
}

/// No-op wake (pure unit tests that never block on the OS).
#[derive(Clone, Copy, Debug, Default)]
pub struct NullWake;

impl WakeSink for NullWake {
    fn wake(&self) {}
}

/// eventfd used as a **completion** wake source for Compio/io_uring.
///
/// Typical product use:
/// 1. Create this fd once on the compositor thread.
/// 2. Submit a Compio async read (8 bytes) — or the driver's equivalent —
///    against the fd; do **not** spin a readiness poll loop.
/// 3. Workers call [`WakeSink::wake`] → write(8).
/// 4. The read **completes** → drain the counter / re-submit the next read →
///    [`run_turn`].
///
/// Multiple `wake()` calls coalesce into one counter value / one completion.
#[derive(Debug)]
pub struct EventfdWake {
    fd: OwnedFd,
}

/// Compio-attached eventfd reader.
///
/// Awaiting [`Self::completed`] submits a read operation. The semantic turn is
/// scheduled only after that operation completes; this type exposes no
/// readiness interest or poll API.
#[derive(Debug)]
pub struct EventfdCompletion {
    fd: AsyncFd<OwnedFd>,
}

#[derive(Debug, Error)]
pub enum EventfdWakeError {
    #[error("eventfd create failed: {0}")]
    Create(#[source] std::io::Error),
}

impl EventfdWake {
    /// Create a non-blocking, cloexec eventfd (Linux product path).
    pub fn new() -> Result<Self, EventfdWakeError> {
        let flags = rustix::event::EventfdFlags::NONBLOCK | rustix::event::EventfdFlags::CLOEXEC;
        let fd = rustix::event::eventfd(0, flags)
            .map_err(|e| EventfdWakeError::Create(std::io::Error::from(e)))?;
        Ok(Self { fd })
    }

    /// Raw fd for Compio/io_uring op submission (not for a readiness registry).
    #[inline]
    pub fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }

    /// Borrowed fd for APIs that take `AsFd`.
    #[inline]
    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    /// Attach a duplicate of the eventfd to the current Compio runtime.
    ///
    /// Call this from within the runtime that will await completions. The
    /// duplicate shares the same eventfd counter while keeping worker wake
    /// ownership separate from the submitted read operation.
    pub fn completion_reader(&self) -> std::io::Result<EventfdCompletion> {
        let fd = rustix::io::dup(self.fd.as_fd()).map_err(std::io::Error::from)?;
        Ok(EventfdCompletion {
            fd: AsyncFd::new(fd)?,
        })
    }

    /// Drain the eventfd counter after a completed wake read (or after a
    /// transitional readiness edge during migration).
    ///
    /// Safe to call when the counter is zero (`WouldBlock` → `Ok(0)`).
    pub fn drain(&self) -> std::io::Result<u64> {
        let mut buf = [0u8; 8];
        match rustix::io::read(self.fd.as_fd(), &mut buf) {
            Ok(8) => Ok(u64::from_ne_bytes(buf)),
            Ok(_) => Ok(0),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(std::io::Error::from(e)),
        }
    }
}

impl EventfdCompletion {
    /// Submit one eventfd read and resolve with its coalesced wake count.
    pub async fn completed(&mut self) -> std::io::Result<u64> {
        let compio::BufResult(result, bytes) = self.fd.read_exact([0u8; 8]).await;
        result?;
        Ok(u64::from_ne_bytes(bytes))
    }
}

impl WakeSink for EventfdWake {
    fn wake(&self) {
        let one = 1u64.to_ne_bytes();
        // Ignore WouldBlock/full: counter saturates; one completion is enough.
        let _ = rustix::io::write(self.fd.as_fd(), &one);
    }
}

impl AsRawFd for EventfdWake {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl AsFd for EventfdWake {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

#[cfg(test)]
mod tests {
    use tensor_event::{Event, EventQueue, InputEvent, TimerId};

    use super::*;
    use crate::WorkerBridge;

    #[test]
    fn configured_driver_is_io_uring() {
        assert!(CompletionDriver::preferred().is_io_uring());
    }

    #[test]
    fn compio_reader_returns_a_completion_value() {
        let runtime = crate::io_uring_runtime(1).expect("Compio runtime");
        runtime.block_on(async {
            let wake = std::sync::Arc::new(EventfdWake::new().expect("eventfd"));
            let mut completion = wake.completion_reader().expect("attach eventfd");
            let writer = std::thread::spawn({
                let wake = std::sync::Arc::clone(&wake);
                move || {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    wake.wake();
                    wake.wake();
                }
            });
            let completed = completion.completed().await.expect("read completion");
            writer.join().expect("wake writer");
            // The first write may complete the submitted read before the
            // second write reaches eventfd. Either CQE coalescing outcome is
            // valid, but the completed and still-pending counters must sum to
            // the two wakes without loss.
            assert_eq!(completed + wake.drain().expect("drain trailing wake"), 2);
        });
    }

    #[test]
    fn turn_injects_and_drains_with_budget() {
        let (tx, rx) = WorkerBridge::bounded(8);
        for i in 0..4 {
            tx.try_send(Event::Input(InputEvent::PointerMotion {
                x: i as f64,
                y: 0.0,
                time_ns: i as u64,
            }))
            .unwrap();
        }
        let mut queue = EventQueue::new();
        let mut seen = 0;
        let summary = run_turn(&rx, &mut queue, TurnBudget::DEFAULT, |_| {
            seen += 1;
            true
        });
        assert!(summary.injected.from_bridge >= 1);
        assert!(seen >= 1);
        assert_eq!(summary.queue_len_after, 0);
    }

    #[test]
    fn drain_budget_stops_early() {
        let (_tx, rx) = WorkerBridge::bounded(4);
        let mut queue = EventQueue::new();
        for i in 0..8 {
            let _ = queue.push(Event::Timer(TimerId(i)));
        }
        let mut seen = 0;
        let summary = run_turn(
            &rx,
            &mut queue,
            TurnBudget {
                inject: 0,
                drain: 3,
            },
            |_| {
                seen += 1;
                true
            },
        );
        assert_eq!(seen, 3);
        assert_eq!(summary.drained, 3);
        assert!(summary.queue_len_after >= 5);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn eventfd_wake_coalesces_and_drains() {
        let wake = EventfdWake::new().expect("eventfd");
        wake.wake();
        wake.wake();
        let n = wake.drain().expect("drain");
        // Non-semaphore eventfd sums the adds.
        assert!(n >= 1);
        assert_eq!(wake.drain().unwrap_or(0), 0);
    }
}

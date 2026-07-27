//! Compio-backed runtime bridges for Tensor.
//!
//! # Role
//!
//! - **Compositor thread** owns [`tensor_event::EventQueue`] and policy.
//! - **I/O model:** Compio is a **completion** runtime (submit op → completion),
//!   not a readiness poll loop. On Linux the product driver is **io_uring**.
//!   The `polling` feature is disabled; runtime construction fails if io_uring
//!   cannot be created.
//! - **Workers and I/O services** (log drain, launch notifications, IPC) use
//!   Compio completions and exchange only value-only messages.
//!
//! This crate does **not** own DRM/KMS or Wayland objects. It provides:
//!
//! 1. [`WorkerBridge`] — bounded MPSC into the compositor (non-blocking send,
//!    explicit overflow).
//! 2. [`CompioWorker`] — dedicated thread with a Compio [`Runtime`] for async I/O
//!    (same pattern as the logging drain; completion + io_uring-first).
//! 3. [`inject_events`] / [`run_turn`] — compositor turn entry **after**
//!    completions (or a transitional idle slot).
//! 4. [`EventfdWake`] — eventfd that workers write; the compositor **submits**
//!    a read (or equivalent) so the wake arrives as a **completion**, not as a
//!    readiness edge in a poll registry.
//! 5. [`OpaqueFdCompletionRuntime`] — a transitional one-source adapter for
//!    libraries that expose only an opaque notifier fd. It submits one
//!    `PollOnce`, publishes its CQE, and requires explicit rearm or finish.
//!
//! # Performance
//!
//! - Bridge capacity is fixed; `try_send` never blocks the worker.
//! - Injection is O(n) in pending messages and reuses the event queue's
//!   zero-alloc push path.
//! - Present/input policy stays on the compositor thread; Compio completes I/O
//!   and posts values only.

mod bridge;
mod completion;
mod fd_completion;
mod inject;
mod local;
mod reactor;
mod runtime;
mod worker;

pub use bridge::{BridgeStats, TrySendError, WorkerBridge, WorkerRx, WorkerTx};
pub use completion::{CompletionRelayError, EventfdCompletionRelay};
pub use fd_completion::{OpaqueFdCompletion, OpaqueFdCompletionError, OpaqueFdCompletionRuntime};
pub use inject::{InjectSummary, inject_events};
pub use local::{LocalCompletionQueue, LocalQueueOverflow};
pub use reactor::{
    CompletionDriver, EventfdCompletion, EventfdWake, EventfdWakeError, NullWake, RuntimeStop,
    TurnBudget, TurnSummary, WakeSink, run_turn,
};
pub use runtime::io_uring_runtime;
pub use worker::{CompioWorker, WorkerError};

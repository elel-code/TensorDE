//! Tensor compositor event layer.
//!
//! This crate owns **what** happens and **in which order**, not how file
//! descriptors. Sources (protocol dispatch, Compio completions, DRM
//! vblank) push [`Event`] values into an [`EventQueue`]; the compositor drains
//! by [`Phase`].
//!
//! # Performance contract
//!
//! - Events are small, `Copy` where possible, and never carry Wayland/DRM handles.
//! - [`EventQueue`] is a fixed-capacity ring: `push` does not allocate.
//! - High-frequency pointer motion is coalesced in place (last sample wins).
//! - Overflow is explicit ([`PushResult`]) with counters; never blocks the producer.
//! - Phase order is fixed and branch-light; dispatch does not sort the ring.
//!
//! Bounded cross-thread channels feed a Tensor-owned **semantic** queue so
//! completion arrival order cannot rewrite policy.

mod coalesce;
mod event;
mod ids;
mod phase;
mod queue;

pub use coalesce::CoalesceStats;
pub use event::{
    Event, GpuTimeline, InputEvent, IpcCommandId, LaunchOutcome, OutputEvent, TimerId,
};
pub use ids::{OutputId, SurfaceId, ViewId};
pub use phase::{PHASES, Phase};
pub use queue::{EventQueue, PushResult, QueueStats};

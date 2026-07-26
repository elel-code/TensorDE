//! Value-only compositor events.
//!
//! Keep variants small (`Copy`) so the ring stores them inline. Heavy payloads
//! (IPC frames, launch env) stay behind IDs resolved by the compositor owner.

use crate::{
    ids::{OutputId, SurfaceId, ViewId},
    phase::Phase,
};

/// Logical input sample after device adapters normalize axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputEvent {
    /// Absolute pointer position in logical compositor coordinates.
    PointerMotion {
        x: f64,
        y: f64,
        /// Monotonic nanoseconds when the sample was taken (source clock).
        time_ns: u64,
    },
    PointerButton {
        button: u32,
        pressed: bool,
        time_ns: u64,
    },
    PointerAxis {
        horizontal: f64,
        vertical: f64,
        time_ns: u64,
    },
    Keyboard {
        key: u32,
        pressed: bool,
        time_ns: u64,
    },
}

/// Output / CRTC topology and present-related signals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputEvent {
    Connected(OutputId),
    Changed(OutputId),
    Disconnected(OutputId),
    VBlank { output: OutputId, sequence: u64 },
}

/// GPU-side readiness that must not own Vulkan handles on the bus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuTimeline {
    pub output: OutputId,
    pub value: u64,
}

/// Opaque IPC command identity (payload lives in the IPC owner).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IpcCommandId(pub u64);

/// Opaque timer identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TimerId(pub u64);

/// Value-only process launch result (mirrors existing spawn worker outcomes).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchOutcome {
    /// `pid` is the OS process id (never negative on Linux).
    Started {
        request: u64,
        pid: u32,
    },
    Failed {
        request: u64,
    },
}

/// Compositor event bus payload.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Input(InputEvent),
    Output(OutputEvent),
    SurfaceCommitted {
        surface: SurfaceId,
        view: Option<ViewId>,
    },
    Gpu(GpuTimeline),
    Ipc(IpcCommandId),
    Launch(LaunchOutcome),
    Timer(TimerId),
    /// Force a full workspace redraw intent (policy decides targets).
    RedrawAll,
    Shutdown,
}

impl Event {
    /// Dispatch phase for this event. Used for bucketed queues (no sort).
    #[inline]
    pub const fn phase(self) -> Phase {
        match self {
            Self::Input(_) => Phase::Input,
            Self::SurfaceCommitted { .. } => Phase::Protocol,
            Self::Output(OutputEvent::Connected(_))
            | Self::Output(OutputEvent::Changed(_))
            | Self::Output(OutputEvent::Disconnected(_)) => Phase::Session,
            Self::Output(OutputEvent::VBlank { .. }) => Phase::Present,
            Self::Gpu(_) => Phase::Gpu,
            Self::RedrawAll => Phase::Scene,
            Self::Ipc(_) | Self::Launch(_) | Self::Timer(_) => Phase::Control,
            Self::Shutdown => Phase::Shutdown,
        }
    }

    /// Whether two events can be replaced by the newer one in the same slot.
    #[inline]
    pub fn coalesces_with(self, newer: Self) -> bool {
        match (self, newer) {
            (
                Self::Input(InputEvent::PointerMotion { .. }),
                Self::Input(InputEvent::PointerMotion { .. }),
            ) => true,
            (
                Self::Output(OutputEvent::VBlank {
                    output: a,
                    sequence: _,
                }),
                Self::Output(OutputEvent::VBlank {
                    output: b,
                    sequence: _,
                }),
            ) => a == b,
            (
                Self::Gpu(GpuTimeline { output: a, .. }),
                Self::Gpu(GpuTimeline { output: b, .. }),
            ) => a == b,
            (Self::RedrawAll, Self::RedrawAll) => true,
            _ => false,
        }
    }
}

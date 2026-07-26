//! Ordered processing phases for one dispatch turn.
//!
//! Inspired by calloop's "process sources, then idle work" split, but made
//! explicit so frame scheduling cannot accidentally interleave with input.

/// Processing phase. Lower discriminant runs first when draining.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Phase {
    /// OS / adapter completions already turned into value events.
    Drain = 0,
    /// Pointer, keyboard, tablet, virtual devices.
    Input = 1,
    /// Surface commits, configure results, activation (value form).
    Protocol = 2,
    /// Output topology, session pause/resume signals.
    Session = 3,
    /// ECS / layout / scene invalidation intents.
    Scene = 4,
    /// GPU timeline / import readiness (not record itself).
    Gpu = 5,
    /// VBlank and present completion.
    Present = 6,
    /// IPC and other control-plane work (after interactive path).
    Control = 7,
    /// Ordered shutdown.
    Shutdown = 8,
}

/// All phases in dispatch order (stable for tests and schedulers).
pub const PHASES: [Phase; 9] = [
    Phase::Drain,
    Phase::Input,
    Phase::Protocol,
    Phase::Session,
    Phase::Scene,
    Phase::Gpu,
    Phase::Present,
    Phase::Control,
    Phase::Shutdown,
];

impl Phase {
    /// Index into per-phase ring buckets.
    #[inline]
    pub const fn index(self) -> usize {
        self as u8 as usize
    }

    /// Number of distinct phases.
    pub const COUNT: usize = 9;
}

use crate::{LogicalPosition, LogicalSize};

/// Runtime-local identifier for a currently advertised Wayland output.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutputId(u32);

impl OutputId {
    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }
}

/// Snapshot of the compositor metadata currently known for an output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputInfo {
    pub id: OutputId,
    pub name: Option<String>,
    pub description: Option<String>,
    pub make: String,
    pub model: String,
    pub logical_position: Option<LogicalPosition>,
    pub logical_size: Option<LogicalSize>,
    pub scale_factor: i32,
    /// Current mode vertical refresh in **millihertz** (`wl_output.mode.refresh`).
    ///
    /// Example: 60 Hz → `60_000`, 144 Hz → `144_000`. `None` if no current mode
    /// has been advertised yet. Useful for wallpaper / present pacing.
    pub refresh_mhz: Option<i32>,
}

impl OutputInfo {
    /// Refresh as hertz when known (`refresh_mhz / 1000` as `f64`).
    pub fn refresh_hz(&self) -> Option<f64> {
        self.refresh_mhz
            .filter(|&m| m > 0)
            .map(|m| f64::from(m) / 1000.0)
    }
}

/// Output hotplug or metadata change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutputEvent {
    Added(OutputInfo),
    Updated(OutputInfo),
    Removed(OutputId),
}

/// Power-management mode for an output that remains in the compositor space.
///
/// This is deliberately separate from output enablement: `Off` requests DPMS
/// power saving without removing the output from the desktop topology.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OutputPowerMode {
    Off,
    On,
}

/// `zwlr_output_power_v1` state and lifecycle events.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OutputPowerEvent {
    Mode {
        output: OutputId,
        mode: OutputPowerMode,
    },
    /// The compositor can no longer control this output's power state.
    Failed { output: OutputId },
}

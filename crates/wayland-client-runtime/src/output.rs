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
}

/// Output hotplug or metadata change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutputEvent {
    Added(OutputInfo),
    Updated(OutputInfo),
    Removed(OutputId),
}

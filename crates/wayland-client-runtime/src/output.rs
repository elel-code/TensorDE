use crate::{LogicalPosition, LogicalSize};

/// Runtime-local identifier for a currently advertised Wayland output.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutputId(u32);

impl OutputId {
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Construct from a compositor registry / native shell output name.
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

#[cfg(feature = "sctk")]
mod sctk_map {
    use super::*;
    use smithay_client_toolkit::output::{OutputInfo as SctkOutputInfo, OutputState};
    use wayland_client::protocol::wl_output::WlOutput;

    pub(crate) fn output_info(state: &OutputState, output: &WlOutput) -> Option<OutputInfo> {
        let info = state.info(output)?;
        Some(map_output_info(info))
    }

    fn map_output_info(info: SctkOutputInfo) -> OutputInfo {
        OutputInfo {
            id: OutputId(info.id),
            name: info.name,
            description: info.description,
            make: info.make,
            model: info.model,
            logical_position: info
                .logical_position
                .map(|(x, y)| LogicalPosition::new(x, y)),
            logical_size: info.logical_size.and_then(|(width, height)| {
                (width >= 0 && height >= 0).then(|| LogicalSize::new(width as u32, height as u32))
            }),
            scale_factor: info.scale_factor,
        }
    }
}

#[cfg(feature = "sctk")]
pub(crate) use sctk_map::output_info;

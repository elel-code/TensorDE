//! Connector and planned-output value snapshots.

use tensor_host::{ConnectorId, ConnectorState, PhysicalMode, SubpixelLayout};
use tensor_util::OutputScale;

/// Discovered connector state after a DRM scan (no FDs).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectorSnapshot {
    pub id: ConnectorId,
    pub name: String,
    pub state: ConnectorState,
    pub physical_size_mm: (i32, i32),
    pub subpixel: SubpixelLayout,
    pub modes: Vec<PhysicalMode>,
    pub preferred_mode: Option<PhysicalMode>,
    pub mapped_crtc: Option<u32>,
    /// Negotiated scanout fourcc+modifier key as opaque plane count + codes.
    /// Format details stay in the compositor/render layer until fully moved.
    pub has_native_format: bool,
}

/// Policy result: a connector that should be scanned out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputDescriptor {
    pub id: ConnectorId,
    pub name: String,
    pub physical_size_mm: (i32, i32),
    pub subpixel: SubpixelLayout,
    pub modes: Vec<PhysicalMode>,
    pub mode: PhysicalMode,
    pub crtc: u32,
    pub scale: OutputScale,
    pub position: Option<(i32, i32)>,
    pub has_native_format: bool,
}

use tensor_util::OutputScale;

/// Per-connector policy resolved at the configuration boundary.
///
/// The DRM backend remains responsible for discovering the supported mode
/// list. This value only expresses the user's stable intent, so a hotplug or
/// a new EDID never leaks parser-specific state into the output policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputRule {
    pub scale: Option<OutputScale>,
    pub mode: Option<OutputMode>,
    /// Logical origin of this output when set; otherwise automatic placement.
    pub position: Option<(i32, i32)>,
    /// When false the connector is ignored for scanout (still discovered).
    pub enabled: bool,
    /// Cap automatic mode selection to this refresh (millihertz). Explicit
    /// `mode` with `@refresh` still wins when that exact mode exists.
    pub max_refresh_millihertz: Option<u32>,
}

impl Default for OutputRule {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputRule {
    pub const fn new() -> Self {
        Self {
            scale: None,
            mode: None,
            position: None,
            enabled: true,
            max_refresh_millihertz: None,
        }
    }
}

/// A DRM mode requested by its visible dimensions and optional exact refresh.
/// Refresh is stored in millihertz, the same unit as [`tensor_host::PhysicalMode`],
/// which prevents a floating-point comparison at the configuration-to-DRM boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputMode {
    pub width: u32,
    pub height: u32,
    pub refresh_millihertz: Option<u32>,
}

impl OutputMode {
    pub const fn new(width: u32, height: u32, refresh_millihertz: Option<u32>) -> Self {
        Self {
            width,
            height,
            refresh_millihertz,
        }
    }
}

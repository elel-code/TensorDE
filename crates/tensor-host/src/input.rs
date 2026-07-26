//! Normalized input samples (device adapters fill these; policy consumes them).

/// Absolute pointer motion in logical compositor coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerMotion {
    pub x: f64,
    pub y: f64,
    pub time_ns: u64,
}

/// Pointer button press/release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerButton {
    pub button: u32,
    pub pressed: bool,
    pub time_ns: u64,
}

/// Scroll / axis sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerAxis {
    pub horizontal: f64,
    pub vertical: f64,
    pub time_ns: u64,
    pub source: AxisSource,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AxisSource {
    #[default]
    Unknown,
    Wheel,
    Finger,
    Continuous,
    WheelTilt,
}

/// Keyboard key state after keymap translation is owned elsewhere.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyState {
    pub key: u32,
    pub pressed: bool,
    pub time_ns: u64,
}

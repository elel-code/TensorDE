//! Compact value-only tablet tool and pad events.

use crate::{DeviceId, TabletToolId, TimeNs};

/// Physical tool kind reported by libinput.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum TabletToolType {
    Pen,
    Eraser,
    Brush,
    Pencil,
    Airbrush,
    Mouse,
    Lens,
}

/// Extra axes implemented by a tool, stored as one compact bitset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct TabletToolCapabilities(u8);

impl TabletToolCapabilities {
    pub const TILT: u8 = 1 << 0;
    pub const PRESSURE: u8 = 1 << 1;
    pub const DISTANCE: u8 = 1 << 2;
    pub const ROTATION: u8 = 1 << 3;
    pub const SLIDER: u8 = 1 << 4;
    pub const WHEEL: u8 = 1 << 5;

    #[inline]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 0x3f)
    }

    #[inline]
    pub const fn contains(self, capability: u8) -> bool {
        self.0 & capability != 0
    }
}

/// Static tool information sent once before the first proximity frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabletToolDescriptor {
    pub id: TabletToolId,
    pub device: DeviceId,
    pub hardware_serial: u64,
    pub hardware_id: u64,
    pub tool_type: TabletToolType,
    pub capabilities: TabletToolCapabilities,
}

/// A tool entering or leaving detectable range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabletToolProximityEvent {
    pub id: TabletToolId,
    pub device: DeviceId,
    pub x: f32,
    pub y: f32,
    pub in_proximity: bool,
    pub time_ns: TimeNs,
}

const AXIS_X: u16 = 1 << 0;
const AXIS_Y: u16 = 1 << 1;
const AXIS_PRESSURE: u16 = 1 << 2;
const AXIS_DISTANCE: u16 = 1 << 3;
const AXIS_TILT_X: u16 = 1 << 4;
const AXIS_TILT_Y: u16 = 1 << 5;
const AXIS_ROTATION: u16 = 1 << 6;
const AXIS_SLIDER: u16 = 1 << 7;
const AXIS_WHEEL: u16 = 1 << 8;
const FINAL_FRAME: u16 = 1 << 9;

/// All optional axes belonging to one libinput hardware frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabletToolAxesEvent {
    pub id: TabletToolId,
    pub time_ns: TimeNs,
    x: f32,
    y: f32,
    pressure: f32,
    distance: f32,
    tilt_x: f32,
    tilt_y: f32,
    rotation: f32,
    slider: f32,
    wheel_degrees: f32,
    wheel_clicks: i16,
    present: u16,
}

impl TabletToolAxesEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: TabletToolId,
        time_ns: TimeNs,
        x: Option<f32>,
        y: Option<f32>,
        pressure: Option<f32>,
        distance: Option<f32>,
        tilt_x: Option<f32>,
        tilt_y: Option<f32>,
        rotation: Option<f32>,
        slider: Option<f32>,
        wheel: Option<(f32, i16)>,
        final_frame: bool,
    ) -> Self {
        let mut present = 0;
        for (value, flag) in [
            (x, AXIS_X),
            (y, AXIS_Y),
            (pressure, AXIS_PRESSURE),
            (distance, AXIS_DISTANCE),
            (tilt_x, AXIS_TILT_X),
            (tilt_y, AXIS_TILT_Y),
            (rotation, AXIS_ROTATION),
            (slider, AXIS_SLIDER),
        ] {
            if value.is_some() {
                present |= flag;
            }
        }
        if wheel.is_some() {
            present |= AXIS_WHEEL;
        }
        if final_frame {
            present |= FINAL_FRAME;
        }
        Self {
            id,
            time_ns,
            x: x.unwrap_or_default(),
            y: y.unwrap_or_default(),
            pressure: pressure.unwrap_or_default(),
            distance: distance.unwrap_or_default(),
            tilt_x: tilt_x.unwrap_or_default(),
            tilt_y: tilt_y.unwrap_or_default(),
            rotation: rotation.unwrap_or_default(),
            slider: slider.unwrap_or_default(),
            wheel_degrees: wheel.map_or(0.0, |value| value.0),
            wheel_clicks: wheel.map_or(0, |value| value.1),
            present,
        }
    }

    #[inline]
    pub const fn has_axes(self) -> bool {
        self.present & (FINAL_FRAME - 1) != 0
    }

    #[inline]
    pub const fn final_frame(self) -> bool {
        self.present & FINAL_FRAME != 0
    }

    #[inline]
    pub const fn x(self) -> Option<f32> {
        optional_axis(self.present, AXIS_X, self.x)
    }

    #[inline]
    pub const fn y(self) -> Option<f32> {
        optional_axis(self.present, AXIS_Y, self.y)
    }

    #[inline]
    pub const fn pressure(self) -> Option<f32> {
        optional_axis(self.present, AXIS_PRESSURE, self.pressure)
    }

    #[inline]
    pub const fn distance(self) -> Option<f32> {
        optional_axis(self.present, AXIS_DISTANCE, self.distance)
    }

    #[inline]
    pub const fn tilt(self) -> Option<(f32, f32)> {
        if self.present & (AXIS_TILT_X | AXIS_TILT_Y) != 0 {
            Some((self.tilt_x, self.tilt_y))
        } else {
            None
        }
    }

    #[inline]
    pub const fn rotation(self) -> Option<f32> {
        optional_axis(self.present, AXIS_ROTATION, self.rotation)
    }

    #[inline]
    pub const fn slider(self) -> Option<f32> {
        optional_axis(self.present, AXIS_SLIDER, self.slider)
    }

    #[inline]
    pub const fn wheel(self) -> Option<(f32, i16)> {
        if self.present & AXIS_WHEEL != 0 {
            Some((self.wheel_degrees, self.wheel_clicks))
        } else {
            None
        }
    }
}

const fn optional_axis(present: u16, flag: u16, value: f32) -> Option<f32> {
    if present & flag != 0 {
        Some(value)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabletToolTipEvent {
    pub id: TabletToolId,
    pub down: bool,
    pub time_ns: TimeNs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabletToolButtonEvent {
    pub id: TabletToolId,
    pub button: u32,
    pub pressed: bool,
    pub time_ns: TimeNs,
}

/// Fixed-size pad topology header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabletPadDescriptor {
    pub device: DeviceId,
    pub buttons: u8,
    pub rings: u8,
    pub strips: u8,
    pub dials: u8,
    pub groups: u8,
}

/// Membership and mode metadata for one pad group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabletPadGroupDescriptor {
    pub device: DeviceId,
    pub index: u8,
    pub modes: u8,
    pub current_mode: u8,
    pub buttons: u64,
    pub rings: u16,
    pub strips: u16,
    pub dials: u16,
    pub final_group: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabletPadRingEvent {
    pub device: DeviceId,
    pub index: u8,
    pub mode_group: u8,
    pub mode: u8,
    pub position: Option<f32>,
    pub finger: bool,
    pub time_ns: TimeNs,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabletPadStripEvent {
    pub device: DeviceId,
    pub index: u8,
    pub mode_group: u8,
    pub mode: u8,
    pub position: Option<f32>,
    pub finger: bool,
    pub time_ns: TimeNs,
}

/// Pad topology and hardware updates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TabletPadEvent {
    Added(TabletPadDescriptor),
    Group(TabletPadGroupDescriptor),
    Button {
        device: DeviceId,
        button: u8,
        mode_group: u8,
        mode: u8,
        pressed: bool,
        time_ns: TimeNs,
    },
    Ring(TabletPadRingEvent),
    Strip(TabletPadStripEvent),
    Dial {
        device: DeviceId,
        index: u8,
        mode_group: u8,
        mode: u8,
        delta_v120: i32,
        time_ns: TimeNs,
    },
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::*;

    #[test]
    fn axes_preserve_presence_in_one_cache_line() {
        let event = TabletToolAxesEvent::new(
            TabletToolId::new(1),
            2,
            Some(0.0),
            None,
            Some(0.5),
            None,
            Some(-3.0),
            None,
            None,
            None,
            Some((1.25, -1)),
            true,
        );
        assert_eq!(event.x(), Some(0.0));
        assert_eq!(event.y(), None);
        assert_eq!(event.pressure(), Some(0.5));
        assert_eq!(event.tilt(), Some((-3.0, 0.0)));
        assert_eq!(event.wheel(), Some((1.25, -1)));
        assert!(event.final_frame());
        assert!(size_of::<TabletToolAxesEvent>() <= 56);
    }
}

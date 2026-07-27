//! Value events produced by an OS input adapter before seat dispatch.

use tensor_host::AxisSource;

use crate::{DeviceCapabilities, DeviceId, Sample, TimeNs};

/// A physical input device entering or leaving the active seat.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceChange {
    Added,
    Removed,
}

/// Allocation-free device identity and capability update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceEvent {
    pub id: DeviceId,
    pub capabilities: DeviceCapabilities,
    pub change: DeviceChange,
}

/// Keyboard key in Linux evdev numbering (before the XKB offset).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyboardEvent {
    pub key: u32,
    pub pressed: bool,
    pub time_ns: TimeNs,
}

impl KeyboardEvent {
    #[inline]
    pub const fn time_msec(self) -> u32 {
        (self.time_ns / 1_000_000) as u32
    }

    #[inline]
    pub fn sample(self) -> Sample {
        Sample::key(self.key, self.pressed, self.time_ns)
    }
}

/// Accelerated and raw relative pointer deltas from one device sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RelativeMotionEvent {
    pub delta_x: f64,
    pub delta_y: f64,
    pub unaccelerated_x: f64,
    pub unaccelerated_y: f64,
    pub time_ns: TimeNs,
}

impl RelativeMotionEvent {
    #[inline]
    pub const fn time_msec(self) -> u32 {
        (self.time_ns / 1_000_000) as u32
    }
}

/// Absolute pointer position normalized to the device's full [0, 1] range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbsoluteMotionEvent {
    pub x: f64,
    pub y: f64,
    pub time_ns: TimeNs,
}

impl AbsoluteMotionEvent {
    #[inline]
    pub const fn time_msec(self) -> u32 {
        (self.time_ns / 1_000_000) as u32
    }
}

/// Pointer button in Linux input-event numbering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerButtonEvent {
    pub button: u32,
    pub pressed: bool,
    pub time_ns: TimeNs,
}

impl PointerButtonEvent {
    #[inline]
    pub const fn time_msec(self) -> u32 {
        (self.time_ns / 1_000_000) as u32
    }

    #[inline]
    pub fn sample(self) -> Sample {
        Sample::pointer_button(self.button, self.pressed, self.time_ns)
    }
}

/// Physical direction associated with a scroll axis.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AxisDirection {
    #[default]
    Identical,
    Inverted,
}

const HORIZONTAL_AMOUNT: u8 = 1 << 0;
const VERTICAL_AMOUNT: u8 = 1 << 1;
const HORIZONTAL_V120: u8 = 1 << 2;
const VERTICAL_V120: u8 = 1 << 3;
const HORIZONTAL_STOP: u8 = 1 << 4;
const VERTICAL_STOP: u8 = 1 << 5;

/// One complete scroll frame. Presence bits preserve optional values and
/// explicit stops without adding discriminants to the inline event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerAxisEvent {
    horizontal: f64,
    vertical: f64,
    horizontal_v120: i32,
    vertical_v120: i32,
    pub time_ns: TimeNs,
    pub source: AxisSource,
    pub horizontal_direction: AxisDirection,
    pub vertical_direction: AxisDirection,
    present: u8,
}

impl PointerAxisEvent {
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn new(
        horizontal: Option<f64>,
        vertical: Option<f64>,
        horizontal_v120: Option<i32>,
        vertical_v120: Option<i32>,
        time_ns: TimeNs,
        source: AxisSource,
        horizontal_direction: AxisDirection,
        vertical_direction: AxisDirection,
    ) -> Self {
        let mut present = 0;
        if horizontal.is_some() {
            present |= HORIZONTAL_AMOUNT;
        }
        if vertical.is_some() {
            present |= VERTICAL_AMOUNT;
        }
        if horizontal_v120.is_some() {
            present |= HORIZONTAL_V120;
        }
        if vertical_v120.is_some() {
            present |= VERTICAL_V120;
        }
        Self {
            horizontal: horizontal.unwrap_or_default(),
            vertical: vertical.unwrap_or_default(),
            horizontal_v120: horizontal_v120.unwrap_or_default(),
            vertical_v120: vertical_v120.unwrap_or_default(),
            time_ns,
            source,
            horizontal_direction,
            vertical_direction,
            present,
        }
    }

    /// Attach explicit end-of-scroll markers without changing the inline
    /// event layout. Stops are independent of zero-valued axis samples.
    #[inline]
    pub const fn with_stops(mut self, horizontal: bool, vertical: bool) -> Self {
        if horizontal {
            self.present |= HORIZONTAL_STOP;
        }
        if vertical {
            self.present |= VERTICAL_STOP;
        }
        self
    }

    #[inline]
    pub const fn horizontal(self) -> Option<f64> {
        if self.present & HORIZONTAL_AMOUNT != 0 {
            Some(self.horizontal)
        } else {
            None
        }
    }

    #[inline]
    pub const fn vertical(self) -> Option<f64> {
        if self.present & VERTICAL_AMOUNT != 0 {
            Some(self.vertical)
        } else {
            None
        }
    }

    #[inline]
    pub const fn horizontal_v120(self) -> Option<i32> {
        if self.present & HORIZONTAL_V120 != 0 {
            Some(self.horizontal_v120)
        } else {
            None
        }
    }

    #[inline]
    pub const fn vertical_v120(self) -> Option<i32> {
        if self.present & VERTICAL_V120 != 0 {
            Some(self.vertical_v120)
        } else {
            None
        }
    }

    #[inline]
    pub const fn horizontal_stopped(self) -> bool {
        self.present & HORIZONTAL_STOP != 0
    }

    #[inline]
    pub const fn vertical_stopped(self) -> bool {
        self.present & VERTICAL_STOP != 0
    }

    #[inline]
    pub const fn time_msec(self) -> u32 {
        (self.time_ns / 1_000_000) as u32
    }

    #[inline]
    pub fn sample(self) -> Option<Sample> {
        let horizontal = self.horizontal;
        let vertical = self.vertical;
        if horizontal == 0.0 && vertical == 0.0 {
            None
        } else {
            Some(Sample::pointer_axis(
                horizontal,
                vertical,
                self.time_ns,
                self.source,
            ))
        }
    }
}

/// Standard input event crossing from a device adapter into compositor policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackendInputEvent {
    Keyboard(KeyboardEvent),
    PointerMotion(RelativeMotionEvent),
    PointerMotionAbsolute(AbsoluteMotionEvent),
    PointerButton(PointerButtonEvent),
    PointerAxis(PointerAxisEvent),
    /// Activity from an event whose protocol routing is not implemented yet.
    Activity,
}

impl BackendInputEvent {
    #[inline]
    pub const fn is_activity(self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::*;

    #[test]
    fn zero_axis_stop_keeps_presence_without_becoming_a_bus_sample() {
        let event = PointerAxisEvent::new(
            None,
            Some(0.0),
            None,
            None,
            4_000_000,
            AxisSource::Finger,
            AxisDirection::Identical,
            AxisDirection::Inverted,
        )
        .with_stops(false, true);

        assert_eq!(event.vertical(), Some(0.0));
        assert_eq!(event.horizontal(), None);
        assert!(!event.horizontal_stopped());
        assert!(event.vertical_stopped());
        assert_eq!(event.time_msec(), 4);
        assert_eq!(event.sample(), None);
    }

    #[test]
    fn backend_events_remain_small_inline_values() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<BackendInputEvent>();
        assert!(size_of::<BackendInputEvent>() <= 64);
    }
}

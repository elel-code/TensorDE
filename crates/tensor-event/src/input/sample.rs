//! Normalized input samples for the event bus.

use crate::{Event, InputEvent};

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

/// Monotonic nanoseconds from the device clock (adapter-normalized).
pub type TimeNs = u64;

/// Linux key code (not keysym).
pub type KeyCode = u32;

/// Linux button code (`BTN_*`).
pub type ButtonCode = u32;

/// One input sample after device axes are normalized to compositor logical space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sample {
    PointerMotion(PointerMotion),
    PointerButton(PointerButton),
    PointerAxis(PointerAxis),
    Key(KeyState),
}

impl Sample {
    #[inline]
    pub fn pointer_motion(x: f64, y: f64, time_ns: TimeNs) -> Self {
        Self::PointerMotion(PointerMotion { x, y, time_ns })
    }

    #[inline]
    pub fn pointer_button(button: ButtonCode, pressed: bool, time_ns: TimeNs) -> Self {
        Self::PointerButton(PointerButton {
            button,
            pressed,
            time_ns,
        })
    }

    #[inline]
    pub fn pointer_axis(
        horizontal: f64,
        vertical: f64,
        time_ns: TimeNs,
        source: AxisSource,
    ) -> Self {
        Self::PointerAxis(PointerAxis {
            horizontal,
            vertical,
            time_ns,
            source,
        })
    }

    #[inline]
    pub fn key(key: KeyCode, pressed: bool, time_ns: TimeNs) -> Self {
        Self::Key(KeyState {
            key,
            pressed,
            time_ns,
        })
    }

    /// Convert to a bus [`Event`] (always `Phase::Input`).
    #[inline]
    pub fn into_event(self) -> Event {
        Event::Input(match self {
            Self::PointerMotion(m) => InputEvent::PointerMotion {
                x: m.x,
                y: m.y,
                time_ns: m.time_ns,
            },
            Self::PointerButton(b) => InputEvent::PointerButton {
                button: b.button,
                pressed: b.pressed,
                time_ns: b.time_ns,
            },
            Self::PointerAxis(a) => InputEvent::PointerAxis {
                horizontal: a.horizontal,
                vertical: a.vertical,
                time_ns: a.time_ns,
            },
            Self::Key(k) => InputEvent::Keyboard {
                key: k.key,
                pressed: k.pressed,
                time_ns: k.time_ns,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Phase;

    #[test]
    fn samples_map_to_input_phase() {
        let e = Sample::pointer_motion(1.0, 2.0, 10).into_event();
        assert_eq!(e.phase(), Phase::Input);
    }

    #[test]
    fn motion_event_preserves_coordinates() {
        let e = Sample::pointer_motion(10.5, 20.25, 99).into_event();
        match e {
            Event::Input(InputEvent::PointerMotion { x, y, time_ns }) => {
                assert_eq!((x, y, time_ns), (10.5, 20.25, 99));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

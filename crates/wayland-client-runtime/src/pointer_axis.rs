use wayland_client::protocol::wl_pointer;

/// Physical source of a pointer axis frame.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PointerAxisSource {
    Wheel,
    Finger,
    Continuous,
    WheelTilt,
}

/// Relationship between physical motion and the compositor's axis direction.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PointerAxisDirection {
    Identical,
    Inverted,
}

/// All values accumulated for one axis in a `wl_pointer.frame`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointerAxisValue {
    /// Continuous compositor-coordinate delta using Wayland's sign convention.
    pub continuous: f64,
    /// Deprecated whole-step delta supplied by older compositors.
    pub discrete: i32,
    /// High-resolution wheel delta; 120 units represent one logical step.
    pub value120: i32,
    /// Physical direction relative to the compositor axis direction.
    pub relative_direction: Option<PointerAxisDirection>,
    /// The device reported that continuous scrolling stopped on this axis.
    pub stopped: bool,
}

impl PointerAxisValue {
    /// Preferred logical step delta for wheel-style consumers.
    pub fn logical_steps(self) -> Option<f64> {
        if self.value120 != 0 {
            Some(f64::from(self.value120) / 120.0)
        } else if self.discrete != 0 {
            Some(f64::from(self.discrete))
        } else {
            None
        }
    }

    pub fn has_motion(self) -> bool {
        self.continuous != 0.0 || self.discrete != 0 || self.value120 != 0
    }
}


pub(crate) fn map_axis_source(source: wl_pointer::AxisSource) -> Option<PointerAxisSource> {
    match source {
        wl_pointer::AxisSource::Wheel => Some(PointerAxisSource::Wheel),
        wl_pointer::AxisSource::Finger => Some(PointerAxisSource::Finger),
        wl_pointer::AxisSource::Continuous => Some(PointerAxisSource::Continuous),
        wl_pointer::AxisSource::WheelTilt => Some(PointerAxisSource::WheelTilt),
        _ => None,
    }
}

pub(crate) fn map_axis_direction(
    direction: wl_pointer::AxisRelativeDirection,
) -> Option<PointerAxisDirection> {
    match direction {
        wl_pointer::AxisRelativeDirection::Identical => Some(PointerAxisDirection::Identical),
        wl_pointer::AxisRelativeDirection::Inverted => Some(PointerAxisDirection::Inverted),
        _ => None,
    }
}

/// Per-seat (or shell-wide) accumulation of axis events until `wl_pointer.frame`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PointerAxisFrameAccum {
    pub horizontal: PointerAxisValue,
    pub vertical: PointerAxisValue,
    pub source: Option<PointerAxisSource>,
}

impl PointerAxisFrameAccum {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn has_content(self) -> bool {
        self.horizontal.has_motion()
            || self.vertical.has_motion()
            || self.horizontal.stopped
            || self.vertical.stopped
            || self.source.is_some()
    }

    pub fn add_continuous(&mut self, vertical: bool, value: f64) {
        if vertical {
            self.vertical.continuous += value;
        } else {
            self.horizontal.continuous += value;
        }
    }

    pub fn add_discrete(&mut self, vertical: bool, value: i32) {
        if vertical {
            self.vertical.discrete = self.vertical.discrete.saturating_add(value);
        } else {
            self.horizontal.discrete = self.horizontal.discrete.saturating_add(value);
        }
    }

    pub fn add_value120(&mut self, vertical: bool, value: i32) {
        if vertical {
            self.vertical.value120 = self.vertical.value120.saturating_add(value);
        } else {
            self.horizontal.value120 = self.horizontal.value120.saturating_add(value);
        }
    }

    pub fn set_stop(&mut self, vertical: bool) {
        if vertical {
            self.vertical.stopped = true;
        } else {
            self.horizontal.stopped = true;
        }
    }

    pub fn set_relative_direction(&mut self, vertical: bool, direction: PointerAxisDirection) {
        if vertical {
            self.vertical.relative_direction = Some(direction);
        } else {
            self.horizontal.relative_direction = Some(direction);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value120_preserves_partial_high_resolution_steps() {
        let value = PointerAxisValue {
            value120: 30,
            ..Default::default()
        };
        assert_eq!(value.logical_steps(), Some(0.25));
    }

    #[test]
    fn deprecated_discrete_steps_remain_a_fallback() {
        let value = PointerAxisValue {
            discrete: 2,
            ..Default::default()
        };
        assert_eq!(value.logical_steps(), Some(2.0));
    }

    #[test]
    fn continuous_and_stop_only_frames_are_not_fabricated_as_steps() {
        let value = PointerAxisValue {
            continuous: 1.5,
            stopped: true,
            ..Default::default()
        };
        assert_eq!(value.logical_steps(), None);
        assert!(value.has_motion());
    }

    #[test]
    fn sctk_axis_fields_and_direction_are_preserved() {
        // Pure mapping without AxisScroll when sctk off.
        let value = PointerAxisValue {
            continuous: -3.0,
            discrete: 1,
            value120: 120,
            relative_direction: Some(PointerAxisDirection::Inverted),
            stopped: false,
        };
        assert_eq!(value.logical_steps(), Some(1.0));
        assert_eq!(value.relative_direction, Some(PointerAxisDirection::Inverted));
    }

    #[test]
    fn frame_accum_merges_axis_events_until_clear() {
        let mut frame = PointerAxisFrameAccum::default();
        frame.add_continuous(true, 12.0);
        frame.add_value120(true, 60);
        frame.add_discrete(false, 1);
        frame.set_stop(true);
        frame.source = Some(PointerAxisSource::Wheel);
        frame.set_relative_direction(false, PointerAxisDirection::Inverted);

        assert!(frame.has_content());
        assert_eq!(frame.vertical.continuous, 12.0);
        assert_eq!(frame.vertical.value120, 60);
        assert!(frame.vertical.stopped);
        assert_eq!(frame.horizontal.discrete, 1);
        assert_eq!(
            frame.horizontal.relative_direction,
            Some(PointerAxisDirection::Inverted)
        );
        assert_eq!(frame.source, Some(PointerAxisSource::Wheel));

        frame.clear();
        assert!(!frame.has_content());
    }
}

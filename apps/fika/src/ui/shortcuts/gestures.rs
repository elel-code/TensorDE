use super::{PathNavigationAction, ZoomAction};

pub(crate) fn zoom_action_for_scroll_delta(delta_y: f32) -> Option<ZoomAction> {
    if delta_y < -f32::EPSILON {
        Some(ZoomAction::In)
    } else if delta_y > f32::EPSILON {
        Some(ZoomAction::Out)
    } else {
        None
    }
}

/// Dolphin's `KItemListController::pinchTriggered()` sensitivity modifier.
const PINCH_ZOOM_SENSITIVITY: f64 = 0.2;

/// Adapts Wayland's gesture-total scale to Dolphin's per-update pinch counter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PinchZoomTracker {
    last_scale: f64,
    counter: f64,
}

impl PinchZoomTracker {
    pub(crate) fn begin() -> Self {
        Self {
            last_scale: 1.0,
            counter: 0.0,
        }
    }

    /// Consume an absolute Wayland pinch scale update.
    ///
    /// Qt supplies Dolphin with a per-update `scaleFactor()`, while Wayland's
    /// protocol scale is cumulative from gesture begin. Dividing consecutive
    /// totals produces the factor Dolphin adds as `scaleFactor() - 1`. Dolphin
    /// resets the accumulator after crossing ±0.2 and emits at most one step
    /// for each update, which also prevents one coalesced event from jumping
    /// across several icon sizes.
    pub(crate) fn update(&mut self, scale: f64) -> Vec<ZoomAction> {
        if !scale.is_finite() || scale <= 0.0 {
            return Vec::new();
        }
        let factor = scale / self.last_scale;
        self.last_scale = scale;
        self.counter += factor - 1.0;
        if self.counter >= PINCH_ZOOM_SENSITIVITY {
            self.counter = 0.0;
            vec![ZoomAction::In]
        } else if self.counter <= -PINCH_ZOOM_SENSITIVITY {
            self.counter = 0.0;
            vec![ZoomAction::Out]
        } else {
            Vec::new()
        }
    }
}

/// Minimum finger count for swipe → history navigation (2-finger is scroll).
const SWIPE_NAV_MIN_FINGERS: u32 = 3;

/// Minimum absolute horizontal travel (logical surface units) to commit navigation.
const SWIPE_NAV_DISTANCE: f64 = 96.0;

/// Accumulates a multi-finger swipe and resolves back/forward on end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SwipeNavigationTracker {
    fingers: u32,
    total_dx: f64,
    total_dy: f64,
}

impl SwipeNavigationTracker {
    pub(crate) fn begin(fingers: u32) -> Self {
        Self {
            fingers,
            total_dx: 0.0,
            total_dy: 0.0,
        }
    }

    pub(crate) fn update(&mut self, delta_x: f64, delta_y: f64) {
        if delta_x.is_finite() {
            self.total_dx += delta_x;
        }
        if delta_y.is_finite() {
            self.total_dy += delta_y;
        }
    }

    /// Resolve navigation when the gesture ends.
    ///
    /// Browser-style: swipe right (positive dx) → Back, swipe left → Forward.
    /// Requires enough horizontal travel, horizontal dominance, and ≥3 fingers.
    pub(crate) fn finish(self, cancelled: bool) -> Option<PathNavigationAction> {
        if cancelled || self.fingers < SWIPE_NAV_MIN_FINGERS {
            return None;
        }
        if self.total_dx.abs() < SWIPE_NAV_DISTANCE {
            return None;
        }
        if self.total_dx.abs() <= self.total_dy.abs() {
            return None;
        }
        if self.total_dx > 0.0 {
            Some(PathNavigationAction::Back)
        } else {
            Some(PathNavigationAction::Forward)
        }
    }
}

#[cfg(test)]
mod pinch_zoom_tests {
    use super::*;

    #[test]
    fn pinch_open_emits_zoom_in_steps() {
        let mut tracker = PinchZoomTracker::begin();
        assert!(tracker.update(1.11).is_empty());
        assert_eq!(tracker.update(1.2321), vec![ZoomAction::In]);
        assert!(tracker.update(1.367631).is_empty());
        assert_eq!(tracker.update(1.51807041), vec![ZoomAction::In]);
    }

    #[test]
    fn pinch_close_emits_zoom_out_steps() {
        let mut tracker = PinchZoomTracker::begin();
        assert!(tracker.update(0.89).is_empty());
        assert_eq!(tracker.update(0.7921), vec![ZoomAction::Out]);
        assert!(tracker.update(0.704969).is_empty());
        assert_eq!(tracker.update(0.62742241), vec![ZoomAction::Out]);
    }

    #[test]
    fn pinch_ignores_non_finite_scale() {
        let mut tracker = PinchZoomTracker::begin();
        assert!(tracker.update(f64::NAN).is_empty());
        assert!(tracker.update(0.0).is_empty());
        assert!(tracker.update(-1.0).is_empty());
    }

    #[test]
    fn coalesced_pinch_update_emits_at_most_one_step_like_dolphin() {
        let mut tracker = PinchZoomTracker::begin();
        assert_eq!(tracker.update(1.8), vec![ZoomAction::In]);
        assert!(tracker.update(1.8).is_empty());
    }

    #[test]
    fn pinch_counter_cancels_opposite_incremental_motion() {
        let mut tracker = PinchZoomTracker::begin();
        assert!(tracker.update(1.15).is_empty());
        assert!(tracker.update(1.0).is_empty());
        assert_eq!(tracker.update(0.75), vec![ZoomAction::Out]);
    }
}

#[cfg(test)]
mod swipe_nav_tests {
    use super::*;

    #[test]
    fn three_finger_swipe_right_goes_back() {
        let mut tracker = SwipeNavigationTracker::begin(3);
        tracker.update(50.0, 5.0);
        tracker.update(60.0, -2.0);
        assert_eq!(tracker.finish(false), Some(PathNavigationAction::Back));
    }

    #[test]
    fn three_finger_swipe_left_goes_forward() {
        let mut tracker = SwipeNavigationTracker::begin(3);
        tracker.update(-120.0, 10.0);
        assert_eq!(tracker.finish(false), Some(PathNavigationAction::Forward));
    }

    #[test]
    fn two_finger_swipe_is_ignored() {
        let mut tracker = SwipeNavigationTracker::begin(2);
        tracker.update(200.0, 0.0);
        assert_eq!(tracker.finish(false), None);
    }

    #[test]
    fn vertical_dominant_swipe_is_ignored() {
        let mut tracker = SwipeNavigationTracker::begin(3);
        tracker.update(50.0, 200.0);
        assert_eq!(tracker.finish(false), None);
    }

    #[test]
    fn cancelled_swipe_is_ignored() {
        let mut tracker = SwipeNavigationTracker::begin(3);
        tracker.update(200.0, 0.0);
        assert_eq!(tracker.finish(true), None);
    }

    #[test]
    fn short_horizontal_swipe_is_ignored() {
        let mut tracker = SwipeNavigationTracker::begin(3);
        tracker.update(40.0, 0.0);
        assert_eq!(tracker.finish(false), None);
    }
}

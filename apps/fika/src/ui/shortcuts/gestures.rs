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

/// Multiplicative finger-spacing change required for one icon zoom step.
///
/// Absolute pinch `scale` is relative to spacing at gesture begin (~1.0). About
/// 12% spacing change maps to one FileManager-style zoom step so continuous pinch
/// does not jump multiple levels on tiny motion.
const PINCH_ZOOM_STEP_RATIO: f64 = 1.12;

/// Tracks absolute pinch scale and emits discrete [`ZoomAction`] steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PinchZoomTracker {
    /// Scale at which the last zoom step was applied (starts at 1.0 on begin).
    last_applied_scale: f64,
}

impl PinchZoomTracker {
    pub(crate) fn begin() -> Self {
        Self {
            last_applied_scale: 1.0,
        }
    }

    /// Consume an absolute pinch scale update; may emit zero or more zoom steps.
    pub(crate) fn update(&mut self, scale: f64) -> Vec<ZoomAction> {
        if !scale.is_finite() || scale <= 0.0 {
            return Vec::new();
        }
        let mut actions = Vec::new();
        let step = PINCH_ZOOM_STEP_RATIO;
        // Fingers farther apart (scale grows) → zoom in; pinch together → out.
        while scale / self.last_applied_scale >= step {
            self.last_applied_scale *= step;
            actions.push(ZoomAction::In);
        }
        while self.last_applied_scale / scale >= step {
            self.last_applied_scale /= step;
            actions.push(ZoomAction::Out);
        }
        actions
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
        assert!(tracker.update(1.05).is_empty());
        assert_eq!(tracker.update(1.12), vec![ZoomAction::In]);
        assert_eq!(tracker.update(1.12 * 1.12), vec![ZoomAction::In]);
    }

    #[test]
    fn pinch_close_emits_zoom_out_steps() {
        let mut tracker = PinchZoomTracker::begin();
        assert_eq!(tracker.update(1.0 / 1.12), vec![ZoomAction::Out]);
        assert_eq!(tracker.update(1.0 / (1.12 * 1.12)), vec![ZoomAction::Out]);
    }

    #[test]
    fn pinch_ignores_non_finite_scale() {
        let mut tracker = PinchZoomTracker::begin();
        assert!(tracker.update(f64::NAN).is_empty());
        assert!(tracker.update(0.0).is_empty());
        assert!(tracker.update(-1.0).is_empty());
    }

    #[test]
    fn large_pinch_jump_emits_multiple_steps() {
        let mut tracker = PinchZoomTracker::begin();
        // 1.12^3 ≈ 1.405; one large jump should yield three zoom-in steps.
        assert_eq!(
            tracker.update(PINCH_ZOOM_STEP_RATIO.powi(3)),
            vec![ZoomAction::In, ZoomAction::In, ZoomAction::In]
        );
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

use crate::windowing::{ActiveEventLoop, MouseScrollDelta, PinchGesture, SwipeGesture};

use super::outcome::ShellActionOutcome;
use crate::ui::animation::ShellAnimationKind;
use crate::ui::shortcuts::{
    PinchZoomTracker, SwipeNavigationTracker, zoom_action_for_scroll_delta,
};
use crate::{FikaApp, SCROLL_REDRAW_FRAMES, scroll_delta_xy};

impl FikaApp {
    pub(crate) fn handle_main_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let (delta_x, delta_y) = scroll_delta_xy(delta, self.scene.ui_scale());
        let shortcut = self.modifiers.state().control_key() || self.modifiers.state().meta_key();
        if shortcut {
            // Zoom follows vertical wheel; horizontal-only gestures do not zoom.
            if let Some(zoom_action) = zoom_action_for_scroll_delta(delta_y)
                && self.set_user_zoom(zoom_action, size)
            {
                self.apply_window_action_outcome(ShellActionOutcome::queue_animation(
                    ShellAnimationKind::ZoomSettle,
                ));
            }
            return;
        }
        if self.scene.scroll_by_delta(delta_x, delta_y, size) {
            self.apply_window_action_outcome(ShellActionOutcome::Queue {
                reason: "wheel-scroll",
                redraw_frames: SCROLL_REDRAW_FRAMES,
            });
        }
    }

    pub(crate) fn handle_main_pinch_gesture(&mut self, gesture: PinchGesture) {
        match gesture {
            PinchGesture::Begin => {
                self.pinch_zoom = Some(PinchZoomTracker::begin());
            }
            PinchGesture::Update { scale } => {
                // Missed begin (rare): start a session at scale 1.0 and apply this update.
                let tracker = self.pinch_zoom.get_or_insert_with(PinchZoomTracker::begin);
                let actions = tracker.update(scale);
                if actions.is_empty() {
                    return;
                }
                let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
                    return;
                };
                let mut changed = false;
                for action in actions {
                    changed |= self.set_user_zoom(action, size);
                }
                if changed {
                    self.apply_window_action_outcome(ShellActionOutcome::queue_animation(
                        ShellAnimationKind::ZoomSettle,
                    ));
                }
            }
            PinchGesture::End { .. } => {
                self.pinch_zoom = None;
            }
        }
    }

    pub(crate) fn handle_main_swipe_gesture(
        &mut self,
        event_loop: &ActiveEventLoop,
        gesture: SwipeGesture,
    ) {
        match gesture {
            SwipeGesture::Begin { fingers } => {
                self.swipe_nav = Some(SwipeNavigationTracker::begin(fingers));
            }
            SwipeGesture::Update { delta_x, delta_y } => {
                // Missed begin: track motion but keep fingers=0 so finish cannot navigate.
                let tracker = self
                    .swipe_nav
                    .get_or_insert_with(|| SwipeNavigationTracker::begin(0));
                tracker.update(delta_x, delta_y);
            }
            SwipeGesture::End { cancelled } => {
                let Some(tracker) = self.swipe_nav.take() else {
                    return;
                };
                if let Some(action) = tracker.finish(cancelled) {
                    self.perform_path_navigation(event_loop, action);
                }
            }
        }
    }
}

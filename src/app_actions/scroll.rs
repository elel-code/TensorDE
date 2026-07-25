use crate::platform::{MouseScrollDelta, PinchGesture};

use super::outcome::ShellActionOutcome;
use crate::shell::shortcuts::{PinchZoomTracker, zoom_action_for_scroll_delta};
use crate::{FikaWgpuApp, SCROLL_REDRAW_FRAMES, ZOOM_REDRAW_FRAMES, scroll_delta_y};

impl FikaWgpuApp {
    pub(crate) fn handle_main_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let delta_y = scroll_delta_y(delta, self.scene.ui_scale());
        let shortcut = self.modifiers.state().control_key() || self.modifiers.state().meta_key();
        if shortcut {
            if let Some(zoom_action) = zoom_action_for_scroll_delta(delta_y)
                && self.scene.zoom(zoom_action, size)
            {
                self.apply_window_action_outcome(ShellActionOutcome::Queue {
                    reason: "wheel-zoom",
                    redraw_frames: ZOOM_REDRAW_FRAMES,
                });
            }
            return;
        }
        if self.scene.scroll_by(delta_y, size) {
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
                    changed |= self.scene.zoom(action, size);
                }
                if changed {
                    self.apply_window_action_outcome(ShellActionOutcome::Queue {
                        reason: "pinch-zoom",
                        redraw_frames: ZOOM_REDRAW_FRAMES,
                    });
                }
            }
            PinchGesture::End { .. } => {
                self.pinch_zoom = None;
            }
        }
    }
}

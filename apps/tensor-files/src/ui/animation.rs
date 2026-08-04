use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tensor_files_core::ViewRect;

use crate::ui::metrics::{
    HOVER_ANIMATION_DURATION, HOVER_ANIMATION_FRAME, ITEM_REFLOW_ANIMATION_DURATION,
    ITEM_REFLOW_ANIMATION_FRAME, LOCATION_FOCUS_SHINE_DELAY, LOCATION_FOCUS_SHINE_DURATION,
    LOCATION_FOCUS_SHINE_FRAME, TEXT_CARET_BLINK_INTERVAL, ZOOM_REDRAW_FRAMES,
};
use crate::ui::pane::ShellPaneId;

/// Named animation timelines that can request presentation via action outcomes.
///
/// Frame budgets are derived from the same duration/frame constants the runtime
/// uses, so Queue scheduling and `about_to_wait` animation polling stay aligned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellAnimationKind {
    /// Item/place hover highlight ease.
    Hover,
    /// Delayed item reflow after layout changes (zoom, resize, reload).
    ItemReflow,
    /// Path bar focus shine after location draft activation.
    LocationFocusShine,
    /// Immediate paint settle after zoom before delayed reflow starts.
    ZoomSettle,
}

/// How an animation should be scheduled through [`crate::app_actions::ShellActionOutcome`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShellAnimationPresentation {
    pub(crate) reason: &'static str,
    pub(crate) redraw_frames: u8,
}

impl ShellAnimationKind {
    pub(crate) fn presentation(self) -> ShellAnimationPresentation {
        match self {
            Self::Hover => ShellAnimationPresentation {
                reason: "hover-animation",
                redraw_frames: animation_redraw_frames(
                    HOVER_ANIMATION_DURATION,
                    HOVER_ANIMATION_FRAME,
                ),
            },
            Self::ItemReflow => ShellAnimationPresentation {
                reason: "item-reflow-animation",
                redraw_frames: animation_redraw_frames(
                    ITEM_REFLOW_ANIMATION_DURATION,
                    ITEM_REFLOW_ANIMATION_FRAME,
                ),
            },
            Self::LocationFocusShine => ShellAnimationPresentation {
                reason: "location-focus-shine",
                redraw_frames: animation_redraw_frames(
                    LOCATION_FOCUS_SHINE_DELAY + LOCATION_FOCUS_SHINE_DURATION,
                    LOCATION_FOCUS_SHINE_FRAME,
                ),
            },
            Self::ZoomSettle => ShellAnimationPresentation {
                reason: "zoom",
                redraw_frames: ZOOM_REDRAW_FRAMES,
            },
        }
    }
}

/// Ceiling of `duration / frame`, clamped to a single byte frame budget.
pub(crate) fn animation_redraw_frames(duration: Duration, frame: Duration) -> u8 {
    let frame_ms = frame.as_millis().max(1);
    let duration_ms = duration.as_millis().max(frame_ms);
    let frames = duration_ms.div_ceil(frame_ms);
    frames.clamp(1, u128::from(u8::MAX)) as u8
}

#[derive(Clone, Debug)]
pub(crate) struct ShellItemReflowTransition {
    pub(crate) pane: ShellPaneId,
    pub(crate) path: PathBuf,
    pub(crate) from: ViewRect,
    pub(crate) to: ViewRect,
    started: Instant,
}

impl ShellItemReflowTransition {
    #[cfg(test)]
    pub(crate) fn moved(&self) -> bool {
        item_reflow_rect_moved(self.from, self.to)
    }

    fn offset(&self, now: Instant) -> Option<(f32, f32)> {
        let elapsed = now.duration_since(self.started);
        if elapsed >= ITEM_REFLOW_ANIMATION_DURATION {
            return None;
        }
        let duration = ITEM_REFLOW_ANIMATION_DURATION
            .as_secs_f32()
            .max(f32::EPSILON);
        let t = (elapsed.as_secs_f32() / duration).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        let remaining = 1.0 - eased;
        Some((
            (self.from.x - self.to.x) * remaining,
            (self.from.y - self.to.y) * remaining,
        ))
    }

    fn active(&self, now: Instant) -> bool {
        now.duration_since(self.started) < ITEM_REFLOW_ANIMATION_DURATION
    }
}

#[derive(Default)]
pub(crate) struct ShellAnimationRuntime {
    item_reflow_transitions: Vec<ShellItemReflowTransition>,
    hover: ShellHoverAnimationRuntime,
    location_focus_shine: ShellLocationFocusShineRuntime,
    text_caret_blink: ShellTextCaretBlinkRuntime,
}

impl ShellAnimationRuntime {
    pub(crate) fn start_item_reflow(
        &mut self,
        pane: ShellPaneId,
        previous_rects: HashMap<PathBuf, ViewRect>,
        next_rects: HashMap<PathBuf, ViewRect>,
    ) -> bool {
        if previous_rects.is_empty() || next_rects.is_empty() {
            return false;
        }
        let started = Instant::now();
        let mut transitions = next_rects
            .into_iter()
            .filter_map(|(path, to)| {
                let from = previous_rects.get(&path).copied()?;
                item_reflow_rect_moved(from, to).then_some(ShellItemReflowTransition {
                    pane,
                    path,
                    from,
                    to,
                    started,
                })
            })
            .collect::<Vec<_>>();
        if transitions.is_empty() {
            return false;
        }
        self.item_reflow_transitions
            .retain(|transition| transition.pane != pane);
        self.item_reflow_transitions.append(&mut transitions);
        // Keep registry presentation reachable so Queue budgets stay in sync
        // when action-layer reflow scheduling is wired later.
        let _ = ShellAnimationKind::ItemReflow.presentation();
        true
    }

    pub(crate) fn item_reflow_offset_for_path(
        &self,
        pane: ShellPaneId,
        path: &Path,
    ) -> Option<(f32, f32)> {
        let now = Instant::now();
        self.item_reflow_transitions
            .iter()
            .find(|transition| transition.pane == pane && transition.path == path)
            .and_then(|transition| transition.offset(now))
    }

    pub(crate) fn active(&self) -> bool {
        let now = Instant::now();
        self.item_reflow_transitions
            .iter()
            .any(|transition| transition.active(now))
            || self.hover.active_at(now)
            || self.location_focus_shine.active_at(now)
    }

    pub(crate) fn next_frame_deadline(&self) -> Option<Instant> {
        let now = Instant::now();
        let mut deadline = None;
        if self
            .item_reflow_transitions
            .iter()
            .any(|transition| transition.active(now))
        {
            deadline = Some(now + ITEM_REFLOW_ANIMATION_FRAME);
        }
        if self.hover.active_at(now) {
            deadline = Some(
                deadline
                    .map(|current| current.min(now + HOVER_ANIMATION_FRAME))
                    .unwrap_or(now + HOVER_ANIMATION_FRAME),
            );
        }
        if let Some(shine_deadline) = self.location_focus_shine.next_frame_deadline_at(now) {
            deadline = Some(
                deadline
                    .map(|current| current.min(shine_deadline))
                    .unwrap_or(shine_deadline),
            );
        }
        deadline
    }

    pub(crate) fn start_hover_transition(&mut self) {
        self.hover.start();
    }

    pub(crate) fn hover_factor(&self) -> f32 {
        self.hover.factor()
    }

    pub(crate) fn start_location_focus_shine(&mut self) {
        self.location_focus_shine.start();
        let _ = ShellAnimationKind::LocationFocusShine.presentation();
    }

    pub(crate) fn location_focus_shine_value(&self) -> Option<f32> {
        self.location_focus_shine.value()
    }

    pub(crate) fn stop_location_focus_shine(&mut self) -> bool {
        self.location_focus_shine.stop()
    }

    pub(crate) fn reset_text_caret_blink(&mut self) {
        self.text_caret_blink.reset();
    }

    pub(crate) fn text_caret_visible(&self) -> bool {
        self.text_caret_blink.visible()
    }

    pub(crate) fn text_caret_dirty_value(&self, active: bool) -> u64 {
        self.text_caret_blink.dirty_value(active)
    }

    pub(crate) fn next_text_caret_blink_deadline(&self, active: bool) -> Option<Instant> {
        self.text_caret_blink.next_deadline(active)
    }

    pub(crate) fn prune_finished(&mut self) -> bool {
        let hover_pruned = self.hover.prune_finished();
        let shine_pruned = self.location_focus_shine.prune_finished();
        if self.item_reflow_transitions.is_empty() {
            return hover_pruned || shine_pruned;
        }
        let now = Instant::now();
        let old_len = self.item_reflow_transitions.len();
        self.item_reflow_transitions
            .retain(|transition| transition.active(now));
        if self.item_reflow_transitions.len() == old_len {
            return hover_pruned || shine_pruned;
        }
        true
    }

    pub(crate) fn clear(&mut self) {
        if self.item_reflow_transitions.is_empty() {
            return;
        }
        self.item_reflow_transitions.clear();
    }

    #[cfg(test)]
    pub(crate) fn item_reflow_transitions(&self) -> &[ShellItemReflowTransition] {
        &self.item_reflow_transitions
    }
}

pub(crate) fn item_reflow_rect_moved(from: ViewRect, to: ViewRect) -> bool {
    (from.x - to.x).abs() >= 0.5 || (from.y - to.y).abs() >= 0.5
}

#[derive(Clone, Debug)]
struct ShellHoverAnimationRuntime {
    started: Instant,
    active: bool,
}

impl Default for ShellHoverAnimationRuntime {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            active: false,
        }
    }
}

impl ShellHoverAnimationRuntime {
    fn start(&mut self) {
        self.started = Instant::now();
        self.active = true;
    }

    fn factor(&self) -> f32 {
        self.factor_at(Instant::now())
    }

    fn factor_at(&self, now: Instant) -> f32 {
        if !self.active {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= HOVER_ANIMATION_DURATION {
            return 1.0;
        }
        let duration = HOVER_ANIMATION_DURATION.as_secs_f32().max(f32::EPSILON);
        let t = (elapsed.as_secs_f32() / duration).clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }

    fn active_at(&self, now: Instant) -> bool {
        self.active && now.saturating_duration_since(self.started) < HOVER_ANIMATION_DURATION
    }

    fn prune_finished(&mut self) -> bool {
        if !self.active || self.active_at(Instant::now()) {
            return false;
        }
        self.active = false;
        true
    }
}

#[derive(Clone, Debug)]
struct ShellLocationFocusShineRuntime {
    started: Instant,
    active: bool,
}

impl Default for ShellLocationFocusShineRuntime {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            active: false,
        }
    }
}

impl ShellLocationFocusShineRuntime {
    fn start(&mut self) {
        self.started = Instant::now() + LOCATION_FOCUS_SHINE_DELAY;
        self.active = true;
    }

    fn stop(&mut self) -> bool {
        if !self.active {
            return false;
        }
        self.active = false;
        true
    }

    fn value(&self) -> Option<f32> {
        self.value_at(Instant::now())
    }

    fn value_at(&self, now: Instant) -> Option<f32> {
        if !self.active || now < self.started {
            return None;
        }
        let elapsed = now.duration_since(self.started);
        if elapsed >= LOCATION_FOCUS_SHINE_DURATION {
            return None;
        }
        let duration = LOCATION_FOCUS_SHINE_DURATION
            .as_secs_f32()
            .max(f32::EPSILON);
        let t = (elapsed.as_secs_f32() / duration).clamp(0.0, 1.0);
        Some((1.0 - t).powi(2))
    }

    fn active_at(&self, now: Instant) -> bool {
        self.active
            && now >= self.started
            && now.duration_since(self.started) < LOCATION_FOCUS_SHINE_DURATION
    }

    fn next_frame_deadline_at(&self, now: Instant) -> Option<Instant> {
        if !self.active {
            return None;
        }
        if now < self.started {
            return Some(self.started);
        }
        self.active_at(now)
            .then_some(now + LOCATION_FOCUS_SHINE_FRAME)
    }

    fn prune_finished(&mut self) -> bool {
        let now = Instant::now();
        if !self.active || now < self.started || self.active_at(now) {
            return false;
        }
        self.active = false;
        true
    }
}

#[derive(Clone, Debug)]
struct ShellTextCaretBlinkRuntime {
    started: Instant,
    generation: u64,
}

impl Default for ShellTextCaretBlinkRuntime {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            generation: 0,
        }
    }
}

impl ShellTextCaretBlinkRuntime {
    fn reset(&mut self) {
        self.started = Instant::now();
        self.generation = self.generation.wrapping_add(1);
    }

    fn visible(&self) -> bool {
        self.visible_at(Instant::now())
    }

    fn visible_at(&self, now: Instant) -> bool {
        self.phase_at(now).is_multiple_of(2)
    }

    fn dirty_value(&self, active: bool) -> u64 {
        if !active {
            return 0;
        }
        self.dirty_value_at(Instant::now())
    }

    fn dirty_value_at(&self, now: Instant) -> u64 {
        (self.generation << 32) ^ self.phase_at(now)
    }

    fn next_deadline(&self, active: bool) -> Option<Instant> {
        active.then(|| self.next_deadline_at(Instant::now()))
    }

    fn next_deadline_at(&self, now: Instant) -> Instant {
        let interval_ms = TEXT_CARET_BLINK_INTERVAL.as_millis().max(1);
        let elapsed_ms = now.saturating_duration_since(self.started).as_millis();
        let next_elapsed_ms = ((elapsed_ms / interval_ms) + 1) * interval_ms;
        self.started + Duration::from_millis(next_elapsed_ms.min(u64::MAX as u128) as u64)
    }

    fn phase_at(&self, now: Instant) -> u64 {
        let interval_ms = TEXT_CARET_BLINK_INTERVAL.as_millis().max(1);
        let phase = now.saturating_duration_since(self.started).as_millis() / interval_ms;
        phase.min(u64::MAX as u128) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_caret_blink_toggles_on_interval_and_reset_generation() {
        let mut blink = ShellTextCaretBlinkRuntime::default();
        let started = blink.started;
        assert!(blink.visible_at(started));
        assert!(blink.visible_at(started + TEXT_CARET_BLINK_INTERVAL / 2));
        assert!(!blink.visible_at(started + TEXT_CARET_BLINK_INTERVAL));
        assert!(blink.visible_at(started + TEXT_CARET_BLINK_INTERVAL + TEXT_CARET_BLINK_INTERVAL));

        let first_dirty = blink.dirty_value_at(started + TEXT_CARET_BLINK_INTERVAL);
        blink.reset();
        assert!(blink.visible());
        assert_ne!(blink.dirty_value_at(blink.started), first_dirty);
        assert_eq!(blink.dirty_value(false), 0);
    }

    #[test]
    fn text_caret_next_deadline_advances_by_blink_phase() {
        let blink = ShellTextCaretBlinkRuntime::default();
        let started = blink.started;
        assert_eq!(
            blink.next_deadline_at(started),
            started + TEXT_CARET_BLINK_INTERVAL
        );
        assert_eq!(
            blink.next_deadline_at(started + TEXT_CARET_BLINK_INTERVAL),
            started + TEXT_CARET_BLINK_INTERVAL + TEXT_CARET_BLINK_INTERVAL
        );
        assert!(blink.next_deadline(true).is_some());
        assert!(blink.next_deadline(false).is_none());
    }

    #[test]
    fn animation_presentation_uses_duration_frame_budgets() {
        assert_eq!(
            ShellAnimationKind::Hover.presentation(),
            ShellAnimationPresentation {
                reason: "hover-animation",
                redraw_frames: animation_redraw_frames(
                    HOVER_ANIMATION_DURATION,
                    HOVER_ANIMATION_FRAME,
                ),
            }
        );
        assert_eq!(
            ShellAnimationKind::ZoomSettle.presentation().redraw_frames,
            ZOOM_REDRAW_FRAMES
        );
        assert_eq!(
            ShellAnimationKind::ItemReflow.presentation().redraw_frames,
            animation_redraw_frames(ITEM_REFLOW_ANIMATION_DURATION, ITEM_REFLOW_ANIMATION_FRAME)
        );
        assert_eq!(
            animation_redraw_frames(Duration::from_millis(130), Duration::from_millis(16)),
            9
        );
    }

    #[test]
    fn hover_animation_eases_to_full_factor_and_prunes() {
        let mut hover = ShellHoverAnimationRuntime::default();
        assert_eq!(hover.factor_at(hover.started), 1.0);
        assert!(!hover.active_at(hover.started));

        hover.start();
        let started = hover.started;
        assert!(hover.active_at(started));
        assert_eq!(hover.factor_at(started), 0.0);
        assert!(hover.factor_at(started + HOVER_ANIMATION_DURATION / 2) > 0.0);
        assert_eq!(hover.factor_at(started + HOVER_ANIMATION_DURATION), 1.0);

        hover.started = Instant::now() - HOVER_ANIMATION_DURATION;
        assert!(hover.prune_finished());
        assert!(!hover.active);
    }

    #[test]
    fn location_focus_shine_waits_then_eases_right_to_left_and_prunes() {
        let mut shine = ShellLocationFocusShineRuntime::default();
        assert!(!shine.active_at(shine.started));

        shine.start();
        let started = shine.started;
        let before_start = started - Duration::from_millis(1);
        assert!(!shine.active_at(before_start));
        assert_eq!(shine.value_at(before_start), None);
        assert_eq!(shine.next_frame_deadline_at(before_start), Some(started));
        assert_eq!(shine.value_at(started), Some(1.0));

        let midpoint = shine
            .value_at(started + LOCATION_FOCUS_SHINE_DURATION / 2)
            .unwrap();
        assert!(midpoint > 0.0);
        assert!(midpoint < 1.0);
        assert_eq!(
            shine.value_at(started + LOCATION_FOCUS_SHINE_DURATION),
            None
        );
        assert!(!shine.active_at(started + LOCATION_FOCUS_SHINE_DURATION));

        shine.started = Instant::now() - LOCATION_FOCUS_SHINE_DURATION;
        assert!(shine.prune_finished());
        assert!(!shine.active);
    }
}

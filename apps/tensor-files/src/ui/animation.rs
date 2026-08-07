use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bevy_ecs::entity::Entity;
use tensor_files_core::ViewRect;

use crate::ui::metrics::{
    HOVER_ANIMATION_DURATION, HOVER_ANIMATION_FRAME, ITEM_REFLOW_ANIMATION_DURATION,
    ITEM_REFLOW_ANIMATION_FRAME, LOCATION_FOCUS_SHINE_DELAY, LOCATION_FOCUS_SHINE_DURATION,
    LOCATION_FOCUS_SHINE_FRAME, TEXT_CARET_BLINK_INTERVAL,
};
use crate::ui::pane::ShellPaneId;

/// Named animation timelines that can request presentation via action outcomes.
///
/// The action layer queues the first paint; subsequent frames are driven by
/// the animation runtime's deadlines in `about_to_wait`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellAnimationKind {
    /// Item/place hover highlight ease.
    Hover,
    /// Path bar focus shine after location draft activation.
    LocationFocusShine,
    /// Immediate paint settle after zoom before delayed reflow starts.
    ZoomSettle,
}

/// How an animation should be scheduled through [`crate::app_actions::ShellActionOutcome`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShellAnimationPresentation {
    pub(crate) reason: &'static str,
}

impl ShellAnimationKind {
    pub(crate) fn presentation(self) -> ShellAnimationPresentation {
        match self {
            Self::Hover => ShellAnimationPresentation {
                reason: "hover-animation",
            },
            Self::LocationFocusShine => ShellAnimationPresentation {
                reason: "location-focus-shine",
            },
            Self::ZoomSettle => ShellAnimationPresentation { reason: "zoom" },
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ShellItemReflowTransition {
    pub(crate) from: ViewRect,
    pub(crate) to: ViewRect,
}

/// Base item geometry retained across a resize transaction.
///
/// A retained visible-item entity is the widget identity. Paths are kept only
/// for the cold frame where the slot pool has not been populated yet, matching
/// Dolphin's widget-keyed moving animation semantics.
#[derive(Clone, Default)]
pub(crate) struct ShellItemReflowRects {
    by_entity: HashMap<Entity, ViewRect>,
    by_path: HashMap<PathBuf, ViewRect>,
}

impl ShellItemReflowRects {
    #[cfg(test)]
    pub(crate) fn from_paths(by_path: HashMap<PathBuf, ViewRect>) -> Self {
        Self {
            by_entity: HashMap::new(),
            by_path,
        }
    }

    pub(crate) fn insert_entity(&mut self, entity: Entity, rect: ViewRect) {
        self.by_entity.insert(entity, rect);
    }

    pub(crate) fn insert_path(&mut self, path: PathBuf, rect: ViewRect) {
        self.by_path.insert(path, rect);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.by_entity.is_empty() && self.by_path.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn rect_for(&self, entity: Option<Entity>, path: &Path) -> Option<ViewRect> {
        entity
            .and_then(|entity| self.by_entity.get(&entity).copied())
            .or_else(|| self.by_path.get(path).copied())
    }
}

#[derive(Clone, Default)]
pub(crate) struct ShellItemReflowOffsets {
    by_entity: HashMap<Entity, (f32, f32)>,
    by_path: HashMap<PathBuf, (f32, f32)>,
}

impl ShellItemReflowOffsets {
    pub(crate) fn from_rects(previous: &ShellItemReflowRects, next: &ShellItemReflowRects) -> Self {
        let mut offsets = Self::default();
        offsets
            .by_entity
            .reserve(previous.by_entity.len().min(next.by_entity.len()));
        offsets
            .by_path
            .reserve(previous.by_path.len().min(next.by_path.len()));
        for (entity, to) in &next.by_entity {
            let Some(from) = previous.by_entity.get(entity).copied() else {
                continue;
            };
            if item_reflow_rect_moved(from, *to) {
                offsets
                    .by_entity
                    .insert(*entity, (from.x - to.x, from.y - to.y));
            }
        }
        for (path, to) in &next.by_path {
            let Some(from) = previous.by_path.get(path).copied() else {
                continue;
            };
            if item_reflow_rect_moved(from, *to) {
                offsets
                    .by_path
                    .insert(path.clone(), (from.x - to.x, from.y - to.y));
            }
        }
        offsets
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.by_entity.is_empty() && self.by_path.is_empty()
    }

    pub(crate) fn offset_for(&self, entity: Option<Entity>, path: &Path) -> Option<(f32, f32)> {
        entity
            .and_then(|entity| self.by_entity.get(&entity).copied())
            .or_else(|| self.by_path.get(path).copied())
    }

    pub(crate) fn clear(&mut self) {
        self.by_entity.clear();
        self.by_path.clear();
    }
}

impl ShellItemReflowTransition {
    #[cfg(test)]
    pub(crate) fn moved(&self) -> bool {
        item_reflow_rect_moved(self.from, self.to)
    }

    fn offset(&self, started: Instant, now: Instant) -> Option<(f32, f32)> {
        let elapsed = now.saturating_duration_since(started);
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
}

#[derive(Default)]
struct ShellItemReflowTransitions {
    by_entity: HashMap<Entity, ShellItemReflowTransition>,
    by_path: HashMap<PathBuf, ShellItemReflowTransition>,
    started: Option<Instant>,
}

impl ShellItemReflowTransitions {
    fn is_empty(&self) -> bool {
        self.by_entity.is_empty() && self.by_path.is_empty()
    }

    fn active_at(&self, now: Instant) -> bool {
        self.started.is_some_and(|started| {
            now.saturating_duration_since(started) < ITEM_REFLOW_ANIMATION_DURATION
        }) && !self.is_empty()
    }

    fn clear(&mut self) {
        self.by_entity.clear();
        self.by_path.clear();
        self.started = None;
    }
}

#[derive(Default)]
pub(crate) struct ShellAnimationRuntime {
    item_reflow_transitions: [ShellItemReflowTransitions; 2],
    item_reflow_entity_staging: HashMap<Entity, ShellItemReflowTransition>,
    item_reflow_path_staging: HashMap<PathBuf, ShellItemReflowTransition>,
    hover: ShellHoverAnimationRuntime,
    location_focus_shine: ShellLocationFocusShineRuntime,
    text_caret_blink: ShellTextCaretBlinkRuntime,
}

impl ShellAnimationRuntime {
    #[cfg(test)]
    pub(crate) fn start_item_reflow(
        &mut self,
        pane: ShellPaneId,
        previous_rects: HashMap<PathBuf, ViewRect>,
        next_rects: HashMap<PathBuf, ViewRect>,
    ) -> bool {
        self.start_item_reflow_from_rects(
            pane,
            ShellItemReflowRects::from_paths(previous_rects),
            ShellItemReflowRects::from_paths(next_rects),
        )
    }

    pub(crate) fn start_item_reflow_from_rects(
        &mut self,
        pane: ShellPaneId,
        previous_rects: ShellItemReflowRects,
        next_rects: ShellItemReflowRects,
    ) -> bool {
        self.item_reflow_entity_staging.clear();
        self.item_reflow_path_staging.clear();
        if previous_rects.is_empty() || next_rects.is_empty() {
            self.item_reflow_transitions[pane.index()].clear();
            return false;
        }
        let started = Instant::now();
        self.item_reflow_entity_staging.reserve(
            previous_rects
                .by_entity
                .len()
                .min(next_rects.by_entity.len()),
        );
        self.item_reflow_path_staging
            .reserve(previous_rects.by_path.len().min(next_rects.by_path.len()));
        for (entity, to) in next_rects.by_entity {
            let Some(from) = previous_rects.by_entity.get(&entity).copied() else {
                continue;
            };
            if item_reflow_rect_moved(from, to) {
                self.item_reflow_entity_staging
                    .insert(entity, ShellItemReflowTransition { from, to });
            }
        }
        for (path, to) in next_rects.by_path {
            let Some(from) = previous_rects.by_path.get(&path).copied() else {
                continue;
            };
            if item_reflow_rect_moved(from, to) {
                self.item_reflow_path_staging
                    .insert(path, ShellItemReflowTransition { from, to });
            }
        }
        if self.item_reflow_entity_staging.is_empty() && self.item_reflow_path_staging.is_empty() {
            self.item_reflow_transitions[pane.index()].clear();
            return false;
        }
        let transitions = &mut self.item_reflow_transitions[pane.index()];
        std::mem::swap(
            &mut transitions.by_entity,
            &mut self.item_reflow_entity_staging,
        );
        std::mem::swap(&mut transitions.by_path, &mut self.item_reflow_path_staging);
        transitions.started = Some(started);
        self.item_reflow_entity_staging.clear();
        self.item_reflow_path_staging.clear();
        true
    }

    #[cfg(test)]
    pub(crate) fn start_item_reflow_with_entity_lookup(
        &mut self,
        pane: ShellPaneId,
        previous_rects: HashMap<PathBuf, ViewRect>,
        next_rects: HashMap<PathBuf, ViewRect>,
        mut entity_for_path: impl FnMut(&Path) -> Option<Entity>,
    ) -> bool {
        let mut previous = ShellItemReflowRects::default();
        let mut next = ShellItemReflowRects::default();
        for (path, rect) in previous_rects {
            if let Some(entity) = entity_for_path(path.as_path()) {
                previous.insert_entity(entity, rect);
            } else {
                previous.insert_path(path, rect);
            }
        }
        for (path, rect) in next_rects {
            if let Some(entity) = entity_for_path(path.as_path()) {
                next.insert_entity(entity, rect);
            } else {
                next.insert_path(path, rect);
            }
        }
        self.start_item_reflow_from_rects(pane, previous, next)
    }

    pub(crate) fn item_reflow_offset_for_entity_at(
        &self,
        pane: ShellPaneId,
        entity: Entity,
        now: Instant,
    ) -> Option<(f32, f32)> {
        let transitions = &self.item_reflow_transitions[pane.index()];
        let started = transitions.started?;
        transitions
            .by_entity
            .get(&entity)
            .and_then(|transition| transition.offset(started, now))
    }

    pub(crate) fn item_reflow_offset_for_path_at(
        &self,
        pane: ShellPaneId,
        path: &Path,
        now: Instant,
    ) -> Option<(f32, f32)> {
        let transitions = &self.item_reflow_transitions[pane.index()];
        let started = transitions.started?;
        transitions
            .by_path
            .get(path)
            .and_then(|transition| transition.offset(started, now))
    }

    pub(crate) fn item_reflow_active_for_pane_at(&self, pane: ShellPaneId, now: Instant) -> bool {
        self.item_reflow_transitions[pane.index()].active_at(now)
    }

    pub(crate) fn has_item_reflow_for_pane(&self, pane: ShellPaneId) -> bool {
        !self.item_reflow_transitions[pane.index()].is_empty()
    }

    pub(crate) fn clear_item_reflow_for_pane(&mut self, pane: ShellPaneId) {
        self.item_reflow_transitions[pane.index()].clear();
    }

    pub(crate) fn active(&self) -> bool {
        let now = Instant::now();
        self.item_reflow_transitions
            .iter()
            .any(|transitions| transitions.active_at(now))
            || self.hover.active_at(now)
            || self.location_focus_shine.active_at(now)
    }

    pub(crate) fn next_frame_deadline(&self) -> Option<Instant> {
        let now = Instant::now();
        let mut deadline = None;
        if self
            .item_reflow_transitions
            .iter()
            .any(|transitions| transitions.active_at(now))
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
        let has_transitions = self
            .item_reflow_transitions
            .iter()
            .any(|transitions| !transitions.is_empty());
        if !has_transitions {
            return hover_pruned || shine_pruned;
        }
        let now = Instant::now();
        let mut reflow_pruned = false;
        for transitions in &mut self.item_reflow_transitions {
            if !transitions.is_empty() && !transitions.active_at(now) {
                transitions.clear();
                reflow_pruned = true;
            }
        }
        hover_pruned || shine_pruned || reflow_pruned
    }

    pub(crate) fn clear(&mut self) {
        for transitions in &mut self.item_reflow_transitions {
            transitions.clear();
        }
        self.item_reflow_entity_staging.clear();
        self.item_reflow_path_staging.clear();
    }

    #[cfg(test)]
    pub(crate) fn item_reflow_transition(
        &self,
        pane: ShellPaneId,
        path: &Path,
    ) -> Option<&ShellItemReflowTransition> {
        self.item_reflow_transitions[pane.index()].by_path.get(path)
    }

    #[cfg(test)]
    pub(crate) fn item_reflow_transition_for_entity(
        &self,
        pane: ShellPaneId,
        entity: Entity,
    ) -> Option<&ShellItemReflowTransition> {
        self.item_reflow_transitions[pane.index()]
            .by_entity
            .get(&entity)
    }

    #[cfg(test)]
    pub(crate) fn item_reflow_transition_count(&self) -> usize {
        self.item_reflow_transitions
            .iter()
            .map(|transitions| transitions.by_entity.len() + transitions.by_path.len())
            .sum()
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
mod reflow_tests;

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
    fn animation_presentation_uses_stable_reasons() {
        assert_eq!(
            ShellAnimationKind::Hover.presentation(),
            ShellAnimationPresentation {
                reason: "hover-animation",
            }
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

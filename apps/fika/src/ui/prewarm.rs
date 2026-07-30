use std::env;
use std::time::{Duration, Instant};

use crate::ui::metrics::{
    FILE_MANAGER_MAX_BLOCK_TIMEOUT, FILE_MANAGER_VISIBLE_RANGE_UPDATE_MS,
    ICON_ROLE_READ_AHEAD_LIMIT, ICON_ROLE_READ_AHEAD_QUEUE_BUDGET_PER_FRAME,
    ICON_VISIBLE_SYNC_RESOLVE_BUDGET, TEXT_RASTER_MISS_BUDGET_PER_FRAME,
    VISIBLE_ICON_ROLE_PREWARM_BUDGET,
};

#[derive(Default)]
pub(crate) struct IconRolePrewarmStats {
    pub(crate) entries: usize,
    pub(crate) deferred: usize,
    pub(crate) read_ahead: usize,
    pub(crate) resolve_us: u128,
    pub(crate) over_budget: bool,
}

const FILE_MANAGER_ICON_SIZE_UPDATE_MS: u64 = 300;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VisibleRoleUpdateKind {
    VisibleRange,
    IconSize,
}

/// Mirrors KFileItemListView's independent visible-range and icon-size
/// single-shot timers. Scrollbar button state is intentionally absent:
/// QScrollBar dragging is not a KItemListView transaction, so a thumb held
/// still for 50 ms must resume visible role updates before mouse release.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct VisibleRoleUpdateState {
    visible_range_deadline: Option<Instant>,
    icon_size_deadline: Option<Instant>,
}

impl VisibleRoleUpdateState {
    pub(crate) fn schedule_visible_range(&mut self, now: Instant) {
        // Dolphin does not start the short timer while the longer icon-size
        // timer is active; updateIconSize() publishes both size and range.
        if self.icon_size_deadline.is_none() {
            self.visible_range_deadline =
                Some(now + Duration::from_millis(FILE_MANAGER_VISIBLE_RANGE_UPDATE_MS));
        }
    }

    pub(crate) fn schedule_icon_size(&mut self, now: Instant) {
        self.visible_range_deadline = None;
        self.icon_size_deadline =
            Some(now + Duration::from_millis(FILE_MANAGER_ICON_SIZE_UPDATE_MS));
    }

    pub(crate) fn schedule(&mut self, kind: VisibleRoleUpdateKind, now: Instant) {
        match kind {
            VisibleRoleUpdateKind::VisibleRange => self.schedule_visible_range(now),
            VisibleRoleUpdateKind::IconSize => self.schedule_icon_size(now),
        }
    }

    pub(crate) fn take_due_update(&mut self, now: Instant) -> Option<VisibleRoleUpdateKind> {
        if self
            .icon_size_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.icon_size_deadline = None;
            return Some(VisibleRoleUpdateKind::IconSize);
        }
        if self
            .visible_range_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.visible_range_deadline = None;
            return Some(VisibleRoleUpdateKind::VisibleRange);
        }
        None
    }

    pub(crate) fn role_updates_paused(self) -> bool {
        self.pending()
    }

    pub(crate) fn pending(self) -> bool {
        self.visible_range_deadline.is_some() || self.icon_size_deadline.is_some()
    }

    pub(crate) fn deadline(self) -> Option<Instant> {
        [self.visible_range_deadline, self.icon_size_deadline]
            .into_iter()
            .flatten()
            .min()
    }
}

pub(crate) fn icon_sync_resolve_budget(role_updates_paused: bool) -> usize {
    if role_updates_paused {
        return 0;
    }
    if let Some(budget) = env_usize("FIKA_ICON_SYNC_RESOLVE_BUDGET")
        .or_else(|| env_usize("FIKA_ICON_RASTER_MISS_BUDGET"))
    {
        return budget;
    }
    ICON_VISIBLE_SYNC_RESOLVE_BUDGET
}

pub(crate) fn icon_work_reason_for_frame(reason: &str, frame_count: u64) -> &str {
    if frame_count == 0 { "startup" } else { reason }
}

pub(crate) fn icon_role_prewarm_budget(resolve_visible_exact: bool) -> Duration {
    if resolve_visible_exact {
        FILE_MANAGER_MAX_BLOCK_TIMEOUT
    } else {
        VISIBLE_ICON_ROLE_PREWARM_BUDGET
    }
}

pub(crate) fn visible_role_update_kind_for_reason(reason: &str) -> Option<VisibleRoleUpdateKind> {
    match reason {
        "autosmoke-scroll" | "wheel-scroll" => Some(VisibleRoleUpdateKind::VisibleRange),
        "zoom" | "wheel-zoom" | "autosmoke-zoom" => Some(VisibleRoleUpdateKind::IconSize),
        _ => None,
    }
}

pub(crate) fn icon_role_read_ahead_queue_budget_for_frame(
    reason: &str,
    small_directory_read_ahead: bool,
    resolve_visible_exact: bool,
) -> usize {
    if matches!(reason, "zoom" | "wheel-zoom" | "autosmoke-zoom") {
        return 0;
    }
    if small_directory_read_ahead || resolve_visible_exact {
        // Dolphin's settled `indexesToResolve()` schedules the complete
        // interesting range (bounded to 500) and then lets its worker advance
        // without requiring another scroll tick. Channel submission is cheap;
        // the expensive theme work remains off the frame thread.
        ICON_ROLE_READ_AHEAD_LIMIT
    } else {
        ICON_ROLE_READ_AHEAD_QUEUE_BUDGET_PER_FRAME
    }
}

pub(crate) fn default_text_raster_miss_budget() -> usize {
    env_usize("FIKA_TEXT_RASTER_MISS_BUDGET").unwrap_or(TEXT_RASTER_MISS_BUDGET_PER_FRAME)
}

fn env_usize(key: &str) -> Option<usize> {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_role_work_follows_pause_state_instead_of_render_reason() {
        assert_eq!(
            visible_role_update_kind_for_reason("wheel-scroll"),
            Some(VisibleRoleUpdateKind::VisibleRange)
        );
        assert_eq!(
            visible_role_update_kind_for_reason("wheel-zoom"),
            Some(VisibleRoleUpdateKind::IconSize)
        );
        assert_eq!(icon_sync_resolve_budget(true), 0);
        assert_eq!(
            icon_sync_resolve_budget(false),
            ICON_VISIBLE_SYNC_RESOLVE_BUDGET
        );
        assert_eq!(
            icon_role_read_ahead_queue_budget_for_frame("scroll-settle", false, true),
            ICON_ROLE_READ_AHEAD_LIMIT
        );
    }

    #[test]
    fn first_surface_frame_keeps_dolphin_visible_icon_correctness() {
        assert_eq!(icon_work_reason_for_frame("redraw", 0), "startup");
        assert_eq!(
            icon_sync_resolve_budget(false),
            ICON_VISIBLE_SYNC_RESOLVE_BUDGET
        );
        assert_eq!(
            icon_work_reason_for_frame("wheel-scroll", 1),
            "wheel-scroll"
        );
    }

    #[test]
    fn scrollbar_pause_expires_while_the_thumb_is_still_held() {
        let start = Instant::now();
        let interval = Duration::from_millis(FILE_MANAGER_VISIBLE_RANGE_UPDATE_MS);
        let mut state = VisibleRoleUpdateState::default();

        state.schedule_visible_range(start);
        assert!(state.role_updates_paused());
        assert_eq!(
            state.take_due_update(start + interval - Duration::from_millis(1)),
            None
        );
        assert_eq!(
            state.take_due_update(start + interval),
            Some(VisibleRoleUpdateKind::VisibleRange)
        );
        assert!(!state.role_updates_paused());
    }

    #[test]
    fn icon_size_timer_suppresses_short_range_updates_and_uses_long_interval() {
        let start = Instant::now();
        let mut state = VisibleRoleUpdateState::default();
        state.schedule_visible_range(start);
        state.schedule_icon_size(start + Duration::from_millis(10));
        state.schedule_visible_range(start + Duration::from_millis(20));
        assert!(state.role_updates_paused());
        assert_eq!(
            state.take_due_update(
                start + Duration::from_millis(FILE_MANAGER_VISIBLE_RANGE_UPDATE_MS)
            ),
            None
        );
        assert_eq!(
            state.take_due_update(
                start + Duration::from_millis(10 + FILE_MANAGER_ICON_SIZE_UPDATE_MS)
            ),
            Some(VisibleRoleUpdateKind::IconSize)
        );
        assert!(!state.role_updates_paused());
    }
}

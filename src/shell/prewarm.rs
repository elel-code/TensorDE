use std::env;
use std::time::Duration;

use crate::shell::metrics::{
    DOLPHIN_MAX_BLOCK_TIMEOUT, ICON_RASTER_VISIBLE_SYNC_BUDGET, ICON_ROLE_READ_AHEAD_LIMIT,
    ICON_ROLE_READ_AHEAD_QUEUE_BUDGET_PER_FRAME, TEXT_RASTER_MISS_BUDGET_PER_FRAME,
    VISIBLE_ICON_ROLE_PREWARM_BUDGET, VISIBLE_TEXT_LABEL_PREWARM_BUDGET,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextLabelPrewarmMode {
    VisibleOnly,
}

#[derive(Default)]
pub(crate) struct IconRolePrewarmStats {
    pub(crate) entries: usize,
    pub(crate) deferred: usize,
    pub(crate) read_ahead: usize,
    pub(crate) resolve_us: u128,
    pub(crate) over_budget: bool,
}

#[derive(Default)]
pub(crate) struct IconRasterPrewarmStats {
    pub(crate) entries: usize,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
    pub(crate) failed: usize,
    pub(crate) raster_us: u128,
    pub(crate) over_budget: bool,
}

#[derive(Default)]
pub(crate) struct TextLabelPrewarmStats {
    pub(crate) entries: usize,
    pub(crate) read_ahead: usize,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
    pub(crate) deferred: usize,
    pub(crate) raster_us: u128,
    pub(crate) over_budget: bool,
}

pub(crate) fn icon_raster_miss_budget_for_frame(reason: &str) -> usize {
    if let Some(budget) = env_usize("FIKA_WGPU_ICON_RASTER_MISS_BUDGET") {
        return budget;
    }
    // Dolphin pauses role updates while scrolling / zooming (ShortInterval /
    // LongInterval timers) so disk-backed icon loads never hitch the fling.
    if matches!(
        reason,
        "autosmoke-scroll" | "wheel-scroll" | "zoom" | "wheel-zoom" | "autosmoke-zoom"
    ) {
        0
    } else if visible_exact_icon_roles_enabled_for_frame(reason) {
        ICON_RASTER_VISIBLE_SYNC_BUDGET
    } else {
        0
    }
}

pub(crate) fn icon_work_reason_for_frame(reason: &str, frame_count: u64) -> &str {
    if frame_count == 0 { "startup" } else { reason }
}

pub(crate) fn icon_role_prewarm_budget_for_frame(reason: &str) -> Duration {
    if visible_exact_icon_roles_enabled_for_frame(reason) {
        DOLPHIN_MAX_BLOCK_TIMEOUT
    } else {
        VISIBLE_ICON_ROLE_PREWARM_BUDGET
    }
}

pub(crate) fn visible_exact_icon_roles_enabled_for_frame(reason: &str) -> bool {
    matches!(
        reason,
        "startup"
            | "activate-directory"
            | "double-click-directory"
            | "context-open"
            | "place-open"
            | "device-mount"
            | "history-back"
            | "history-forward"
            | "parent-directory"
            | "location-commit"
            | "reload-directory"
            | "toggle-hidden"
            | "context-toggle-hidden"
            | "auto-cycle"
            | "mode-click"
            | "switch-immediate"
    )
}

pub(crate) fn icon_role_read_ahead_queue_budget_for_frame(
    reason: &str,
    small_directory_read_ahead: bool,
) -> usize {
    if matches!(reason, "zoom" | "wheel-zoom" | "autosmoke-zoom") {
        return 0;
    }
    if small_directory_read_ahead {
        ICON_ROLE_READ_AHEAD_LIMIT
    } else {
        ICON_ROLE_READ_AHEAD_QUEUE_BUDGET_PER_FRAME
    }
}

pub(crate) fn text_label_prewarm_mode_for_scene_prewarm(_reason: &str) -> TextLabelPrewarmMode {
    TextLabelPrewarmMode::VisibleOnly
}

pub(crate) fn text_label_prewarm_mode_for_frame(_reason: &str) -> TextLabelPrewarmMode {
    // Dolphin caches text on visible/recycled item widgets, not as invisible read-ahead rasters.
    TextLabelPrewarmMode::VisibleOnly
}

pub(crate) fn text_label_prewarm_budget_for_mode(_mode: TextLabelPrewarmMode) -> Duration {
    VISIBLE_TEXT_LABEL_PREWARM_BUDGET
}

pub(crate) fn text_label_raster_miss_budget_for_mode(_mode: TextLabelPrewarmMode) -> usize {
    TEXT_RASTER_MISS_BUDGET_PER_FRAME
}

pub(crate) fn default_text_raster_miss_budget() -> usize {
    env_usize("FIKA_WGPU_TEXT_RASTER_MISS_BUDGET").unwrap_or(TEXT_RASTER_MISS_BUDGET_PER_FRAME)
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
    fn prewarm_modes_keep_scroll_and_zoom_visible_only() {
        assert_eq!(
            text_label_prewarm_mode_for_frame("wheel-scroll"),
            TextLabelPrewarmMode::VisibleOnly
        );
        assert_eq!(
            text_label_prewarm_mode_for_frame("wheel-zoom"),
            TextLabelPrewarmMode::VisibleOnly
        );
    }

    #[test]
    fn scene_prewarm_keeps_text_labels_visible_only_for_navigation_reasons() {
        assert!(visible_exact_icon_roles_enabled_for_frame(
            "activate-directory"
        ));
        assert_eq!(
            text_label_prewarm_mode_for_scene_prewarm("activate-directory"),
            TextLabelPrewarmMode::VisibleOnly
        );
    }

    #[test]
    fn first_surface_frame_uses_startup_icon_work_policy() {
        assert_eq!(icon_work_reason_for_frame("redraw", 0), "startup");
        assert_eq!(
            icon_work_reason_for_frame("wheel-scroll", 1),
            "wheel-scroll"
        );
        assert_eq!(
            icon_raster_miss_budget_for_frame(icon_work_reason_for_frame("redraw", 0)),
            ICON_RASTER_VISIBLE_SYNC_BUDGET
        );
    }
}

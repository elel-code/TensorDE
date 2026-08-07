use std::path::{Path, PathBuf};
use std::time::Instant;

use tensor_files_core::{Entry, ItemLayout, ViewRect, ViewSize, read_entries_sync};

use crate::filtered_indexes_for_entries;
use crate::ui::{
    metrics::{
        FILE_MANAGER_COMPACT_PREVIEW_ZOOM_LEVEL_DEFAULT,
        FILE_MANAGER_DETAILS_PREVIEW_ZOOM_LEVEL_DEFAULT,
        FILE_MANAGER_ICONS_PREVIEW_ZOOM_LEVEL_DEFAULT, FILE_MANAGER_ZOOM_LEVEL_MAX,
        FILE_MANAGER_ZOOM_LEVEL_MIN,
    },
    options::ShellViewMode,
    selection::ShellSelection,
};

mod ecs;
mod visible_items;

pub(crate) use ecs::ShellPaneStates;
pub(crate) use visible_items::{
    ShellPaneVisibleSlotPools, ShellVisibleItemSlotStats, ShellVisibleSlotItem,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShellPaneZoomLevels {
    levels: [i32; 3],
}

impl Default for ShellPaneZoomLevels {
    fn default() -> Self {
        Self {
            // Dolphin global view properties default to previews enabled. Its
            // per-mode PreviewSize settings are 64px for Icons and 48px for
            // Compact/Details, corresponding to ZoomLevelInfo levels 4/3/3.
            levels: [
                FILE_MANAGER_ICONS_PREVIEW_ZOOM_LEVEL_DEFAULT,
                FILE_MANAGER_COMPACT_PREVIEW_ZOOM_LEVEL_DEFAULT,
                FILE_MANAGER_DETAILS_PREVIEW_ZOOM_LEVEL_DEFAULT,
            ],
        }
    }
}

impl ShellPaneZoomLevels {
    fn index(mode: ShellViewMode) -> usize {
        match mode {
            ShellViewMode::Icons => 0,
            ShellViewMode::Compact => 1,
            ShellViewMode::Details => 2,
        }
    }

    pub(crate) fn get(self, mode: ShellViewMode) -> i32 {
        self.levels[Self::index(mode)]
    }

    pub(crate) fn set(&mut self, mode: ShellViewMode, level: i32) {
        self.levels[Self::index(mode)] =
            level.clamp(FILE_MANAGER_ZOOM_LEVEL_MIN, FILE_MANAGER_ZOOM_LEVEL_MAX);
    }
}

#[derive(bevy_ecs::component::Component, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ShellPaneId {
    Slot0,
    Slot1,
}

impl ShellPaneId {
    pub(crate) const SLOT_0: Self = Self::Slot0;
    pub(crate) const SLOT_1: Self = Self::Slot1;
    pub(crate) const ALL: [Self; 2] = [Self::SLOT_0, Self::SLOT_1];

    pub(crate) fn index(self) -> usize {
        match self {
            Self::Slot0 => 0,
            Self::Slot1 => 1,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Slot0 => "pane-0",
            Self::Slot1 => "pane-1",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellPaneGeometry {
    pub(crate) kind: ShellPaneId,
    pub(crate) pane: ViewRect,
    pub(crate) top_bar: ViewRect,
    pub(crate) content: ViewRect,
    pub(crate) status_bar: ViewRect,
}

#[derive(bevy_ecs::component::Component, Clone, Debug)]
pub(crate) struct ShellPaneState {
    pub(crate) path: PathBuf,
    /// Target currently being enumerated. The committed `path` and its entry
    /// storage stay intact until the asynchronous listing has succeeded, but
    /// projections expose this path with an empty model meanwhile. That makes
    /// navigation a visible one-shot transaction rather than leaving the old
    /// directory interactable until completion.
    pub(crate) pending_path: Option<PathBuf>,
    pub(crate) view_mode: ShellViewMode,
    pub(crate) zoom_levels: ShellPaneZoomLevels,
    pub(crate) entries: Vec<Entry>,
    pub(crate) dir_count: usize,
    pub(crate) filtered_indexes: Vec<usize>,
    pub(crate) selection: ShellSelection,
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
}

impl ShellPaneState {
    pub(crate) fn from_entries(
        path: PathBuf,
        view_mode: ShellViewMode,
        entries: Vec<Entry>,
        show_hidden: bool,
        filter_pattern: &str,
    ) -> Self {
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let filtered_indexes = filtered_indexes_for_entries(&entries, show_hidden, filter_pattern);
        Self {
            path,
            pending_path: None,
            view_mode,
            zoom_levels: ShellPaneZoomLevels::default(),
            entries,
            dir_count,
            filtered_indexes,
            selection: ShellSelection::default(),
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }

    pub(crate) fn load(
        path: PathBuf,
        view_mode: ShellViewMode,
        show_hidden: bool,
    ) -> Result<Self, String> {
        let load_start = Instant::now();
        let entries = read_entries_sync(&path)
            .map_err(|error| format!("read pane directory {}: {error}", path.display()))?;
        let elapsed = load_start.elapsed();
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let filtered_indexes = filtered_indexes_for_entries(&entries, show_hidden, "");
        tensor_files_log!(
            "[tensor-files] split-pane path={} entries={} dirs={} files={} visible={} load={}us",
            path.display(),
            entries.len(),
            dir_count,
            entries.len().saturating_sub(dir_count),
            filtered_indexes.len(),
            elapsed.as_micros()
        );
        Ok(Self {
            path,
            pending_path: None,
            view_mode,
            zoom_levels: ShellPaneZoomLevels::default(),
            entries,
            dir_count,
            filtered_indexes,
            selection: ShellSelection::default(),
            scroll_x: 0.0,
            scroll_y: 0.0,
        })
    }

    pub(crate) fn rebuild_filtered_indexes_with_pattern(
        &mut self,
        show_hidden: bool,
        filter_pattern: &str,
    ) -> bool {
        self.filtered_indexes =
            filtered_indexes_for_entries(&self.entries, show_hidden, filter_pattern);
        self.selection.retain_indexes(&self.filtered_indexes)
    }

    pub(crate) fn display_path(&self) -> &Path {
        self.pending_path.as_deref().unwrap_or(&self.path)
    }

    pub(crate) fn pending_path_matches(&self, path: &Path) -> bool {
        self.pending_path.as_deref() == Some(path)
    }

    pub(crate) fn zoom_level(&self) -> i32 {
        self.zoom_levels.get(self.view_mode)
    }

    pub(crate) fn set_zoom_level(&mut self, level: i32) {
        self.zoom_levels.set(self.view_mode, level);
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ShellPaneView<'a> {
    pub(crate) path: &'a Path,
    pub(crate) view_mode: ShellViewMode,
    pub(crate) zoom_level: i32,
    pub(crate) entries: &'a [Entry],
    pub(crate) dir_count: usize,
    pub(crate) filtered_indexes: &'a [usize],
    pub(crate) selection: &'a ShellSelection,
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
}

impl<'a> ShellPaneView<'a> {
    pub(crate) fn from_state(state: &'a ShellPaneState) -> Self {
        Self {
            // The requested URL is published through `display_path()`, while
            // this view keeps the committed directory and its retained visual
            // model until the replacement listing commits atomically.
            path: &state.path,
            view_mode: state.view_mode,
            zoom_level: state.zoom_level(),
            entries: &state.entries,
            dir_count: state.dir_count,
            filtered_indexes: &state.filtered_indexes,
            selection: &state.selection,
            scroll_x: state.scroll_x,
            scroll_y: state.scroll_y,
        }
    }

    pub(crate) fn filtered_entry_count(self) -> usize {
        self.filtered_indexes.len()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ShellPaneProjection<'a> {
    pub(crate) view: ShellPaneView<'a>,
    pub(crate) geometry: ShellPaneGeometry,
    pub(crate) visible_items: Vec<ShellPaneVisibleItem>,
    pub(crate) scroll_metrics: ShellPaneScrollMetrics,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellPaneVisibleItem {
    pub(crate) layout: ItemLayout,
    pub(crate) entry_index: Option<usize>,
    pub(crate) slot_id: u64,
    pub(crate) reflow_offset: (f32, f32),
}

impl ShellVisibleSlotItem for ShellPaneVisibleItem {
    fn visible_slot_entry_index(&self) -> Option<usize> {
        self.entry_index
    }

    fn set_visible_slot_id(&mut self, slot_id: u64) {
        self.slot_id = slot_id;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellPaneScrollMetrics {
    pub(crate) content_size: ViewSize,
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
    pub(crate) max_scroll_x: f32,
    pub(crate) max_scroll_y: f32,
}

impl ShellPaneScrollMetrics {
    pub(crate) fn new(content_size: ViewSize, viewport: ViewRect) -> Self {
        let viewport_width = viewport.width.max(1.0);
        let viewport_height = viewport.height.max(1.0);
        Self {
            content_size,
            viewport_width,
            viewport_height,
            max_scroll_x: (content_size.width - viewport_width).max(0.0),
            max_scroll_y: (content_size.height - viewport_height).max(0.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellPaneSplitMetrics {
    pub(crate) divider: ViewRect,
    pub(crate) right_pane: ViewRect,
    pub(crate) left_width: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_zoom_levels_use_and_retain_dolphin_preview_defaults_by_mode() {
        let mut levels = ShellPaneZoomLevels::default();

        assert_eq!(levels.get(ShellViewMode::Icons), 4);
        assert_eq!(levels.get(ShellViewMode::Compact), 3);
        assert_eq!(levels.get(ShellViewMode::Details), 3);

        levels.set(ShellViewMode::Icons, 7);
        levels.set(ShellViewMode::Compact, 99);
        assert_eq!(levels.get(ShellViewMode::Icons), 7);
        assert_eq!(levels.get(ShellViewMode::Compact), 16);
        assert_eq!(levels.get(ShellViewMode::Details), 3);
    }

    #[test]
    fn pane_visible_slot_pools_are_addressed_by_pane_id() {
        let path = PathBuf::from("/tmp/shared-name");
        let mut pools = ShellPaneVisibleSlotPools::default();

        let slot0_stats = pools.update_visible_items(ShellPaneId::SLOT_0, [path.clone()]);
        assert_eq!(slot0_stats.active, 1);
        assert!(
            pools
                .get(ShellPaneId::SLOT_0)
                .slot_for_path(&path)
                .is_some()
        );
        assert!(
            pools
                .get(ShellPaneId::SLOT_1)
                .slot_for_path(&path)
                .is_none()
        );

        let slot1_stats = pools.update_visible_items(ShellPaneId::SLOT_1, [path.clone()]);
        assert_eq!(slot1_stats.active, 1);
        assert!(
            pools
                .get(ShellPaneId::SLOT_1)
                .slot_for_path(&path)
                .is_some()
        );

        pools.clear(ShellPaneId::SLOT_1);
        assert!(
            pools
                .get(ShellPaneId::SLOT_0)
                .slot_for_path(&path)
                .is_some()
        );
        assert!(
            pools
                .get(ShellPaneId::SLOT_1)
                .slot_for_path(&path)
                .is_none()
        );
    }
}

use std::collections::HashMap;
use std::ops::{Index, IndexMut};
use std::path::{Path, PathBuf};
use std::time::Instant;

use fika_core::{Entry, ItemLayout, ViewRect, ViewSize, read_entries_sync};

use crate::filtered_indexes_for_entries;
use crate::ui::{
    metrics::{
        FILE_MANAGER_COMPACT_ZOOM_LEVEL_DEFAULT, FILE_MANAGER_DETAILS_ZOOM_LEVEL_DEFAULT,
        FILE_MANAGER_ICONS_ZOOM_LEVEL_DEFAULT, FILE_MANAGER_ZOOM_LEVEL_MAX,
        FILE_MANAGER_ZOOM_LEVEL_MIN,
    },
    options::ShellViewMode,
    selection::ShellSelection,
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
                FILE_MANAGER_ICONS_ZOOM_LEVEL_DEFAULT,
                FILE_MANAGER_COMPACT_ZOOM_LEVEL_DEFAULT,
                FILE_MANAGER_DETAILS_ZOOM_LEVEL_DEFAULT,
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

#[derive(Clone, Debug)]
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
        fika_log!(
            "[fika] split-pane path={} entries={} dirs={} files={} visible={} load={}us",
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

#[derive(Clone, Debug)]
pub(crate) struct ShellPaneStates {
    panes: [Option<ShellPaneState>; 2],
}

impl ShellPaneStates {
    pub(crate) fn new(slot0: ShellPaneState) -> Self {
        Self {
            panes: [Some(slot0), None],
        }
    }

    pub(crate) fn get(&self, pane: ShellPaneId) -> Option<&ShellPaneState> {
        self.panes[pane.index()].as_ref()
    }

    pub(crate) fn get_mut(&mut self, pane: ShellPaneId) -> Option<&mut ShellPaneState> {
        self.panes[pane.index()].as_mut()
    }

    pub(crate) fn set(&mut self, pane: ShellPaneId, state: ShellPaneState) {
        self.panes[pane.index()] = Some(state);
    }

    pub(crate) fn take(&mut self, pane: ShellPaneId) -> Option<ShellPaneState> {
        self.panes[pane.index()].take()
    }

    pub(crate) fn is_open(&self, pane: ShellPaneId) -> bool {
        self.panes[pane.index()].is_some()
    }
}

impl Index<ShellPaneId> for ShellPaneStates {
    type Output = ShellPaneState;

    fn index(&self, pane: ShellPaneId) -> &Self::Output {
        self.get(pane).expect("pane slot is not open")
    }
}

impl IndexMut<ShellPaneId> for ShellPaneStates {
    fn index_mut(&mut self, pane: ShellPaneId) -> &mut Self::Output {
        self.get_mut(pane).expect("pane slot is not open")
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
        let loading = state.pending_path.is_some();
        Self {
            path: state.display_path(),
            view_mode: state.view_mode,
            zoom_level: state.zoom_level(),
            entries: if loading { &[] } else { &state.entries },
            dir_count: if loading { 0 } else { state.dir_count },
            filtered_indexes: if loading {
                &[]
            } else {
                &state.filtered_indexes
            },
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
    pub(crate) slot_id: u64,
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellVisibleItemSlot {
    slot_id: u64,
    visible_epoch: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellVisibleItemSlotStats {
    pub(crate) active: usize,
    pub(crate) free: usize,
    pub(crate) reused: usize,
    pub(crate) recycled: usize,
    pub(crate) allocated: usize,
}

impl ShellVisibleItemSlotStats {
    pub(crate) fn merged(self, other: Self) -> Self {
        Self {
            active: self.active + other.active,
            free: self.free + other.free,
            reused: self.reused + other.reused,
            recycled: self.recycled + other.recycled,
            allocated: self.allocated + other.allocated,
        }
    }
}

pub(crate) trait ShellVisibleSlotItem {
    fn visible_slot_path(&self) -> Option<&Path>;
    fn visible_slot_id(&self) -> u64;
    fn set_visible_slot_id(&mut self, slot_id: u64);
    fn release_visible_slot_path(&mut self) {}
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ShellVisibleItemSlotPool {
    next_slot_id: u64,
    visible_epoch: u64,
    slot_by_path: HashMap<PathBuf, ShellVisibleItemSlot>,
    free_slots: Vec<u64>,
}

impl ShellVisibleItemSlotPool {
    const MAX_FREE_SLOTS: usize = 100;

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn update_visible_items<P>(
        &mut self,
        visible_paths: impl IntoIterator<Item = P>,
    ) -> ShellVisibleItemSlotStats
    where
        P: AsRef<Path>,
    {
        self.visible_epoch = self.visible_epoch.wrapping_add(1).max(1);
        let visible_epoch = self.visible_epoch;
        let mut reused = 0usize;

        for path in visible_paths {
            let path = path.as_ref();
            if let Some(slot) = self.slot_by_path.get_mut(path) {
                reused += usize::from(slot.visible_epoch != visible_epoch);
                slot.visible_epoch = visible_epoch;
            } else {
                self.slot_by_path.insert(
                    path.to_path_buf(),
                    ShellVisibleItemSlot {
                        slot_id: 0,
                        visible_epoch,
                    },
                );
            }
        }

        let free_slots = &mut self.free_slots;
        self.slot_by_path.retain(|_, slot| {
            if slot.visible_epoch == visible_epoch {
                true
            } else {
                if slot.slot_id != 0 {
                    free_slots.push(slot.slot_id);
                }
                false
            }
        });
        if self.free_slots.len() > Self::MAX_FREE_SLOTS {
            self.free_slots.truncate(Self::MAX_FREE_SLOTS);
        }

        let mut recycled = 0usize;
        let mut allocated = 0usize;
        for slot in self.slot_by_path.values_mut() {
            if slot.slot_id != 0 {
                continue;
            }
            if let Some(slot_id) = self.free_slots.pop() {
                slot.slot_id = slot_id;
                recycled += 1;
            } else {
                self.next_slot_id += 1;
                slot.slot_id = self.next_slot_id;
                allocated += 1;
            }
        }

        ShellVisibleItemSlotStats {
            active: self.slot_by_path.len(),
            free: self.free_slots.len(),
            reused,
            recycled,
            allocated,
        }
    }

    pub(crate) fn update_visible_item_slots(
        &mut self,
        visible_items: &mut [impl ShellVisibleSlotItem],
    ) -> ShellVisibleItemSlotStats {
        self.visible_epoch = self.visible_epoch.wrapping_add(1).max(1);
        let visible_epoch = self.visible_epoch;
        let mut reused = 0usize;

        for item in visible_items.iter_mut() {
            let Some(path) = item.visible_slot_path() else {
                item.set_visible_slot_id(0);
                continue;
            };
            if let Some(slot) = self.slot_by_path.get_mut(path) {
                reused += usize::from(slot.visible_epoch != visible_epoch);
                slot.visible_epoch = visible_epoch;
                item.set_visible_slot_id(slot.slot_id);
            } else {
                self.slot_by_path.insert(
                    path.to_path_buf(),
                    ShellVisibleItemSlot {
                        slot_id: 0,
                        visible_epoch,
                    },
                );
                item.set_visible_slot_id(0);
            }
        }

        let free_slots = &mut self.free_slots;
        self.slot_by_path.retain(|_, slot| {
            if slot.visible_epoch == visible_epoch {
                true
            } else {
                if slot.slot_id != 0 {
                    free_slots.push(slot.slot_id);
                }
                false
            }
        });
        if self.free_slots.len() > Self::MAX_FREE_SLOTS {
            self.free_slots.truncate(Self::MAX_FREE_SLOTS);
        }

        let mut recycled = 0usize;
        let mut allocated = 0usize;
        for slot in self.slot_by_path.values_mut() {
            if slot.slot_id != 0 {
                continue;
            }
            if let Some(slot_id) = self.free_slots.pop() {
                slot.slot_id = slot_id;
                recycled += 1;
            } else {
                self.next_slot_id += 1;
                slot.slot_id = self.next_slot_id;
                allocated += 1;
            }
        }

        for item in visible_items.iter_mut() {
            let slot_id = if item.visible_slot_id() != 0 {
                item.visible_slot_id()
            } else {
                item.visible_slot_path()
                    .and_then(|path| self.slot_for_path(path))
                    .unwrap_or_default()
            };
            item.set_visible_slot_id(slot_id);
            item.release_visible_slot_path();
        }

        ShellVisibleItemSlotStats {
            active: self.slot_by_path.len(),
            free: self.free_slots.len(),
            reused,
            recycled,
            allocated,
        }
    }

    pub(crate) fn slot_for_path(&self, path: &Path) -> Option<u64> {
        self.slot_by_path
            .get(path)
            .map(|slot| slot.slot_id)
            .filter(|slot_id| *slot_id != 0)
    }

    pub(crate) fn clear(&mut self) {
        self.slot_by_path.clear();
        self.free_slots.clear();
        self.visible_epoch = 0;
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ShellPaneVisibleSlotPools {
    pools: [ShellVisibleItemSlotPool; 2],
}

impl ShellPaneVisibleSlotPools {
    pub(crate) fn get(&self, pane: ShellPaneId) -> &ShellVisibleItemSlotPool {
        &self.pools[pane.index()]
    }

    pub(crate) fn get_mut(&mut self, pane: ShellPaneId) -> &mut ShellVisibleItemSlotPool {
        &mut self.pools[pane.index()]
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn update_visible_items<P>(
        &mut self,
        pane: ShellPaneId,
        visible_paths: impl IntoIterator<Item = P>,
    ) -> ShellVisibleItemSlotStats
    where
        P: AsRef<Path>,
    {
        self.pools[pane.index()].update_visible_items(visible_paths)
    }

    pub(crate) fn clear(&mut self, pane: ShellPaneId) {
        self.pools[pane.index()].clear();
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

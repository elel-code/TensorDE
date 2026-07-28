use super::directory::{DirectoryLister, DirectoryListerEvent, LoadMode};
use super::entries::ItemId;
use super::model::{DirectoryModel, DirectoryModelSignal, SortDescriptor, SortOrder, SortRole};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PaneId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Generation(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestSerial(pub u64);

#[derive(Clone, Debug, Default)]
pub struct PaneGenerationCounter {
    current: u64,
}

impl PaneGenerationCounter {
    pub fn advance(&mut self) -> Generation {
        self.current += 1;
        Generation(self.current)
    }

    pub fn current(&self) -> Generation {
        Generation(self.current)
    }

    pub fn is_current(&self, generation: Generation) -> bool {
        self.current == generation.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct PaneIdAllocator {
    next: u64,
}

impl PaneIdAllocator {
    pub fn allocate(&mut self) -> PaneId {
        self.next += 1;
        PaneId(self.next)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionState {
    selected_ids: Vec<ItemId>,
    excluded_ids: Vec<ItemId>,
    all_selected: bool,
    anchor_id: Option<ItemId>,
    active_id: Option<ItemId>,
    revision: u64,
}

impl SelectionState {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn selected_ids(&self) -> &[ItemId] {
        &self.selected_ids
    }

    pub fn anchor_id(&self) -> Option<ItemId> {
        self.anchor_id
    }

    pub fn active_id(&self) -> Option<ItemId> {
        self.active_id
    }

    pub fn len(&self) -> usize {
        self.selected_ids.len()
    }

    pub fn count_for_model(&self, model_len: usize) -> usize {
        if self.all_selected {
            model_len.saturating_sub(self.excluded_ids.len())
        } else {
            self.selected_ids.len()
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.all_selected && self.selected_ids.is_empty()
    }

    pub fn is_all_selected(&self) -> bool {
        self.all_selected
    }

    pub fn is_excluded(&self, id: ItemId) -> bool {
        self.excluded_ids.contains(&id)
    }

    pub fn is_selected(&self, id: ItemId) -> bool {
        if self.all_selected {
            !self.is_excluded(id)
        } else {
            self.selected_ids.contains(&id)
        }
    }

    pub fn is_only_selected(&self, id: ItemId) -> bool {
        !self.all_selected
            && self.excluded_ids.is_empty()
            && self.selected_ids.as_slice() == [id]
            && self.anchor_id == Some(id)
            && self.active_id == Some(id)
    }

    pub fn clear(&mut self) {
        self.selected_ids.clear();
        self.excluded_ids.clear();
        self.all_selected = false;
        self.anchor_id = None;
        self.active_id = None;
        self.bump_revision();
    }

    pub fn select_only(&mut self, id: ItemId) {
        self.selected_ids.clear();
        self.excluded_ids.clear();
        self.all_selected = false;
        self.selected_ids.push(id);
        self.anchor_id = Some(id);
        self.active_id = Some(id);
        self.bump_revision();
    }

    pub fn toggle(&mut self, id: ItemId) -> bool {
        if self.all_selected {
            self.anchor_id = Some(id);
            self.active_id = Some(id);
            if let Some(index) = self
                .excluded_ids
                .iter()
                .position(|excluded| *excluded == id)
            {
                self.excluded_ids.remove(index);
                self.bump_revision();
                return true;
            }
            self.excluded_ids.push(id);
            self.bump_revision();
            return false;
        }
        self.anchor_id = Some(id);
        self.active_id = Some(id);
        if let Some(index) = self
            .selected_ids
            .iter()
            .position(|selected| *selected == id)
        {
            self.selected_ids.remove(index);
            self.bump_revision();
            false
        } else {
            self.selected_ids.push(id);
            self.bump_revision();
            true
        }
    }

    pub fn replace(&mut self, ids: Vec<ItemId>) {
        self.all_selected = false;
        self.excluded_ids.clear();
        let mut seen = BTreeSet::new();
        self.selected_ids = ids
            .into_iter()
            .filter(|id| id.is_assigned() && seen.insert(*id))
            .collect();
        if self
            .anchor_id
            .is_none_or(|anchor| !self.selected_ids.contains(&anchor))
        {
            self.anchor_id = self.selected_ids.first().copied();
        }
        if self
            .active_id
            .is_none_or(|active| !self.selected_ids.contains(&active))
        {
            self.active_id = self.selected_ids.first().copied();
        }
        self.bump_revision();
    }

    pub fn select_all(&mut self, anchor_id: Option<ItemId>) {
        if anchor_id.is_none() {
            self.clear();
            return;
        }
        self.selected_ids.clear();
        self.excluded_ids.clear();
        self.all_selected = true;
        self.anchor_id = anchor_id;
        self.active_id = anchor_id;
        self.bump_revision();
    }

    pub fn replace_range(&mut self, anchor_id: ItemId, ids: Vec<ItemId>) {
        self.replace(ids);
        self.anchor_id = Some(anchor_id);
    }

    pub fn replace_range_with_active(
        &mut self,
        anchor_id: ItemId,
        active_id: ItemId,
        ids: Vec<ItemId>,
    ) {
        self.replace(ids);
        self.anchor_id = Some(anchor_id);
        self.active_id = Some(active_id);
    }

    pub fn retain_existing_by(
        &mut self,
        mut exists: impl FnMut(ItemId) -> bool,
        fallback_id: Option<ItemId>,
    ) {
        if self.all_selected {
            let before_excluded_len = self.excluded_ids.len();
            let before_anchor = self.anchor_id;
            let before_active = self.active_id;
            self.excluded_ids.retain(|id| exists(*id));
            if self.anchor_id.is_some_and(|anchor| !exists(anchor)) {
                self.anchor_id = fallback_id;
            }
            if self.active_id.is_some_and(|active| !exists(active)) {
                self.active_id = fallback_id;
            }
            if fallback_id.is_none() {
                self.clear();
            }
            if self.excluded_ids.len() != before_excluded_len
                || self.anchor_id != before_anchor
                || self.active_id != before_active
            {
                self.bump_revision();
            }
            return;
        }

        let before_selected_len = self.selected_ids.len();
        let before_anchor = self.anchor_id;
        let before_active = self.active_id;
        self.selected_ids.retain(|id| exists(*id));
        if self.anchor_id.is_some_and(|anchor| !exists(anchor)) {
            self.anchor_id = self.selected_ids.first().copied();
        }
        if self.active_id.is_some_and(|active| !exists(active)) {
            self.active_id = self.selected_ids.first().copied();
        }
        if self.selected_ids.len() != before_selected_len
            || self.anchor_id != before_anchor
            || self.active_id != before_active
        {
            self.bump_revision();
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionMove {
    Previous,
    Next,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewState {
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub zoom_level: i32,
    pub view_mode: ViewMode,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            max_scroll_x: 0.0,
            max_scroll_y: 0.0,
            viewport_width: 720.0,
            viewport_height: 520.0,
            zoom_level: DEFAULT_ZOOM_LEVEL,
            view_mode: ViewMode::Compact,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewMode {
    Icons,
    Compact,
    Details,
}

impl ViewMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "icons" => Ok(Self::Icons),
            "compact" => Ok(Self::Compact),
            "details" => Ok(Self::Details),
            _ => Err(format!("unknown view mode: {value}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Icons => "icons",
            Self::Compact => "compact",
            Self::Details => "details",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Icons => "Icons",
            Self::Compact => "Compact",
            Self::Details => "Details",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Icons => Self::Compact,
            Self::Compact => Self::Details,
            Self::Details => Self::Icons,
        }
    }
}

pub const MIN_ZOOM_LEVEL: i32 = 0;
pub const MAX_ZOOM_LEVEL: i32 = 16;
pub const DEFAULT_ZOOM_LEVEL: i32 = 3;

pub fn icon_size_for_zoom_level(level: i32) -> f32 {
    match level.clamp(MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL) {
        0 => 16.0,
        1 => 22.0,
        2 => 32.0,
        3 => 48.0,
        4 => 64.0,
        level => 64.0 + ((level - 4) as f32 * 16.0),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZoomChange {
    In,
    Out,
    Reset,
}

impl ViewState {
    pub fn reset_scroll(&mut self) {
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
    }

    pub fn icon_size(&self) -> f32 {
        icon_size_for_zoom_level(self.zoom_level)
    }

    pub fn set_zoom_level(&mut self, level: i32) -> bool {
        let level = level.clamp(MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL);
        if self.zoom_level == level {
            return false;
        }
        self.zoom_level = level;
        true
    }

    pub fn apply_zoom_change(&mut self, change: ZoomChange) -> bool {
        match change {
            ZoomChange::In => self.set_zoom_level(self.zoom_level + 1),
            ZoomChange::Out => self.set_zoom_level(self.zoom_level - 1),
            ZoomChange::Reset => self.set_zoom_level(DEFAULT_ZOOM_LEVEL),
        }
    }

    pub fn set_view_mode(&mut self, view_mode: ViewMode) -> bool {
        if self.view_mode == view_mode {
            return false;
        }
        self.view_mode = view_mode;
        self.reset_scroll();
        true
    }
}

#[derive(Debug)]
pub struct PaneState {
    pub id: PaneId,
    pub generation: Generation,
    pub current_dir: PathBuf,
    pub model: DirectoryModel,
    pub selection: SelectionState,
    pub view: ViewState,
    pub lister: DirectoryLister,
    history_back: Vec<PathBuf>,
    history_forward: Vec<PathBuf>,
    sort_order_by_role: HashMap<SortRole, SortOrder>,
}

impl PaneState {
    pub fn new(id: PaneId, current_dir: PathBuf) -> Self {
        let generation = Generation::default();
        Self {
            id,
            generation,
            current_dir: current_dir.clone(),
            model: DirectoryModel::for_directory(current_dir.clone()),
            selection: SelectionState::default(),
            view: ViewState::default(),
            lister: DirectoryLister::new(id, current_dir, generation),
            history_back: Vec::new(),
            history_forward: Vec::new(),
            sort_order_by_role: HashMap::new(),
        }
    }

    pub fn preferred_sort_order(&self, role: SortRole) -> SortOrder {
        self.sort_order_by_role
            .get(&role)
            .copied()
            .unwrap_or_else(|| role.default_order())
    }

    fn remember_sort_order(&mut self, sort: SortDescriptor) {
        self.sort_order_by_role.insert(sort.role, sort.order);
    }

    pub fn navigate_to(&mut self, path: PathBuf, generation: Generation) {
        if self.current_dir != path {
            self.history_back.push(self.current_dir.clone());
            self.history_forward.clear();
            self.current_dir = path.clone();
            self.selection.clear();
            self.view.reset_scroll();
        }
        self.generation = generation;
        self.lister.set_target(self.id, path, generation);
    }

    pub fn can_go_back(&self) -> bool {
        !self.history_back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.history_forward.is_empty()
    }

    pub fn pop_back(&mut self, generation: Generation) -> Option<PathBuf> {
        let previous = self.history_back.pop()?;
        self.history_forward.push(self.current_dir.clone());
        self.current_dir = previous.clone();
        self.selection.clear();
        self.view.reset_scroll();
        self.generation = generation;
        self.lister
            .set_target(self.id, previous.clone(), generation);
        Some(previous)
    }

    pub fn pop_forward(&mut self, generation: Generation) -> Option<PathBuf> {
        let next = self.history_forward.pop()?;
        self.history_back.push(self.current_dir.clone());
        self.current_dir = next.clone();
        self.selection.clear();
        self.view.reset_scroll();
        self.generation = generation;
        self.lister.set_target(self.id, next.clone(), generation);
        Some(next)
    }
}

#[derive(Debug)]
pub struct PaneController {
    allocator: PaneIdAllocator,
    generation_counter: PaneGenerationCounter,
    panes: HashMap<PaneId, PaneState>,
    order: Vec<PaneId>,
    focused: Option<PaneId>,
}

impl PaneController {
    pub fn new(start_dir: PathBuf) -> Self {
        let mut allocator = PaneIdAllocator::default();
        let id = allocator.allocate();
        let pane = PaneState::new(id, start_dir);
        let panes = HashMap::from([(id, pane)]);
        Self {
            allocator,
            generation_counter: PaneGenerationCounter::default(),
            panes,
            order: vec![id],
            focused: Some(id),
        }
    }

    pub fn pane_ids(&self) -> &[PaneId] {
        &self.order
    }

    pub fn focused(&self) -> Option<PaneId> {
        self.focused
    }

    pub fn focus(&mut self, pane_id: PaneId) -> bool {
        if self.panes.contains_key(&pane_id) {
            self.focused = Some(pane_id);
            true
        } else {
            false
        }
    }

    pub fn pane(&self, pane_id: PaneId) -> Option<&PaneState> {
        self.panes.get(&pane_id)
    }

    pub fn pane_mut(&mut self, pane_id: PaneId) -> Option<&mut PaneState> {
        self.panes.get_mut(&pane_id)
    }

    pub fn split(&mut self, source: PaneId) -> Option<PaneId> {
        let source_pane = self.panes.get(&source)?;
        let current_dir = source_pane.current_dir.clone();
        let generation = source_pane.generation;
        let model = source_pane.model.fork_for_pane();
        let view = source_pane.view.clone();
        let sort_order_by_role = source_pane.sort_order_by_role.clone();
        let id = self.allocator.allocate();
        let mut pane = PaneState::new(id, current_dir);
        pane.generation = generation;
        pane.lister
            .set_target(id, pane.current_dir.clone(), generation);
        pane.model = model;
        pane.view = view;
        pane.sort_order_by_role = sort_order_by_role;
        self.panes.insert(id, pane);
        let insert_at = self
            .order
            .iter()
            .position(|existing| *existing == source)
            .map_or(self.order.len(), |index| index + 1);
        self.order.insert(insert_at, id);
        self.focused = Some(id);
        Some(id)
    }

    pub fn close(&mut self, pane_id: PaneId) -> bool {
        if self.order.len() <= 1 || self.panes.remove(&pane_id).is_none() {
            return false;
        }
        self.order.retain(|id| *id != pane_id);
        if self.focused == Some(pane_id) {
            self.focused = self.order.first().copied();
        }
        true
    }

    pub fn load(&mut self, pane_id: PaneId, path: PathBuf) -> Option<DirectoryListerEvent> {
        let generation = self.generation_counter.advance();
        let pane = self.panes.get_mut(&pane_id)?;
        pane.navigate_to(path, generation);
        Some(pane.lister.load_directory(LoadMode::Load))
    }

    pub fn reload(&mut self, pane_id: PaneId) -> Option<DirectoryListerEvent> {
        let pane = self.panes.get_mut(&pane_id)?;
        Some(pane.lister.load_directory(LoadMode::Reload))
    }

    pub fn can_go_back(&self, pane_id: PaneId) -> bool {
        self.panes.get(&pane_id).is_some_and(PaneState::can_go_back)
    }

    pub fn can_go_forward(&self, pane_id: PaneId) -> bool {
        self.panes
            .get(&pane_id)
            .is_some_and(PaneState::can_go_forward)
    }

    pub fn go_back(&mut self, pane_id: PaneId) -> Option<DirectoryListerEvent> {
        let generation = self.generation_counter.advance();
        let pane = self.panes.get_mut(&pane_id)?;
        pane.pop_back(generation)?;
        Some(pane.lister.load_directory(LoadMode::Load))
    }

    pub fn go_forward(&mut self, pane_id: PaneId) -> Option<DirectoryListerEvent> {
        let generation = self.generation_counter.advance();
        let pane = self.panes.get_mut(&pane_id)?;
        pane.pop_forward(generation)?;
        Some(pane.lister.load_directory(LoadMode::Load))
    }

    pub fn reload_panes_showing(&mut self, path: &Path) -> Vec<DirectoryListerEvent> {
        let pane_ids = self
            .order
            .iter()
            .copied()
            .filter(|pane_id| {
                self.panes
                    .get(pane_id)
                    .is_some_and(|pane| pane.current_dir == path)
            })
            .collect::<Vec<_>>();
        pane_ids
            .into_iter()
            .filter_map(|pane_id| self.reload(pane_id))
            .collect()
    }

    pub fn selected_paths(&self, pane_id: PaneId) -> Option<Vec<PathBuf>> {
        let pane = self.panes.get(&pane_id)?;
        Some(selected_paths_from_model(pane))
    }

    pub fn selection_anchor_path(&self, pane_id: PaneId) -> Option<PathBuf> {
        let pane = self.panes.get(&pane_id)?;
        path_for_selection_id(pane, pane.selection.anchor_id()?)
    }

    pub fn selection_active_path(&self, pane_id: PaneId) -> Option<PathBuf> {
        let pane = self.panes.get(&pane_id)?;
        path_for_selection_id(pane, pane.selection.active_id()?)
    }

    pub fn selected_count(&self, pane_id: PaneId) -> Option<usize> {
        self.panes
            .get(&pane_id)
            .map(|pane| pane.selection.count_for_model(pane.model.len()))
    }

    pub fn is_selected(&self, pane_id: PaneId, path: &Path) -> bool {
        self.panes.get(&pane_id).is_some_and(|pane| {
            pane.model
                .index_of_path(path)
                .map(|index| pane.selection.is_selected(pane.model.entries()[index].id))
                .unwrap_or(false)
        })
    }

    pub fn select_only(&mut self, pane_id: PaneId, path: PathBuf) -> bool {
        let Some(pane) = self.panes.get_mut(&pane_id) else {
            return false;
        };
        let Some(entry_id) = pane
            .model
            .index_of_path(&path)
            .map(|index| pane.model.entries()[index].id)
        else {
            return false;
        };
        if pane.selection.is_only_selected(entry_id) {
            return false;
        }
        pane.selection.select_only(entry_id);
        true
    }

    pub fn toggle_selection(&mut self, pane_id: PaneId, path: PathBuf) -> Option<bool> {
        let pane = self.panes.get_mut(&pane_id)?;
        let entry_id = pane
            .model
            .index_of_path(&path)
            .map(|index| pane.model.entries()[index].id)?;
        Some(pane.selection.toggle(entry_id))
    }

    pub fn select_range_to(&mut self, pane_id: PaneId, path: PathBuf) -> Option<usize> {
        let pane = self.panes.get_mut(&pane_id)?;
        let target_index = pane.model.index_of_path(&path)?;
        let target_id = pane.model.entries()[target_index].id;
        let anchor_id = pane
            .selection
            .anchor_id()
            .filter(|id| pane.model.index_of_id(*id).is_some())
            .unwrap_or(target_id);
        let anchor_index = pane.model.index_of_id(anchor_id).unwrap_or(target_index);
        let (start, end) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        let ids = pane.model.entries()[start..=end]
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection
            .replace_range_with_active(anchor_id, target_id, ids);
        Some(count)
    }

    pub fn move_selection(
        &mut self,
        pane_id: PaneId,
        direction: SelectionMove,
        extend: bool,
    ) -> Option<usize> {
        let pane = self.panes.get_mut(&pane_id)?;
        if pane.model.is_empty() {
            return None;
        }

        let current_index = pane
            .selection
            .active_id()
            .and_then(|active| pane.model.index_of_id(active))
            .or_else(|| {
                pane.selection
                    .selected_ids()
                    .last()
                    .and_then(|id| pane.model.index_of_id(*id))
            });
        let target_index = match (current_index, direction) {
            (Some(index), SelectionMove::Previous) => index.saturating_sub(1),
            (Some(index), SelectionMove::Next) => (index + 1).min(pane.model.len() - 1),
            (None, SelectionMove::Previous) => pane.model.len() - 1,
            (None, SelectionMove::Next) => 0,
        };
        let target_id = pane.model.entries()[target_index].id;

        if !extend {
            pane.selection.select_only(target_id);
            return Some(1);
        }

        let anchor_id = pane
            .selection
            .anchor_id()
            .filter(|id| pane.model.index_of_id(*id).is_some())
            .unwrap_or(target_id);
        let anchor_index = pane.model.index_of_id(anchor_id).unwrap_or(target_index);
        let (start, end) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        let ids = pane.model.entries()[start..=end]
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection
            .replace_range_with_active(anchor_id, target_id, ids);
        Some(count)
    }

    pub fn select_all(&mut self, pane_id: PaneId) -> Option<usize> {
        let pane = self.panes.get_mut(&pane_id)?;
        let count = pane.model.len();
        let anchor_id = pane.model.get(0).map(|entry| entry.id);
        pane.selection.select_all(anchor_id);
        Some(count)
    }

    pub fn replace_selection_by_indexes(
        &mut self,
        pane_id: PaneId,
        indexes: impl IntoIterator<Item = usize>,
    ) -> Option<usize> {
        let pane = self.panes.get_mut(&pane_id)?;
        let ids = indexes
            .into_iter()
            .filter_map(|index| pane.model.get(index).map(|entry| entry.id))
            .collect::<Vec<_>>();
        let count = ids.len();
        pane.selection.replace(ids);
        Some(count)
    }

    pub fn set_zoom_level(&mut self, pane_id: PaneId, level: i32) -> Option<ViewState> {
        let pane = self.panes.get_mut(&pane_id)?;
        pane.view.set_zoom_level(level);
        Some(pane.view.clone())
    }

    pub fn set_view_mode(&mut self, pane_id: PaneId, view_mode: ViewMode) -> Option<ViewState> {
        let pane = self.panes.get_mut(&pane_id)?;
        pane.view.set_view_mode(view_mode);
        Some(pane.view.clone())
    }

    pub fn apply_zoom_change(&mut self, pane_id: PaneId, change: ZoomChange) -> Option<ViewState> {
        let pane = self.panes.get_mut(&pane_id)?;
        pane.view.apply_zoom_change(change);
        Some(pane.view.clone())
    }

    pub fn set_sort(
        &mut self,
        pane_id: PaneId,
        sort: SortDescriptor,
    ) -> Option<Vec<DirectoryModelSignal>> {
        let pane = self.panes.get_mut(&pane_id)?;
        pane.remember_sort_order(sort);
        Some(apply_pane_sort(pane, sort))
    }

    pub fn set_sort_role(
        &mut self,
        pane_id: PaneId,
        role: SortRole,
    ) -> Option<(SortDescriptor, Vec<DirectoryModelSignal>)> {
        let pane = self.panes.get_mut(&pane_id)?;
        let current_sort = pane.model.sort_descriptor();
        let sort = SortDescriptor {
            role,
            order: pane.preferred_sort_order(role),
            folders_first: current_sort.folders_first,
            hidden_last: current_sort.hidden_last,
        };
        pane.remember_sort_order(sort);
        let signals = apply_pane_sort(pane, sort);
        Some((sort, signals))
    }

    pub fn set_sort_order(
        &mut self,
        pane_id: PaneId,
        order: SortOrder,
    ) -> Option<(SortDescriptor, Vec<DirectoryModelSignal>)> {
        let pane = self.panes.get_mut(&pane_id)?;
        let mut sort = pane.model.sort_descriptor();
        sort.order = order;
        pane.remember_sort_order(sort);
        let signals = apply_pane_sort(pane, sort);
        Some((sort, signals))
    }

    pub fn set_sort_folders_first(
        &mut self,
        pane_id: PaneId,
        folders_first: bool,
    ) -> Option<(SortDescriptor, Vec<DirectoryModelSignal>)> {
        let pane = self.panes.get_mut(&pane_id)?;
        let mut sort = pane.model.sort_descriptor();
        sort.folders_first = folders_first;
        pane.remember_sort_order(sort);
        let signals = apply_pane_sort(pane, sort);
        Some((sort, signals))
    }

    pub fn set_sort_hidden_last(
        &mut self,
        pane_id: PaneId,
        hidden_last: bool,
    ) -> Option<(SortDescriptor, Vec<DirectoryModelSignal>)> {
        let pane = self.panes.get_mut(&pane_id)?;
        let mut sort = pane.model.sort_descriptor();
        sort.hidden_last = hidden_last;
        pane.remember_sort_order(sort);
        let signals = apply_pane_sort(pane, sort);
        Some((sort, signals))
    }

    pub fn preferred_sort_order(&self, pane_id: PaneId, role: SortRole) -> Option<SortOrder> {
        self.panes
            .get(&pane_id)
            .map(|pane| pane.preferred_sort_order(role))
    }

    pub fn sort_descriptor(&self, pane_id: PaneId) -> Option<SortDescriptor> {
        self.panes
            .get(&pane_id)
            .map(|pane| pane.model.sort_descriptor())
    }

    pub fn scroll_view(
        &mut self,
        pane_id: PaneId,
        delta_x: f32,
        delta_y: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> Option<ViewState> {
        let pane = self.panes.get_mut(&pane_id)?;
        let max_scroll_x = max_scroll_x.max(0.0);
        let max_scroll_y = max_scroll_y.max(0.0);
        let next_x = (pane.view.scroll_x + delta_x).clamp(0.0, max_scroll_x);
        let next_y = (pane.view.scroll_y + delta_y).clamp(0.0, max_scroll_y);
        if next_x == pane.view.scroll_x
            && next_y == pane.view.scroll_y
            && viewport_value_eq(pane.view.max_scroll_x, max_scroll_x)
            && viewport_value_eq(pane.view.max_scroll_y, max_scroll_y)
        {
            return Some(pane.view.clone());
        }
        pane.view.max_scroll_x = max_scroll_x;
        pane.view.max_scroll_y = max_scroll_y;
        pane.view.scroll_x = next_x;
        pane.view.scroll_y = next_y;
        Some(pane.view.clone())
    }

    pub fn set_view_scroll(
        &mut self,
        pane_id: PaneId,
        scroll_x: f32,
        scroll_y: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> Option<ViewState> {
        let pane = self.panes.get_mut(&pane_id)?;
        pane.view.max_scroll_x = max_scroll_x.max(0.0);
        pane.view.max_scroll_y = max_scroll_y.max(0.0);
        pane.view.scroll_x = scroll_x.clamp(0.0, pane.view.max_scroll_x);
        pane.view.scroll_y = scroll_y.clamp(0.0, pane.view.max_scroll_y);
        Some(pane.view.clone())
    }

    pub fn set_viewport_bounds(
        &mut self,
        pane_id: PaneId,
        viewport_width: f32,
        viewport_height: f32,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> Option<bool> {
        let pane = self.panes.get_mut(&pane_id)?;
        let viewport_width = normalize_viewport_extent(viewport_width);
        let viewport_height = normalize_viewport_extent(viewport_height);
        let max_scroll_x = max_scroll_x.max(0.0);
        let max_scroll_y = max_scroll_y.max(0.0);
        let scroll_x = pane.view.scroll_x.clamp(0.0, max_scroll_x);
        let scroll_y = pane.view.scroll_y.clamp(0.0, max_scroll_y);
        if viewport_value_eq(pane.view.viewport_width, viewport_width)
            && viewport_value_eq(pane.view.viewport_height, viewport_height)
            && viewport_value_eq(pane.view.max_scroll_x, max_scroll_x)
            && viewport_value_eq(pane.view.max_scroll_y, max_scroll_y)
            && viewport_value_eq(pane.view.scroll_x, scroll_x)
            && viewport_value_eq(pane.view.scroll_y, scroll_y)
        {
            return Some(false);
        }
        pane.view.viewport_width = viewport_width;
        pane.view.viewport_height = viewport_height;
        pane.view.max_scroll_x = max_scroll_x;
        pane.view.max_scroll_y = max_scroll_y;
        pane.view.scroll_x = scroll_x;
        pane.view.scroll_y = scroll_y;
        Some(true)
    }

    pub fn clear_selection(&mut self, pane_id: PaneId) -> bool {
        let Some(pane) = self.panes.get_mut(&pane_id) else {
            return false;
        };
        pane.selection.clear();
        true
    }

    pub fn apply_lister_event(
        &mut self,
        event: DirectoryListerEvent,
    ) -> Option<Vec<DirectoryModelSignal>> {
        let pane_id = event.pane_id();
        let pane = self.panes.get_mut(&pane_id)?;
        if !event.matches_target(pane.id, pane.generation, &pane.current_dir) {
            return None;
        }
        let signals = pane.lister.apply_event_to_model(event, &mut pane.model);
        if !signals.is_empty() {
            let fallback_id = pane.model.get(0).map(|entry| entry.id);
            let model = &pane.model;
            pane.selection
                .retain_existing_by(|id| model.index_of_id(id).is_some(), fallback_id);
        }
        Some(signals)
    }
}

include!("pane/helpers.rs");

#[cfg(test)]
#[path = "pane/tests.rs"]
mod tests;

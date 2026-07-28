use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::Arc;

use fika_core::{CompactLayout, IconsLayout, ItemLayout, ViewPoint, ViewRect, ViewSize};

use crate::shell::selection::NavigationAction;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CompactLayoutCacheKey {
    pub(crate) pane: usize,
    pub(crate) item_count: usize,
    pub(crate) rows_per_column: usize,
    pub(crate) item_width: u32,
    pub(crate) item_height: u32,
    pub(crate) padding: u32,
    pub(crate) icon_size: u32,
    pub(crate) text_gap: u32,
    pub(crate) text_scale: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct CompactLayoutCacheValue {
    pub(crate) text_widths: Arc<[f32]>,
    pub(crate) column_widths: Arc<[f32]>,
}

#[derive(Clone, Debug)]
struct BoundedLayoutCache<K, V> {
    entries: HashMap<K, V>,
    recency: VecDeque<K>,
    max_entries: usize,
}

impl<K, V> BoundedLayoutCache<K, V>
where
    K: Copy + Eq + Hash,
    V: Clone,
{
    fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            recency: VecDeque::new(),
            max_entries,
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        let value = self.entries.get(key).cloned()?;
        self.touch(*key);
        Some(value)
    }

    fn insert(&mut self, key: K, value: V) {
        self.entries.insert(key, value);
        self.touch(key);
        self.prune();
    }

    fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.entries.retain(|key, _| keep(key));
        self.recency.retain(|key| self.entries.contains_key(key));
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.recency.clear();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    fn touch(&mut self, key: K) {
        self.recency.retain(|candidate| candidate != &key);
        self.recency.push_back(key);
    }

    fn prune(&mut self) {
        while self.entries.len() > self.max_entries {
            let Some(key) = self.recency.pop_front() else {
                break;
            };
            self.entries.remove(&key);
        }
    }
}

pub(crate) struct CompactLayoutCache {
    cache: RefCell<BoundedLayoutCache<CompactLayoutCacheKey, CompactLayoutCacheValue>>,
}

impl Default for CompactLayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CompactLayoutCache {
    const MAX_ENTRIES: usize = 8;

    pub(crate) fn new() -> Self {
        Self {
            cache: RefCell::new(BoundedLayoutCache::new(Self::MAX_ENTRIES)),
        }
    }

    pub(crate) fn get(&self, key: &CompactLayoutCacheKey) -> Option<CompactLayoutCacheValue> {
        self.cache.borrow_mut().get(key)
    }

    pub(crate) fn insert(&self, key: CompactLayoutCacheKey, value: CompactLayoutCacheValue) {
        self.cache.borrow_mut().insert(key, value);
    }

    pub(crate) fn invalidate_pane(&self, pane_index: usize) {
        self.cache.borrow_mut().retain(|key| key.pane != pane_index);
    }

    pub(crate) fn clear(&self) {
        self.cache.borrow_mut().clear();
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct IconsLayoutHeightCacheKey {
    pub(crate) pane: usize,
    pub(crate) item_count: usize,
    pub(crate) item_width: u32,
    pub(crate) item_height: u32,
    pub(crate) padding: u32,
    pub(crate) icon_size: u32,
    pub(crate) text_height: u32,
    pub(crate) text_scale: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct IconsLayoutHeightCacheValue {
    pub(crate) item_heights: Arc<[f32]>,
}

pub(crate) struct IconsLayoutHeightCache {
    cache: RefCell<BoundedLayoutCache<IconsLayoutHeightCacheKey, IconsLayoutHeightCacheValue>>,
}

impl Default for IconsLayoutHeightCache {
    fn default() -> Self {
        Self::new()
    }
}

impl IconsLayoutHeightCache {
    const MAX_ENTRIES: usize = 8;

    pub(crate) fn new() -> Self {
        Self {
            cache: RefCell::new(BoundedLayoutCache::new(Self::MAX_ENTRIES)),
        }
    }

    pub(crate) fn get(
        &self,
        key: &IconsLayoutHeightCacheKey,
    ) -> Option<IconsLayoutHeightCacheValue> {
        self.cache.borrow_mut().get(key)
    }

    pub(crate) fn insert(
        &self,
        key: IconsLayoutHeightCacheKey,
        value: IconsLayoutHeightCacheValue,
    ) {
        self.cache.borrow_mut().insert(key, value);
    }

    pub(crate) fn invalidate_pane(&self, pane_index: usize) {
        self.cache.borrow_mut().retain(|key| key.pane != pane_index);
    }

    pub(crate) fn clear(&self) {
        self.cache.borrow_mut().clear();
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.cache.borrow().len()
    }
}

pub(crate) fn navigation_target(
    action: NavigationAction,
    current: usize,
    item_count: usize,
    layout: &ShellLayout,
) -> Option<usize> {
    if item_count == 0 {
        return None;
    }
    let last = item_count - 1;
    let current = current.min(last);
    match layout {
        ShellLayout::Icons(layout) => {
            let columns = layout.columns_per_row().max(1);
            let page_stride = layout.visible_items().count().max(columns).max(1);
            Some(match action {
                NavigationAction::Left => current.saturating_sub(1),
                NavigationAction::Right => (current + 1).min(last),
                NavigationAction::Up => current.saturating_sub(columns),
                NavigationAction::Down => (current + columns).min(last),
                NavigationAction::Home => 0,
                NavigationAction::End => last,
                NavigationAction::PageUp => current.saturating_sub(page_stride),
                NavigationAction::PageDown => (current + page_stride).min(last),
            })
        }
        ShellLayout::Compact(layout) => {
            let rows = layout.rows_per_column().max(1);
            let row = current % rows;
            let page_stride = layout.visible_item_count().max(rows).max(1);
            Some(match action {
                NavigationAction::Left => current.saturating_sub(rows),
                NavigationAction::Right => (current + rows).min(last),
                NavigationAction::Up => {
                    if row == 0 {
                        current
                    } else {
                        current - 1
                    }
                }
                NavigationAction::Down => {
                    if row + 1 >= rows {
                        current
                    } else {
                        (current + 1).min(last)
                    }
                }
                NavigationAction::Home => 0,
                NavigationAction::End => last,
                NavigationAction::PageUp => current.saturating_sub(page_stride),
                NavigationAction::PageDown => (current + page_stride).min(last),
            })
        }
        ShellLayout::Details(layout) => {
            let page_stride = layout.visible_item_count().max(1);
            Some(match action {
                NavigationAction::Left | NavigationAction::Up => current.saturating_sub(1),
                NavigationAction::Right | NavigationAction::Down => (current + 1).min(last),
                NavigationAction::Home => 0,
                NavigationAction::End => last,
                NavigationAction::PageUp => current.saturating_sub(page_stride),
                NavigationAction::PageDown => (current + page_stride).min(last),
            })
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ShellLayout {
    Icons(IconsLayout),
    Compact(ShellCompactLayout),
    Details(DetailsLayout),
}

impl ShellLayout {
    pub(crate) fn content_size(&self) -> ViewSize {
        match self {
            Self::Icons(layout) => layout.content_size(),
            Self::Compact(layout) => layout.content_size(),
            Self::Details(layout) => layout.content_size(),
        }
    }

    pub(crate) fn item(&self, index: usize) -> Option<ItemLayout> {
        match self {
            Self::Icons(layout) => layout.item(index),
            Self::Compact(layout) => layout.item(index),
            Self::Details(layout) => layout.item(index),
        }
    }

    pub(crate) fn visible_item_count(&self) -> usize {
        match self {
            Self::Icons(layout) => layout.visible_items().count(),
            Self::Compact(layout) => layout.visible_item_count(),
            Self::Details(layout) => layout.visible_item_count(),
        }
    }

    pub(crate) fn for_each_visible_item(&self, mut visit: impl FnMut(ItemLayout)) {
        match self {
            Self::Icons(layout) => {
                for item in layout.visible_items() {
                    visit(item);
                }
            }
            Self::Compact(layout) => layout.for_each_visible_item(visit),
            Self::Details(layout) => layout.for_each_visible_item(visit),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn visible_items(&self) -> Vec<ItemLayout> {
        let mut items = Vec::with_capacity(self.visible_item_count());
        self.for_each_visible_item(|item| items.push(item));
        items
    }

    pub(crate) fn hit_test_content_point(&self, point: ViewPoint) -> Option<usize> {
        match self {
            Self::Icons(layout) => layout.hit_test_content_point(point),
            Self::Compact(layout) => layout.hit_test_content_point(point),
            Self::Details(layout) => layout.hit_test_content_point(point),
        }
    }

    pub(crate) fn indexes_intersecting(&self, rect: ViewRect) -> Vec<usize> {
        match self {
            Self::Icons(layout) => layout.indexes_intersecting(rect).indexes().to_vec(),
            Self::Compact(layout) => layout.indexes_intersecting(rect),
            Self::Details(layout) => layout.indexes_intersecting(rect),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ShellCompactLayout {
    layout: CompactLayout,
    text_widths: Arc<[f32]>,
}

impl ShellCompactLayout {
    pub(crate) fn new(layout: CompactLayout, text_widths: impl Into<Arc<[f32]>>) -> Self {
        Self {
            layout,
            text_widths: text_widths.into(),
        }
    }

    pub(crate) fn content_size(&self) -> ViewSize {
        self.layout.content_size()
    }

    pub(crate) fn rows_per_column(&self) -> usize {
        self.layout.rows_per_column()
    }

    pub(crate) fn item(&self, index: usize) -> Option<ItemLayout> {
        self.layout
            .item_with_required_text_width(index, self.text_widths.get(index).copied())
    }

    fn visible_item_count(&self) -> usize {
        self.layout.visible_items().count()
    }

    fn for_each_visible_item(&self, mut visit: impl FnMut(ItemLayout)) {
        for item in self
            .layout
            .visible_items()
            .filter_map(|item| self.item(item.model_index))
        {
            visit(item);
        }
    }

    fn hit_test_content_point(&self, point: ViewPoint) -> Option<usize> {
        self.layout.hit_test_content_point(point)
    }

    fn indexes_intersecting(&self, rect: ViewRect) -> Vec<usize> {
        self.layout.indexes_intersecting(rect).indexes().to_vec()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DetailsLayout {
    item_count: usize,
    viewport_height: f32,
    scroll_y: f32,
    content_width: f32,
    row_height: f32,
    icon_size: f32,
    scale_factor: f32,
    name_width: f32,
    text_height: f32,
}

impl DetailsLayout {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        item_count: usize,
        viewport_width: f32,
        viewport_height: f32,
        scroll_y: f32,
        row_height: f32,
        icon_size: f32,
        scale_factor: f32,
        name_width: f32,
        size_width: f32,
        modified_width: f32,
        text_height: f32,
    ) -> Self {
        Self {
            item_count,
            viewport_height,
            scroll_y,
            content_width: (name_width + size_width + modified_width).max(viewport_width),
            row_height,
            icon_size,
            scale_factor,
            name_width,
            text_height,
        }
    }

    pub(crate) fn content_size(&self) -> ViewSize {
        ViewSize {
            width: self.content_width,
            height: (self.item_count as f32 * self.row_height).max(1.0),
        }
    }

    pub(crate) fn item(&self, index: usize) -> Option<ItemLayout> {
        if index >= self.item_count {
            return None;
        }
        let y = index as f32 * self.row_height;
        let item_rect = ViewRect {
            x: 0.0,
            y,
            width: self.content_width,
            height: self.row_height,
        };
        let icon_padding = (8.0 * self.scale_factor).round().max(1.0);
        let text_gap = (8.0 * self.scale_factor).round().max(1.0);
        let text_x = icon_padding + self.icon_size + text_gap;
        let icon_rect = ViewRect {
            x: icon_padding,
            y: y + (self.row_height - self.icon_size) / 2.0,
            width: self.icon_size,
            height: self.icon_size,
        };
        Some(ItemLayout {
            model_index: index,
            column: 0,
            row: index,
            item_rect,
            visual_rect: item_rect,
            icon_rect,
            text_rect: ViewRect {
                x: text_x,
                y: y + (self.row_height - self.text_height).max(0.0) / 2.0,
                width: (self.name_width - text_x - text_gap).max(1.0),
                height: self.text_height,
            },
        })
    }

    fn visible_item_count(&self) -> usize {
        self.visible_row_range().len()
    }

    fn for_each_visible_item(&self, mut visit: impl FnMut(ItemLayout)) {
        for item in self
            .visible_row_range()
            .filter_map(|index| self.item(index))
        {
            visit(item);
        }
    }

    fn hit_test_content_point(&self, point: ViewPoint) -> Option<usize> {
        if point.x < 0.0 || point.x >= self.content_width || point.y < 0.0 {
            return None;
        }
        let index = (point.y / self.row_height).floor() as usize;
        (index < self.item_count).then_some(index)
    }

    fn indexes_intersecting(&self, rect: ViewRect) -> Vec<usize> {
        if self.item_count == 0 || rect.right() <= 0.0 || rect.x >= self.content_width {
            return Vec::new();
        }
        let start = (rect.y / self.row_height).floor().max(0.0) as usize;
        let end = (rect.bottom() / self.row_height).ceil().max(0.0) as usize;
        (start..end.min(self.item_count)).collect()
    }

    fn visible_row_range(&self) -> std::ops::Range<usize> {
        if self.item_count == 0 {
            return 0..0;
        }
        let start = (self.scroll_y / self.row_height).floor().max(0.0) as usize;
        let end = ((self.scroll_y + self.viewport_height) / self.row_height)
            .ceil()
            .max(0.0) as usize
            + 1;
        start.min(self.item_count)..end.min(self.item_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_layout_cache_prunes_least_recently_used_entry() {
        let mut cache = BoundedLayoutCache::new(2);
        cache.insert(1usize, "one");
        cache.insert(2usize, "two");

        assert_eq!(cache.get(&1), Some("one"));
        cache.insert(3usize, "three");

        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&1), Some("one"));
        assert_eq!(cache.get(&3), Some("three"));
        assert_eq!(cache.len(), 2);
    }
}

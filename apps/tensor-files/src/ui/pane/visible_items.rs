use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy_ecs::{
    component::Component,
    entity::Entity,
    query::{Changed, Or},
    world::World,
};

use super::ShellPaneId;
use crate::Entry;
use crate::ui::icon_roles::FileIconRoleCacheKey;

mod cache;
mod icon_role;
#[cfg(test)]
mod retained_role_tests;
use cache::{CachedEntryMatch, CachedVisibleItemSlot};
use icon_role::{refresh_visible_icon_role, retained_icon_role_for_entry};

#[derive(Component)]
struct VisibleItemPath(Arc<Path>);

#[derive(Component)]
struct VisibleItemLocalName(Arc<str>);

#[derive(Component)]
struct VisibleItemSlot(u64);

#[derive(Component)]
struct VisibleItemEpoch(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellVisibleItemRenderBinding {
    pub(crate) path: Arc<Path>,
    pub(crate) slot_id: u64,
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
    fn visible_slot_entry_index(&self) -> Option<usize>;
    fn set_visible_slot_id(&mut self, slot_id: u64);
}

/// Retained ECS entities for one pane's currently visible item range.
///
/// Path and residency values live as components, while the bounded free list
/// models reusable GPU-facing slots independently from Bevy entity generations.
/// Local entries use their directory-unique name as the frame-path lookup, so
/// the full path is allocated only when an item first becomes visible.
pub(crate) struct ShellVisibleItemSlotPool {
    world: World,
    entity_by_path: HashMap<Arc<Path>, Entity>,
    entity_by_local_name: HashMap<Arc<str>, Entity>,
    visible_entity_staging: Vec<Option<Entity>>,
    projection_cache_directory: Option<Arc<Path>>,
    projection_cache: Vec<CachedVisibleItemSlot>,
    free_slots: Vec<u64>,
    next_slot_id: u64,
    visible_epoch: u64,
}

impl Default for ShellVisibleItemSlotPool {
    fn default() -> Self {
        Self {
            world: World::new(),
            entity_by_path: HashMap::new(),
            entity_by_local_name: HashMap::new(),
            visible_entity_staging: Vec::new(),
            projection_cache_directory: None,
            projection_cache: Vec::new(),
            free_slots: Vec::new(),
            next_slot_id: 0,
            visible_epoch: 0,
        }
    }
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
        self.clear_projection_cache();
        let visible_epoch = self.begin_update();
        let mut reused = 0;
        for path in visible_paths {
            reused += usize::from(self.mark_visible(path.as_ref(), visible_epoch).1);
        }
        self.finish_update(visible_epoch, reused)
    }

    pub(crate) fn update_visible_item_slots(
        &mut self,
        directory: &Path,
        entries: &[Entry],
        visible_items: &mut [impl ShellVisibleSlotItem],
    ) -> ShellVisibleItemSlotStats {
        if let Some(stats) = self.try_reuse_projection_slots(directory, entries, visible_items) {
            return stats;
        }

        self.clear_projection_cache();
        let visible_epoch = self.begin_update();
        let mut reused = 0;
        self.visible_entity_staging.clear();
        self.visible_entity_staging.reserve(visible_items.len());
        for item in visible_items.iter() {
            let Some(entry) = item
                .visible_slot_entry_index()
                .and_then(|index| entries.get(index))
            else {
                self.visible_entity_staging.push(None);
                continue;
            };
            let (entity, was_reused) = self.mark_visible_entry(directory, entry, visible_epoch);
            reused += usize::from(was_reused);
            self.visible_entity_staging.push(Some(entity));
        }

        let stats = self.finish_update(visible_epoch, reused);
        self.projection_cache.reserve(visible_items.len());
        for (item, entity) in visible_items
            .iter_mut()
            .zip(self.visible_entity_staging.iter().copied())
        {
            let slot_id = entity
                .and_then(|entity| self.slot_for_entity(entity))
                .unwrap_or_default();
            let entry_index = item.visible_slot_entry_index();
            item.set_visible_slot_id(slot_id);
            self.projection_cache
                .push(CachedVisibleItemSlot::from_entry_index(
                    entry_index,
                    entries,
                    entity,
                    slot_id,
                ));
        }
        self.projection_cache_directory = Some(Arc::from(directory));
        self.visible_entity_staging.clear();
        stats
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn slot_for_path(&self, path: &Path) -> Option<u64> {
        let entity = self.entity_for_path(path)?;
        self.slot_for_entity(entity)
    }

    pub(crate) fn slot_for_entry(&self, entry: &Entry) -> Option<u64> {
        let entity = self.entity_for_entry(entry)?;
        self.slot_for_entity(entity)
    }

    pub(crate) fn retained_shared_path_for_entry(&self, entry: &Entry) -> Option<Arc<Path>> {
        let entity = self.entity_for_entry(entry)?;
        self.world
            .get::<VisibleItemPath>(entity)
            .map(|path| Arc::clone(&path.0))
    }

    pub(crate) fn retained_visual_for_entry(
        &self,
        entry: &Entry,
    ) -> Option<(&Arc<Path>, Option<&FileIconRoleCacheKey>)> {
        let entity = self.entity_for_entry(entry)?;
        let path = self.world.get::<VisibleItemPath>(entity)?;
        let role = retained_icon_role_for_entry(&self.world, entity, entry);
        Some((&path.0, role))
    }

    #[cfg(test)]
    pub(crate) fn retained_icon_role_for_entry(
        &self,
        entry: &Entry,
    ) -> Option<&FileIconRoleCacheKey> {
        let entity = self.entity_for_entry(entry)?;
        retained_icon_role_for_entry(&self.world, entity, entry)
    }

    pub(crate) fn retained_entity_path_for_entry(&self, entry: &Entry) -> Option<(Entity, &Path)> {
        let entity = self.entity_for_entry(entry)?;
        let path = self
            .world
            .get::<VisibleItemPath>(entity)
            .map(|path| path.0.as_ref())?;
        Some((entity, path))
    }

    pub(crate) fn entity_for_entry(&self, entry: &Entry) -> Option<Entity> {
        if let Some(path) = entry.target_path.as_deref() {
            self.entity_for_path(path)
        } else {
            self.entity_by_local_name.get(entry.name.as_ref()).copied()
        }
    }

    pub(crate) fn entity_for_path(&self, path: &Path) -> Option<Entity> {
        self.entity_by_path.get(path).copied()
    }

    fn slot_for_entity(&self, entity: Entity) -> Option<u64> {
        self.world
            .get::<VisibleItemSlot>(entity)
            .map(|slot| slot.0)
            .filter(|slot_id| *slot_id != 0)
    }

    /// Extract only GPU slot bindings that changed since the previous
    /// extraction boundary. The caller retains `output`, so ordinary frames
    /// do not allocate; path payloads are shared through `Arc<Path>`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn extract_changed_slot_bindings(
        &mut self,
        output: &mut Vec<ShellVisibleItemRenderBinding>,
    ) {
        output.clear();
        let mut query = self
            .world
            .query_filtered::<
                (&VisibleItemPath, &VisibleItemSlot),
                Or<(Changed<VisibleItemPath>, Changed<VisibleItemSlot>)>,
            >();
        output.extend(query.iter(&self.world).filter(|(_, slot)| slot.0 != 0).map(
            |(path, slot)| ShellVisibleItemRenderBinding {
                path: Arc::clone(&path.0),
                slot_id: slot.0,
            },
        ));
        output.sort_unstable_by_key(|binding| binding.slot_id);
        self.world.clear_trackers();
    }

    pub(crate) fn clear(&mut self) {
        for (_, entity) in self.entity_by_path.drain() {
            let removed = self.world.despawn(entity);
            debug_assert!(removed, "visible item entity disappeared before clear");
        }
        self.entity_by_local_name.clear();
        self.visible_entity_staging.clear();
        self.clear_projection_cache();
        self.free_slots.clear();
        self.visible_epoch = 0;
    }

    fn begin_update(&mut self) -> u64 {
        self.visible_epoch = self.visible_epoch.wrapping_add(1).max(1);
        self.visible_epoch
    }

    /// Refill prepared item slots without touching retained ECS state when the
    /// pane directory and every visible entry are unchanged.
    fn try_reuse_projection_slots(
        &mut self,
        directory: &Path,
        entries: &[Entry],
        visible_items: &mut [impl ShellVisibleSlotItem],
    ) -> Option<ShellVisibleItemSlotStats> {
        if self.projection_cache_directory.as_deref() != Some(directory)
            || self.projection_cache.len() != visible_items.len()
        {
            return None;
        }

        let world = &mut self.world;
        for (item, cached) in visible_items.iter_mut().zip(&mut self.projection_cache) {
            if cached.slot_id == 0 {
                return None;
            }
            let entry_index = item.visible_slot_entry_index();
            let entry = entry_index.and_then(|index| entries.get(index));
            match cached.classify(entry_index, entry) {
                CachedEntryMatch::Exact => {}
                CachedEntryMatch::Equivalent => {
                    let (Some(entity), Some(entry)) = (cached.entity, entry) else {
                        return None;
                    };
                    cached.refresh_entry(entry);
                    refresh_visible_icon_role(world, entity, entry);
                }
                CachedEntryMatch::Mismatch => return None,
            }
            item.set_visible_slot_id(cached.slot_id);
        }
        Some(ShellVisibleItemSlotStats {
            active: self.entity_by_path.len(),
            free: self.free_slots.len(),
            reused: self.entity_by_path.len(),
            recycled: 0,
            allocated: 0,
        })
    }

    fn clear_projection_cache(&mut self) {
        self.projection_cache_directory = None;
        self.projection_cache.clear();
    }

    #[cfg(test)]
    pub(crate) fn visible_epoch(&self) -> u64 {
        self.visible_epoch
    }

    fn mark_visible(&mut self, path: &Path, visible_epoch: u64) -> (Entity, bool) {
        if let Some(&entity) = self.entity_by_path.get(path) {
            return (entity, self.mark_entity_visible(entity, visible_epoch));
        }

        let path = Arc::<Path>::from(path);
        let entity = self.spawn_visible_entity(Arc::clone(&path), None, visible_epoch);
        self.entity_by_path.insert(path, entity);
        (entity, false)
    }

    fn mark_visible_entry(
        &mut self,
        directory: &Path,
        entry: &Entry,
        visible_epoch: u64,
    ) -> (Entity, bool) {
        if let Some(path) = entry.target_path.as_deref() {
            let marked = self.mark_visible(path, visible_epoch);
            refresh_visible_icon_role(&mut self.world, marked.0, entry);
            return marked;
        }
        if let Some(&entity) = self.entity_by_local_name.get(entry.name.as_ref()) {
            let path_needs_rebind = self
                .world
                .get::<VisibleItemPath>(entity)
                .is_none_or(|path| path.0.parent() != Some(directory));
            if path_needs_rebind {
                self.rebind_local_entity_path(entity, directory.join(entry.name.as_ref()));
            }
            refresh_visible_icon_role(&mut self.world, entity, entry);
            return (entity, self.mark_entity_visible(entity, visible_epoch));
        }

        let path = Arc::<Path>::from(directory.join(entry.name.as_ref()));
        let name = Arc::clone(&entry.name);
        let entity =
            self.spawn_visible_entity(Arc::clone(&path), Some(Arc::clone(&name)), visible_epoch);
        self.entity_by_path.insert(path, entity);
        self.entity_by_local_name.insert(name, entity);
        refresh_visible_icon_role(&mut self.world, entity, entry);
        (entity, false)
    }

    fn rebind_local_entity_path(&mut self, entity: Entity, new_path: PathBuf) {
        let Some(old_path) = self
            .world
            .get::<VisibleItemPath>(entity)
            .map(|path| Arc::clone(&path.0))
        else {
            return;
        };
        if old_path.as_ref() == new_path.as_path() {
            return;
        }

        self.entity_by_path.remove(old_path.as_ref());
        let new_path = Arc::<Path>::from(new_path);
        if let Some(existing) = self.entity_by_path.insert(Arc::clone(&new_path), entity) {
            debug_assert_eq!(
                existing, entity,
                "local slot path collides with another entity"
            );
        }
        self.world
            .get_mut::<VisibleItemPath>(entity)
            .expect("visible item entity is missing its path")
            .0 = new_path;
    }

    fn mark_entity_visible(&mut self, entity: Entity, visible_epoch: u64) -> bool {
        let mut epoch = self
            .world
            .get_mut::<VisibleItemEpoch>(entity)
            .expect("visible item index points at an entity without an epoch");
        let reused = epoch.0 != visible_epoch;
        epoch.0 = visible_epoch;
        reused
    }

    fn spawn_visible_entity(
        &mut self,
        path: Arc<Path>,
        local_name: Option<Arc<str>>,
        visible_epoch: u64,
    ) -> Entity {
        let mut entity = self.world.spawn((
            VisibleItemPath(path),
            VisibleItemSlot(0),
            VisibleItemEpoch(visible_epoch),
        ));
        if let Some(local_name) = local_name {
            entity.insert(VisibleItemLocalName(local_name));
        }
        entity.id()
    }

    fn finish_update(&mut self, visible_epoch: u64, reused: usize) -> ShellVisibleItemSlotStats {
        let world = &mut self.world;
        let free_slots = &mut self.free_slots;
        let entity_by_local_name = &mut self.entity_by_local_name;
        self.entity_by_path.retain(|path, entity| {
            let epoch = world
                .get::<VisibleItemEpoch>(*entity)
                .expect("visible item index points at an entity without an epoch");
            if epoch.0 == visible_epoch {
                return true;
            }
            let stored_path = world
                .get::<VisibleItemPath>(*entity)
                .expect("visible item entity is missing its path");
            debug_assert_eq!(stored_path.0.as_ref(), path.as_ref());
            let slot = world
                .get::<VisibleItemSlot>(*entity)
                .expect("visible item entity is missing its slot");
            if slot.0 != 0 {
                free_slots.push(slot.0);
            }
            if let Some(local_name) = world.get::<VisibleItemLocalName>(*entity) {
                let indexed_entity = entity_by_local_name.remove(local_name.0.as_ref());
                debug_assert_eq!(indexed_entity, Some(*entity));
            }
            let removed = world.despawn(*entity);
            debug_assert!(removed, "visible item entity disappeared before recycle");
            false
        });
        if self.free_slots.len() > Self::MAX_FREE_SLOTS {
            self.free_slots.truncate(Self::MAX_FREE_SLOTS);
        }

        let mut recycled = 0;
        let mut allocated = 0;
        for entity in self.entity_by_path.values().copied() {
            let mut slot = self
                .world
                .get_mut::<VisibleItemSlot>(entity)
                .expect("visible item entity is missing its slot");
            if slot.0 != 0 {
                continue;
            }
            if let Some(slot_id) = self.free_slots.pop() {
                slot.0 = slot_id;
                recycled += 1;
            } else {
                self.next_slot_id += 1;
                slot.0 = self.next_slot_id;
                allocated += 1;
            }
        }

        ShellVisibleItemSlotStats {
            active: self.entity_by_path.len(),
            free: self.free_slots.len(),
            reused,
            recycled,
            allocated,
        }
    }
}

#[derive(Default)]
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tensor_files_core::EntryData;

    use super::*;

    struct PreparedEntry {
        entry_index: Option<usize>,
        slot_id: u64,
    }

    impl ShellVisibleSlotItem for PreparedEntry {
        fn visible_slot_entry_index(&self) -> Option<usize> {
            self.entry_index
        }

        fn set_visible_slot_id(&mut self, slot_id: u64) {
            self.slot_id = slot_id;
        }
    }

    fn entry(name: &str, target_path: Option<PathBuf>) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: 0,
            target_path,
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: true,
            mime_type: None,
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }

    #[test]
    fn visible_item_entity_is_stable_while_path_stays_visible() {
        let path = PathBuf::from("/tmp/retained");
        let mut pool = ShellVisibleItemSlotPool::default();

        let first = pool.update_visible_items([&path]);
        let entity = pool.entity_for_path(&path).unwrap();
        let slot = pool.slot_for_path(&path).unwrap();
        let second = pool.update_visible_items([&path]);

        assert_eq!(first.allocated, 1);
        assert_eq!(second.reused, 1);
        assert_eq!(pool.entity_for_path(&path), Some(entity));
        assert_eq!(pool.slot_for_path(&path), Some(slot));
    }

    #[test]
    fn offscreen_entity_is_despawned_and_its_slot_is_recycled() {
        let old_path = PathBuf::from("/tmp/old");
        let next_path = PathBuf::from("/tmp/next");
        let mut pool = ShellVisibleItemSlotPool::default();

        pool.update_visible_items([&old_path]);
        let old_entity = pool.entity_for_path(&old_path).unwrap();
        let old_slot = pool.slot_for_path(&old_path).unwrap();
        let stats = pool.update_visible_items([&next_path]);

        assert!(pool.world.get_entity(old_entity).is_err());
        assert_eq!(stats.active, 1);
        assert_eq!(stats.recycled, 1);
        assert_eq!(stats.allocated, 0);
        assert_eq!(pool.slot_for_path(&next_path), Some(old_slot));
    }

    #[test]
    fn render_extraction_reports_only_changed_slot_bindings() {
        let retained = PathBuf::from("/tmp/retained");
        let replacement = PathBuf::from("/tmp/replacement");
        let mut pool = ShellVisibleItemSlotPool::default();
        let mut bindings = Vec::new();

        pool.update_visible_items([&retained]);
        pool.extract_changed_slot_bindings(&mut bindings);
        assert_eq!(bindings.len(), 1);
        let retained_slot = bindings[0].slot_id;
        assert_eq!(bindings[0].path.as_ref(), retained.as_path());

        pool.update_visible_items([&retained]);
        pool.extract_changed_slot_bindings(&mut bindings);
        assert!(bindings.is_empty(), "retained slots stay out of extraction");

        pool.update_visible_items([&replacement]);
        pool.extract_changed_slot_bindings(&mut bindings);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].slot_id, retained_slot);
        assert_eq!(bindings[0].path.as_ref(), replacement.as_path());
    }

    #[test]
    fn local_entry_identity_reuses_entity_and_slot_by_name() {
        let directory = Path::new("/tmp/visible-items");
        let retained = entry("retained.txt", None);
        let retained_path = directory.join("retained.txt");
        let mut pool = ShellVisibleItemSlotPool::default();
        let mut first = [PreparedEntry {
            entry_index: Some(0),
            slot_id: 0,
        }];

        let entries = [retained];
        let first_stats = pool.update_visible_item_slots(directory, &entries, &mut first);
        let entity = pool.entity_for_path(&retained_path).unwrap();
        let slot = first[0].slot_id;
        let mut second = [PreparedEntry {
            entry_index: Some(0),
            slot_id: 0,
        }];
        let second_stats = pool.update_visible_item_slots(directory, &entries, &mut second);

        assert_eq!(first_stats.allocated, 1);
        assert_eq!(second_stats.reused, 1);
        assert!(Entry::ptr_eq(
            pool.projection_cache[0].entry.as_ref().unwrap(),
            &entries[0]
        ));
        assert_eq!(pool.entity_for_path(&retained_path), Some(entity));
        assert_eq!(second[0].slot_id, slot);
        assert_eq!(second[0].entry_index, Some(0));
        let (_, retained_path_ref) = pool
            .retained_entity_path_for_entry(&entries[0])
            .expect("retained entry should borrow its ECS path");
        assert_eq!(retained_path_ref, retained_path.as_path());
        let stored_path = pool
            .world
            .get::<VisibleItemPath>(entity)
            .expect("retained entity should keep its path component");
        assert!(std::ptr::eq(retained_path_ref, stored_path.0.as_ref()));
    }

    #[test]
    fn visible_slot_update_reuses_entity_staging_capacity_and_slots() {
        let directory = Path::new("/tmp/visible-items");
        let entries = [entry("alpha.txt", None), entry("beta.txt", None)];
        let mut pool = ShellVisibleItemSlotPool::default();
        let mut prepared = [
            PreparedEntry {
                entry_index: Some(0),
                slot_id: 0,
            },
            PreparedEntry {
                entry_index: Some(1),
                slot_id: 0,
            },
        ];

        pool.update_visible_item_slots(directory, &entries, &mut prepared);
        let slots = prepared.each_ref().map(|item| item.slot_id);
        let staging_capacity = pool.visible_entity_staging.capacity();
        assert!(pool.visible_entity_staging.is_empty());
        assert!(staging_capacity >= prepared.len());

        prepared.iter_mut().for_each(|item| item.slot_id = 0);
        let stats = pool.update_visible_item_slots(directory, &entries, &mut prepared);

        assert_eq!(stats.reused, prepared.len());
        assert_eq!(prepared.each_ref().map(|item| item.slot_id), slots);
        assert_eq!(pool.visible_entity_staging.capacity(), staging_capacity);
        assert!(pool.visible_entity_staging.is_empty());
    }

    #[test]
    fn local_entry_rebinds_path_when_directory_changes() {
        let first_directory = Path::new("/tmp/first-directory");
        let second_directory = Path::new("/tmp/second-directory");
        let first_path = first_directory.join("same-name.txt");
        let second_path = second_directory.join("same-name.txt");
        let first_entries = [entry("same-name.txt", None)];
        let second_entries = [entry("same-name.txt", None)];
        let mut pool = ShellVisibleItemSlotPool::default();
        let mut prepared = [PreparedEntry {
            entry_index: Some(0),
            slot_id: 0,
        }];
        let mut bindings = Vec::new();

        pool.update_visible_item_slots(first_directory, &first_entries, &mut prepared);
        let slot = prepared[0].slot_id;
        pool.extract_changed_slot_bindings(&mut bindings);
        assert_eq!(bindings[0].path.as_ref(), first_path.as_path());

        pool.update_visible_item_slots(second_directory, &second_entries, &mut prepared);
        pool.extract_changed_slot_bindings(&mut bindings);

        assert_eq!(prepared[0].slot_id, slot);
        assert_eq!(bindings[0].path.as_ref(), second_path.as_path());
        assert_eq!(pool.slot_for_path(&first_path), None);
        assert_eq!(pool.slot_for_path(&second_path), Some(slot));
    }

    #[test]
    fn missing_entry_index_does_not_allocate_or_reuse_a_slot() {
        let mut pool = ShellVisibleItemSlotPool::default();
        let entries = [entry("visible.txt", None)];
        let mut prepared = [PreparedEntry {
            entry_index: Some(99),
            slot_id: 0,
        }];

        let stats = pool.update_visible_item_slots(
            Path::new("/tmp/visible-items"),
            &entries,
            &mut prepared,
        );

        assert_eq!(stats.active, 0);
        assert_eq!(stats.allocated, 0);
        assert_eq!(prepared[0].slot_id, 0);
    }

    #[test]
    fn network_entries_with_equal_names_keep_exact_target_identity() {
        let first_path = PathBuf::from("smb://server/one/item");
        let second_path = PathBuf::from("smb://server/two/item");
        let first_entry = entry("item", Some(first_path.clone()));
        let second_entry = entry("item", Some(second_path.clone()));
        let mut pool = ShellVisibleItemSlotPool::default();
        let mut bindings = Vec::new();
        let mut first = [PreparedEntry {
            entry_index: Some(0),
            slot_id: 0,
        }];

        let first_entries = [first_entry];
        pool.update_visible_item_slots(Path::new("smb://server/one"), &first_entries, &mut first);
        pool.extract_changed_slot_bindings(&mut bindings);
        let slot = first[0].slot_id;
        assert_eq!(bindings[0].path.as_ref(), first_path.as_path());

        let mut second = [PreparedEntry {
            entry_index: Some(0),
            slot_id: 0,
        }];
        let second_entries = [second_entry];
        let stats = pool.update_visible_item_slots(
            Path::new("smb://server/two"),
            &second_entries,
            &mut second,
        );
        pool.extract_changed_slot_bindings(&mut bindings);

        assert_eq!(stats.recycled, 1);
        assert_eq!(second[0].slot_id, slot);
        assert_eq!(bindings[0].path.as_ref(), second_path.as_path());
        assert_eq!(pool.slot_for_path(&first_path), None);
    }
}

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bevy_ecs::{component::Component, entity::Entity, query::Changed, world::World};

use super::ShellPaneId;

#[derive(Component)]
struct VisibleItemPath(Arc<Path>);

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
    fn visible_slot_path(&self) -> Option<&Path>;
    fn visible_slot_id(&self) -> u64;
    fn set_visible_slot_id(&mut self, slot_id: u64);
    fn release_visible_slot_path(&mut self) {}
}

/// Retained ECS entities for one pane's currently visible item range.
///
/// `entity_by_path` is only an O(1) identity index. Path and residency values
/// live as components, while the bounded free list models reusable GPU-facing
/// slots independently from Bevy entity generations.
pub(crate) struct ShellVisibleItemSlotPool {
    world: World,
    entity_by_path: HashMap<Arc<Path>, Entity>,
    free_slots: Vec<u64>,
    next_slot_id: u64,
    visible_epoch: u64,
}

impl Default for ShellVisibleItemSlotPool {
    fn default() -> Self {
        Self {
            world: World::new(),
            entity_by_path: HashMap::new(),
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
        let visible_epoch = self.begin_update();
        let mut reused = 0;
        for path in visible_paths {
            reused += usize::from(self.mark_visible(path.as_ref(), visible_epoch));
        }
        self.finish_update(visible_epoch, reused)
    }

    pub(crate) fn update_visible_item_slots(
        &mut self,
        visible_items: &mut [impl ShellVisibleSlotItem],
    ) -> ShellVisibleItemSlotStats {
        let visible_epoch = self.begin_update();
        let mut reused = 0;
        for item in visible_items.iter_mut() {
            let Some(path) = item.visible_slot_path() else {
                item.set_visible_slot_id(0);
                continue;
            };
            reused += usize::from(self.mark_visible(path, visible_epoch));
            item.set_visible_slot_id(self.slot_for_path(path).unwrap_or_default());
        }

        let stats = self.finish_update(visible_epoch, reused);
        for item in visible_items {
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
        stats
    }

    pub(crate) fn slot_for_path(&self, path: &Path) -> Option<u64> {
        let entity = *self.entity_by_path.get(path)?;
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
            .query_filtered::<(&VisibleItemPath, &VisibleItemSlot), Changed<VisibleItemSlot>>();
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
        self.free_slots.clear();
        self.visible_epoch = 0;
    }

    fn begin_update(&mut self) -> u64 {
        self.visible_epoch = self.visible_epoch.wrapping_add(1).max(1);
        self.visible_epoch
    }

    fn mark_visible(&mut self, path: &Path, visible_epoch: u64) -> bool {
        if let Some(&entity) = self.entity_by_path.get(path) {
            let mut epoch = self
                .world
                .get_mut::<VisibleItemEpoch>(entity)
                .expect("visible item index points at an entity without an epoch");
            let reused = epoch.0 != visible_epoch;
            epoch.0 = visible_epoch;
            return reused;
        }

        let path = Arc::<Path>::from(path);
        let entity = self
            .world
            .spawn((
                VisibleItemPath(Arc::clone(&path)),
                VisibleItemSlot(0),
                VisibleItemEpoch(visible_epoch),
            ))
            .id();
        self.entity_by_path.insert(path, entity);
        false
    }

    fn finish_update(&mut self, visible_epoch: u64, reused: usize) -> ShellVisibleItemSlotStats {
        let world = &mut self.world;
        let free_slots = &mut self.free_slots;
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

    #[cfg(test)]
    fn entity_for_path(&self, path: &Path) -> Option<Entity> {
        self.entity_by_path.get(path).copied()
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

    use super::*;

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
}

use std::ops::{Index, IndexMut};

use bevy_ecs::{entity::Entity, world::World};

use super::{ShellPaneId, ShellPaneState};

/// Persistent ECS ownership for the shell's pane entities.
///
/// The fixed-size entity index is only an O(1) lookup cache. Pane state lives
/// in `World`, and an open pane keeps the same entity until it is closed. This
/// preserves the existing shell API while giving later item, selection, and
/// render-extraction systems a stable ECS identity to target.
pub(crate) struct ShellPaneStates {
    world: World,
    entities: [Option<Entity>; 2],
}

impl ShellPaneStates {
    pub(crate) fn new(slot0: ShellPaneState) -> Self {
        let mut panes = Self {
            world: World::new(),
            entities: [None, None],
        };
        panes.set(ShellPaneId::SLOT_0, slot0);
        panes
    }

    pub(crate) fn get(&self, pane: ShellPaneId) -> Option<&ShellPaneState> {
        self.world
            .get::<ShellPaneState>(self.entities[pane.index()]?)
    }

    pub(crate) fn get_mut(&mut self, pane: ShellPaneId) -> Option<&mut ShellPaneState> {
        self.world
            .get_mut::<ShellPaneState>(self.entities[pane.index()]?)
            .map(|state| state.into_inner())
    }

    pub(crate) fn set(&mut self, pane: ShellPaneId, state: ShellPaneState) {
        if let Some(entity) = self.entities[pane.index()] {
            self.world.entity_mut(entity).insert(state);
            return;
        }

        let entity = self.world.spawn((pane, state)).id();
        self.entities[pane.index()] = Some(entity);
    }

    pub(crate) fn take(&mut self, pane: ShellPaneId) -> Option<ShellPaneState> {
        let entity = self.entities[pane.index()].take()?;
        let state = self.world.entity_mut(entity).take::<ShellPaneState>();
        let removed = self.world.despawn(entity);
        debug_assert!(removed, "pane entity disappeared before close");
        state
    }

    pub(crate) fn is_open(&self, pane: ShellPaneId) -> bool {
        self.entities[pane.index()].is_some()
    }

    #[cfg(test)]
    fn entity(&self, pane: ShellPaneId) -> Option<Entity> {
        self.entities[pane.index()]
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::ui::options::ShellViewMode;

    use super::*;

    fn pane(path: &str) -> ShellPaneState {
        ShellPaneState::from_entries(
            PathBuf::from(path),
            ShellViewMode::Icons,
            Vec::new(),
            false,
            "",
        )
    }

    #[test]
    fn pane_state_has_stable_entity_identity_until_close() {
        let mut panes = ShellPaneStates::new(pane("/tmp/left"));
        let entity = panes.entity(ShellPaneId::SLOT_0).unwrap();

        panes.set(ShellPaneId::SLOT_0, pane("/tmp/replaced"));

        assert_eq!(panes.entity(ShellPaneId::SLOT_0), Some(entity));
        assert_eq!(
            panes[ShellPaneId::SLOT_0].path,
            PathBuf::from("/tmp/replaced")
        );
    }

    #[test]
    fn closing_and_reopening_pane_uses_a_new_entity() {
        let mut panes = ShellPaneStates::new(pane("/tmp/left"));
        panes.set(ShellPaneId::SLOT_1, pane("/tmp/right"));
        let closed_entity = panes.entity(ShellPaneId::SLOT_1).unwrap();

        let closed = panes.take(ShellPaneId::SLOT_1).unwrap();
        assert_eq!(closed.path, PathBuf::from("/tmp/right"));
        assert!(!panes.is_open(ShellPaneId::SLOT_1));

        panes.set(ShellPaneId::SLOT_1, pane("/tmp/next"));
        assert_ne!(panes.entity(ShellPaneId::SLOT_1), Some(closed_entity));
    }
}

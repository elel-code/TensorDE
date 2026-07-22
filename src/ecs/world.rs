use std::collections::HashMap;

use bevy_ecs::{entity::Entity, world::World};
use thiserror::Error;

use super::components::{Focused, View, ViewGeometry, Workspace};
use super::{ViewId, WorkspaceId};
use crate::layout::{LayoutEngine, Rect};

pub struct CompositorWorld {
    world: World,
    view_entities: HashMap<ViewId, bevy_ecs::entity::Entity>,
}

impl CompositorWorld {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            view_entities: HashMap::new(),
        }
    }

    pub fn spawn_view(
        &mut self,
        view_id: ViewId,
        workspace_id: WorkspaceId,
    ) -> Result<(), ViewLifecycleError> {
        if self.view_entities.contains_key(&view_id) {
            return Err(ViewLifecycleError::DuplicateViewId(view_id));
        }
        let entity = self
            .world
            .spawn((View { id: view_id }, Workspace { id: workspace_id }))
            .id();
        self.view_entities.insert(view_id, entity);
        Ok(())
    }

    pub fn remove_view(&mut self, view_id: ViewId) -> Result<(), ViewLifecycleError> {
        let entity = self
            .view_entities
            .remove(&view_id)
            .ok_or(ViewLifecycleError::MissingViewId(view_id))?;
        let removed = self.world.despawn(entity);
        debug_assert!(removed, "view index must reference a live ECS entity");
        Ok(())
    }

    pub fn move_view(
        &mut self,
        view_id: ViewId,
        workspace_id: WorkspaceId,
    ) -> Result<(), ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        let current_workspace_id = self
            .world
            .get::<Workspace>(entity)
            .expect("every view has a workspace")
            .id;
        if current_workspace_id == workspace_id {
            return Ok(());
        }
        let was_focused = self.world.get::<Focused>(entity).is_some();
        if was_focused {
            for focused_entity in self.focused_entities(workspace_id) {
                self.world.entity_mut(focused_entity).remove::<Focused>();
            }
        }
        let mut entity = self.world.entity_mut(entity);
        entity
            .get_mut::<Workspace>()
            .expect("every view has a workspace")
            .id = workspace_id;
        entity.remove::<ViewGeometry>();
        Ok(())
    }

    pub fn focus_view(&mut self, view_id: ViewId) -> Result<(), ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        let workspace_id = self
            .world
            .get::<Workspace>(entity)
            .expect("every view has a workspace")
            .id;
        let focused_entities = self.focused_entities(workspace_id);
        for focused_entity in focused_entities {
            self.world.entity_mut(focused_entity).remove::<Focused>();
        }
        self.world.entity_mut(entity).insert(Focused);
        Ok(())
    }

    pub fn focused_view(&mut self, workspace_id: WorkspaceId) -> Option<ViewId> {
        let mut query = self.world.query::<(&View, &Workspace, Option<&Focused>)>();
        query
            .iter(&self.world)
            .find(|(_, workspace, focused)| workspace.id == workspace_id && focused.is_some())
            .map(|(view, _, _)| view.id)
    }

    pub fn arrange_workspace(
        &mut self,
        workspace_id: WorkspaceId,
        engine: LayoutEngine,
        output: Rect,
    ) {
        let mut entities = {
            let mut query = self.world.query::<(Entity, &View, &Workspace)>();
            query
                .iter(&self.world)
                .filter(|(_, _, workspace)| workspace.id == workspace_id)
                .map(|(entity, view, _)| (view.id, entity))
                .collect::<Vec<_>>()
        };
        entities.sort_unstable_by_key(|(view_id, _)| *view_id);

        for ((_, entity), geometry) in entities
            .into_iter()
            .zip(engine.arrange(output, self.view_count(workspace_id)))
        {
            self.world.entity_mut(entity).insert(ViewGeometry(geometry));
        }
    }

    pub fn view_count(&mut self, workspace_id: WorkspaceId) -> usize {
        let mut query = self.world.query::<&Workspace>();
        query
            .iter(&self.world)
            .filter(|workspace| workspace.id == workspace_id)
            .count()
    }

    pub fn geometry(&self, view_id: ViewId) -> Option<Rect> {
        let entity = self.view_entities.get(&view_id).copied()?;
        self.world.get::<ViewGeometry>(entity).map(|value| value.0)
    }

    pub fn is_focused(&self, view_id: ViewId) -> bool {
        self.view_entities
            .get(&view_id)
            .and_then(|entity| self.world.get::<Focused>(*entity))
            .is_some()
    }

    fn entity_for(&self, view_id: ViewId) -> Result<bevy_ecs::entity::Entity, ViewLifecycleError> {
        self.view_entities
            .get(&view_id)
            .copied()
            .ok_or(ViewLifecycleError::MissingViewId(view_id))
    }

    fn focused_entities(&mut self, workspace_id: WorkspaceId) -> Vec<Entity> {
        let mut query = self.world.query::<(Entity, &Workspace, Option<&Focused>)>();
        query
            .iter(&self.world)
            .filter(|(_, workspace, focused)| workspace.id == workspace_id && focused.is_some())
            .map(|(entity, _, _)| entity)
            .collect()
    }
}

impl Default for CompositorWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ViewLifecycleError {
    #[error("view ID {} is already registered", .0.get())]
    DuplicateViewId(ViewId),
    #[error("view ID {} is not registered", .0.get())]
    MissingViewId(ViewId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutKind;

    fn view(value: u64) -> ViewId {
        ViewId::new(value)
    }

    fn workspace(value: u32) -> WorkspaceId {
        WorkspaceId::new(value)
    }

    #[test]
    fn layout_system_uses_stable_view_ids() {
        let mut world = CompositorWorld::new();
        world.spawn_view(view(20), workspace(1)).unwrap();
        world.spawn_view(view(10), workspace(1)).unwrap();

        world.arrange_workspace(
            workspace(1),
            LayoutEngine::new(LayoutKind::Scrolling1D),
            Rect::new(0, 0, 100, 80),
        );

        assert_eq!(world.geometry(view(10)), Some(Rect::new(0, 0, 50, 80)));
        assert_eq!(world.geometry(view(20)), Some(Rect::new(50, 0, 50, 80)));
    }

    #[test]
    fn duplicate_view_ids_are_rejected_without_replacing_the_original() {
        let mut world = CompositorWorld::new();
        world.spawn_view(view(7), workspace(1)).unwrap();

        assert_eq!(
            world.spawn_view(view(7), workspace(2)),
            Err(ViewLifecycleError::DuplicateViewId(view(7)))
        );
        assert_eq!(world.view_count(workspace(1)), 1);
        assert_eq!(world.view_count(workspace(2)), 0);
    }

    #[test]
    fn focus_is_unique_per_workspace_and_survives_workspace_moves() {
        let mut world = CompositorWorld::new();
        world.spawn_view(view(1), workspace(1)).unwrap();
        world.spawn_view(view(2), workspace(1)).unwrap();
        world.spawn_view(view(3), workspace(2)).unwrap();

        world.focus_view(view(1)).unwrap();
        world.focus_view(view(2)).unwrap();
        assert!(!world.is_focused(view(1)));
        assert!(world.is_focused(view(2)));
        assert_eq!(world.focused_view(workspace(1)), Some(view(2)));

        world.arrange_workspace(
            workspace(1),
            LayoutEngine::new(LayoutKind::Scrolling1D),
            Rect::new(0, 0, 100, 80),
        );
        assert!(world.geometry(view(2)).is_some());
        world.focus_view(view(3)).unwrap();
        world.move_view(view(2), workspace(2)).unwrap();
        assert_eq!(world.focused_view(workspace(1)), None);
        assert_eq!(world.focused_view(workspace(2)), Some(view(2)));
        assert!(world.is_focused(view(2)));
        assert!(!world.is_focused(view(3)));
        assert_eq!(world.geometry(view(2)), None);
    }

    #[test]
    fn removed_views_release_their_stable_id() {
        let mut world = CompositorWorld::new();
        world.spawn_view(view(9), workspace(1)).unwrap();
        world.focus_view(view(9)).unwrap();
        world.remove_view(view(9)).unwrap();

        assert_eq!(world.geometry(view(9)), None);
        assert_eq!(world.focused_view(workspace(1)), None);
        assert_eq!(world.spawn_view(view(9), workspace(2)), Ok(()));
    }
}

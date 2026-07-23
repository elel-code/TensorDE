use std::collections::HashMap;

use bevy_ecs::{entity::Entity, world::World};
use thiserror::Error;

use super::components::{
    Focused, StackingOrder, View, ViewContent, ViewEffects, ViewGeometry, ViewLayout, Workspace,
};
use super::{ViewId, WorkspaceId};
use crate::layout::{LayoutEngine, LayoutSnapshot, LayoutState, Rect, SizeConstraints};
use crate::scene::{EffectStyle, SceneNode, SceneSnapshot, SurfaceContent};

pub struct CompositorWorld {
    world: World,
    view_entities: HashMap<ViewId, bevy_ecs::entity::Entity>,
    layout_states: HashMap<WorkspaceId, LayoutState>,
    layout_snapshots: HashMap<WorkspaceId, LayoutSnapshot>,
    next_stacking_order: u64,
}

impl CompositorWorld {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            view_entities: HashMap::new(),
            layout_states: HashMap::new(),
            layout_snapshots: HashMap::new(),
            next_stacking_order: 1,
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
        let stacking_order = self.allocate_stacking_order();
        let entity = self
            .world
            .spawn((
                View { id: view_id },
                Workspace { id: workspace_id },
                ViewLayout::default(),
                ViewContent::default(),
                ViewEffects::default(),
                StackingOrder(stacking_order),
            ))
            .id();
        self.view_entities.insert(view_id, entity);
        self.layout_snapshots.remove(&workspace_id);
        Ok(())
    }

    pub fn remove_view(&mut self, view_id: ViewId) -> Result<(), ViewLifecycleError> {
        let entity = self
            .view_entities
            .remove(&view_id)
            .ok_or(ViewLifecycleError::MissingViewId(view_id))?;
        let workspace_id = self
            .world
            .get::<Workspace>(entity)
            .expect("every view has a workspace")
            .id;
        let removed = self.world.despawn(entity);
        debug_assert!(removed, "view index must reference a live ECS entity");
        self.layout_snapshots.remove(&workspace_id);
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
        self.layout_snapshots.remove(&current_workspace_id);
        self.layout_snapshots.remove(&workspace_id);
        Ok(())
    }

    pub fn set_view_layout(
        &mut self,
        view_id: ViewId,
        layout: ViewLayout,
    ) -> Result<(), ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        let workspace_id = self
            .world
            .get::<Workspace>(entity)
            .expect("every view has a workspace")
            .id;
        self.world.entity_mut(entity).insert(layout);
        self.layout_snapshots.remove(&workspace_id);
        Ok(())
    }

    pub fn set_view_constraints(
        &mut self,
        view_id: ViewId,
        constraints: SizeConstraints,
    ) -> Result<bool, ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        let current = self
            .world
            .get::<ViewLayout>(entity)
            .expect("every view has layout state")
            .constraints;
        if current == constraints {
            return Ok(false);
        }
        let workspace_id = self
            .world
            .get::<Workspace>(entity)
            .expect("every view has a workspace")
            .id;
        self.world
            .get_mut::<ViewLayout>(entity)
            .expect("every view has layout state")
            .constraints = constraints;
        self.layout_snapshots.remove(&workspace_id);
        Ok(true)
    }

    pub fn reset_layout_states(&mut self) {
        self.layout_states.clear();
        self.layout_snapshots.clear();
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
        let stacking_order = self.allocate_stacking_order();
        self.world
            .entity_mut(entity)
            .insert((Focused, StackingOrder(stacking_order)));
        self.layout_snapshots.remove(&workspace_id);
        Ok(())
    }

    pub fn set_view_effects(
        &mut self,
        view_id: ViewId,
        effects: EffectStyle,
    ) -> Result<bool, ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        let mut current = self
            .world
            .get_mut::<ViewEffects>(entity)
            .expect("every view has effect state");
        if current.0 == effects {
            return Ok(false);
        }
        current.0 = effects;
        Ok(true)
    }

    pub fn set_view_content(
        &mut self,
        view_id: ViewId,
        surfaces: Vec<SurfaceContent>,
    ) -> Result<bool, ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        let mut current = self
            .world
            .get_mut::<ViewContent>(entity)
            .expect("every view has content state");
        if current.surfaces == surfaces {
            return Ok(false);
        }
        current.surfaces = surfaces;
        Ok(true)
    }

    pub fn view_content(&self, view_id: ViewId) -> Option<ViewContent> {
        let entity = self.view_entities.get(&view_id).copied()?;
        self.world.get::<ViewContent>(entity).cloned()
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
    ) -> &LayoutSnapshot {
        let mut entities = {
            let mut query = self
                .world
                .query::<(Entity, &View, &Workspace, &ViewLayout, Option<&Focused>)>();
            query
                .iter(&self.world)
                .filter(|(_, _, workspace, _, _)| workspace.id == workspace_id)
                .map(|(entity, view, _, layout, focused)| {
                    (view.id, entity, layout.item(), focused.is_some())
                })
                .collect::<Vec<_>>()
        };
        entities.sort_unstable_by_key(|(view_id, _, _, _)| *view_id);

        let items = entities
            .iter()
            .map(|(_, _, item, _)| *item)
            .collect::<Vec<_>>();
        let focused = entities.iter().position(|(_, _, _, focused)| *focused);
        let state = self.layout_states.entry(workspace_id).or_default();
        let snapshot = engine.arrange(state, output, &items, focused);

        for ((_, entity, _, _), placement) in entities
            .into_iter()
            .zip(snapshot.placements.iter().copied())
        {
            self.world
                .entity_mut(entity)
                .insert(ViewGeometry(placement.geometry));
        }
        self.layout_snapshots.insert(workspace_id, snapshot);
        self.layout_snapshots
            .get(&workspace_id)
            .expect("layout snapshot was just inserted")
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

    pub fn view_layout(&self, view_id: ViewId) -> Option<ViewLayout> {
        let entity = self.view_entities.get(&view_id).copied()?;
        self.world.get::<ViewLayout>(entity).copied()
    }

    pub fn layout_snapshot(&self, workspace_id: WorkspaceId) -> Option<&LayoutSnapshot> {
        self.layout_snapshots.get(&workspace_id)
    }

    pub fn extract_scene(&mut self, workspace_id: WorkspaceId) -> Option<SceneSnapshot> {
        let mut view_ids = {
            let mut query = self.world.query::<(&View, &Workspace)>();
            query
                .iter(&self.world)
                .filter(|(_, workspace)| workspace.id == workspace_id)
                .map(|(view, _)| view.id)
                .collect::<Vec<_>>()
        };
        view_ids.sort_unstable();

        let layout = self.layout_snapshots.get(&workspace_id)?;
        if view_ids.len() != layout.placements.len() {
            return None;
        }
        let mut nodes = Vec::with_capacity(view_ids.len());
        let mut contents = Vec::new();
        for (view_id, placement) in view_ids.into_iter().zip(layout.placements.iter().copied()) {
            let entity = self.view_entities[&view_id];
            let stacking_order = self
                .world
                .get::<StackingOrder>(entity)
                .expect("every view has stacking state")
                .0;
            let effects = self
                .world
                .get::<ViewEffects>(entity)
                .expect("every view has effect state")
                .0;
            let content = self
                .world
                .get::<ViewContent>(entity)
                .expect("every view has content state");
            let start = contents.len();
            contents.extend(content.surfaces.iter().copied());
            let span = crate::scene::ContentSpan::new(start, content.surfaces.len())
                .expect("compositor scene content table exhausted");
            nodes.push(
                SceneNode::new(view_id, stacking_order, placement, effects).with_content(span),
            );
        }
        Some(SceneSnapshot::with_content(
            workspace_id,
            layout.viewport,
            nodes,
            contents,
        ))
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

    fn allocate_stacking_order(&mut self) -> u64 {
        let order = self.next_stacking_order;
        self.next_stacking_order = self
            .next_stacking_order
            .checked_add(1)
            .expect("compositor exhausted the stacking-order space");
        order
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
    use tensor_util::Size;

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

        assert_eq!(world.geometry(view(10)), Some(Rect::new(8, 8, 38, 64)));
        assert_eq!(world.geometry(view(20)), Some(Rect::new(54, 8, 38, 64)));
    }

    #[test]
    fn focused_view_drives_workspace_local_scrolling_state() {
        let mut world = CompositorWorld::new();
        for id in 1..=3 {
            world.spawn_view(view(id), workspace(1)).unwrap();
        }
        world.focus_view(view(3)).unwrap();

        let snapshot = world.arrange_workspace(
            workspace(1),
            LayoutEngine::new(LayoutKind::Scrolling1D),
            Rect::new(0, 0, 100, 80),
        );

        assert_eq!(snapshot.horizontal_offset, -46);
        assert_eq!(world.geometry(view(3)), Some(Rect::new(54, 8, 38, 64)));
    }

    #[test]
    fn per_view_constraints_flow_into_layout_geometry() {
        use crate::layout::{LayoutLength, SizeConstraints};
        use tensor_util::Size;

        let mut world = CompositorWorld::new();
        world.spawn_view(view(1), workspace(1)).unwrap();
        world
            .set_view_layout(
                view(1),
                ViewLayout {
                    constraints: SizeConstraints::new(Size::new(1, 1), Some(200), Some(100)),
                    primary_size: Some(LayoutLength::fixed(400)),
                },
            )
            .unwrap();

        world.arrange_workspace(
            workspace(1),
            LayoutEngine::new(LayoutKind::Spatial2D),
            Rect::new(0, 0, 500, 300),
        );

        assert_eq!(world.geometry(view(1)), Some(Rect::new(150, 100, 200, 100)));
    }

    #[test]
    fn protocol_constraints_preserve_the_configured_primary_size() {
        use crate::layout::{LayoutLength, SizeConstraints};
        use tensor_util::Size;

        let mut world = CompositorWorld::new();
        world.spawn_view(view(1), workspace(1)).unwrap();
        world
            .set_view_layout(
                view(1),
                ViewLayout {
                    constraints: SizeConstraints::default(),
                    primary_size: Some(LayoutLength::fixed(420)),
                },
            )
            .unwrap();

        let constraints = SizeConstraints::new(Size::new(200, 100), Some(800), Some(600));
        assert!(world.set_view_constraints(view(1), constraints).unwrap());
        assert!(!world.set_view_constraints(view(1), constraints).unwrap());

        assert_eq!(
            world.view_layout(view(1)),
            Some(ViewLayout {
                constraints,
                primary_size: Some(LayoutLength::fixed(420)),
            })
        );
    }

    #[test]
    fn layout_snapshot_is_shared_and_invalidated_by_scene_changes() {
        let mut world = CompositorWorld::new();
        world.spawn_view(view(1), workspace(1)).unwrap();
        world.arrange_workspace(
            workspace(1),
            LayoutEngine::new(LayoutKind::Scrolling1D),
            Rect::new(0, 0, 100, 80),
        );

        assert_eq!(
            world.layout_snapshot(workspace(1)).unwrap().placements[0].geometry,
            Rect::new(8, 8, 38, 64)
        );

        world.spawn_view(view(2), workspace(1)).unwrap();
        assert_eq!(world.layout_snapshot(workspace(1)), None);
    }

    #[test]
    fn scene_extraction_separates_stable_nodes_from_draw_order() {
        use crate::scene::{LinearRgba16, ShadowStyle};

        let mut world = CompositorWorld::new();
        world.spawn_view(view(2), workspace(1)).unwrap();
        world.spawn_view(view(1), workspace(1)).unwrap();
        world.focus_view(view(2)).unwrap();
        let effects = EffectStyle {
            corner_radius: 12,
            shadow: Some(ShadowStyle {
                offset_x: 2,
                offset_y: 3,
                blur_radius: 8,
                spread: 1,
                color: LinearRgba16::new(0, 0, 0, 32_768),
            }),
            ..Default::default()
        };
        assert!(world.set_view_effects(view(2), effects).unwrap());
        assert!(!world.set_view_effects(view(2), effects).unwrap());
        world.arrange_workspace(
            workspace(1),
            LayoutEngine::new(LayoutKind::Scrolling1D),
            Rect::new(0, 0, 100, 80),
        );

        let scene = world.extract_scene(workspace(1)).unwrap();

        assert_eq!(
            scene
                .nodes()
                .iter()
                .map(|node| node.view_id)
                .collect::<Vec<_>>(),
            [view(1), view(2)]
        );
        assert_eq!(
            scene
                .draw_order()
                .map(|node| node.view_id)
                .collect::<Vec<_>>(),
            [view(1), view(2)]
        );
        assert_eq!(scene.nodes()[1].effects, effects);
    }

    #[test]
    fn scene_extraction_keeps_surface_content_out_of_smithay_and_entity_ids() {
        use crate::scene::{ContentRevision, SurfaceContent, SurfaceTransform};

        let mut world = CompositorWorld::new();
        world.spawn_view(view(1), workspace(1)).unwrap();
        let content = SurfaceContent {
            surface_id: crate::ecs::SurfaceId::new(7),
            buffer_id: crate::ecs::SurfaceBufferId::new(9),
            revision: ContentRevision::new(3),
            buffer_size: Size::new(640, 480),
            local_geometry: Rect::new(0, 0, 640, 480),
            buffer_scale: 1,
            transform: SurfaceTransform::Normal,
        };
        assert!(world.set_view_content(view(1), vec![content]).unwrap());
        assert_eq!(world.view_content(view(1)).unwrap().surfaces, [content]);
        world.arrange_workspace(
            workspace(1),
            LayoutEngine::new(LayoutKind::Scrolling1D),
            Rect::new(0, 0, 100, 80),
        );

        let scene = world.extract_scene(workspace(1)).unwrap();
        assert_eq!(scene.contents(), [content]);
        assert_eq!(scene.contents_for(&scene.nodes()[0]), [content]);
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

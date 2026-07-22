use bevy_ecs::{entity::Entity, world::World};

use super::components::{View, ViewGeometry, Workspace};
use crate::layout::{LayoutEngine, Rect};

pub struct CompositorWorld {
    world: World,
}

#[allow(dead_code)]
impl CompositorWorld {
    pub fn new() -> Self {
        Self {
            world: World::new(),
        }
    }

    pub fn spawn_view(&mut self, view_id: u64, workspace_id: u32) -> Entity {
        self.world
            .spawn((View { id: view_id }, Workspace { id: workspace_id }))
            .id()
    }

    pub fn arrange_workspace(&mut self, workspace_id: u32, engine: LayoutEngine, output: Rect) {
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

    pub fn view_count(&mut self, workspace_id: u32) -> usize {
        let mut query = self.world.query::<&Workspace>();
        query
            .iter(&self.world)
            .filter(|workspace| workspace.id == workspace_id)
            .count()
    }

    pub fn geometry(&self, entity: Entity) -> Option<Rect> {
        self.world.get::<ViewGeometry>(entity).map(|value| value.0)
    }
}

impl Default for CompositorWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutKind;

    #[test]
    fn layout_system_uses_stable_view_ids() {
        let mut world = CompositorWorld::new();
        let second = world.spawn_view(20, 1);
        let first = world.spawn_view(10, 1);

        world.arrange_workspace(
            1,
            LayoutEngine::new(LayoutKind::Niri1D),
            Rect::new(0, 0, 100, 80),
        );

        assert_eq!(world.geometry(first), Some(Rect::new(0, 0, 50, 80)));
        assert_eq!(world.geometry(second), Some(Rect::new(50, 0, 50, 80)));
    }
}

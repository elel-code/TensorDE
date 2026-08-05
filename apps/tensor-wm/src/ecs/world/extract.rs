//! Value-only scene extraction from retained ECS views.

use super::CompositorWorld;
use crate::{
    ecs::WorkspaceId,
    ecs::components::{
        Focused, StackingOrder, View, ViewBackdropRegion, ViewContent, ViewEffects, ViewGeometry,
        ViewPresentationHint, Workspace,
    },
    scene::{SceneNode, SceneSnapshot},
};

impl CompositorWorld {
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
        let mut nodes = Vec::with_capacity(view_ids.len());
        let mut contents = Vec::new();
        for view_id in view_ids {
            let entity = self.view_entities[&view_id];
            let geometry = self.world.get::<ViewGeometry>(entity)?.0;
            let placement = crate::layout::LayoutPlacement::new(geometry, layout.viewport);
            let stacking_order = self
                .world
                .get::<StackingOrder>(entity)
                .expect("every view has stacking state")
                .0;
            let mut effects = self
                .world
                .get::<ViewEffects>(entity)
                .expect("every view has effect state")
                .0;
            if let Some(shadow) = self.appearance.window_shadow.effect() {
                effects.shadow = Some(shadow);
            }
            if self.appearance.window_corners.radius > 0 {
                effects.corner_radius = self.appearance.window_corners.radius;
            }
            let backdrop_region = self
                .world
                .get::<ViewBackdropRegion>(entity)
                .expect("every view has backdrop-region state")
                .0
                .clone();
            let focus_outline = self
                .world
                .get::<Focused>(entity)
                .is_some()
                .then(|| self.appearance.focus_ring.outline())
                .flatten();
            let presentation_hint = self
                .world
                .get::<ViewPresentationHint>(entity)
                .expect("every view has presentation state")
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
                SceneNode::new(view_id, stacking_order, placement, effects)
                    .with_backdrop_region(backdrop_region)
                    .with_focus_outline(focus_outline)
                    .with_presentation_hint(presentation_hint)
                    .with_content(span),
            );
        }
        Some(SceneSnapshot::with_content(
            workspace_id,
            layout.viewport,
            nodes,
            contents,
        ))
    }
}

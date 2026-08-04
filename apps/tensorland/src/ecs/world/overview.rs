//! Stable value inventory used to construct compositor-owned overview plans.

use super::{CompositorWorld, ViewId, WorkspaceId};
use crate::{
    ecs::components::{
        Focused, LastViewGeometry, StackingOrder, View, ViewGeometry, ViewPlacement, Workspace,
    },
    layout::Rect,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverviewViewKind {
    Tiled,
    Floating,
    Attached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverviewView {
    pub id: ViewId,
    pub root: ViewId,
    pub geometry: Option<Rect>,
    pub focused: bool,
    pub kind: OverviewViewKind,
    pub stacking_order: u64,
}

impl CompositorWorld {
    /// Return a stable back-to-front inventory without exposing Bevy entities.
    /// Current geometry wins; workspace-hidden views retain their last valid
    /// arranged geometry solely for overview planning.
    pub fn overview_views(&mut self, workspace_id: WorkspaceId) -> Vec<OverviewView> {
        let mut views = {
            let mut query = self.world.query::<(
                &View,
                &Workspace,
                &ViewPlacement,
                Option<&ViewGeometry>,
                Option<&LastViewGeometry>,
                Option<&Focused>,
                &StackingOrder,
            )>();
            query
                .iter(&self.world)
                .filter(|(_, workspace, _, _, _, _, _)| workspace.id == workspace_id)
                .map(
                    |(view, _, placement, geometry, last_geometry, focused, stacking)| {
                        let (kind, placement_geometry) = match placement {
                            ViewPlacement::Tiled => (OverviewViewKind::Tiled, None),
                            ViewPlacement::Floating { geometry } => {
                                (OverviewViewKind::Floating, Some(*geometry))
                            }
                            ViewPlacement::Attached { .. } => (OverviewViewKind::Attached, None),
                        };
                        (
                            view.id,
                            geometry
                                .map(|geometry| geometry.0)
                                .or_else(|| last_geometry.map(|geometry| geometry.0))
                                .or(placement_geometry),
                            focused.is_some(),
                            kind,
                            stacking.0,
                        )
                    },
                )
                .collect::<Vec<_>>()
        };
        views.sort_unstable_by_key(|(id, _, _, _, stacking)| (*stacking, *id));
        views
            .into_iter()
            .filter_map(|(id, geometry, focused, kind, stacking_order)| {
                Some(OverviewView {
                    id,
                    root: self.tiled_ancestor(id)?,
                    geometry,
                    focused,
                    kind,
                    stacking_order,
                })
            })
            .collect()
    }
}

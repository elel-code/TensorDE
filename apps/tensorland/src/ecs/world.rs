use std::collections::HashMap;

use bevy_ecs::{entity::Entity, world::World};
use thiserror::Error;

use super::components::{
    Focused, LastViewGeometry, MinimizedFrom, StackingOrder, View, ViewBackdropRegion, ViewContent,
    ViewEffects, ViewGeometry, ViewLayout, ViewPlacement, ViewPresentationHint, Workspace,
};
use super::{ViewId, WorkspaceId};
use crate::layout::{LayoutEngine, LayoutSnapshot, LayoutState, Rect, SizeConstraints};
use crate::scene::{BackdropBlur, BackdropRegion, EffectStyle, SceneAppearance, SurfaceContent};
use tensor_util::Size;

mod extract;
mod minimize;
mod overview;

pub use overview::{OverviewView, OverviewViewKind};

pub struct CompositorWorld {
    world: World,
    appearance: SceneAppearance,
    view_entities: HashMap<ViewId, bevy_ecs::entity::Entity>,
    layout_states: HashMap<WorkspaceId, LayoutState>,
    layout_snapshots: HashMap<WorkspaceId, LayoutSnapshot>,
    next_stacking_order: u64,
}

#[derive(Clone, Copy)]
struct WorkspaceView {
    id: ViewId,
    entity: Entity,
    layout: ViewLayout,
    placement: ViewPlacement,
    focused: bool,
}

impl CompositorWorld {
    pub fn new() -> Self {
        Self::with_appearance(SceneAppearance::default())
    }

    /// Create an ECS world with compositor appearance kept outside per-view
    /// components. Style changes therefore never leak renderer or
    /// configuration ownership into protocol entities.
    pub fn with_appearance(appearance: SceneAppearance) -> Self {
        Self {
            world: World::new(),
            appearance,
            view_entities: HashMap::new(),
            layout_states: HashMap::new(),
            layout_snapshots: HashMap::new(),
            next_stacking_order: 1,
        }
    }

    /// Replace the extracted scene style. Configuration reload can use this
    /// without changing stable view identities or their protocol state.
    pub fn set_appearance(&mut self, appearance: SceneAppearance) -> bool {
        if self.appearance == appearance {
            return false;
        }
        self.appearance = appearance;
        true
    }

    pub const fn appearance(&self) -> SceneAppearance {
        self.appearance
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
                ViewPlacement::default(),
                ViewContent::default(),
                ViewEffects::default(),
                ViewPresentationHint::default(),
                ViewBackdropRegion::default(),
                StackingOrder(stacking_order),
            ))
            .id();
        self.view_entities.insert(view_id, entity);
        self.layout_snapshots.remove(&workspace_id);
        Ok(())
    }

    pub fn remove_view(&mut self, view_id: ViewId) -> Result<(), ViewLifecycleError> {
        if let Some(child) = self.attached_children(view_id).into_iter().next() {
            return Err(ViewLifecycleError::AttachedChild {
                owner: view_id,
                child,
            });
        }
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
        if self
            .world
            .get::<ViewPlacement>(entity)
            .expect("every view has placement state")
            .owner()
            .is_some()
        {
            return Err(ViewLifecycleError::AttachedViewCannotMove(view_id));
        }
        let current_workspace_id = self
            .world
            .get::<Workspace>(entity)
            .expect("every view has a workspace")
            .id;
        if current_workspace_id == workspace_id {
            return Ok(());
        }
        let family = self.view_family(view_id);
        let was_focused = family.iter().copied().any(|member| {
            self.view_entities
                .get(&member)
                .and_then(|entity| self.world.get::<Focused>(*entity))
                .is_some()
        });
        if was_focused {
            for focused_entity in self.focused_entities(workspace_id) {
                self.world.entity_mut(focused_entity).remove::<Focused>();
            }
        }
        for member in family {
            let entity = self.view_entities[&member];
            let mut entity = self.world.entity_mut(entity);
            entity
                .get_mut::<Workspace>()
                .expect("every view has a workspace")
                .id = workspace_id;
            entity.remove::<ViewGeometry>();
            entity.remove::<MinimizedFrom>();
        }
        self.layout_snapshots.remove(&current_workspace_id);
        self.layout_snapshots.remove(&workspace_id);
        Ok(())
    }

    /// Change whether a view participates in primary workspace layout or
    /// derives a separate dialog-like rectangle from an owning view.
    pub fn set_view_placement(
        &mut self,
        view_id: ViewId,
        placement: ViewPlacement,
    ) -> Result<bool, ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        let current = *self
            .world
            .get::<ViewPlacement>(entity)
            .expect("every view has placement state");
        if current == placement {
            return Ok(false);
        }
        let workspace_id = self
            .world
            .get::<Workspace>(entity)
            .expect("every view has a workspace")
            .id;
        if let Some(owner) = placement.owner() {
            if owner == view_id {
                return Err(ViewLifecycleError::SelfAttachment(view_id));
            }
            let owner_entity = self.entity_for(owner)?;
            let owner_workspace = self
                .world
                .get::<Workspace>(owner_entity)
                .expect("every view has a workspace")
                .id;
            if owner_workspace != workspace_id {
                return Err(ViewLifecycleError::CrossWorkspaceAttachment {
                    view: view_id,
                    owner,
                });
            }
            if self.attachment_would_cycle(view_id, owner) {
                return Err(ViewLifecycleError::AttachmentCycle {
                    view: view_id,
                    owner,
                });
            }
        }
        self.world.entity_mut(entity).insert(placement);
        self.layout_snapshots.remove(&workspace_id);
        Ok(true)
    }

    pub fn view_placement(&self, view_id: ViewId) -> Option<ViewPlacement> {
        let entity = self.view_entities.get(&view_id).copied()?;
        self.world.get::<ViewPlacement>(entity).copied()
    }

    /// Update a floating view without invalidating the tiled layout snapshot.
    /// Interactive moves change only one retained scene node.
    pub fn update_floating_geometry(
        &mut self,
        view_id: ViewId,
        geometry: Rect,
    ) -> Result<bool, ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        let placement = self
            .world
            .get::<ViewPlacement>(entity)
            .copied()
            .expect("every view has placement state");
        let ViewPlacement::Floating { geometry: current } = placement else {
            return Ok(false);
        };
        if current == geometry {
            return Ok(false);
        }
        self.world.entity_mut(entity).insert((
            ViewPlacement::Floating { geometry },
            ViewGeometry(geometry),
            LastViewGeometry(geometry),
        ));
        Ok(true)
    }

    /// Return the direct attached children in stable ID order.
    pub fn attached_children(&self, owner: ViewId) -> Vec<ViewId> {
        let mut children = self
            .view_entities
            .iter()
            .filter_map(|(view_id, entity)| {
                (self.world.get::<ViewPlacement>(*entity)?.owner() == Some(owner))
                    .then_some(*view_id)
            })
            .collect::<Vec<_>>();
        children.sort_unstable();
        children
    }

    /// Resolve a view to the tiled ancestor which owns workspace navigation.
    pub fn tiled_ancestor(&self, view_id: ViewId) -> Option<ViewId> {
        let mut current = view_id;
        for _ in 0..self.view_entities.len() {
            let placement = self.view_placement(current)?;
            let Some(owner) = placement.owner() else {
                return Some(current);
            };
            current = owner;
        }
        None
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
        // A late input capability can need to restore only the protocol seat
        // focus for the view that ECS already selected. Re-selecting that
        // exact view must not perturb scene stacking or invalidate the
        // current layout snapshot: no compositor-visible scene state changed.
        if self.world.get::<Focused>(entity).is_some() {
            return Ok(());
        }
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
        self.raise_view_family(view_id)?;
        self.layout_snapshots.remove(&workspace_id);
        Ok(())
    }

    pub fn view_effects(&self, view_id: ViewId) -> Option<EffectStyle> {
        let entity = self.view_entities.get(&view_id).copied()?;
        self.world
            .get::<ViewEffects>(entity)
            .map(|effects| effects.0)
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

    pub(crate) fn set_view_backdrop_effect(
        &mut self,
        view_id: ViewId,
        blur: Option<BackdropBlur>,
        region: Option<BackdropRegion>,
    ) -> Result<bool, ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        let current_blur = self
            .world
            .get::<ViewEffects>(entity)
            .expect("every view has effect state")
            .0
            .backdrop_blur;
        let current_region = &self
            .world
            .get::<ViewBackdropRegion>(entity)
            .expect("every view has backdrop-region state")
            .0;
        if current_blur == blur && current_region == &region {
            return Ok(false);
        }
        self.world
            .get_mut::<ViewEffects>(entity)
            .expect("every view has effect state")
            .0
            .backdrop_blur = blur;
        self.world
            .get_mut::<ViewBackdropRegion>(entity)
            .expect("every view has backdrop-region state")
            .0 = region;
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

    /// Select the view that should inherit focus when `view_id` disappears.
    ///
    /// The policy is deliberately part of the ECS ownership boundary rather
    /// than the Wayland teardown path. An attached dialog returns to its
    /// owner; a tiled view returns to the most recently raised surviving view
    /// in the same workspace. This gives close-time focus the same stable
    /// scene ordering that rendering and input already use, while keeping
    /// protocol resources out of ECS.
    #[cfg(any(feature = "tty", test))]
    pub(crate) fn focus_replacement_after_removal(
        &mut self,
        view_id: ViewId,
    ) -> Result<Option<ViewId>, ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        if self.world.get::<Focused>(entity).is_none() {
            return Ok(None);
        }
        let workspace_id = self
            .world
            .get::<Workspace>(entity)
            .expect("every view has a workspace")
            .id;
        if let Some(owner) = self
            .world
            .get::<ViewPlacement>(entity)
            .expect("every view has placement state")
            .owner()
        {
            return Ok(Some(owner));
        }

        let mut query = self.world.query::<(&View, &Workspace, &StackingOrder)>();
        Ok(query
            .iter(&self.world)
            .filter(|(view, workspace, _)| view.id != view_id && workspace.id == workspace_id)
            .max_by_key(|(view, _, stacking)| (stacking.0, view.id))
            .map(|(view, _, _)| view.id))
    }

    /// Select the most recently raised view outside a complete attachment
    /// family which is about to leave its workspace.
    #[cfg(any(feature = "tty", test))]
    pub(crate) fn focus_replacement_after_family_removal(
        &mut self,
        root: ViewId,
    ) -> Result<Option<ViewId>, ViewLifecycleError> {
        let entity = self.entity_for(root)?;
        let workspace_id = self
            .world
            .get::<Workspace>(entity)
            .expect("every view has a workspace")
            .id;
        let family = self.view_family(root);
        let mut query = self.world.query::<(&View, &Workspace, &StackingOrder)>();
        Ok(query
            .iter(&self.world)
            .filter(|(view, workspace, _)| {
                workspace.id == workspace_id && !family.contains(&view.id)
            })
            .max_by_key(|(view, _, stacking)| (stacking.0, view.id))
            .map(|(view, _, _)| view.id))
    }

    pub fn arrange_workspace(
        &mut self,
        workspace_id: WorkspaceId,
        engine: LayoutEngine,
        output: Rect,
    ) -> &LayoutSnapshot {
        let mut views = {
            let mut query = self.world.query::<(
                Entity,
                &View,
                &Workspace,
                &ViewLayout,
                &ViewPlacement,
                Option<&Focused>,
            )>();
            query
                .iter(&self.world)
                .filter(|(_, _, workspace, _, _, _)| workspace.id == workspace_id)
                .map(
                    |(entity, view, _, layout, placement, focused)| WorkspaceView {
                        id: view.id,
                        entity,
                        layout: *layout,
                        placement: *placement,
                        focused: focused.is_some(),
                    },
                )
                .collect::<Vec<_>>()
        };
        views.sort_unstable_by_key(|view| view.id);

        let tiled = views
            .iter()
            .filter(|view| view.placement.is_tiled())
            .copied()
            .collect::<Vec<_>>();
        let items = tiled
            .iter()
            .map(|view| view.layout.item())
            .collect::<Vec<_>>();
        let focused = views
            .iter()
            .find(|view| view.focused)
            .and_then(|view| self.tiled_ancestor(view.id))
            .and_then(|owner| tiled.iter().position(|view| view.id == owner));
        let state = self.layout_states.entry(workspace_id).or_default();
        let snapshot = engine.arrange(state, output, &items, focused);

        let mut geometries = HashMap::with_capacity(views.len());
        for (view, placement) in tiled.into_iter().zip(snapshot.placements.iter().copied()) {
            geometries.insert(view.id, placement.geometry);
        }

        let mut pending = views
            .iter()
            .copied()
            .filter(|view| !view.placement.is_tiled())
            .collect::<Vec<_>>();
        for _ in 0..pending.len() {
            let mut unresolved = Vec::new();
            let mut progressed = false;
            for view in pending {
                match view.placement {
                    ViewPlacement::Tiled => {}
                    ViewPlacement::Floating { geometry } => {
                        geometries.insert(view.id, geometry);
                        progressed = true;
                    }
                    ViewPlacement::Attached {
                        owner,
                        preferred_size,
                    } => {
                        let Some(owner_geometry) = geometries.get(&owner).copied() else {
                            unresolved.push(view);
                            continue;
                        };
                        geometries.insert(
                            view.id,
                            attached_geometry(
                                owner_geometry,
                                view.layout.constraints,
                                preferred_size,
                            ),
                        );
                        progressed = true;
                    }
                }
            }
            if unresolved.is_empty() {
                break;
            }
            debug_assert!(
                progressed,
                "validated attachment graph must resolve into a tiled root"
            );
            if !progressed {
                break;
            }
            pending = unresolved;
        }

        for view in views {
            if let Some(geometry) = geometries.get(&view.id).copied() {
                self.world
                    .entity_mut(view.entity)
                    .insert((ViewGeometry(geometry), LastViewGeometry(geometry)));
            } else {
                self.world.entity_mut(view.entity).remove::<ViewGeometry>();
            }
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

    pub fn view_workspace(&self, view_id: ViewId) -> Option<WorkspaceId> {
        let entity = self.view_entities.get(&view_id).copied()?;
        self.world.get::<Workspace>(entity).map(|ws| ws.id)
    }

    pub fn set_presentation_hint(
        &mut self,
        view_id: ViewId,
        hint: tensor_protocol::SurfacePresentationHint,
    ) -> Result<bool, ViewLifecycleError> {
        let entity = self.entity_for(view_id)?;
        let current = self
            .world
            .get::<ViewPresentationHint>(entity)
            .expect("every view has presentation state")
            .0;
        if current == hint {
            return Ok(false);
        }
        self.world
            .entity_mut(entity)
            .insert(ViewPresentationHint(hint));
        Ok(true)
    }

    pub fn presentation_hint(
        &self,
        view_id: ViewId,
    ) -> Option<tensor_protocol::SurfacePresentationHint> {
        let entity = self.view_entities.get(&view_id).copied()?;
        self.world
            .get::<ViewPresentationHint>(entity)
            .map(|value| value.0)
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

    fn attachment_would_cycle(&self, view_id: ViewId, owner: ViewId) -> bool {
        let mut current = owner;
        for _ in 0..self.view_entities.len() {
            if current == view_id {
                return true;
            }
            let Some(next) = self.view_placement(current).and_then(ViewPlacement::owner) else {
                return false;
            };
            current = next;
        }
        true
    }

    fn view_family(&self, root: ViewId) -> Vec<ViewId> {
        let mut family = vec![root];
        let mut index = 0;
        while let Some(owner) = family.get(index).copied() {
            family.extend(self.attached_children(owner));
            index += 1;
        }
        family
    }

    fn raise_view_family(&mut self, focused: ViewId) -> Result<(), ViewLifecycleError> {
        let root = self
            .tiled_ancestor(focused)
            .ok_or(ViewLifecycleError::BrokenAttachment(focused))?;
        let mut family = self.view_family(root);
        if focused != root {
            let focused_subtree = self.view_family(focused);
            family.retain(|view_id| !focused_subtree.contains(view_id));
            family.extend(focused_subtree);
        }
        for view_id in family {
            let entity = self.entity_for(view_id)?;
            let stacking_order = self.allocate_stacking_order();
            self.world
                .entity_mut(entity)
                .insert(StackingOrder(stacking_order));
        }
        Ok(())
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

fn attached_geometry(owner: Rect, constraints: SizeConstraints, preferred_size: Size) -> Rect {
    let size = constraints.constrain(preferred_size);
    Rect::new(
        centered_axis(owner.x, owner.width, size.width),
        centered_axis(owner.y, owner.height, size.height),
        size.width,
        size.height,
    )
}

fn centered_axis(origin: i32, available: u32, desired: u32) -> i32 {
    let center = i64::from(origin).saturating_add(i64::from(available) / 2);
    let start = center.saturating_sub(i64::from(desired) / 2);
    i32::try_from(start).unwrap_or(if start.is_negative() {
        i32::MIN
    } else {
        i32::MAX
    })
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
    #[error("view {} cannot be attached to itself", .0.get())]
    SelfAttachment(ViewId),
    #[error("view {} cannot attach to view {} in another workspace", .view.get(), .owner.get())]
    CrossWorkspaceAttachment { view: ViewId, owner: ViewId },
    #[error("attaching view {} to view {} would create a cycle", .view.get(), .owner.get())]
    AttachmentCycle { view: ViewId, owner: ViewId },
    #[error("view {} cannot move independently while attached", .0.get())]
    AttachedViewCannotMove(ViewId),
    #[error("view {} still owns attached view {}", .owner.get(), .child.get())]
    AttachedChild { owner: ViewId, child: ViewId },
    #[error("view {} has an invalid attachment chain", .0.get())]
    BrokenAttachment(ViewId),
}

#[cfg(test)]
mod tests;

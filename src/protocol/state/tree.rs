use tracing::warn;
use wayland_server::{Resource, backend::ObjectId, protocol::wl_surface::WlSurface};

use crate::protocol::globals::compositor::{
    SUBSURFACE_ROLE, SubsurfaceCachedState, SurfaceData, TraversalAction, get_parent,
    with_surface_tree_upward,
};
use crate::scene::SurfaceLayer;

use super::{
    RuntimeState, find_popup_root_surface,
    surfaces::{SurfaceCommit, surface_render_snapshot},
};

impl RuntimeState {
    pub(crate) fn defer_surface_sync(
        &mut self,
        root: &WlSurface,
        surface: &WlSurface,
        points: Option<super::ExplicitSyncPoints>,
    ) {
        let deferred = super::DeferredSurfaceSync {
            root: root.id(),
            surface: surface.clone(),
            points,
        };
        if let Some(previous) = self.pending_surface_sync.insert(surface.id(), deferred)
            && let Some(points) = previous.points
        {
            self.finish_unused_explicit_sync(points);
        }
    }

    pub(crate) fn reconcile_deferred_surface_sync(&mut self, root: &WlSurface) {
        let root_id = root.id();
        let surfaces = self
            .pending_surface_sync
            .iter()
            .filter_map(|(id, deferred)| (deferred.root == root_id).then_some(id.clone()))
            .collect::<Vec<_>>();
        for id in surfaces {
            let deferred = self
                .pending_surface_sync
                .remove(&id)
                .expect("deferred surface sync key was collected from the same map");
            self.reconcile_surface_sync(&deferred.surface, deferred.points);
        }
    }

    pub(crate) fn discard_deferred_surface_sync(&mut self, surface: &WlSurface) {
        if let Some(deferred) = self.pending_surface_sync.remove(&surface.id())
            && let Some(points) = deferred.points
        {
            self.finish_unused_explicit_sync(points);
        }
    }

    pub(crate) fn discard_deferred_view_sync(&mut self, root: &ObjectId) {
        let surfaces = self
            .pending_surface_sync
            .iter()
            .filter_map(|(id, deferred)| (&deferred.root == root).then_some(id.clone()))
            .collect::<Vec<_>>();
        for id in surfaces {
            let deferred = self
                .pending_surface_sync
                .remove(&id)
                .expect("deferred view sync key was collected from the same map");
            if let Some(points) = deferred.points {
                self.finish_unused_explicit_sync(points);
            }
        }
    }

    /// Resolve any toplevel, subsurface, popup, or popup-subsurface resource
    /// to the mapped toplevel that owns its value-only scene node.
    pub(crate) fn owning_view_root(&self, surface: &WlSurface) -> Option<WlSurface> {
        let mut tree_root = surface.clone();
        while let Some(parent) = get_parent(&tree_root) {
            tree_root = parent;
        }
        if self.view_for_surface(&tree_root).is_some() {
            return Some(tree_root);
        }
        if let Some(popup) = self.popups.find_popup(&tree_root)
            && let Ok(root) = find_popup_root_surface(&popup)
            && self.view_for_surface(&root).is_some()
        {
            return Some(root);
        }
        if let Some(parent) = self.protocol_globals.input_method.popup_parent(&tree_root) {
            return self.owning_view_root(&parent);
        }
        #[cfg(feature = "xwayland")]
        if let Some(root) = self.x11_popup_owner_for_surface(&tree_root) {
            return Some(root);
        }

        let root_id = self.surface_buffers.view_root_for_surface(&surface.id())?;
        self.mapped_root_by_id(root_id)
    }

    /// Rebuild one view's flat surface table in exact draw order. Wayland
    /// resources stay local to this traversal; ECS receives only stable IDs,
    /// geometry, buffer identities, and content revisions.
    pub(crate) fn update_surface_content(&mut self, root: &WlSurface) -> bool {
        let Some(view_id) = self.view_for_surface(root) else {
            return false;
        };
        let Some(window) = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(root))
            .cloned()
        else {
            return false;
        };

        let geometry = window.geometry();
        let mut commits = Vec::new();
        collect_surface_tree(
            root,
            (
                geometry.loc.x.saturating_neg(),
                geometry.loc.y.saturating_neg(),
            ),
            SurfaceLayer::View,
            &mut commits,
        );

        for (popup, offset) in self.popups.popups_for_surface(root).rev() {
            let popup_geometry = popup.geometry();
            collect_surface_tree(
                popup.wl_surface(),
                popup_base(
                    (offset.x, offset.y),
                    (popup_geometry.loc.x, popup_geometry.loc.y),
                ),
                SurfaceLayer::Popup,
                &mut commits,
            );
        }

        #[cfg(feature = "xwayland")]
        for (popup, base) in self.x11_popup_surface_trees_for_root(root) {
            collect_surface_tree(&popup, base, SurfaceLayer::Popup, &mut commits);
        }

        let input_popup_base = self
            .protocol_globals
            .input_method
            .active_popup_context()
            .and_then(|(focused, rectangle)| {
                let focused_offset = commits
                    .iter()
                    .find(|(surface, _)| *surface == focused.id())?
                    .1
                    .local_offset;
                Some((
                    focused_offset.0.saturating_add(rectangle.loc.x),
                    focused_offset
                        .1
                        .saturating_add(rectangle.loc.y)
                        .saturating_add(rectangle.size.h),
                ))
            });
        if let Some(base) = input_popup_base {
            self.protocol_globals
                .input_method
                .for_each_visible_popup(|popup| {
                    collect_surface_tree(
                        popup.wl_surface(),
                        base,
                        SurfaceLayer::Popup,
                        &mut commits,
                    );
                });
        }

        let Some(update) = self.surface_buffers.update_view_tree(&root.id(), commits) else {
            warn!(
                view_id = view_id.get(),
                "surface identity space is exhausted"
            );
            return false;
        };
        for surface_id in update.removed_surfaces {
            if let Some(sync) = self.surface_sync.remove(surface_id) {
                self.finish_surface_sync(surface_id, sync.release);
            }
        }
        self.release_client_buffers(update.released_buffers);
        self.flush_client_releases();

        match self.world.set_view_content(view_id, update.contents) {
            Ok(changed) => changed,
            Err(error) => {
                warn!(%error, view_id = view_id.get(), "failed to update view surface tree");
                false
            }
        }
    }

    fn mapped_root_by_id(&self, root: &ObjectId) -> Option<WlSurface> {
        self.space
            .elements()
            .filter_map(|window| window.wl_surface().map(|surface| surface.into_owned()))
            .find(|surface| &surface.id() == root)
    }
}

pub(super) fn collect_surface_tree(
    root: &WlSurface,
    base: (i32, i32),
    layer: SurfaceLayer,
    commits: &mut Vec<(ObjectId, SurfaceCommit)>,
) {
    with_surface_tree_upward(
        root,
        base,
        |surface, states, parent| {
            if !surface.is_alive() {
                return TraversalAction::SkipChildren;
            }
            TraversalAction::DoChildren(accumulate_offset(*parent, subsurface_offset(states)))
        },
        |surface, states, parent| {
            if !surface.is_alive() {
                return;
            }
            let offset = accumulate_offset(*parent, subsurface_offset(states));
            if let Some(snapshot) = surface_snapshot(states, offset, layer) {
                commits.push((surface.id(), snapshot));
            }
        },
        |_, _, _| true,
    );
}

fn surface_snapshot(
    states: &SurfaceData,
    local_offset: (i32, i32),
    layer: SurfaceLayer,
) -> Option<SurfaceCommit> {
    let renderer = surface_render_snapshot(states)?;
    Some(SurfaceCommit {
        buffer: renderer.buffer,
        logical_size: renderer.logical_size,
        local_offset,
        commit: renderer.commit,
        buffer_scale: renderer.buffer_scale,
        transform: renderer.transform,
        source: renderer.source,
        layer,
        alpha: renderer.alpha,
    })
}

fn subsurface_offset(states: &SurfaceData) -> (i32, i32) {
    if states.role != Some(SUBSURFACE_ROLE) {
        return (0, 0);
    }
    let mut cached = states.cached_state.get::<SubsurfaceCachedState>();
    let location = cached.current().location;
    (location.x, location.y)
}

fn accumulate_offset(parent: (i32, i32), local: (i32, i32)) -> (i32, i32) {
    (
        parent.0.saturating_add(local.0),
        parent.1.saturating_add(local.1),
    )
}

fn popup_base(offset: (i32, i32), geometry: (i32, i32)) -> (i32, i32) {
    (
        offset.0.saturating_sub(geometry.0),
        offset.1.saturating_sub(geometry.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_offsets_saturate_instead_of_wrapping() {
        assert_eq!(accumulate_offset((10, 20), (3, -4)), (13, 16));
        assert_eq!(
            accumulate_offset((i32::MAX, i32::MIN), (1, -1)),
            (i32::MAX, i32::MIN)
        );
    }

    #[test]
    fn popup_surface_origin_matches_window_rendering() {
        assert_eq!(popup_base((120, 80), (7, 9)), (113, 71));
    }
}

use smithay::{
    backend::renderer::utils::RendererSurfaceStateUserData,
    desktop::{PopupManager, find_popup_root_surface},
    reexports::wayland_server::{Resource, backend::ObjectId, protocol::wl_surface::WlSurface},
    utils::IsAlive,
    wayland::{
        compositor::{
            SUBSURFACE_ROLE, SubsurfaceCachedState, SurfaceData, TraversalAction, get_parent,
            with_surface_tree_upward,
        },
        seat::WaylandFocus,
    },
};
use tracing::warn;

use crate::scene::{SurfaceLayer, SurfaceTransform};

use super::{RuntimeState, surfaces::SurfaceCommit};

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

        let root_id = self.surface_buffers.view_root_for_surface(&surface.id())?;
        self.mapped_root_by_id(root_id)
    }

    /// Rebuild one view's flat surface table in exact draw order. Smithay
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

        let mut popups = PopupManager::popups_for_surface(root).collect::<Vec<_>>();
        popups.reverse();
        for (popup, offset) in popups {
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

fn collect_surface_tree(
    root: &WlSurface,
    base: (i32, i32),
    layer: SurfaceLayer,
    commits: &mut Vec<(ObjectId, SurfaceCommit)>,
) {
    with_surface_tree_upward(
        root,
        base,
        |surface, states, parent| {
            if !surface.alive() {
                return TraversalAction::SkipChildren;
            }
            TraversalAction::DoChildren(accumulate_offset(*parent, subsurface_offset(states)))
        },
        |surface, states, parent| {
            if !surface.alive() {
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
    let renderer = states.data_map.get::<RendererSurfaceStateUserData>()?;
    let renderer = renderer.lock().unwrap();
    let buffer = renderer.buffer().map(|buffer| buffer.id());
    let logical_size = renderer.surface_size().and_then(|size| {
        Some(tensor_util::Size::new(
            u32::try_from(size.w).ok()?,
            u32::try_from(size.h).ok()?,
        ))
    });
    Some(SurfaceCommit {
        buffer,
        logical_size,
        local_offset,
        commit: renderer.current_commit(),
        buffer_scale: u32::try_from(renderer.buffer_scale()).unwrap_or(1),
        transform: surface_transform(renderer.buffer_transform()),
        layer,
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

fn surface_transform(transform: smithay::utils::Transform) -> SurfaceTransform {
    match transform {
        smithay::utils::Transform::Normal => SurfaceTransform::Normal,
        smithay::utils::Transform::_90 => SurfaceTransform::Rotate90,
        smithay::utils::Transform::_180 => SurfaceTransform::Rotate180,
        smithay::utils::Transform::_270 => SurfaceTransform::Rotate270,
        smithay::utils::Transform::Flipped => SurfaceTransform::Flipped,
        smithay::utils::Transform::Flipped90 => SurfaceTransform::Flipped90,
        smithay::utils::Transform::Flipped180 => SurfaceTransform::Flipped180,
        smithay::utils::Transform::Flipped270 => SurfaceTransform::Flipped270,
    }
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
    fn popup_surface_origin_matches_smithay_window_rendering() {
        assert_eq!(popup_base((120, 80), (7, 9)), (113, 71));
    }
}

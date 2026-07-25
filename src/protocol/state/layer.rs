//! wlr-layer-shell commit, exclusive-zone layout, and value-only scene merge.
//!
//! Layer surfaces stay outside the ECS view graph: they are output-local Smithay
//! state. The frame path only receives the same value-only surface content that
//! tiled views use, so the renderer never sees a LayerMap or WlSurface.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use smithay::{
    backend::renderer::utils::with_renderer_surface_state,
    desktop::{LayerSurface, WindowSurfaceType, layer_map_for_output},
    reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface},
    utils::IsAlive,
    wayland::{
        compositor::{get_parent, send_surface_state, with_states},
        fractional_scale::with_fractional_scale,
        shell::wlr_layer::Layer as WlrLayer,
    },
};
use tensor_util::Rect;
use tracing::warn;

use crate::{
    ecs::ViewId,
    layout::LayoutPlacement,
    scene::{EffectStyle, SceneNode, SceneSnapshot, SurfaceLayer},
};

use super::{RuntimeState, tree::collect_surface_tree};

impl RuntimeState {
    /// Handle a commit for a layer-shell surface tree.
    ///
    /// Returns `true` when the surface belonged to a layer map so the ordinary
    /// xdg-toplevel path should not run.
    #[cfg(feature = "tty")]
    pub(crate) fn handle_layer_shell_commit(&mut self, surface: &WlSurface) -> bool {
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        let Some(output) = self.layer_output_for_surface(&root) else {
            return false;
        };

        {
            let mut map = layer_map_for_output(&output);
            map.arrange();
            let Some(layer) = map
                .layer_for_surface(&root, WindowSurfaceType::TOPLEVEL)
                .cloned()
            else {
                return true;
            };
            layer.layer_surface().send_pending_configure();
            with_states(layer.wl_surface(), |states| {
                let scale = output.current_scale();
                let transform = output.current_transform();
                send_surface_state(layer.wl_surface(), states, scale.integer_scale(), transform);
                with_fractional_scale(states, |fractional| {
                    fractional.set_preferred_scale(scale.fractional_scale());
                });
            });
        }

        if surface_has_buffer(&root) {
            let _ = self.update_layer_surface_content(&root);
        } else {
            self.clear_layer_surface_content(&root);
        }

        // Exclusive zones reshape the workspace; always reflow when a layer
        // commits so bars reserve space before the next frame.
        let _ = self.reflow_default_workspace_layout();
        self.request_redraw_all();
        true
    }

    /// Drop protocol-owned surface content when a layer is destroyed.
    #[cfg(feature = "tty")]
    pub(crate) fn forget_layer_surface(&mut self, surface: &WlSurface) {
        self.clear_layer_surface_content(surface);
    }

    #[cfg(feature = "tty")]
    fn layer_output_for_surface(&self, surface: &WlSurface) -> Option<smithay::output::Output> {
        self.space.outputs().find_map(|output| {
            let map = layer_map_for_output(output);
            map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                .is_some()
                .then(|| output.clone())
        })
    }

    #[cfg(feature = "tty")]
    fn update_layer_surface_content(&mut self, root: &WlSurface) -> bool {
        let mut commits = Vec::new();
        collect_surface_tree(root, (0, 0), SurfaceLayer::Popup, &mut commits);
        let Some(update) = self.surface_buffers.update_view_tree(&root.id(), commits) else {
            warn!("layer surface identity space is exhausted");
            return false;
        };
        for surface_id in update.removed_surfaces {
            if let Some(sync) = self.surface_sync.remove(surface_id) {
                self.finish_surface_sync(surface_id, sync.release);
            }
        }
        self.release_client_buffers(update.released_buffers);
        self.flush_client_releases();
        update.changed
    }

    #[cfg(feature = "tty")]
    fn clear_layer_surface_content(&mut self, root: &WlSurface) {
        let removal = self.surface_buffers.remove_view_tree(&root.id());
        for surface_id in removal.surfaces {
            if let Some(sync) = self.surface_sync.remove(surface_id) {
                self.finish_surface_sync(surface_id, sync.release);
            }
        }
        self.release_client_buffers(removal.released_buffers);
        self.flush_client_releases();
    }

    /// Merge output-local layer surfaces into a workspace (or blank) scene.
    ///
    /// Reads only previously registered buffer state so the submit path does
    /// not allocate or release client images mid-frame.
    #[cfg(feature = "tty")]
    pub(super) fn merge_layer_surfaces(
        &self,
        scene: SceneSnapshot,
        output: &smithay::output::Output,
        logical: Rect,
    ) -> SceneSnapshot {
        let map = layer_map_for_output(output);
        let mut nodes = scene.nodes().to_vec();
        let mut contents = scene.contents().to_vec();
        let mut layer_index = 0u64;
        for layer in map.layers() {
            if !layer.alive() || !surface_has_buffer(layer.wl_surface()) {
                continue;
            }
            let Some(geometry) = map.layer_geometry(layer) else {
                continue;
            };
            let layer_contents = self
                .surface_buffers
                .view_tree_contents(&layer.wl_surface().id());
            if layer_contents.is_empty() {
                continue;
            }
            let start = contents.len();
            contents.extend(layer_contents.iter().copied());
            let Some(span) = crate::scene::ContentSpan::new(start, layer_contents.len()) else {
                continue;
            };
            let global = Rect::new(
                logical.x.saturating_add(geometry.loc.x),
                logical.y.saturating_add(geometry.loc.y),
                u32::try_from(geometry.size.w).unwrap_or(0),
                u32::try_from(geometry.size.h).unwrap_or(0),
            );
            if global.width == 0 || global.height == 0 {
                continue;
            }
            let stacking = layer_stacking_order(layer.layer(), layer_index);
            layer_index = layer_index.saturating_add(1);
            let view_id = ViewId::new(layer_view_id(layer));
            nodes.push(
                SceneNode::new(
                    view_id,
                    stacking,
                    LayoutPlacement::new(global, logical),
                    EffectStyle::default(),
                )
                .with_content(span),
            );
        }
        drop(map);
        SceneSnapshot::with_content(scene.workspace_id, logical, nodes, contents)
    }

    /// Logical workspace rectangle after layer exclusive zones.
    #[cfg(feature = "tty")]
    pub(super) fn exclusive_workspace_area(
        &self,
        output: &smithay::output::Output,
        geometry: smithay::utils::Rectangle<i32, smithay::utils::Logical>,
    ) -> Option<Rect> {
        let map = layer_map_for_output(output);
        let zone = map.non_exclusive_zone();
        let width = u32::try_from(zone.size.w).ok()?;
        let height = u32::try_from(zone.size.h).ok()?;
        (width > 0 && height > 0).then(|| {
            Rect::new(
                geometry.loc.x.saturating_add(zone.loc.x),
                geometry.loc.y.saturating_add(zone.loc.y),
                width,
                height,
            )
        })
    }
}

#[cfg(feature = "tty")]
fn surface_has_buffer(surface: &WlSurface) -> bool {
    with_renderer_surface_state(surface, |state| state.buffer().is_some()).unwrap_or(false)
}

/// Synthetic view ids for layer surfaces. High-bit tags keep them out of the
/// ordinary ECS view-id space that grows from 1.
#[cfg(feature = "tty")]
fn layer_view_id(layer: &LayerSurface) -> u64 {
    let mut hasher = DefaultHasher::new();
    layer.wl_surface().id().hash(&mut hasher);
    0xC000_0000_0000_0000 | (hasher.finish() & 0x0FFF_FFFF_FFFF_FFFF)
}

#[cfg(feature = "tty")]
fn layer_stacking_order(layer: WlrLayer, index: u64) -> u64 {
    // ECS views use ordinary low stacking orders. Layers bookend them.
    let base = match layer {
        WlrLayer::Background => 0,
        WlrLayer::Bottom => 1_000,
        WlrLayer::Top => u64::MAX / 4,
        WlrLayer::Overlay => u64::MAX / 2,
    };
    base.saturating_add(index)
}

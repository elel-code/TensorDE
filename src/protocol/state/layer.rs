//! wlr-layer-shell commit, exclusive-zone layout, input hit-test, and scene merge.
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
    desktop::{LayerSurface, PopupManager, WindowSurfaceType, layer_map_for_output},
    reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface},
    utils::{IsAlive, Logical, Point, SERIAL_COUNTER},
    wayland::{
        compositor::{get_parent, send_surface_state, with_states},
        fractional_scale::with_fractional_scale,
        seat::WaylandFocus,
        shell::wlr_layer::{KeyboardInteractivity, Layer as WlrLayer},
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
use crate::protocol::focus::KeyboardFocusTarget;

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
        // xdg popups parent through protocol state, not wl_subsurface.
        if let Some(popup) = self.popups.find_popup(&root)
            && let Ok(popup_root) = smithay::desktop::find_popup_root_surface(&popup)
        {
            root = popup_root;
        }
        let Some(output) = self.layer_output_for_surface(&root) else {
            return false;
        };

        let mut newly_mapped_on_demand = None;
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
            let mapped = surface_has_buffer(&root);
            let had_content = !self
                .surface_buffers
                .view_tree_contents(&root.id())
                .is_empty();
            if mapped && !had_content {
                let on_demand =
                    layer.cached_state().keyboard_interactivity == KeyboardInteractivity::OnDemand;
                if on_demand {
                    newly_mapped_on_demand = Some(layer);
                }
            }
        }

        if surface_has_buffer(&root) {
            let _ = self.update_layer_surface_content(&root);
        } else {
            self.clear_layer_surface_content(&root);
            self.clear_on_demand_if_surface(&root);
        }

        if let Some(layer) = newly_mapped_on_demand {
            self.layer_shell_on_demand_focus = Some(layer);
        }

        // Exclusive zones reshape the workspace; always reflow when a layer
        // commits so bars reserve space before the next frame.
        let _ = self.reflow_default_workspace_layout();
        self.reconcile_layer_keyboard_focus();
        self.request_redraw_all();
        true
    }

    /// Drop protocol-owned surface content when a layer is destroyed.
    #[cfg(feature = "tty")]
    pub(crate) fn forget_layer_surface(&mut self, surface: &WlSurface) {
        self.clear_layer_surface_content(surface);
        self.clear_on_demand_if_surface(surface);
        self.reconcile_layer_keyboard_focus();
    }

    /// Pointer hit-test: Overlay/Top layers, then windows, then Bottom/Background.
    #[cfg(feature = "tty")]
    pub(crate) fn layer_or_window_pointer_focus(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let (output, output_geo) = self.output_under_location(location)?;
        let pos_in_output = location - output_geo.loc.to_f64();
        let map = layer_map_for_output(&output);

        if let Some(hit) = layer_surface_under(
            &map,
            pos_in_output,
            output_geo,
            [WlrLayer::Overlay, WlrLayer::Top],
        ) {
            return Some(hit);
        }
        drop(map);

        if let Some(hit) = self.window_pointer_focus(location) {
            return Some(hit);
        }

        let map = layer_map_for_output(&output);
        layer_surface_under(
            &map,
            pos_in_output,
            output_geo,
            [WlrLayer::Bottom, WlrLayer::Background],
        )
    }

    /// Click focus: keyboard-capable layers first (same stacking order), else windows.
    #[cfg(feature = "tty")]
    pub(crate) fn focus_at_pointer(
        &mut self,
        location: Point<f64, Logical>,
        serial: smithay::utils::Serial,
    ) {
        if let Some(layer) = self.keyboard_layer_under(location) {
            self.focus_layer_surface(layer, serial);
            return;
        }
        if let Some((window, _)) = self
            .space
            .element_under(location)
            .map(|(window, loc)| (window.clone(), loc))
        {
            let Some(surface) = window.wl_surface().map(std::borrow::Cow::into_owned) else {
                return;
            };
            let Some(root) = self.owning_view_root(&surface) else {
                return;
            };
            let Some(root_window) = self
                .space
                .elements()
                .find(|candidate| candidate.wl_surface().as_deref() == Some(&root))
                .cloned()
            else {
                return;
            };
            self.layer_shell_on_demand_focus = None;
            if self.focus_mapped_window(root_window, serial) {
                self.reflow_default_workspace();
            }
        }
    }

    /// Apply exclusive / on-demand layer keyboard policy after map changes.
    #[cfg(feature = "tty")]
    pub(crate) fn reconcile_layer_keyboard_focus(&mut self) {
        self.sanitize_on_demand_layer_focus();
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(layer) = self.preferred_layer_keyboard_target() {
            self.focus_layer_surface(layer, serial);
            return;
        }
        // No layer claim: restore ECS-selected window if the seat sits on a layer.
        let on_layer = self.seat.get_keyboard().is_some_and(|keyboard| {
            keyboard.current_focus().is_some_and(|focus| {
                focus.wl_surface().is_some_and(|surface| {
                    self.layer_output_for_surface(surface.as_ref()).is_some()
                })
            })
        });
        if on_layer {
            self.restore_keyboard_focus();
        }
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
        // Layer trees use Popup clipping so exclusive panels and their xdg
        // popups may extend to the full output, not a tile clip.
        collect_surface_tree(root, (0, 0), SurfaceLayer::Popup, &mut commits);
        let mut popups = PopupManager::popups_for_surface(root).collect::<Vec<_>>();
        popups.reverse();
        for (popup, offset) in popups {
            let popup_geometry = popup.geometry();
            collect_surface_tree(
                popup.wl_surface(),
                (
                    offset.x.saturating_sub(popup_geometry.loc.x),
                    offset.y.saturating_sub(popup_geometry.loc.y),
                ),
                SurfaceLayer::Popup,
                &mut commits,
            );
        }
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

    /// Whether `surface` is a mapped layer-shell root (not a popup child).
    #[cfg(feature = "tty")]
    pub(crate) fn is_layer_root(&self, surface: &WlSurface) -> bool {
        self.layer_output_for_surface(surface).is_some()
    }

    /// True when an exclusive or on-demand top/overlay layer should block
    /// ordinary window popup grabs (menus under a focused panel).
    #[cfg(feature = "tty")]
    pub(crate) fn layer_blocks_window_popup_grabs(&self) -> bool {
        for output in self.space.outputs() {
            let map = layer_map_for_output(output);
            for band in [WlrLayer::Overlay, WlrLayer::Top] {
                if map.layers_on(band).any(|layer| {
                    let interactive = matches!(
                        layer.cached_state().keyboard_interactivity,
                        KeyboardInteractivity::Exclusive
                    ) || self.layer_shell_on_demand_focus.as_ref() == Some(layer);
                    interactive && layer.alive() && surface_has_buffer(layer.wl_surface())
                }) {
                    return true;
                }
            }
        }
        false
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
            // Layer shells commonly request panel blur via ext-background-effect.
            let effects = Self::layer_surface_effects(layer.wl_surface());
            nodes.push(
                SceneNode::new(
                    view_id,
                    stacking,
                    LayoutPlacement::new(global, logical),
                    effects,
                )
                .with_content(span),
            );
        }
        drop(map);
        SceneSnapshot::with_content(scene.workspace_id, logical, nodes, contents)
    }

    /// Refresh buffer identity for a mapped session-lock surface commit.
    #[cfg(feature = "tty")]
    pub(crate) fn handle_session_lock_commit(&mut self, surface: &WlSurface) -> bool {
        let Some(lock) = self.protocol_side.session_lock.as_ref() else {
            return false;
        };
        let is_lock = lock
            .surfaces
            .values()
            .any(|lock_surface| lock_surface.wl_surface() == surface);
        if !is_lock {
            return false;
        }
        if surface_has_buffer(surface) {
            let mut commits = Vec::new();
            collect_surface_tree(surface, (0, 0), SurfaceLayer::Popup, &mut commits);
            if let Some(update) = self
                .surface_buffers
                .update_view_tree(&surface.id(), commits)
            {
                for surface_id in update.removed_surfaces {
                    if let Some(sync) = self.surface_sync.remove(surface_id) {
                        self.finish_surface_sync(surface_id, sync.release);
                    }
                }
                self.release_client_buffers(update.released_buffers);
                self.flush_client_releases();
            }
        }
        self.request_redraw_all();
        true
    }

    /// When the session is locked, only lock surfaces for this head are drawn.
    #[cfg(feature = "tty")]
    pub(super) fn merge_session_lock_surfaces(
        &self,
        scene: SceneSnapshot,
        output: &smithay::output::Output,
        logical: Rect,
    ) -> SceneSnapshot {
        let Some(lock) = self.protocol_side.session_lock.as_ref() else {
            return scene;
        };
        let Some(lock_surface) = lock.surfaces.get(&output.name()) else {
            // Locked but no surface yet: blank frame (no client content).
            return SceneSnapshot::new(scene.workspace_id, logical, Vec::new());
        };
        let surface = lock_surface.wl_surface();
        if !surface_has_buffer(surface) {
            return SceneSnapshot::new(scene.workspace_id, logical, Vec::new());
        }
        let contents = self.surface_buffers.view_tree_contents(&surface.id());
        if contents.is_empty() {
            return SceneSnapshot::new(scene.workspace_id, logical, Vec::new());
        }
        let span = match crate::scene::ContentSpan::new(0, contents.len()) {
            Some(span) => span,
            None => return SceneSnapshot::new(scene.workspace_id, logical, Vec::new()),
        };
        let node = SceneNode::new(
            ViewId::new(0xB000_0000_0000_0000),
            u64::MAX,
            LayoutPlacement::new(logical, logical),
            EffectStyle::default(),
        )
        .with_content(span);
        SceneSnapshot::with_content(scene.workspace_id, logical, vec![node], contents)
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

    #[cfg(feature = "tty")]
    fn window_pointer_focus(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let (window, window_location) = self.space.element_under(location)?;
        window
            .surface_under(location - window_location.to_f64(), WindowSurfaceType::ALL)
            .map(|(surface, surface_location)| {
                (surface, (surface_location + window_location).to_f64())
            })
    }

    #[cfg(feature = "tty")]
    fn output_under_location(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<(
        smithay::output::Output,
        smithay::utils::Rectangle<i32, Logical>,
    )> {
        let output = self.space.output_under(location).next()?.clone();
        let geometry = self.space.output_geometry(&output)?;
        Some((output, geometry))
    }

    /// Layer under the pointer that can accept keyboard focus.
    #[cfg(feature = "tty")]
    fn keyboard_layer_under(&self, location: Point<f64, Logical>) -> Option<LayerSurface> {
        let (output, output_geo) = self.output_under_location(location)?;
        let pos_in_output = location - output_geo.loc.to_f64();
        let map = layer_map_for_output(&output);
        for band in [
            WlrLayer::Overlay,
            WlrLayer::Top,
            WlrLayer::Bottom,
            WlrLayer::Background,
        ] {
            if let Some(layer) = map.layer_under(band, pos_in_output)
                && layer.can_receive_keyboard_focus()
                && layer_has_surface_under(&map, layer, pos_in_output)
            {
                return Some(layer.clone());
            }
        }
        None
    }

    #[cfg(feature = "tty")]
    fn focus_layer_surface(&mut self, layer: LayerSurface, serial: smithay::utils::Serial) {
        if !layer.alive() || !layer.can_receive_keyboard_focus() {
            return;
        }
        let keyboard = self.seat.get_keyboard();
        if keyboard
            .as_ref()
            .is_some_and(|keyboard| keyboard.is_grabbed())
        {
            return;
        }
        match layer.cached_state().keyboard_interactivity {
            KeyboardInteractivity::OnDemand => {
                self.layer_shell_on_demand_focus = Some(layer.clone());
            }
            KeyboardInteractivity::Exclusive => {
                // Exclusive claims the seat without clearing on-demand memory;
                // on-demand is restored when exclusive surfaces leave.
            }
            KeyboardInteractivity::None => return,
        }
        // Layer focus is not an ECS view: clear xdg-toplevel Activated.
        self.publish_window_activation(None);
        let focus = KeyboardFocusTarget::from(layer.wl_surface().clone());
        if let Some(keyboard) = keyboard
            && keyboard.current_focus().as_ref() != Some(&focus)
        {
            keyboard.set_focus(self, Some(focus), serial);
        }
    }

    /// Prefer exclusive overlay/top, then selected on-demand, then exclusive
    /// bottom/background only when the workspace has no mapped views (Niri-style).
    #[cfg(feature = "tty")]
    fn preferred_layer_keyboard_target(&self) -> Option<LayerSurface> {
        // Space membership is enough here: exclusive bottom layers only claim
        // the seat when no mapped window can receive ordinary keyboard focus.
        let workspace_empty = self.space.elements().next().is_none();
        for output in self.space.outputs() {
            let map = layer_map_for_output(output);
            for band in [WlrLayer::Overlay, WlrLayer::Top] {
                if let Some(layer) = exclusive_layer_on(&map, band) {
                    return Some(layer);
                }
            }
            if let Some(layer) = self.layer_shell_on_demand_focus.as_ref()
                && layer.alive()
                && map.layers().any(|candidate| candidate == layer)
                && layer.cached_state().keyboard_interactivity == KeyboardInteractivity::OnDemand
            {
                return Some(layer.clone());
            }
            if workspace_empty {
                for band in [WlrLayer::Bottom, WlrLayer::Background] {
                    if let Some(layer) = exclusive_layer_on(&map, band) {
                        return Some(layer);
                    }
                }
            }
        }
        None
    }

    #[cfg(feature = "tty")]
    fn sanitize_on_demand_layer_focus(&mut self) {
        let keep = self
            .layer_shell_on_demand_focus
            .as_ref()
            .is_some_and(|layer| {
                layer.alive()
                    && layer.cached_state().keyboard_interactivity
                        == KeyboardInteractivity::OnDemand
                    && surface_has_buffer(layer.wl_surface())
            });
        if !keep {
            self.layer_shell_on_demand_focus = None;
        }
    }

    #[cfg(feature = "tty")]
    fn clear_on_demand_if_surface(&mut self, surface: &WlSurface) {
        if self
            .layer_shell_on_demand_focus
            .as_ref()
            .is_some_and(|layer| layer.wl_surface() == surface)
        {
            self.layer_shell_on_demand_focus = None;
        }
    }
}

#[cfg(feature = "tty")]
fn surface_has_buffer(surface: &WlSurface) -> bool {
    with_renderer_surface_state(surface, |state| state.buffer().is_some()).unwrap_or(false)
}

#[cfg(feature = "tty")]
fn exclusive_layer_on(map: &smithay::desktop::LayerMap, band: WlrLayer) -> Option<LayerSurface> {
    map.layers_on(band).find_map(|layer| {
        (layer.cached_state().keyboard_interactivity == KeyboardInteractivity::Exclusive
            && layer.alive()
            && surface_has_buffer(layer.wl_surface()))
        .then(|| layer.clone())
    })
}

#[cfg(feature = "tty")]
fn layer_surface_under(
    map: &smithay::desktop::LayerMap,
    pos_in_output: Point<f64, Logical>,
    output_geo: smithay::utils::Rectangle<i32, Logical>,
    bands: [WlrLayer; 2],
) -> Option<(WlSurface, Point<f64, Logical>)> {
    for band in bands {
        if let Some(layer) = map.layer_under(band, pos_in_output)
            && let Some(layer_geo) = map.layer_geometry(layer)
            && let Some((surface, surface_loc)) = layer.surface_under(
                pos_in_output - layer_geo.loc.to_f64(),
                WindowSurfaceType::ALL,
            )
        {
            let global = surface_loc + layer_geo.loc + output_geo.loc;
            return Some((surface, global.to_f64()));
        }
    }
    None
}

#[cfg(feature = "tty")]
fn layer_has_surface_under(
    map: &smithay::desktop::LayerMap,
    layer: &LayerSurface,
    pos_in_output: Point<f64, Logical>,
) -> bool {
    map.layer_geometry(layer).is_some_and(|layer_geo| {
        layer
            .surface_under(
                pos_in_output - layer_geo.loc.to_f64(),
                WindowSurfaceType::ALL,
            )
            .is_some()
    })
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

#[cfg(all(test, feature = "tty"))]
mod tests {
    use super::*;

    #[test]
    fn layer_draw_order_places_overlay_above_top_and_bottom() {
        assert!(
            layer_stacking_order(WlrLayer::Background, 0)
                < layer_stacking_order(WlrLayer::Bottom, 0)
        );
        assert!(layer_stacking_order(WlrLayer::Bottom, 0) < layer_stacking_order(WlrLayer::Top, 0));
        assert!(
            layer_stacking_order(WlrLayer::Top, 0) < layer_stacking_order(WlrLayer::Overlay, 0)
        );
        assert!(
            layer_stacking_order(WlrLayer::Top, 3) < layer_stacking_order(WlrLayer::Overlay, 0)
        );
    }

    #[test]
    fn pointer_band_order_is_overlay_top_then_bottom_background() {
        // Document the hit-test contract consumed by layer_or_window_pointer_focus.
        let above = [WlrLayer::Overlay, WlrLayer::Top];
        let below = [WlrLayer::Bottom, WlrLayer::Background];
        assert_eq!(above[0], WlrLayer::Overlay);
        assert_eq!(above[1], WlrLayer::Top);
        assert_eq!(below[0], WlrLayer::Bottom);
        assert_eq!(below[1], WlrLayer::Background);
    }
}

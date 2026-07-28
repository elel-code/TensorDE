//! wlr-layer-shell commit, exclusive-zone layout, input hit-test, and scene merge.
//!
//! Layer surfaces stay outside the ECS view graph in output-local Tensor maps.
//! The frame path only receives the same value-only surface content that tiled
//! views use, so the renderer never sees a LayerMap or WlSurface.

#[cfg(feature = "tty")]
mod dnd;
mod lifecycle;
mod map;
mod surface;

#[cfg(feature = "tty")]
use map::LayerMap;
pub(super) use map::LayerMaps;
pub(in crate::protocol) use surface::{
    Anchor, ExclusiveZone, KeyboardInteractivity, Layer as WlrLayer, LayerSurface,
    LayerSurfaceState, Margins,
};

#[cfg(feature = "tty")]
use crate::protocol::globals::compositor::{get_parent, send_surface_state, with_states};
#[cfg(feature = "tty")]
use tensor_util::{LogicalPoint, LogicalRect, Rect};
#[cfg(feature = "tty")]
use tracing::warn;
#[cfg(feature = "tty")]
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

#[cfg(feature = "tty")]
use crate::protocol::globals::output::Output;
#[cfg(feature = "tty")]
use crate::{
    ecs::ViewId,
    layout::LayoutPlacement,
    scene::{EffectStyle, SceneNode, SceneSnapshot, SurfaceLayer},
};

use super::RuntimeState;
#[cfg(feature = "tty")]
use super::{find_popup_root_surface, surfaces::surface_has_buffer, tree::collect_surface_tree};
#[cfg(feature = "tty")]
use crate::protocol::serial::{Serial, next_serial};

#[cfg(feature = "tty")]
pub(super) struct LayerPopupContext {
    pub(super) output: Output,
    pub(super) geometry: LogicalRect<i32>,
    pub(super) layer: WlrLayer,
    pub(super) non_exclusive_zone: LogicalRect<i32>,
}

impl RuntimeState {
    #[cfg(feature = "tty")]
    pub(super) fn arrange_layer_output(&mut self, output: &Output) {
        if let Some(map) = self.layer_maps.for_output_mut(output) {
            map.arrange(&self.popups);
        }
    }

    #[cfg(feature = "tty")]
    pub(crate) fn remove_layer_output(&mut self, output: &Output) {
        let removed = self.layer_maps.remove_output(output, &self.popups);
        for surface in removed {
            surface.close();
            self.clear_layer_surface_content(surface.wl_surface());
            self.clear_on_demand_if_surface(surface.wl_surface());
        }
        self.reconcile_layer_keyboard_focus();
    }

    #[cfg(feature = "tty")]
    pub(crate) fn clear_layer_on_demand_focus(&mut self) {
        self.layer_shell_on_demand_focus = None;
    }

    #[cfg(all(test, feature = "tty"))]
    pub(crate) fn layer_test_snapshot(&self, output: &Output) -> Option<(usize, LogicalRect<i32>)> {
        let output_geometry = self.space.output_geometry(output)?;
        Some(
            self.layer_maps
                .for_output(output)
                .map(|map| (map.layers().count(), map.non_exclusive_zone()))
                .unwrap_or_else(|| (0, LogicalRect::from_size(output_geometry.size))),
        )
    }

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
            && let Ok(popup_root) = find_popup_root_surface(&popup)
        {
            root = popup_root;
        }
        let mut newly_mapped_on_demand = None;
        {
            let Some((layer, output)) = self.layer_maps.arrange_for_root(&root, &self.popups)
            else {
                return false;
            };
            layer.send_pending_configure();
            let snapshot = output.snapshot();
            let scale = snapshot.scale;
            let transform = snapshot.transform;
            with_states(layer.wl_surface(), |states| {
                send_surface_state(
                    layer.wl_surface(),
                    states,
                    super::output_integer_scale(scale),
                    super::wayland_transform(transform),
                );
            });
            self.protocol_globals
                .set_preferred_fractional_scale(layer.wl_surface(), scale);
            let mapped = layer.mapped() && surface_has_buffer(&root);
            let had_content = !self
                .surface_buffers
                .view_tree_contents(&root.id())
                .is_empty();
            if mapped && !had_content {
                let on_demand =
                    layer.current().keyboard_interactivity == KeyboardInteractivity::OnDemand;
                if on_demand {
                    newly_mapped_on_demand = Some(layer.clone());
                }
            }
        }

        if self
            .layer_maps
            .layer_and_output_for_root(&root)
            .is_some_and(|(layer, _)| layer.mapped())
            && surface_has_buffer(&root)
        {
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
        location: LogicalPoint<f64>,
    ) -> Option<(WlSurface, LogicalPoint<f64>)> {
        let (output, output_geo) = self.output_under_location(location)?;
        let pos_in_output = location - output_geo.loc.to_f64();
        let map = self.layer_maps.for_output(&output);

        if let Some(map) = map
            && let Some(hit) = layer_surface_under(
                map,
                &self.popups,
                pos_in_output,
                output_geo,
                [WlrLayer::Overlay, WlrLayer::Top],
            )
        {
            return Some(hit);
        }
        if let Some(hit) = self.window_pointer_focus(location) {
            return Some(hit);
        }

        layer_surface_under(
            map?,
            &self.popups,
            pos_in_output,
            output_geo,
            [WlrLayer::Bottom, WlrLayer::Background],
        )
    }

    /// Click focus: keyboard-capable layers first (same stacking order), else windows.
    #[cfg(feature = "tty")]
    pub(crate) fn focus_at_pointer(&mut self, location: LogicalPoint<f64>, serial: Serial) {
        if let Some(layer) = self.keyboard_layer_under(location) {
            self.focus_layer_surface(layer, serial);
            return;
        }
        let mut dnd_active = None;
        let window = self
            .space
            .element_under(&self.popups, location, || {
                *dnd_active.get_or_insert_with(|| self.xwayland_dnd_pointer_grab_active())
            })
            .map(|hit| hit.window.clone());
        if let Some(window) = window {
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
        let serial = next_serial();
        if let Some(layer) = self.preferred_layer_keyboard_target() {
            self.focus_layer_surface(layer, serial);
            return;
        }
        // No layer claim: restore ECS-selected window if the seat sits on a layer.
        let on_layer = self
            .input_seat
            .keyboard_focus()
            .is_some_and(|surface| self.layer_output_for_surface(surface).is_some());
        if on_layer {
            self.restore_keyboard_focus();
        }
    }

    #[cfg(feature = "tty")]
    fn layer_output_for_surface(&self, surface: &WlSurface) -> Option<&Output> {
        self.layer_maps
            .layer_and_output_for_root(surface)
            .map(|(_, output)| output)
    }

    #[cfg(feature = "tty")]
    pub(super) fn layer_popup_context(&self, root: &WlSurface) -> Option<LayerPopupContext> {
        let (layer, output) = self.layer_maps.layer_and_output_for_root(root)?;
        let map = self.layer_maps.for_output(output)?;
        Some(LayerPopupContext {
            output: output.clone(),
            geometry: map.layer_geometry(layer)?,
            layer: layer.layer(),
            non_exclusive_zone: map.non_exclusive_zone(),
        })
    }

    #[cfg(feature = "tty")]
    pub(crate) fn layer_surface_origin(&self, surface: &WlSurface) -> Option<LogicalPoint<i32>> {
        let (layer, output) = self.layer_maps.layer_and_output_for_root(surface)?;
        let local = self
            .layer_maps
            .for_output(output)?
            .layer_geometry(layer)?
            .loc;
        Some(self.space.output_geometry(output)?.loc + local)
    }

    #[cfg(feature = "tty")]
    fn update_layer_surface_content(&mut self, root: &WlSurface) -> bool {
        let mut commits = Vec::new();
        // Layer trees use Popup clipping so exclusive panels and their xdg
        // popups may extend to the full output, not a tile clip.
        collect_surface_tree(root, (0, 0), SurfaceLayer::Popup, &mut commits);
        for (popup, offset) in self.popups.popups_for_surface(root).rev() {
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
            let Some(map) = self.layer_maps.for_output(output) else {
                continue;
            };
            for band in [WlrLayer::Overlay, WlrLayer::Top] {
                if map.layers_on(band).any(|layer| {
                    let interactive = matches!(
                        layer.current().keyboard_interactivity,
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
        output: &Output,
        logical: Rect,
    ) -> SceneSnapshot {
        let Some(map) = self.layer_maps.for_output(output) else {
            return scene;
        };
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
            let view_id = ViewId::new(layer.view_id());
            // Layer shells commonly request panel blur via ext-background-effect.
            let effects = self.layer_surface_effects(layer.wl_surface());
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
        SceneSnapshot::with_content(scene.workspace_id, logical, nodes, contents)
    }

    /// Refresh buffer identity for a mapped session-lock surface commit.
    #[cfg(feature = "tty")]
    pub(crate) fn handle_session_lock_commit(&mut self, surface: &WlSurface) -> bool {
        if !self.protocol_globals.session_lock.is_locked() {
            return false;
        }
        if self
            .protocol_globals
            .session_lock
            .contains_active_surface(surface)
        {
            self.refresh_session_lock_surface_tree(surface);
            return true;
        }
        let Some(mut root) = get_parent(surface) else {
            return false;
        };
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        if !self
            .protocol_globals
            .session_lock
            .contains_active_surface(&root)
        {
            return false;
        }
        self.refresh_session_lock_surface_tree(&root);
        true
    }

    #[cfg(feature = "tty")]
    fn refresh_session_lock_surface_tree(&mut self, root: &WlSurface) {
        if surface_has_buffer(root) {
            let mut commits = Vec::new();
            collect_surface_tree(root, (0, 0), SurfaceLayer::Popup, &mut commits);
            if let Some(update) = self.surface_buffers.update_view_tree(&root.id(), commits) {
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
    }

    /// When the session is locked, only lock surfaces for this head are drawn.
    #[cfg(feature = "tty")]
    pub(super) fn merge_session_lock_surfaces(
        &self,
        scene: SceneSnapshot,
        output: &Output,
        logical: Rect,
    ) -> SceneSnapshot {
        if !self.protocol_globals.session_lock.is_locked() {
            return scene;
        }
        let Some(surface) = self
            .protocol_globals
            .session_lock
            .surface_for_output(output.id())
        else {
            // Locked but no surface yet: blank frame (no client content).
            return SceneSnapshot::new(scene.workspace_id, logical, Vec::new());
        };
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
        output: &Output,
        geometry: LogicalRect<i32>,
    ) -> Option<Rect> {
        let zone = self
            .layer_maps
            .for_output(output)
            .map(LayerMap::non_exclusive_zone)
            .unwrap_or_else(|| LogicalRect::from_size(geometry.size));
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
        location: LogicalPoint<f64>,
    ) -> Option<(WlSurface, LogicalPoint<f64>)> {
        let mut dnd_active = None;
        let hit = self.space.element_under(&self.popups, location, || {
            *dnd_active.get_or_insert_with(|| self.xwayland_dnd_pointer_grab_active())
        })?;
        Some((
            hit.surface,
            (hit.surface_location + hit.window_location).to_f64(),
        ))
    }

    #[cfg(feature = "tty")]
    fn output_under_location(
        &self,
        location: LogicalPoint<f64>,
    ) -> Option<(Output, LogicalRect<i32>)> {
        let output = self.space.output_under(location).next()?.clone();
        let geometry = self.space.output_geometry(&output)?;
        Some((output, geometry))
    }

    /// Layer under the pointer that can accept keyboard focus.
    #[cfg(feature = "tty")]
    fn keyboard_layer_under(&self, location: LogicalPoint<f64>) -> Option<LayerSurface> {
        let (output, output_geo) = self.output_under_location(location)?;
        let pos_in_output = location - output_geo.loc.to_f64();
        let map = self.layer_maps.for_output(&output)?;
        for band in [
            WlrLayer::Overlay,
            WlrLayer::Top,
            WlrLayer::Bottom,
            WlrLayer::Background,
        ] {
            if let Some(layer) = map.layer_under(&self.popups, band, pos_in_output)
                && layer.can_receive_keyboard_focus()
                && layer_has_surface_under(map, &self.popups, layer, pos_in_output)
            {
                return Some(layer.clone());
            }
        }
        None
    }

    #[cfg(feature = "tty")]
    fn focus_layer_surface(&mut self, layer: LayerSurface, serial: Serial) {
        if self.popup_grab.is_some() || !layer.alive() || !layer.can_receive_keyboard_focus() {
            return;
        }
        match layer.current().keyboard_interactivity {
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
        if self.input_seat.keyboard_focus() != Some(layer.wl_surface()) {
            self.set_keyboard_focus(Some(layer.wl_surface().clone()), serial);
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
            let Some(map) = self.layer_maps.for_output(output) else {
                continue;
            };
            for band in [WlrLayer::Overlay, WlrLayer::Top] {
                if let Some(layer) = exclusive_layer_on(map, band) {
                    return Some(layer);
                }
            }
            if let Some(layer) = self.layer_shell_on_demand_focus.as_ref()
                && layer.alive()
                && map.layers().any(|candidate| candidate == layer)
                && layer.current().keyboard_interactivity == KeyboardInteractivity::OnDemand
            {
                return Some(layer.clone());
            }
            if workspace_empty {
                for band in [WlrLayer::Bottom, WlrLayer::Background] {
                    if let Some(layer) = exclusive_layer_on(map, band) {
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
                    && layer.current().keyboard_interactivity == KeyboardInteractivity::OnDemand
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
fn exclusive_layer_on(map: &LayerMap, band: WlrLayer) -> Option<LayerSurface> {
    map.layers_on(band).find_map(|layer| {
        (layer.current().keyboard_interactivity == KeyboardInteractivity::Exclusive
            && layer.alive()
            && surface_has_buffer(layer.wl_surface()))
        .then(|| layer.clone())
    })
}

#[cfg(feature = "tty")]
fn layer_surface_under(
    map: &LayerMap,
    popups: &super::PopupManager,
    pos_in_output: LogicalPoint<f64>,
    output_geo: LogicalRect<i32>,
    bands: [WlrLayer; 2],
) -> Option<(WlSurface, LogicalPoint<f64>)> {
    for band in bands {
        if let Some(layer) = map.layer_under(popups, band, pos_in_output)
            && let Some(layer_geo) = map.layer_geometry(layer)
            && let Some((surface, surface_loc)) =
                layer.surface_under(popups, pos_in_output - layer_geo.loc.to_f64())
        {
            let global = surface_loc + layer_geo.loc + output_geo.loc;
            return Some((surface, global.to_f64()));
        }
    }
    None
}

#[cfg(feature = "tty")]
fn layer_has_surface_under(
    map: &LayerMap,
    popups: &super::PopupManager,
    layer: &LayerSurface,
    pos_in_output: LogicalPoint<f64>,
) -> bool {
    map.layer_geometry(layer).is_some_and(|layer_geo| {
        layer
            .surface_under(popups, pos_in_output - layer_geo.loc.to_f64())
            .is_some()
    })
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

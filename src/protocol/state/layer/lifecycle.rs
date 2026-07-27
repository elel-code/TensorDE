//! Layer-surface creation, destruction, and xdg-popup parenting.

use smithay::wayland::{compositor::with_states, shell::xdg::XdgPopupSurfaceData};
use tracing::warn;
use wayland_protocols::xdg::shell::server::xdg_popup::XdgPopup;
use wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_surface_v1::{
    self, ZwlrLayerSurfaceV1,
};
use wayland_server::{
    Resource,
    protocol::{wl_output::WlOutput, wl_surface::WlSurface},
};

use super::{LayerSurface, WlrLayer};
use crate::protocol::{globals::output::Output, state::RuntimeState};

impl RuntimeState {
    #[cfg(all(test, feature = "tty"))]
    pub(crate) fn layer_test_popup_count(&self, output: &Output) -> usize {
        self.layer_maps
            .for_output(output)
            .map(|map| {
                map.layers()
                    .map(|layer| self.popups.popups_for_surface(layer.wl_surface()).len())
                    .sum()
            })
            .unwrap_or(0)
    }

    pub(in crate::protocol) fn register_layer_surface(
        &mut self,
        wl_surface: WlSurface,
        protocol: ZwlrLayerSurfaceV1,
        output: Option<WlOutput>,
        layer: WlrLayer,
        namespace: String,
    ) {
        let view_id = self.allocate_view_id().get();
        let surface = LayerSurface::new(wl_surface, protocol, layer, namespace.clone(), view_id);
        self.protocol_globals.layer_shell.insert(surface.clone());
        let output = match output {
            Some(output) => Output::from_resource(&output),
            None => self.space.outputs().next().cloned(),
        };
        let Some(output) = output else {
            warn!(%namespace, "layer surface created without a live output");
            surface.close();
            return;
        };
        self.map_layer_surface(&output, surface);
        #[cfg(feature = "tty")]
        self.request_redraw_all();
    }

    fn map_layer_surface(&mut self, output: &Output, surface: LayerSurface) {
        let (layer_maps, popups) = (&mut self.layer_maps, &self.popups);
        if let Err(error) = layer_maps.map(output, surface, popups) {
            warn!(%error, "failed to map layer surface");
        }
    }

    fn unmap_layer_surface(&mut self, surface: &LayerSurface) -> bool {
        self.layer_maps.unmap(surface, &self.popups)
    }

    pub(in crate::protocol) fn layer_surface_resource_destroyed(
        &mut self,
        resource: &ZwlrLayerSurfaceV1,
    ) {
        let Some(surface) = self.protocol_globals.layer_shell.remove_resource(resource) else {
            return;
        };
        self.remove_layer_surface_handle(surface);
    }

    pub(crate) fn layer_surface_wl_destroyed(&mut self, wl_surface: &WlSurface) {
        let Some(surface) = self.protocol_globals.layer_shell.remove_surface(wl_surface) else {
            return;
        };
        surface.close();
        self.remove_layer_surface_handle(surface);
    }

    fn remove_layer_surface_handle(&mut self, surface: LayerSurface) {
        #[cfg(feature = "tty")]
        self.forget_layer_surface(surface.wl_surface());
        self.unmap_layer_surface(&surface);
        #[cfg(feature = "tty")]
        {
            let _ = self.reflow_default_workspace_layout();
            self.request_redraw_all();
        }
    }

    pub(in crate::protocol) fn attach_layer_popup(
        &mut self,
        parent: &LayerSurface,
        popup: &XdgPopup,
        layer_resource: &ZwlrLayerSurfaceV1,
    ) {
        let Some(popup) = self.xdg_shell_state.get_popup(popup) else {
            layer_resource.post_error(
                zwlr_layer_surface_v1::Error::InvalidSurfaceState,
                "xdg_popup is not owned by the active xdg shell",
            );
            return;
        };
        if popup.is_initial_configure_sent() || popup.get_parent_surface().is_some() {
            layer_resource.post_error(
                zwlr_layer_surface_v1::Error::InvalidSurfaceState,
                "xdg_popup already has a parent or committed initial state",
            );
            return;
        }
        let parent_set = with_states(popup.wl_surface(), |states| {
            let Some(attributes) = states.data_map.get::<XdgPopupSurfaceData>() else {
                return false;
            };
            let mut attributes = attributes.lock().unwrap();
            if attributes.parent.is_some() {
                return false;
            }
            attributes.parent = Some(parent.wl_surface().clone());
            true
        });
        if !parent_set {
            layer_resource.post_error(
                zwlr_layer_surface_v1::Error::InvalidSurfaceState,
                "xdg_popup parent state is unavailable",
            );
            return;
        }
        let popup = super::super::PopupKind::Xdg(popup);
        self.unconstrain_popup(&popup);
        self.popups.commit(popup.wl_surface());
    }
}

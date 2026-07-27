//! Tensor-owned `wlr-layer-shell-unstable-v1` wire state.

use std::collections::HashMap;

use smithay::wayland::compositor::{self, BufferAssignment, SurfaceAttributes, with_states};
use wayland_protocols_wlr::layer_shell::v1::server::{
    zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, WEnum,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::{
        RuntimeState,
        layer::{Anchor, ExclusiveZone, KeyboardInteractivity, LayerSurface, Margins, WlrLayer},
        surface_has_buffer,
    },
};

const LAYER_SHELL_VERSION: u32 = 5;
const LAYER_SURFACE_ROLE: &str = "zwlr_layer_surface_v1";

pub(crate) struct LayerShellProtocol {
    _global: GlobalId,
    surfaces: HashMap<ObjectId, LayerSurface>,
    surface_index: HashMap<ObjectId, ObjectId>,
}

impl LayerShellProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, ZwlrLayerShellV1, _>(
                LAYER_SHELL_VERSION,
                LayerShellGlobalData,
            ),
            surfaces: HashMap::new(),
            surface_index: HashMap::new(),
        }
    }

    pub(in crate::protocol) fn insert(&mut self, surface: LayerSurface) {
        let protocol_id = surface.protocol_id();
        let wl_surface_id = surface.wl_surface().id();
        assert!(
            self.surface_index
                .insert(wl_surface_id, protocol_id.clone())
                .is_none(),
            "a wl_surface cannot own two layer roles"
        );
        assert!(
            self.surfaces.insert(protocol_id, surface).is_none(),
            "a layer resource cannot be registered twice"
        );
    }

    pub(in crate::protocol) fn get(&self, resource: &ZwlrLayerSurfaceV1) -> Option<LayerSurface> {
        self.surfaces.get(&resource.id()).cloned()
    }

    pub(in crate::protocol) fn for_surface(&self, surface: &WlSurface) -> Option<&LayerSurface> {
        let protocol = self.surface_index.get(&surface.id())?;
        self.surfaces.get(protocol)
    }

    pub(in crate::protocol) fn remove_resource(
        &mut self,
        resource: &ZwlrLayerSurfaceV1,
    ) -> Option<LayerSurface> {
        let surface = self.surfaces.remove(&resource.id())?;
        self.surface_index.remove(&surface.wl_surface().id());
        Some(surface)
    }

    pub(in crate::protocol) fn remove_surface(
        &mut self,
        wl_surface: &WlSurface,
    ) -> Option<LayerSurface> {
        let protocol = self.surface_index.remove(&wl_surface.id())?;
        self.surfaces.remove(&protocol)
    }

    #[cfg(test)]
    pub(crate) fn surface_count(&self) -> usize {
        self.surfaces.len()
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct LayerShellGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct LayerShellData;

#[derive(Debug)]
pub(in crate::protocol) struct LayerSurfaceData;

impl GlobalDispatchDelegate<ZwlrLayerShellV1, RuntimeState> for LayerShellGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrLayerShellV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, LayerShellData);
    }
}

impl DispatchDelegate<ZwlrLayerShellV1, RuntimeState> for LayerShellData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        shell: &ZwlrLayerShellV1,
        request: zwlr_layer_shell_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwlr_layer_shell_v1::Request::GetLayerSurface {
                id,
                surface,
                output,
                layer,
                namespace,
            } => {
                let Some(layer) = decode_layer(layer) else {
                    shell.post_error(
                        zwlr_layer_shell_v1::Error::InvalidLayer,
                        "unknown layer value",
                    );
                    return;
                };
                if surface_has_buffer_or_pending(&surface) {
                    shell.post_error(
                        zwlr_layer_shell_v1::Error::AlreadyConstructed,
                        "wl_surface already has a buffer attached or committed",
                    );
                    return;
                }
                if compositor::give_role(&surface, LAYER_SURFACE_ROLE).is_err() {
                    shell.post_error(
                        zwlr_layer_shell_v1::Error::Role,
                        "wl_surface already has a role",
                    );
                    return;
                }

                let resource = data_init.init(id, LayerSurfaceData);
                compositor::add_pre_commit_hook::<RuntimeState, _>(
                    &surface,
                    layer_surface_pre_commit,
                );
                state.register_layer_surface(surface, resource, output, layer, namespace);
            }
            zwlr_layer_shell_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwlrLayerSurfaceV1, RuntimeState> for LayerSurfaceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &ZwlrLayerSurfaceV1,
        request: zwlr_layer_surface_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let Some(surface) = state.protocol_globals.layer_shell.get(resource) else {
            return;
        };
        if !surface.alive() {
            return;
        }
        match request {
            zwlr_layer_surface_v1::Request::SetSize { width, height } => {
                let (Ok(width), Ok(height)) = (i32::try_from(width), i32::try_from(height)) else {
                    resource.post_error(
                        zwlr_layer_surface_v1::Error::InvalidSize,
                        "layer size exceeds the compositor coordinate range",
                    );
                    return;
                };
                surface.update_pending(|state| {
                    state.size = (width, height).into();
                });
            }
            zwlr_layer_surface_v1::Request::SetAnchor { anchor } => {
                let Some(anchor) = decode_anchor(anchor) else {
                    resource.post_error(
                        zwlr_layer_surface_v1::Error::InvalidAnchor,
                        "anchor contains unknown bits",
                    );
                    return;
                };
                surface.update_pending(|state| state.anchor = anchor);
            }
            zwlr_layer_surface_v1::Request::SetExclusiveZone { zone } => {
                let zone = match zone {
                    -1 => ExclusiveZone::DontCare,
                    0 => ExclusiveZone::Neutral,
                    1.. => ExclusiveZone::Exclusive(zone as u32),
                    _ => {
                        resource.post_error(
                            zwlr_layer_surface_v1::Error::InvalidSurfaceState,
                            "exclusive zone must be -1, 0, or positive",
                        );
                        return;
                    }
                };
                surface.update_pending(|state| state.exclusive_zone = zone);
            }
            zwlr_layer_surface_v1::Request::SetMargin {
                top,
                right,
                bottom,
                left,
            } => surface.update_pending(|state| {
                state.margin = Margins {
                    top,
                    right,
                    bottom,
                    left,
                };
            }),
            zwlr_layer_surface_v1::Request::SetKeyboardInteractivity {
                keyboard_interactivity,
            } => {
                let Some(interactivity) = decode_keyboard_interactivity(keyboard_interactivity)
                else {
                    resource.post_error(
                        zwlr_layer_surface_v1::Error::InvalidKeyboardInteractivity,
                        "unknown keyboard interactivity value",
                    );
                    return;
                };
                if interactivity == KeyboardInteractivity::OnDemand && resource.version() < 4 {
                    resource.post_error(
                        zwlr_layer_surface_v1::Error::InvalidKeyboardInteractivity,
                        "on_demand requires layer-shell version 4",
                    );
                    return;
                }
                surface.update_pending(|state| state.keyboard_interactivity = interactivity);
            }
            zwlr_layer_surface_v1::Request::GetPopup { popup } => {
                state.attach_layer_popup(&surface, &popup, resource);
            }
            zwlr_layer_surface_v1::Request::AckConfigure { serial } => {
                if !surface.ack_configure(serial) {
                    resource.post_error(
                        zwlr_layer_surface_v1::Error::InvalidSurfaceState,
                        format!("unknown or consumed configure serial {serial}"),
                    );
                }
            }
            zwlr_layer_surface_v1::Request::Destroy => {}
            zwlr_layer_surface_v1::Request::SetLayer { layer } => {
                let Some(layer) = decode_layer(layer) else {
                    resource.post_error(
                        zwlr_layer_surface_v1::Error::InvalidSurfaceState,
                        "unknown layer value",
                    );
                    return;
                };
                surface.update_pending(|state| state.layer = layer);
            }
            zwlr_layer_surface_v1::Request::SetExclusiveEdge { edge } => {
                let Some(edge) = decode_anchor(edge) else {
                    resource.post_error(
                        zwlr_layer_surface_v1::Error::InvalidExclusiveEdge,
                        "exclusive edge contains unknown bits",
                    );
                    return;
                };
                let edge = match edge.bits().count_ones() {
                    0 => None,
                    1 => Some(edge),
                    _ => {
                        resource.post_error(
                            zwlr_layer_surface_v1::Error::InvalidExclusiveEdge,
                            "exclusive edge must contain at most one edge",
                        );
                        return;
                    }
                };
                surface.update_pending(|state| state.exclusive_edge = edge);
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &ZwlrLayerSurfaceV1,
    ) {
        state.layer_surface_resource_destroyed(resource);
    }
}

fn decode_layer(layer: WEnum<zwlr_layer_shell_v1::Layer>) -> Option<WlrLayer> {
    match layer {
        WEnum::Value(zwlr_layer_shell_v1::Layer::Background) => Some(WlrLayer::Background),
        WEnum::Value(zwlr_layer_shell_v1::Layer::Bottom) => Some(WlrLayer::Bottom),
        WEnum::Value(zwlr_layer_shell_v1::Layer::Top) => Some(WlrLayer::Top),
        WEnum::Value(zwlr_layer_shell_v1::Layer::Overlay) => Some(WlrLayer::Overlay),
        WEnum::Unknown(_) => None,
        _ => None,
    }
}

fn decode_anchor(anchor: WEnum<zwlr_layer_surface_v1::Anchor>) -> Option<Anchor> {
    match anchor {
        WEnum::Value(anchor) => Anchor::from_bits(anchor.bits()),
        WEnum::Unknown(_) => None,
    }
}

fn decode_keyboard_interactivity(
    interactivity: WEnum<zwlr_layer_surface_v1::KeyboardInteractivity>,
) -> Option<KeyboardInteractivity> {
    match interactivity {
        WEnum::Value(zwlr_layer_surface_v1::KeyboardInteractivity::None) => {
            Some(KeyboardInteractivity::None)
        }
        WEnum::Value(zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive) => {
            Some(KeyboardInteractivity::Exclusive)
        }
        WEnum::Value(zwlr_layer_surface_v1::KeyboardInteractivity::OnDemand) => {
            Some(KeyboardInteractivity::OnDemand)
        }
        WEnum::Unknown(_) => None,
        _ => None,
    }
}

fn surface_has_buffer_or_pending(surface: &WlSurface) -> bool {
    surface_has_buffer(surface)
        || with_states(surface, |states| {
            let mut attributes = states.cached_state.get::<SurfaceAttributes>();
            matches!(
                attributes.pending().buffer,
                Some(BufferAssignment::NewBuffer(_))
            ) || matches!(
                attributes.current().buffer,
                Some(BufferAssignment::NewBuffer(_))
            )
        })
}

fn layer_surface_pre_commit(
    state: &mut RuntimeState,
    _display: &DisplayHandle,
    wl_surface: &WlSurface,
) {
    let Some(surface) = state.protocol_globals.layer_shell.for_surface(wl_surface) else {
        return;
    };
    let has_buffer = with_states(wl_surface, |states| {
        let mut attributes = states.cached_state.get::<SurfaceAttributes>();
        match &attributes.pending().buffer {
            Some(BufferAssignment::NewBuffer(_)) => true,
            Some(BufferAssignment::Removed) => false,
            None => surface.mapped(),
        }
    });
    surface.commit(has_buffer);
}

delegate_global_dispatch!(RuntimeState, ZwlrLayerShellV1, LayerShellGlobalData);
delegate_dispatch!(RuntimeState, ZwlrLayerShellV1, LayerShellData);
delegate_dispatch!(RuntimeState, ZwlrLayerSurfaceV1, LayerSurfaceData);

//! zxdg-decoration-unstable-v1 dispatch for the native shell.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::xdg::decoration::zv1::client::{
    zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1,
};

use super::types::NativeShellState;
use crate::surface::DecorationPreference;

impl Dispatch<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &zxdg_decoration_manager_v1::ZxdgDecorationManagerV1,
        _: zxdg_decoration_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        deco: &zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1,
        event: zxdg_toplevel_decoration_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_toplevel_decoration_v1::Event::Configure { mode } = event {
            let mode = match mode {
                WEnum::Value(zxdg_toplevel_decoration_v1::Mode::ServerSide) => {
                    DecorationPreference::Server
                }
                WEnum::Value(zxdg_toplevel_decoration_v1::Mode::ClientSide) => {
                    DecorationPreference::Client
                }
                _ => return,
            };
            let Some(surface) = state
                .decoration_objects
                .get(&deco.id().protocol_id())
                .copied()
            else {
                return;
            };
            if let Some(record) = state.toplevels.get_mut(&surface) {
                record.decoration_mode = Some(mode);
            }
            state.pending_csd_refresh.insert(surface);
        }
    }
}

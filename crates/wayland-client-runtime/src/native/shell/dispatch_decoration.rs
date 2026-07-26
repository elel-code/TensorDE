//! zxdg-decoration-unstable-v1 dispatch for the native shell.

use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::xdg::decoration::zv1::client::{
    zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1,
};

use super::types::NativeShellState;

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
        _: &mut Self,
        _: &zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1,
        _: zxdg_toplevel_decoration_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Configure mode event is informational; Fika uses client chrome when
        // ServerSide is not granted. No application event required yet.
    }
}

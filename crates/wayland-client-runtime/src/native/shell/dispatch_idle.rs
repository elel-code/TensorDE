//! Idle inhibit protocol dispatch (no events; keep proxies alive).

use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::idle_inhibit::zv1::client::{
    zwp_idle_inhibit_manager_v1, zwp_idle_inhibitor_v1,
};

use super::types::NativeShellState;

impl Dispatch<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1,
        _: zwp_idle_inhibit_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
        _: zwp_idle_inhibitor_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

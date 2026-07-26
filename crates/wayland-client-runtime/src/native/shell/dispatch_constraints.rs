//! pointer-constraints-v1 dispatch for the native shell.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::pointer_constraints::zv1::client::{
    zwp_confined_pointer_v1, zwp_locked_pointer_v1, zwp_pointer_constraints_v1,
};

use super::types::{NativeShellEvent, NativeShellState};

impl Dispatch<zwp_pointer_constraints_v1::ZwpPointerConstraintsV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &zwp_pointer_constraints_v1::ZwpPointerConstraintsV1,
        _: zwp_pointer_constraints_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwp_locked_pointer_v1::ZwpLockedPointerV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        proxy: &zwp_locked_pointer_v1::ZwpLockedPointerV1,
        event: zwp_locked_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let surface = state
            .locked_pointer
            .as_ref()
            .filter(|(_, p)| p.id() == proxy.id())
            .map(|(sid, _)| *sid);
        let Some(surface) = surface else {
            return;
        };
        match event {
            zwp_locked_pointer_v1::Event::Locked => {
                state.push(NativeShellEvent::PointerConstraint {
                    surface,
                    kind: 2,
                    active: true,
                });
            }
            zwp_locked_pointer_v1::Event::Unlocked => {
                state.push(NativeShellEvent::PointerConstraint {
                    surface,
                    kind: 2,
                    active: false,
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_confined_pointer_v1::ZwpConfinedPointerV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        proxy: &zwp_confined_pointer_v1::ZwpConfinedPointerV1,
        event: zwp_confined_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let surface = state
            .confined_pointer
            .as_ref()
            .filter(|(_, p)| p.id() == proxy.id())
            .map(|(sid, _)| *sid);
        let Some(surface) = surface else {
            return;
        };
        match event {
            zwp_confined_pointer_v1::Event::Confined => {
                state.push(NativeShellEvent::PointerConstraint {
                    surface,
                    kind: 1,
                    active: true,
                });
            }
            zwp_confined_pointer_v1::Event::Unconfined => {
                state.push(NativeShellEvent::PointerConstraint {
                    surface,
                    kind: 1,
                    active: false,
                });
            }
            _ => {}
        }
    }
}

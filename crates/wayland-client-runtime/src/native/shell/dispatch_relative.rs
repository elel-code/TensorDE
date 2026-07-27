//! relative-pointer-v1 dispatch for the native shell.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::relative_pointer::zv1::client::{
    zwp_relative_pointer_manager_v1, zwp_relative_pointer_v1,
};

use super::types::{NativeShellEvent, NativeShellState};

impl Dispatch<zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1, ()>
    for NativeShellState
{
    fn event(
        _: &mut Self,
        _: &zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
        _: zwp_relative_pointer_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwp_relative_pointer_v1::ZwpRelativePointerV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        rel: &zwp_relative_pointer_v1::ZwpRelativePointerV1,
        event: zwp_relative_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_relative_pointer_v1::Event::RelativeMotion {
            utime_hi,
            utime_lo,
            dx,
            dy,
            dx_unaccel,
            dy_unaccel,
        } = event
        {
            let utime = (u64::from(utime_hi) << 32) | u64::from(utime_lo);
            let seat = state.seat_for_relative_pointer(rel).or_else(|| {
                // Compat: shell-wide primary stream without object map entry.
                state
                    .relative_pointer
                    .as_ref()
                    .filter(|r| r.id() == rel.id())
                    .and_then(|_| {
                        state.pointer.as_ref().and_then(|p| state.seat_for_pointer(p))
                    })
            });
            state.push(NativeShellEvent::RelativePointer {
                utime,
                dx,
                dy,
                dx_unaccel,
                dy_unaccel,
                seat,
            });
        }
    }
}

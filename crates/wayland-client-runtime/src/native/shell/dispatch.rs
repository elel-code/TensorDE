//! Dispatch implementations for the native shell queue.

use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool,
    wl_surface,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_v1,
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use super::types::{NativeShellEvent, NativeShellState, NativeSurfaceId};
use crate::geometry::SuggestedSize;


impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for NativeShellState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            if interface == "wl_seat" && state.seat.is_none() {
                let v = version.min(9).max(1);
                state.seat = Some(registry.bind(name, v, qh, ()));
            }
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surface.ack_configure(serial);
            let id = state
                .xdg_surface_objects
                .get(&xdg_surface.id().protocol_id())
                .copied();
            if let Some(id) = id {
                if let Some(record) = state.toplevels.get_mut(&id) {
                    record.configured = true;
                    if let Some((w, h)) = record.pending_size {
                        if w > 0 && h > 0 {
                            record.logical_w = w as u32;
                            record.logical_h = h as u32;
                            if let Some(vp) = record.viewport.as_ref() {
                                vp.set_destination(w, h);
                            }
                        }
                    }
                    let suggested = SuggestedSize::new(
                        Some(record.logical_w).filter(|&w| w > 0),
                        Some(record.logical_h).filter(|&h| h > 0),
                    );
                    if let Some(buffer) = record.buffer.as_ref() {
                        record.wl.attach(Some(buffer), 0, 0);
                        record.wl.damage_buffer(0, 0, i32::MAX, i32::MAX);
                        record.wl.commit();
                    }
                    state.push(NativeShellEvent::ToplevelConfigure {
                        surface: id,
                        suggested_size: suggested,
                    });
                }
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        toplevel: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = state
            .toplevel_objects
            .get(&toplevel.id().protocol_id())
            .copied();
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                if let Some(id) = id {
                    if let Some(record) = state.toplevels.get_mut(&id) {
                        if width > 0 && height > 0 {
                            record.pending_size = Some((width, height));
                        }
                    }
                }
            }
            xdg_toplevel::Event::Close => {
                if let Some(id) = id {
                    state.push(NativeShellEvent::ToplevelClose { surface: id });
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        {
            state.seat_capabilities = capabilities;
            if capabilities.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            }
            if capabilities.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key {
            key,
            state: key_state,
            ..
        } = event
        {
            let pressed = matches!(
                key_state,
                WEnum::Value(wl_keyboard::KeyState::Pressed)
            );
            state.push(NativeShellEvent::SeatKeyboardKey { key, pressed });
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface, surface_x, surface_y, ..
            } => {
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                if let Some(id) = id {
                    state.pointer_focus = Some(id);
                    state.push(NativeShellEvent::PointerEnter {
                        surface: id,
                        x: surface_x,
                        y: surface_y,
                    });
                }
            }
            wl_pointer::Event::Leave { surface, .. } => {
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied()
                    .or(state.pointer_focus);
                state.pointer_focus = None;
                if let Some(id) = id {
                    state.push(NativeShellEvent::PointerLeave { surface: id });
                }
            }
            wl_pointer::Event::Motion {
                surface_x, surface_y, ..
            } => {
                if let Some(id) = state.pointer_focus {
                    state.push(NativeShellEvent::PointerMotion {
                        surface: id,
                        x: surface_x,
                        y: surface_y,
                    });
                }
            }
            wl_pointer::Event::Button {
                button,
                state: btn_state,
                ..
            } => {
                let pressed = matches!(btn_state, WEnum::Value(wl_pointer::ButtonState::Pressed));
                state.push(NativeShellEvent::PointerButton {
                    surface: state.pointer_focus,
                    button,
                    pressed,
                });
            }
            wl_pointer::Event::Axis { axis, value, .. } => match axis {
                WEnum::Value(wl_pointer::Axis::VerticalScroll) => state.axis_v += value,
                WEnum::Value(wl_pointer::Axis::HorizontalScroll) => state.axis_h += value,
                _ => {}
            },
            wl_pointer::Event::Frame => {
                if state.axis_h != 0.0 || state.axis_v != 0.0 {
                    state.push(NativeShellEvent::PointerAxis {
                        surface: state.pointer_focus,
                        horizontal: state.axis_h,
                        vertical: state.axis_v,
                    });
                    state.axis_h = 0.0;
                    state.axis_v = 0.0;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wp_viewporter::WpViewporter, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wp_viewporter::WpViewporter,
        _: wp_viewporter::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_viewport::WpViewport, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wp_viewport::WpViewport,
        _: wp_viewport::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        _: wp_fractional_scale_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        fractional: &wp_fractional_scale_v1::WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            let id = state
                .fractional_objects
                .get(&fractional.id().protocol_id())
                .copied();
            if let Some(id) = id {
                let factor = f64::from(scale) / 120.0;
                if let Some(record) = state.toplevels.get_mut(&id) {
                    record.scale_factor = factor;
                }
                state.push(NativeShellEvent::ScaleFactorChanged {
                    surface: id,
                    factor,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::shell::NativeShell;

    #[test]
    fn native_shell_creates_toplevel_when_compositor_present() {
        let Ok(mut shell) = NativeShell::connect_to_env() else {
            return;
        };
        let id = shell
            .create_toplevel("fika-native-smoke", "dev.fika.NativeSmoke")
            .expect("create toplevel");
        assert_eq!(shell.toplevel_count(), 1);

        compio::runtime::Runtime::new()
            .expect("compio")
            .block_on(async {
                for _ in 0..32 {
                    let _ = shell.pump_once().await;
                    if shell.is_configured(id) {
                        break;
                    }
                }
            });

        let mut events = Vec::new();
        shell.drain_events_into(&mut events);
        let _ = shell.destroy_toplevel(id);
    }
}

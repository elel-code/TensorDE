//! Dispatch implementations for the native shell queue.

use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_output, wl_registry, wl_seat, wl_shm, wl_shm_pool,
    wl_subcompositor, wl_subsurface, wl_surface,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::cursor_shape::v1::client::{
    wp_cursor_shape_device_v1, wp_cursor_shape_manager_v1,
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_v1,
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols::xdg::shell::client::{
    xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};

use super::types::{NativeShellEvent, NativeShellState};
use crate::event::ToplevelState;
use crate::geometry::SuggestedSize;

/// Decode `xdg_toplevel.configure` states array (native-endian u32 words).
fn decode_toplevel_states(states: &[u8]) -> ToplevelState {
    // Wire values from xdg-shell: maximized=1 … suspended=9.
    const MAXIMIZED: u32 = 1;
    const FULLSCREEN: u32 = 2;
    const RESIZING: u32 = 3;
    const ACTIVATED: u32 = 4;
    const TILED_LEFT: u32 = 5;
    const TILED_RIGHT: u32 = 6;
    const TILED_TOP: u32 = 7;
    const TILED_BOTTOM: u32 = 8;
    const SUSPENDED: u32 = 9;
    let mut out = ToplevelState::empty();
    for word in states.chunks_exact(4) {
        let value = u32::from_ne_bytes([word[0], word[1], word[2], word[3]]);
        match value {
            MAXIMIZED => out.set(ToplevelState::MAXIMIZED, true),
            FULLSCREEN => out.set(ToplevelState::FULLSCREEN, true),
            RESIZING => out.set(ToplevelState::RESIZING, true),
            ACTIVATED => out.set(ToplevelState::ACTIVATED, true),
            TILED_LEFT => out.set(ToplevelState::TILED_LEFT, true),
            TILED_RIGHT => out.set(ToplevelState::TILED_RIGHT, true),
            TILED_TOP => out.set(ToplevelState::TILED_TOP, true),
            TILED_BOTTOM => out.set(ToplevelState::TILED_BOTTOM, true),
            SUSPENDED => out.set(ToplevelState::SUSPENDED, true),
            _ => {}
        }
    }
    out
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for NativeShellState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                if interface == "wl_seat" && !state.seats.contains_key(&name) {
                    let v = version.clamp(1, 9);
                    let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, v, qh, ());
                    state.register_seat(name, seat);
                }
                // Hotplug: bind outputs advertised after the initial registry dump.
                if interface == "wl_output" && !state.outputs.contains_key(&name) {
                    let v = version.clamp(1, 4);
                    let output = registry.bind::<wl_output::WlOutput, _, _>(name, v, qh, ());
                    state.output_objects.insert(output.id().protocol_id(), name);
                    state.output_proxies.insert(name, output);
                    state.outputs.insert(
                        name,
                        super::types::OutputRecord {
                            scale: 1,
                            make: String::new(),
                            model: String::new(),
                            name: None,
                            description: None,
                            x: 0,
                            y: 0,
                            physical_width: 0,
                            physical_height: 0,
                            mode_width: 0,
                            mode_height: 0,
                            mode_refresh_mhz: 0,
                            done: false,
                        },
                    );
                    if state.session_lock.is_some() {
                        state.pending_session_lock_outputs.push(name);
                    }
                }
            }
            wl_registry::Event::GlobalRemove { name } => {
                if state.outputs.remove(&name).is_some() {
                    if let Some(surface) = state.session_lock_outputs.get(&name).copied() {
                        state.push(NativeShellEvent::SessionLockSurfaceRemoved {
                            surface,
                            output: name,
                        });
                    }
                    if state.output_powers.contains_key(&name) {
                        if let Some((_, retain_failed)) = state
                            .pending_output_power_destroy
                            .iter_mut()
                            .find(|(output, _)| *output == name)
                        {
                            *retain_failed = false;
                        } else {
                            state.pending_output_power_destroy.push((name, false));
                        }
                    }
                    state.output_proxies.remove(&name);
                    state.output_objects.retain(|_, n| *n != name);
                    state.push(NativeShellEvent::OutputRemoved { output: name });
                }
                if state.seats.contains_key(&name) {
                    state.unregister_seat(name);
                }
            }
            _ => {}
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
        state: &mut Self,
        surface: &wl_surface::WlSurface,
        event: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let surface_id = state
            .wl_surface_objects
            .get(&surface.id().protocol_id())
            .copied();
        match event {
            wl_surface::Event::Enter { output } => {
                if let (Some(surface), Some(&output_name)) = (
                    surface_id,
                    state.output_objects.get(&output.id().protocol_id()),
                ) {
                    state.push(NativeShellEvent::SurfaceOutputEnter {
                        surface,
                        output: output_name,
                    });
                }
            }
            wl_surface::Event::Leave { output } => {
                if let (Some(surface), Some(&output_name)) = (
                    surface_id,
                    state.output_objects.get(&output.id().protocol_id()),
                ) {
                    state.push(NativeShellEvent::SurfaceOutputLeave {
                        surface,
                        output: output_name,
                    });
                }
            }
            _ => {}
        }
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
        state: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // SHM / icon buffers ignore release; dmabuf-backed ones notify the app.
        if matches!(event, wl_buffer::Event::Release) {
            let proto = buffer.id().protocol_id();
            if let Some(&id) = state.dmabuf_buffer_by_proto.get(&proto) {
                state.push(NativeShellEvent::DmabufBufferReleased {
                    id: crate::dmabuf::DmabufBufferId(id),
                });
            }
        }
    }
}

impl Dispatch<wl_subcompositor::WlSubcompositor, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_subcompositor::WlSubcompositor,
        _: wl_subcompositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_subsurface::WlSubsurface, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_subsurface::WlSubsurface,
        _: wl_subsurface::Event,
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
                    record.last_configure_serial = serial;
                    if let Some((w, h)) = record.pending_size
                        && w > 0
                        && h > 0
                    {
                        record.logical_w = w as u32;
                        record.logical_h = h as u32;
                        if let Some(vp) = record.viewport.as_ref() {
                            vp.set_destination(w, h);
                        }
                    }
                    let suggested = SuggestedSize::new(
                        Some(record.logical_w).filter(|&w| w > 0),
                        Some(record.logical_h).filter(|&h| h > 0),
                    );
                    let toplevel_state = record.pending_states;
                    let scale = record.scale_factor;
                    let logical_w = record.logical_w;
                    let logical_h = record.logical_h;
                    if let Some(buffer) = record.buffer.as_ref() {
                        record.wl.attach(Some(buffer), 0, 0);
                        record.wl.damage_buffer(0, 0, i32::MAX, i32::MAX);
                        record.wl.commit();
                    }
                    if let Some(frame) = state.csd_frames.get_mut(&id) {
                        frame.set_content_size(logical_w, logical_h);
                        frame.set_toplevel_state(toplevel_state);
                        frame.set_scale(scale);
                    }
                    state.pending_csd_refresh.insert(id);
                    state.push(NativeShellEvent::ToplevelConfigure {
                        surface: id,
                        suggested_size: suggested,
                        state: toplevel_state,
                        serial,
                    });
                } else if let Some(record) = state.popups.get_mut(&id) {
                    record.configured = true;
                    record.last_configure_serial = serial;
                    let (x, y, w, h) = record.pending_geom.unwrap_or((
                        0,
                        0,
                        record.logical_w as i32,
                        record.logical_h as i32,
                    ));
                    if w > 0 && h > 0 {
                        record.logical_w = w as u32;
                        record.logical_h = h as u32;
                    }
                    let reposition_token = record.pending_reposition_token.take();
                    if let Some(buffer) = record.buffer.as_ref() {
                        record.wl.attach(Some(buffer), 0, 0);
                        record.wl.damage_buffer(0, 0, i32::MAX, i32::MAX);
                        record.wl.commit();
                    }
                    state.push(NativeShellEvent::PopupConfigure {
                        surface: id,
                        x,
                        y,
                        width: w,
                        height: h,
                        serial,
                        reposition_token,
                    });
                }
            }
        }
    }
}

impl Dispatch<xdg_popup::XdgPopup, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        popup: &xdg_popup::XdgPopup,
        event: xdg_popup::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = state.popup_objects.get(&popup.id().protocol_id()).copied();
        match event {
            xdg_popup::Event::Configure {
                x,
                y,
                width,
                height,
            } => {
                if let Some(id) = id
                    && let Some(record) = state.popups.get_mut(&id)
                {
                    record.pending_geom = Some((x, y, width, height));
                }
            }
            xdg_popup::Event::PopupDone => {
                if let Some(id) = id {
                    state.push(NativeShellEvent::PopupDone { surface: id });
                }
            }
            xdg_popup::Event::Repositioned { token } => {
                if let Some(id) = id
                    && let Some(record) = state.popups.get_mut(&id)
                {
                    record.pending_reposition_token = Some(token);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_positioner::XdgPositioner, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &xdg_positioner::XdgPositioner,
        _: xdg_positioner::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
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
            xdg_toplevel::Event::Configure {
                width,
                height,
                states,
            } => {
                if let Some(id) = id
                    && let Some(record) = state.toplevels.get_mut(&id)
                {
                    if width > 0 && height > 0 {
                        record.pending_size = Some((width, height));
                    }
                    record.pending_states = decode_toplevel_states(&states);
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
                if let Some(record) = state.layers.get_mut(&id) {
                    record.scale_factor = factor;
                }
                if let Some(record) = state.session_lock_surfaces.get_mut(&id) {
                    record.scale_factor = factor;
                }
                if let Some(frame) = state.csd_frames.get_mut(&id) {
                    frame.set_scale(factor);
                }
                state.push(NativeShellEvent::ScaleFactorChanged {
                    surface: id,
                    factor,
                });
            }
        }
    }
}

impl Dispatch<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
        _: wp_cursor_shape_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_cursor_shape_device_v1::WpCursorShapeDeviceV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wp_cursor_shape_device_v1::WpCursorShapeDeviceV1,
        _: wp_cursor_shape_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        callback: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { callback_data } = event {
            let id = state.frame_callbacks.remove(&callback.id().protocol_id());
            if let Some(surface) = id {
                state.frame_pending.remove(&surface);
                state.push(NativeShellEvent::Frame {
                    surface,
                    time: callback_data,
                });
            }
        }
    }
}

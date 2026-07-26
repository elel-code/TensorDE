//! Dispatch implementations for the native shell queue.

use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_registry, wl_seat,
    wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface, wl_touch,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
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
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
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
        let id = state
            .popup_objects
            .get(&popup.id().protocol_id())
            .copied();
        match event {
            xdg_popup::Event::Configure {
                x,
                y,
                width,
                height,
            } => {
                if let Some(id) = id {
                    if let Some(record) = state.popups.get_mut(&id) {
                        record.pending_geom = Some((x, y, width, height));
                    }
                }
            }
            xdg_popup::Event::PopupDone => {
                if let Some(id) = id {
                    state.push(NativeShellEvent::PopupDone { surface: id });
                }
            }
            xdg_popup::Event::Repositioned { token } => {
                if let Some(id) = id {
                    if let Some(record) = state.popups.get_mut(&id) {
                        record.pending_reposition_token = Some(token);
                    }
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
                if let Some(id) = id {
                    if let Some(record) = state.toplevels.get_mut(&id) {
                        if width > 0 && height > 0 {
                            record.pending_size = Some((width, height));
                        }
                        record.pending_states = decode_toplevel_states(&states);
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
                let pointer = seat.get_pointer(qh, ());
                if let Some(manager) = state.pointer_gestures.as_ref() {
                    if state.swipe_gesture.is_none() {
                        state.swipe_gesture =
                            Some(manager.get_swipe_gesture(&pointer, qh, ()));
                    }
                    if state.pinch_gesture.is_none() {
                        state.pinch_gesture =
                            Some(manager.get_pinch_gesture(&pointer, qh, ()));
                    }
                    if manager.version() >= 3 && state.hold_gesture.is_none() {
                        state.hold_gesture = Some(manager.get_hold_gesture(&pointer, qh, ()));
                    }
                }
                // Relative pointer is opt-in via enable_relative_pointer (capture / games).
                state.pointer = Some(pointer);
            }
            if capabilities.contains(wl_seat::Capability::Touch) && state.touch.is_none() {
                state.touch = Some(seat.get_touch(qh, ()));
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
        match event {
            wl_keyboard::Event::Keymap { format, fd, size } => {
                match format {
                    WEnum::Value(wl_keyboard::KeymapFormat::XkbV1) => {
                        state.xkb =
                            crate::native::protocols::core::NativeXkb::from_fd(fd, size);
                    }
                    WEnum::Value(wl_keyboard::KeymapFormat::NoKeymap) => {
                        state.xkb = None;
                    }
                    _ => {
                        state.xkb = None;
                    }
                }
            }
            wl_keyboard::Event::Enter { surface, serial, .. } => {
                state.last_input_serial = Some(serial);
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                state.keyboard_focus = id;
                state.push(NativeShellEvent::SeatKeyboardEnter { surface: id });
            }
            wl_keyboard::Event::Leave { surface, .. } => {
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied()
                    .or(state.keyboard_focus);
                state.keyboard_focus = None;
                state.push(NativeShellEvent::SeatKeyboardLeave { surface: id });
            }
            wl_keyboard::Event::Key {
                serial,
                key,
                state: key_state,
                ..
            } => {
                state.last_input_serial = Some(serial);
                let pressed = matches!(key_state, WEnum::Value(wl_keyboard::KeyState::Pressed));
                let (keysym, text) = if let Some(xkb) = state.xkb.as_mut() {
                    let lookup = xkb.key_event(key, pressed);
                    (lookup.keysym, lookup.text)
                } else {
                    (0, None)
                };
                state.push(NativeShellEvent::SeatKeyboardKey {
                    key,
                    pressed,
                    keysym,
                    text,
                });
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                if let Some(xkb) = state.xkb.as_mut() {
                    xkb.update_mask(mods_depressed, mods_latched, mods_locked, group);
                }
                state.push(NativeShellEvent::SeatModifiers {
                    mods_depressed,
                    mods_latched,
                    mods_locked,
                    group,
                });
            }
            _ => {}
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
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                serial,
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                if let Some(id) = id {
                    state.pointer_enter_serial = Some(serial);
                    // CSD decoration parts: handle chrome input, map focus to parent.
                    if let Some(&(parent, kind)) = state.csd_part_owners.get(&id) {
                        state.csd_pointer_part = Some((parent, kind));
                        state.on_pointer_focus_changed(Some(parent), qh);
                        if let Some(frame) = state.csd_frames.get_mut(&parent) {
                            let cursor = frame.on_pointer_enter(kind, surface_x, surface_y);
                            state.pending_csd_cursor = Some(cursor);
                        }
                        return;
                    }
                    state.csd_pointer_part = None;
                    state.on_pointer_focus_changed(Some(id), qh);
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
                if let Some(id) = id {
                    if state.csd_part_owners.contains_key(&id)
                        || state.csd_pointer_part.is_some()
                    {
                        if let Some((parent, _)) = state.csd_pointer_part.take() {
                            if let Some(frame) = state.csd_frames.get_mut(&parent) {
                                frame.on_pointer_leave();
                            }
                        }
                        state.on_pointer_focus_changed(None, qh);
                        return;
                    }
                }
                state.csd_pointer_part = None;
                state.on_pointer_focus_changed(None, qh);
                if let Some(id) = id {
                    state.push(NativeShellEvent::PointerLeave { surface: id });
                }
            }
            wl_pointer::Event::Motion {
                surface_x, surface_y, ..
            } => {
                if let Some((parent, kind)) = state.csd_pointer_part {
                    if let Some(frame) = state.csd_frames.get_mut(&parent) {
                        let cursor = frame.on_pointer_motion(kind, surface_x, surface_y);
                        state.pending_csd_cursor = Some(cursor);
                    }
                    return;
                }
                if let Some(id) = state.pointer_focus {
                    state.push(NativeShellEvent::PointerMotion {
                        surface: id,
                        x: surface_x,
                        y: surface_y,
                    });
                }
            }
            wl_pointer::Event::Button {
                serial,
                button,
                state: btn_state,
                ..
            } => {
                state.last_input_serial = Some(serial);
                let pressed = matches!(btn_state, WEnum::Value(wl_pointer::ButtonState::Pressed));
                if let Some((parent, _)) = state.csd_pointer_part {
                    if let Some(frame) = state.csd_frames.get_mut(&parent) {
                        if let Some(action) = frame.on_pointer_button(button, pressed) {
                            state.pending_frame_actions.push((parent, action));
                        }
                    }
                    return;
                }
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
            wl_pointer::Event::AxisValue120 { axis, value120 } => match axis {
                WEnum::Value(wl_pointer::Axis::VerticalScroll) => {
                    state.axis_v120 = state.axis_v120.saturating_add(value120);
                }
                WEnum::Value(wl_pointer::Axis::HorizontalScroll) => {
                    state.axis_h120 = state.axis_h120.saturating_add(value120);
                }
                _ => {}
            },
            wl_pointer::Event::Frame => {
                if state.axis_h != 0.0
                    || state.axis_v != 0.0
                    || state.axis_h120 != 0
                    || state.axis_v120 != 0
                {
                    state.push(NativeShellEvent::PointerAxis {
                        surface: state.pointer_focus,
                        horizontal: state.axis_h,
                        vertical: state.axis_v,
                        horizontal_value120: state.axis_h120,
                        vertical_value120: state.axis_v120,
                    });
                    state.axis_h = 0.0;
                    state.axis_v = 0.0;
                    state.axis_h120 = 0;
                    state.axis_v120 = 0;
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
            let id = state
                .frame_callbacks
                .remove(&callback.id().protocol_id());
            if let Some(surface) = id {
                state.push(NativeShellEvent::Frame {
                    surface,
                    time: callback_data,
                });
            }
        }
    }
}

impl Dispatch<wl_touch::WlTouch, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        _: &wl_touch::WlTouch,
        event: wl_touch::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_touch::Event::Down {
                surface, id, x, y, ..
            } => {
                let surface_id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                if let Some(surface) = surface_id {
                    state.push(NativeShellEvent::TouchDown {
                        surface,
                        id,
                        x,
                        y,
                    });
                }
            }
            wl_touch::Event::Up { id, .. } => {
                state.push(NativeShellEvent::TouchUp { id });
            }
            wl_touch::Event::Motion { id, x, y, .. } => {
                state.push(NativeShellEvent::TouchMotion { id, x, y });
            }
            wl_touch::Event::Frame => {
                state.push(NativeShellEvent::TouchFrame);
            }
            wl_touch::Event::Cancel => {
                state.push(NativeShellEvent::TouchCancel);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(&name) = state.output_objects.get(&output.id().protocol_id()) else {
            return;
        };
        match event {
            wl_output::Event::Geometry {
                x,
                y,
                physical_width,
                physical_height,
                make,
                model,
                ..
            } => {
                if let Some(record) = state.outputs.get_mut(&name) {
                    record.make = make.clone();
                    record.model = model.clone();
                }
                state.push(NativeShellEvent::OutputGeometry {
                    output: name,
                    x,
                    y,
                    physical_width,
                    physical_height,
                    make,
                    model,
                });
            }
            wl_output::Event::Mode {
                flags,
                width,
                height,
                refresh,
            } => {
                let current = match flags {
                    WEnum::Value(f) => f.contains(wl_output::Mode::Current),
                    _ => false,
                };
                state.push(NativeShellEvent::OutputMode {
                    output: name,
                    width,
                    height,
                    refresh,
                    current,
                });
            }
            wl_output::Event::Scale { factor } => {
                if let Some(record) = state.outputs.get_mut(&name) {
                    record.scale = factor;
                }
                state.push(NativeShellEvent::OutputScale {
                    output: name,
                    factor,
                });
            }
            wl_output::Event::Done => {
                state.push(NativeShellEvent::OutputDone { output: name });
            }
            _ => {}
        }
    }
}

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

use super::types::{NativeShellEvent, NativeShellState, NativeSurfaceId};
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
                    let v = version.min(9).max(1);
                    let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, v, qh, ());
                    state.register_seat(name, seat);
                }
                // Hotplug: bind outputs advertised after the initial registry dump.
                if interface == "wl_output" && !state.outputs.contains_key(&name) {
                    let v = version.min(4).max(1);
                    let output = registry.bind::<wl_output::WlOutput, _, _>(name, v, qh, ());
                    state
                        .output_objects
                        .insert(output.id().protocol_id(), name);
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
                }
            }
            wl_registry::Event::GlobalRemove { name } => {
                if state.outputs.remove(&name).is_some() {
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
        let seat_global = state.seat_objects.get(&seat.id().protocol_id()).copied();
        match event {
            wl_seat::Event::Name { name } => {
                if let Some(global) = seat_global {
                    if let Some(rec) = state.seats.get_mut(&global) {
                        rec.name = Some(name);
                    }
                }
            }
            wl_seat::Event::Capabilities {
                capabilities: WEnum::Value(capabilities),
            } => {
                // Keep shell-wide capability bits as the union of all seats for
                // capability queries; devices still attach per-seat.
                state.seat_capabilities |= capabilities;
                let is_primary = state
                    .seat
                    .as_ref()
                    .is_some_and(|s| s.id() == seat.id());

                if let Some(global) = seat_global {
                    if let Some(rec) = state.seats.get_mut(&global) {
                        rec.capabilities = capabilities;
                    }
                }

                if capabilities.contains(wl_seat::Capability::Keyboard) {
                    let need = match seat_global.and_then(|g| state.seats.get(&g)) {
                        Some(rec) => rec.keyboard.is_none(),
                        None => state.keyboard.is_none(),
                    };
                    if need {
                        let keyboard = seat.get_keyboard(qh, ());
                        if let Some(global) = seat_global {
                            state
                                .keyboard_objects
                                .insert(keyboard.id().protocol_id(), global);
                            if let Some(rec) = state.seats.get_mut(&global) {
                                rec.keyboard = Some(keyboard.clone());
                            }
                        }
                        // Primary-seat keyboard fills the legacy single field.
                        if is_primary || state.keyboard.is_none() {
                            state.keyboard = Some(keyboard);
                        }
                    }
                }
                if capabilities.contains(wl_seat::Capability::Pointer) {
                    let need = match seat_global.and_then(|g| state.seats.get(&g)) {
                        Some(rec) => rec.pointer.is_none(),
                        None => state.pointer.is_none(),
                    };
                    if need {
                        let pointer = seat.get_pointer(qh, ());
                        if let Some(global) = seat_global {
                            state
                                .pointer_objects
                                .insert(pointer.id().protocol_id(), global);
                        }
                        // Gestures / relative pointer attach only to the primary
                        // pointer (single stream APIs today).
                        if is_primary || state.pointer.is_none() {
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
                                    state.hold_gesture =
                                        Some(manager.get_hold_gesture(&pointer, qh, ()));
                                }
                            }
                            state.pointer = Some(pointer.clone());
                        }
                        if let Some(global) = seat_global {
                            if let Some(rec) = state.seats.get_mut(&global) {
                                rec.pointer = Some(pointer);
                            }
                        }
                    }
                }
                if capabilities.contains(wl_seat::Capability::Touch) {
                    let need = match seat_global.and_then(|g| state.seats.get(&g)) {
                        Some(rec) => rec.touch.is_none(),
                        None => state.touch.is_none(),
                    };
                    if need {
                        let touch = seat.get_touch(qh, ());
                        if let Some(global) = seat_global {
                            state
                                .touch_objects
                                .insert(touch.id().protocol_id(), global);
                            if let Some(rec) = state.seats.get_mut(&global) {
                                rec.touch = Some(touch.clone());
                            }
                        }
                        if is_primary || state.touch.is_none() {
                            state.touch = Some(touch);
                        }
                    }
                }
                if !capabilities.contains(wl_seat::Capability::Touch) && is_primary {
                    if state.touch.take().is_some()
                        || !state.touch_active.is_empty()
                        || !state.touch_pending.is_empty()
                    {
                        state.touch_pending.clear();
                        state.touch_active.clear();
                        state.touch_points.clear();
                        state.push(NativeShellEvent::TouchCancel);
                    }
                    if let Some(global) = seat_global {
                        if let Some(rec) = state.seats.get_mut(&global) {
                            rec.touch = None;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        keyboard: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let seat_global = state.seat_for_keyboard(keyboard);
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
                state.note_seat_serial(seat_global, serial);
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                state.keyboard_focus = id;
                if let Some(g) = seat_global {
                    if let Some(rec) = state.seats.get_mut(&g) {
                        rec.keyboard_focus = id;
                    }
                }
                state.push(NativeShellEvent::SeatKeyboardEnter { surface: id });
            }
            wl_keyboard::Event::Leave { surface, .. } => {
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied()
                    .or(state.keyboard_focus);
                state.keyboard_focus = None;
                if let Some(g) = seat_global {
                    if let Some(rec) = state.seats.get_mut(&g) {
                        rec.keyboard_focus = None;
                    }
                }
                state.push(NativeShellEvent::SeatKeyboardLeave { surface: id });
            }
            wl_keyboard::Event::Key {
                serial,
                key,
                state: key_state,
                ..
            } => {
                state.note_seat_serial(seat_global, serial);
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
        pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let seat_global = state.seat_for_pointer(pointer);
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
                    state.note_seat_serial(seat_global, serial);
                    if let Some(g) = seat_global {
                        if let Some(rec) = state.seats.get_mut(&g) {
                            rec.pointer_enter_serial = Some(serial);
                        }
                    }
                    // CSD decoration parts: handle chrome input, map focus to parent.
                    if let Some(&(parent, kind)) = state.csd_part_owners.get(&id) {
                        state.csd_pointer_part = Some((parent, kind));
                        state.on_pointer_focus_changed(Some(parent), qh);
                        if let Some(g) = seat_global {
                            if let Some(rec) = state.seats.get_mut(&g) {
                                rec.pointer_focus = Some(parent);
                            }
                        }
                        if let Some(frame) = state.csd_frames.get_mut(&parent) {
                            let cursor = frame.on_pointer_enter(kind, surface_x, surface_y);
                            state.pending_csd_cursor = Some(cursor);
                        }
                        return;
                    }
                    state.csd_pointer_part = None;
                    state.on_pointer_focus_changed(Some(id), qh);
                    if let Some(g) = seat_global {
                        if let Some(rec) = state.seats.get_mut(&g) {
                            rec.pointer_focus = Some(id);
                        }
                    }
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
                        if let Some(g) = seat_global {
                            if let Some(rec) = state.seats.get_mut(&g) {
                                rec.pointer_focus = None;
                            }
                        }
                        return;
                    }
                }
                state.csd_pointer_part = None;
                state.on_pointer_focus_changed(None, qh);
                if let Some(g) = seat_global {
                    if let Some(rec) = state.seats.get_mut(&g) {
                        rec.pointer_focus = None;
                    }
                }
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
                state.note_seat_serial(seat_global, serial);
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
                if let Some(record) = state.layers.get_mut(&id) {
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
                state.frame_pending.remove(&surface);
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
        touch: &wl_touch::WlTouch,
        event: wl_touch::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use super::types::PendingTouchEvent;
        let seat_global = state.seat_for_touch(touch);

        // SCTK-compatible frame buffering: hold down/up/motion/shape/orientation
        // until Frame, with Weston's missing-final-frame workaround (flush when
        // the last active point is released).
        let mut flush = false;
        match event {
            wl_touch::Event::Down {
                serial,
                time,
                surface,
                id,
                x,
                y,
            } => {
                state.note_seat_serial(seat_global, serial);
                let Some(surface_id) = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied()
                else {
                    return;
                };
                if let Err(pos) = state.touch_active.binary_search(&id) {
                    state.touch_active.insert(pos, id);
                }
                state.touch_points.insert(id, surface_id);
                state.touch_pending.push(PendingTouchEvent::Down {
                    surface: surface_id,
                    id,
                    x,
                    y,
                    serial,
                    time,
                });
            }
            wl_touch::Event::Up { serial, time, id } => {
                state.note_seat_serial(seat_global, serial);
                if let Ok(pos) = state.touch_active.binary_search(&id) {
                    state.touch_active.remove(pos);
                }
                state.touch_pending.push(PendingTouchEvent::Up { id, serial, time });
                // Weston may omit Frame after the last touch-up.
                if state.touch_active.is_empty() {
                    flush = true;
                }
            }
            wl_touch::Event::Motion { time, id, x, y } => {
                state.touch_pending.push(PendingTouchEvent::Motion { id, x, y, time });
            }
            wl_touch::Event::Shape { id, major, minor } => {
                state.touch_pending.push(PendingTouchEvent::Shape { id, major, minor });
            }
            wl_touch::Event::Orientation { id, orientation } => {
                state
                    .touch_pending
                    .push(PendingTouchEvent::Orientation { id, degrees: orientation });
            }
            wl_touch::Event::Frame => {
                flush = true;
            }
            wl_touch::Event::Cancel => {
                state.touch_pending.clear();
                state.touch_active.clear();
                state.touch_points.clear();
                state.push(NativeShellEvent::TouchCancel);
            }
            _ => {}
        }

        if flush {
            state.flush_touch_pending();
        }
    }
}

impl NativeShellState {
    /// Drain the frame buffer into public [`NativeShellEvent`]s.
    pub(crate) fn flush_touch_pending(&mut self) {
        use super::types::PendingTouchEvent;

        if self.touch_pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.touch_pending);
        for ev in pending {
            match ev {
                PendingTouchEvent::Down {
                    surface,
                    id,
                    x,
                    y,
                    serial,
                    time,
                } => {
                    self.push(NativeShellEvent::TouchDown {
                        surface,
                        id,
                        x,
                        y,
                        serial,
                        time,
                    });
                }
                PendingTouchEvent::Up { id, serial, time } => {
                    self.touch_points.remove(&id);
                    self.push(NativeShellEvent::TouchUp { id, serial, time });
                }
                PendingTouchEvent::Motion { id, x, y, time } => {
                    self.push(NativeShellEvent::TouchMotion { id, x, y, time });
                }
                PendingTouchEvent::Shape { id, major, minor } => {
                    self.push(NativeShellEvent::TouchShape { id, major, minor });
                }
                PendingTouchEvent::Orientation { id, degrees } => {
                    self.push(NativeShellEvent::TouchOrientation { id, degrees });
                }
            }
        }
        self.push(NativeShellEvent::TouchFrame);
    }

    /// Drop tracked touch points for a destroyed surface (emit cancel once).
    ///
    /// Losing any live point on that surface invalidates the whole seat
    /// gesture (Wayland convention: compositor may cancel the full set).
    pub(crate) fn cancel_touch_for_surface(&mut self, surface: NativeSurfaceId) {
        let had = self.touch_points.values().any(|&s| s == surface)
            || self.touch_pending.iter().any(|ev| {
                matches!(ev, super::types::PendingTouchEvent::Down { surface: s, .. } if *s == surface)
            });
        if !had {
            return;
        }
        self.touch_pending.clear();
        self.touch_active.clear();
        self.touch_points.clear();
        self.push(NativeShellEvent::TouchCancel);
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
                    record.x = x;
                    record.y = y;
                    record.physical_width = physical_width;
                    record.physical_height = physical_height;
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
                if current {
                    if let Some(record) = state.outputs.get_mut(&name) {
                        record.mode_width = width;
                        record.mode_height = height;
                        record.mode_refresh_mhz = refresh;
                    }
                }
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
            wl_output::Event::Name { name: output_name } => {
                if let Some(record) = state.outputs.get_mut(&name) {
                    record.name = Some(output_name);
                }
            }
            wl_output::Event::Description { description } => {
                if let Some(record) = state.outputs.get_mut(&name) {
                    record.description = Some(description);
                }
            }
            wl_output::Event::Done => {
                if let Some(record) = state.outputs.get_mut(&name) {
                    record.done = true;
                }
                state.push(NativeShellEvent::OutputDone { output: name });
            }
            _ => {}
        }
    }
}

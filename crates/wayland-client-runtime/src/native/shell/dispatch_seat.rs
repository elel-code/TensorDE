//! Seat, keyboard, pointer, and touch dispatch for the native shell.

use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_seat, wl_touch};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};

use super::types::{NativeShellEvent, NativeShellState, NativeSurfaceId};

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
                if let Some(global) = seat_global
                    && let Some(rec) = state.seats.get_mut(&global)
                {
                    rec.name = Some(name);
                    state.push_seat_changed(global);
                }
            }
            wl_seat::Event::Capabilities {
                capabilities: WEnum::Value(capabilities),
            } => {
                // Shell-wide capability bits are the union of every seat; devices
                // still attach per-seat (and are released when a seat loses them).
                let is_primary = state
                    .seat
                    .as_ref()
                    .is_some_and(|s| s.id() == seat.id());

                if let Some(global) = seat_global
                    && let Some(rec) = state.seats.get_mut(&global)
                {
                    rec.capabilities = capabilities;
                }
                state.recompute_seat_capabilities_union();

                let mut devices_changed = false;

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
                        devices_changed = true;
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
                        // Relative pointer remains primary-only (single stream API).
                        if is_primary || state.pointer.is_none() {
                            state.pointer = Some(pointer.clone());
                        }
                        if let Some(global) = seat_global {
                            if let Some(rec) = state.seats.get_mut(&global) {
                                rec.pointer = Some(pointer.clone());
                            }
                            // Per-seat gesture objects (multi-seat compositors).
                            state.bind_gestures_for_seat(global, &pointer, qh, is_primary);
                        }
                        devices_changed = true;
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
                        devices_changed = true;
                    }
                }

                // Release devices (and gestures) for capabilities that went away.
                if state.release_lost_seat_capabilities(seat_global, capabilities, is_primary) {
                    devices_changed = true;
                }

                if devices_changed && let Some(global) = seat_global {
                    state.push_seat_changed(global);
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
                state.set_keyboard_focus(seat_global, id);
                state.push(NativeShellEvent::SeatKeyboardEnter {
                    surface: id,
                    seat: seat_global,
                });
            }
            wl_keyboard::Event::Leave { surface, .. } => {
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied()
                    .or(state.keyboard_focus);
                state.set_keyboard_focus(seat_global, None);
                state.push(NativeShellEvent::SeatKeyboardLeave {
                    surface: id,
                    seat: seat_global,
                });
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
                    seat: seat_global,
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
                    seat: seat_global,
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
                    if let Some(g) = seat_global
                        && let Some(rec) = state.seats.get_mut(&g) {
                            rec.pointer_enter_serial = Some(serial);
                        }
                    // CSD decoration parts: handle chrome input, map focus to parent.
                    if let Some(&(parent, kind)) = state.csd_part_owners.get(&id) {
                        state.csd_pointer_part = Some((parent, kind));
                        state.on_pointer_focus_changed(Some(parent), qh);
                        state.set_seat_pointer_focus(seat_global, Some(parent));
                        if let Some(frame) = state.csd_frames.get_mut(&parent) {
                            let cursor = frame.on_pointer_enter(kind, surface_x, surface_y);
                            state.pending_csd_cursor = Some(cursor);
                        }
                        return;
                    }
                    state.csd_pointer_part = None;
                    state.on_pointer_focus_changed(Some(id), qh);
                    state.set_seat_pointer_focus(seat_global, Some(id));
                    state.push(NativeShellEvent::PointerEnter {
                        surface: id,
                        x: surface_x,
                        y: surface_y,
                        seat: seat_global,
                    });
                }
            }
            wl_pointer::Event::Leave { surface, .. } => {
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied()
                    .or(state.pointer_focus);
                if let Some(id) = id
                    && (state.csd_part_owners.contains_key(&id)
                        || state.csd_pointer_part.is_some())
                    {
                        if let Some((parent, _)) = state.csd_pointer_part.take()
                            && let Some(frame) = state.csd_frames.get_mut(&parent) {
                                frame.on_pointer_leave();
                            }
                        state.on_pointer_focus_changed(None, qh);
                        state.set_seat_pointer_focus(seat_global, None);
                        return;
                    }
                state.csd_pointer_part = None;
                state.on_pointer_focus_changed(None, qh);
                state.set_seat_pointer_focus(seat_global, None);
                if let Some(id) = id {
                    state.push(NativeShellEvent::PointerLeave {
                        surface: id,
                        seat: seat_global,
                    });
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
                let focus = seat_global
                    .and_then(|g| state.seats.get(&g).and_then(|r| r.pointer_focus))
                    .or(state.pointer_focus);
                if let Some(id) = focus {
                    state.push(NativeShellEvent::PointerMotion {
                        surface: id,
                        x: surface_x,
                        y: surface_y,
                        seat: seat_global,
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
                    if let Some(frame) = state.csd_frames.get_mut(&parent)
                        && let Some(action) = frame.on_pointer_button(button, pressed) {
                            state.pending_frame_actions.push((parent, action));
                        }
                    return;
                }
                let focus = seat_global
                    .and_then(|g| state.seats.get(&g).and_then(|r| r.pointer_focus))
                    .or(state.pointer_focus);
                state.push(NativeShellEvent::PointerButton {
                    surface: focus,
                    button,
                    pressed,
                    seat: seat_global,
                });
            }
            wl_pointer::Event::Axis { axis, value, .. } => {
                // Accumulate per-seat so concurrent multi-seat scrolls do not mix.
                if let Some(g) = seat_global
                    && let Some(rec) = state.seats.get_mut(&g)
                {
                    match axis {
                        WEnum::Value(wl_pointer::Axis::VerticalScroll) => rec.axis_v += value,
                        WEnum::Value(wl_pointer::Axis::HorizontalScroll) => rec.axis_h += value,
                        _ => {}
                    }
                } else {
                    match axis {
                        WEnum::Value(wl_pointer::Axis::VerticalScroll) => state.axis_v += value,
                        WEnum::Value(wl_pointer::Axis::HorizontalScroll) => state.axis_h += value,
                        _ => {}
                    }
                }
            }
            wl_pointer::Event::AxisValue120 { axis, value120 } => {
                if let Some(g) = seat_global
                    && let Some(rec) = state.seats.get_mut(&g)
                {
                    match axis {
                        WEnum::Value(wl_pointer::Axis::VerticalScroll) => {
                            rec.axis_v120 = rec.axis_v120.saturating_add(value120);
                        }
                        WEnum::Value(wl_pointer::Axis::HorizontalScroll) => {
                            rec.axis_h120 = rec.axis_h120.saturating_add(value120);
                        }
                        _ => {}
                    }
                } else {
                    match axis {
                        WEnum::Value(wl_pointer::Axis::VerticalScroll) => {
                            state.axis_v120 = state.axis_v120.saturating_add(value120);
                        }
                        WEnum::Value(wl_pointer::Axis::HorizontalScroll) => {
                            state.axis_h120 = state.axis_h120.saturating_add(value120);
                        }
                        _ => {}
                    }
                }
            }
            wl_pointer::Event::Frame => {
                let (h, v, h120, v120, focus) = if let Some(g) = seat_global
                    && let Some(rec) = state.seats.get_mut(&g)
                {
                    let out = (rec.axis_h, rec.axis_v, rec.axis_h120, rec.axis_v120, rec.pointer_focus);
                    rec.axis_h = 0.0;
                    rec.axis_v = 0.0;
                    rec.axis_h120 = 0;
                    rec.axis_v120 = 0;
                    out
                } else {
                    let out = (
                        state.axis_h,
                        state.axis_v,
                        state.axis_h120,
                        state.axis_v120,
                        state.pointer_focus,
                    );
                    state.axis_h = 0.0;
                    state.axis_v = 0.0;
                    state.axis_h120 = 0;
                    state.axis_v120 = 0;
                    out
                };
                if h != 0.0 || v != 0.0 || h120 != 0 || v120 != 0 {
                    state.push(NativeShellEvent::PointerAxis {
                        surface: focus,
                        horizontal: h,
                        vertical: v,
                        horizontal_value120: h120,
                        vertical_value120: v120,
                        seat: seat_global,
                    });
                }
            }
            _ => {}
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
                    seat: seat_global,
                });
            }
            wl_touch::Event::Up { serial, time, id } => {
                state.note_seat_serial(seat_global, serial);
                if let Ok(pos) = state.touch_active.binary_search(&id) {
                    state.touch_active.remove(pos);
                }
                state.touch_pending.push(PendingTouchEvent::Up {
                    id,
                    serial,
                    time,
                    seat: seat_global,
                });
                // Weston may omit Frame after the last touch-up.
                if state.touch_active.is_empty() {
                    flush = true;
                }
            }
            wl_touch::Event::Motion { time, id, x, y } => {
                state.touch_pending.push(PendingTouchEvent::Motion {
                    id,
                    x,
                    y,
                    time,
                    seat: seat_global,
                });
            }
            wl_touch::Event::Shape { id, major, minor } => {
                state.touch_pending.push(PendingTouchEvent::Shape {
                    id,
                    major,
                    minor,
                    seat: seat_global,
                });
            }
            wl_touch::Event::Orientation { id, orientation } => {
                state.touch_pending.push(PendingTouchEvent::Orientation {
                    id,
                    degrees: orientation,
                    seat: seat_global,
                });
            }
            wl_touch::Event::Frame => {
                flush = true;
            }
            wl_touch::Event::Cancel => {
                state.touch_pending.clear();
                state.touch_active.clear();
                state.touch_points.clear();
                state.push(NativeShellEvent::TouchCancel {
                    seat: seat_global,
                });
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
        // All events in one frame share the same seat (single wl_touch).
        let frame_seat = match pending.first() {
            Some(PendingTouchEvent::Down { seat, .. })
            | Some(PendingTouchEvent::Up { seat, .. })
            | Some(PendingTouchEvent::Motion { seat, .. })
            | Some(PendingTouchEvent::Shape { seat, .. })
            | Some(PendingTouchEvent::Orientation { seat, .. }) => *seat,
            None => None,
        };
        for ev in pending {
            match ev {
                PendingTouchEvent::Down {
                    surface,
                    id,
                    x,
                    y,
                    serial,
                    time,
                    seat,
                } => {
                    self.push(NativeShellEvent::TouchDown {
                        surface,
                        id,
                        x,
                        y,
                        serial,
                        time,
                        seat,
                    });
                }
                PendingTouchEvent::Up {
                    id,
                    serial,
                    time,
                    seat,
                } => {
                    self.touch_points.remove(&id);
                    self.push(NativeShellEvent::TouchUp {
                        id,
                        serial,
                        time,
                        seat,
                    });
                }
                PendingTouchEvent::Motion {
                    id,
                    x,
                    y,
                    time,
                    seat,
                } => {
                    self.push(NativeShellEvent::TouchMotion {
                        id,
                        x,
                        y,
                        time,
                        seat,
                    });
                }
                PendingTouchEvent::Shape {
                    id,
                    major,
                    minor,
                    seat,
                } => {
                    self.push(NativeShellEvent::TouchShape {
                        id,
                        major,
                        minor,
                        seat,
                    });
                }
                PendingTouchEvent::Orientation {
                    id,
                    degrees,
                    seat,
                } => {
                    self.push(NativeShellEvent::TouchOrientation {
                        id,
                        degrees,
                        seat,
                    });
                }
            }
        }
        self.push(NativeShellEvent::TouchFrame { seat: frame_seat });
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
        // Surface teardown cancel is not seat-scoped (any seat may own points).
        self.push(NativeShellEvent::TouchCancel { seat: None });
    }
}

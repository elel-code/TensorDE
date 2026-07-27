//! Seat lifecycle: bind/unbind, focus/serial bookkeeping, transfer-device rebind.
//!
//! Kept out of `types` / `api` so multi-seat state machines stay in one place.

use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_seat, wl_touch};
use wayland_client::Proxy;

use super::api::NativeShell;
use super::types::{NativeShellEvent, NativeShellState, NativeSurfaceId, SeatRecord};
use crate::native::connection::NativeError;

impl NativeShellState {
    /// Seat registry name owning a keyboard/pointer/touch proxy, if known.
    pub(crate) fn seat_for_keyboard(&self, keyboard: &wl_keyboard::WlKeyboard) -> Option<u32> {
        self.keyboard_objects
            .get(&keyboard.id().protocol_id())
            .copied()
    }

    pub(crate) fn seat_for_pointer(&self, pointer: &wl_pointer::WlPointer) -> Option<u32> {
        self.pointer_objects
            .get(&pointer.id().protocol_id())
            .copied()
    }

    pub(crate) fn seat_for_touch(&self, touch: &wl_touch::WlTouch) -> Option<u32> {
        self.touch_objects
            .get(&touch.id().protocol_id())
            .copied()
    }

    pub(crate) fn note_seat_serial(&mut self, seat_global: Option<u32>, serial: u32) {
        self.last_input_serial = Some(serial);
        if let Some(g) = seat_global
            && let Some(rec) = self.seats.get_mut(&g)
        {
            rec.last_input_serial = Some(serial);
        }
    }

    /// Update per-seat keyboard focus (and shell-wide last-wins focus).
    pub(crate) fn set_keyboard_focus(
        &mut self,
        seat_global: Option<u32>,
        surface: Option<NativeSurfaceId>,
    ) {
        self.keyboard_focus = surface;
        if let Some(g) = seat_global
            && let Some(rec) = self.seats.get_mut(&g)
        {
            rec.keyboard_focus = surface;
        }
    }

    /// Update per-seat pointer focus (shell-wide focus is set via constraints /
    /// pointer dispatch).
    pub(crate) fn set_seat_pointer_focus(
        &mut self,
        seat_global: Option<u32>,
        surface: Option<NativeSurfaceId>,
    ) {
        if let Some(g) = seat_global
            && let Some(rec) = self.seats.get_mut(&g)
        {
            rec.pointer_focus = surface;
        }
    }

    /// Register a newly bound seat as primary if none yet; always store in `seats`.
    ///
    /// Emits [`NativeShellEvent::SeatAdded`] after insert.
    pub(crate) fn register_seat(&mut self, global_name: u32, seat: wl_seat::WlSeat) {
        let proto = seat.id().protocol_id();
        self.seat_objects.insert(proto, global_name);
        if self.seat.is_none() {
            self.seat = Some(seat.clone());
        }
        self.seats.insert(
            global_name,
            SeatRecord {
                global_name,
                seat,
                name: None,
                capabilities: wl_seat::Capability::empty(),
                keyboard: None,
                pointer: None,
                touch: None,
                keyboard_focus: None,
                pointer_focus: None,
                last_input_serial: None,
                pointer_enter_serial: None,
                data_device: None,
                primary_device: None,
                swipe_gesture: None,
                pinch_gesture: None,
                hold_gesture: None,
                axis: crate::pointer_axis::PointerAxisFrameAccum::default(),
                relative_pointer: None,
            },
        );
        self.push(NativeShellEvent::SeatAdded {
            seat: global_name,
            name: None,
            has_keyboard: false,
            has_pointer: false,
            has_touch: false,
        });
    }

    /// Tear down a seat removed from the registry (devices released with proxy drop).
    pub(crate) fn unregister_seat(&mut self, global_name: u32) {
        let Some(rec) = self.seats.remove(&global_name) else {
            return;
        };
        self.seat_objects.retain(|_, name| *name != global_name);
        self.keyboard_objects.retain(|_, name| *name != global_name);
        self.pointer_objects.retain(|_, name| *name != global_name);
        self.touch_objects.retain(|_, name| *name != global_name);
        self.swipe_objects.retain(|_, name| *name != global_name);
        self.pinch_objects.retain(|_, name| *name != global_name);
        self.hold_objects.retain(|_, name| *name != global_name);
        self.relative_pointer_objects
            .retain(|_, name| *name != global_name);

        let was_primary = self
            .seat
            .as_ref()
            .is_some_and(|s| s.id() == rec.seat.id());

        // Drop capability devices; proxies are destroyed when `rec` drops.
        drop(rec.keyboard);
        drop(rec.pointer);
        drop(rec.touch);
        drop(rec.swipe_gesture);
        drop(rec.pinch_gesture);
        drop(rec.hold_gesture);
        if let Some(rel) = rec.relative_pointer {
            if self
                .relative_pointer
                .as_ref()
                .is_some_and(|r| r.id() == rel.id())
            {
                self.relative_pointer = None;
            }
            rel.destroy();
        }

        if was_primary {
            // Promote another seat if available, else clear primary fields.
            if let Some((_, next)) = self.seats.iter().next() {
                self.seat = Some(next.seat.clone());
                self.keyboard = next.keyboard.clone();
                self.pointer = next.pointer.clone();
                self.touch = next.touch.clone();
                self.seat_capabilities = next.capabilities;
            } else {
                self.seat = None;
                self.keyboard = None;
                self.pointer = None;
                self.touch = None;
                self.seat_capabilities = wl_seat::Capability::empty();
                // Clear touch tracking when the last seat vanishes.
                if !self.touch_active.is_empty() || !self.touch_pending.is_empty() {
                    self.touch_pending.clear();
                    self.touch_active.clear();
                    self.touch_points.clear();
                    self.push(NativeShellEvent::TouchCancel { seat: None });
                }
            }
            // Gestures / constraints were bound to the old primary pointer.
            self.swipe_gesture = None;
            self.pinch_gesture = None;
            self.hold_gesture = None;
            self.relative_pointer = None;
            self.locked_pointer = None;
            self.confined_pointer = None;
            self.gesture_surface = None;
            // Seat-scoped devices must rebind to the new primary (or clear).
            self.data_device = None;
            self.primary_device = None;
            self.text_input = None;
            self.pending_primary_seat_rebind = true;
        }

        self.recompute_seat_capabilities_union();
        self.push(NativeShellEvent::SeatRemoved { seat: global_name });
    }

    /// Emit [`NativeShellEvent::SeatChanged`] with the current device snapshot.
    pub(crate) fn push_seat_changed(&mut self, global: u32) {
        let Some(rec) = self.seats.get(&global) else {
            return;
        };
        self.push(NativeShellEvent::SeatChanged {
            seat: global,
            name: rec.name.clone(),
            has_keyboard: rec.keyboard.is_some(),
            has_pointer: rec.pointer.is_some(),
            has_touch: rec.touch.is_some(),
        });
    }

    /// Drop keyboard/pointer/touch (and gestures) when a seat loses those caps.
    ///
    /// Returns whether any device was released (caller may emit `SeatChanged`).
    pub(crate) fn release_lost_seat_capabilities(
        &mut self,
        global: Option<u32>,
        capabilities: wl_seat::Capability,
        is_primary: bool,
    ) -> bool {
        let Some(global) = global else {
            return false;
        };
        let mut changed = false;

        if !capabilities.contains(wl_seat::Capability::Keyboard) {
            let keyboard = self
                .seats
                .get_mut(&global)
                .and_then(|rec| rec.keyboard.take());
            if let Some(kb) = keyboard {
                self.keyboard_objects
                    .remove(&kb.id().protocol_id());
                if is_primary && self.keyboard.as_ref().is_some_and(|k| k.id() == kb.id()) {
                    self.keyboard = None;
                }
                if let Some(rec) = self.seats.get_mut(&global) {
                    rec.keyboard_focus = None;
                }
                kb.release();
                changed = true;
            }
        }

        if !capabilities.contains(wl_seat::Capability::Pointer) {
            let pointer = self
                .seats
                .get_mut(&global)
                .and_then(|rec| rec.pointer.take());
            if let Some(ptr) = pointer {
                self.pointer_objects
                    .remove(&ptr.id().protocol_id());
                // Drop gesture objects bound to this seat's pointer.
                let swipe = self
                    .seats
                    .get_mut(&global)
                    .and_then(|rec| rec.swipe_gesture.take());
                let pinch = self
                    .seats
                    .get_mut(&global)
                    .and_then(|rec| rec.pinch_gesture.take());
                let hold = self
                    .seats
                    .get_mut(&global)
                    .and_then(|rec| rec.hold_gesture.take());
                if let Some(g) = swipe {
                    self.swipe_objects.remove(&g.id().protocol_id());
                    if is_primary && self.swipe_gesture.as_ref().is_some_and(|s| s.id() == g.id()) {
                        self.swipe_gesture = None;
                    }
                    g.destroy();
                }
                if let Some(g) = pinch {
                    self.pinch_objects.remove(&g.id().protocol_id());
                    if is_primary && self.pinch_gesture.as_ref().is_some_and(|s| s.id() == g.id()) {
                        self.pinch_gesture = None;
                    }
                    g.destroy();
                }
                if let Some(g) = hold {
                    self.hold_objects.remove(&g.id().protocol_id());
                    if is_primary && self.hold_gesture.as_ref().is_some_and(|s| s.id() == g.id()) {
                        self.hold_gesture = None;
                    }
                    g.destroy();
                }
                // Drop per-seat relative pointer with the pointer.
                let rel = self
                    .seats
                    .get_mut(&global)
                    .and_then(|rec| rec.relative_pointer.take());
                if let Some(rel) = rel {
                    self.relative_pointer_objects
                        .remove(&rel.id().protocol_id());
                    if self
                        .relative_pointer
                        .as_ref()
                        .is_some_and(|r| r.id() == rel.id())
                    {
                        self.relative_pointer = None;
                    }
                    rel.destroy();
                }
                if is_primary && self.pointer.as_ref().is_some_and(|p| p.id() == ptr.id()) {
                    self.pointer = None;
                    // Constraints were bound to the primary pointer.
                    self.locked_pointer = None;
                    self.confined_pointer = None;
                    self.gesture_surface = None;
                }
                if let Some(rec) = self.seats.get_mut(&global) {
                    rec.pointer_focus = None;
                    rec.pointer_enter_serial = None;
                }
                ptr.release();
                changed = true;
            }
        }

        if !capabilities.contains(wl_seat::Capability::Touch) {
            let touch = self
                .seats
                .get_mut(&global)
                .and_then(|rec| rec.touch.take());
            if let Some(t) = touch {
                self.touch_objects.remove(&t.id().protocol_id());
                if is_primary && self.touch.as_ref().is_some_and(|x| x.id() == t.id()) {
                    self.touch = None;
                    if !self.touch_active.is_empty() || !self.touch_pending.is_empty() {
                        self.touch_pending.clear();
                        self.touch_active.clear();
                        self.touch_points.clear();
                        self.push(NativeShellEvent::TouchCancel {
                            seat: Some(global),
                        });
                    }
                }
                t.release();
                changed = true;
            }
        }

        if changed {
            self.recompute_seat_capabilities_union();
            // If the primary seat lost a device, mirror another seat's proxy so
            // single-seat APIs keep working when any seat still has that cap.
            if is_primary {
                self.promote_primary_device_mirrors();
            }
        }
        changed
    }

    /// Fill empty shell-wide keyboard/pointer/touch from any seat that still has them.
    pub(crate) fn promote_primary_device_mirrors(&mut self) {
        if self.keyboard.is_none() {
            self.keyboard = self
                .seats
                .values()
                .find_map(|rec| rec.keyboard.clone());
        }
        if self.pointer.is_none()
            && let Some(rec) = self.seats.values().find(|rec| rec.pointer.is_some())
        {
            self.pointer = rec.pointer.clone();
            self.swipe_gesture = rec.swipe_gesture.clone();
            self.pinch_gesture = rec.pinch_gesture.clone();
            self.hold_gesture = rec.hold_gesture.clone();
            self.relative_pointer = rec.relative_pointer.clone();
        }
        if self.touch.is_none() {
            self.touch = self.seats.values().find_map(|rec| rec.touch.clone());
        }
    }

    /// Shell-wide capability bits = union of every bound seat's capabilities.
    pub(crate) fn recompute_seat_capabilities_union(&mut self) {
        let mut caps = wl_seat::Capability::empty();
        for rec in self.seats.values() {
            caps |= rec.capabilities;
        }
        self.seat_capabilities = caps;
    }

    /// Seat + serial for compositor grabs (move/resize/menu).
    ///
    /// Prefer the seat whose `last_input_serial` matches the shell-wide last-wins
    /// serial so multi-seat input pairs correctly. Falls back to primary seat.
    pub(crate) fn resolve_grab_seat_serial(
        &self,
        seat: Option<crate::SeatId>,
    ) -> Result<(wl_seat::WlSeat, u32), NativeError> {
        if let Some(id) = seat {
            let rec = self.seats.get(&id.get()).ok_or_else(|| {
                NativeError::Protocol(format!("unknown seat {}", id.get()))
            })?;
            let serial = rec.last_input_serial.ok_or_else(|| {
                NativeError::Protocol("no input serial for toplevel interaction".into())
            })?;
            return Ok((rec.seat.clone(), serial));
        }
        if let Some(serial) = self.last_input_serial {
            for rec in self.seats.values() {
                if rec.last_input_serial == Some(serial) {
                    return Ok((rec.seat.clone(), serial));
                }
            }
            // Shell serial without a matching seat record: use primary seat.
            if let Some(primary) = self.seat.clone() {
                return Ok((primary, serial));
            }
        }
        Err(NativeError::Protocol(
            "no input serial for toplevel interaction".into(),
        ))
    }

    /// Pointer + enter serial for cursor shape (optional seat override).
    pub(crate) fn resolve_cursor_pointer_serial(
        &self,
        seat: Option<crate::SeatId>,
    ) -> Result<(wl_pointer::WlPointer, u32), NativeError> {
        if let Some(id) = seat {
            let rec = self.seats.get(&id.get()).ok_or_else(|| {
                NativeError::Protocol(format!("unknown seat {}", id.get()))
            })?;
            let pointer = rec.pointer.clone().ok_or_else(|| {
                NativeError::Protocol("seat has no pointer".into())
            })?;
            let serial = rec.pointer_enter_serial.ok_or_else(|| {
                NativeError::Protocol("no pointer enter serial".into())
            })?;
            return Ok((pointer, serial));
        }
        // Prefer the seat that owns the shell-wide enter serial.
        if let Some(serial) = self.pointer_enter_serial {
            for rec in self.seats.values() {
                if rec.pointer_enter_serial == Some(serial)
                    && let Some(pointer) = rec.pointer.clone()
                {
                    return Ok((pointer, serial));
                }
            }
            if let Some(pointer) = self.pointer.clone() {
                return Ok((pointer, serial));
            }
        }
        Err(NativeError::Protocol("no pointer enter serial".into()))
    }

    /// Bind gesture objects for one seat's pointer (and optionally mirror primary).
    ///
    /// Called from seat capability dispatch when a pointer is created, and from
    /// [`NativeShell::ensure_all_seat_gestures`] during rebind.
    pub(crate) fn bind_gestures_for_seat(
        &mut self,
        global: u32,
        pointer: &wl_pointer::WlPointer,
        qh: &wayland_client::QueueHandle<NativeShellState>,
        is_primary: bool,
    ) {
        let Some(manager) = self.pointer_gestures.clone() else {
            return;
        };
        let hold_ok = manager.version() >= 3;
        let (need_swipe, need_pinch, need_hold) = match self.seats.get(&global) {
            Some(rec) => (
                rec.swipe_gesture.is_none(),
                rec.pinch_gesture.is_none(),
                hold_ok && rec.hold_gesture.is_none(),
            ),
            None => return,
        };
        if need_swipe {
            let g = manager.get_swipe_gesture(pointer, qh, ());
            self.swipe_objects.insert(g.id().protocol_id(), global);
            if let Some(rec) = self.seats.get_mut(&global) {
                rec.swipe_gesture = Some(g.clone());
            }
            if is_primary {
                self.swipe_gesture = Some(g);
            }
        }
        if need_pinch {
            let g = manager.get_pinch_gesture(pointer, qh, ());
            self.pinch_objects.insert(g.id().protocol_id(), global);
            if let Some(rec) = self.seats.get_mut(&global) {
                rec.pinch_gesture = Some(g.clone());
            }
            if is_primary {
                self.pinch_gesture = Some(g);
            }
        }
        if need_hold {
            let g = manager.get_hold_gesture(pointer, qh, ());
            self.hold_objects.insert(g.id().protocol_id(), global);
            if let Some(rec) = self.seats.get_mut(&global) {
                rec.hold_gesture = Some(g.clone());
            }
            if is_primary {
                self.hold_gesture = Some(g);
            }
        }
    }
}

impl NativeShell {
    /// Number of bound seats.
    #[inline]
    pub fn seat_count(&self) -> usize {
        self.state.seats.len()
    }

    /// Snapshot of bound seats (registry name + optional compositor seat name).
    pub fn seats(&self) -> Vec<crate::SeatInfo> {
        let mut list: Vec<_> = self
            .state
            .seats
            .values()
            .map(|s| crate::SeatInfo {
                id: crate::SeatId::from_raw(s.global_name),
                name: s.name.clone(),
                has_keyboard: s.keyboard.is_some(),
                has_pointer: s.pointer.is_some(),
                has_touch: s.touch.is_some(),
            })
            .collect();
        list.sort_by_key(|s| s.id.get());
        list
    }

    /// Keyboard-focused surface on `seat`, if any.
    pub fn seat_keyboard_focus(&self, seat: crate::SeatId) -> Option<NativeSurfaceId> {
        self.state
            .seats
            .get(&seat.get())
            .and_then(|s| s.keyboard_focus)
    }

    /// Pointer-focused surface on `seat`, if any.
    pub fn seat_pointer_focus(&self, seat: crate::SeatId) -> Option<NativeSurfaceId> {
        self.state
            .seats
            .get(&seat.get())
            .and_then(|s| s.pointer_focus)
    }

    /// Latest input serial on `seat` (for seat-scoped grabs / selections).
    pub fn seat_last_input_serial(&self, seat: crate::SeatId) -> Option<u32> {
        self.state
            .seats
            .get(&seat.get())
            .and_then(|s| s.last_input_serial)
    }

    /// Pointer-enter serial on `seat` (cursor shape, etc.).
    pub fn seat_pointer_enter_serial(&self, seat: crate::SeatId) -> Option<u32> {
        self.state
            .seats
            .get(&seat.get())
            .and_then(|s| s.pointer_enter_serial)
    }

    /// Build an [`crate::InputSerial`] from the latest serial on `seat`.
    ///
    /// Returns `None` if the seat is unknown or has no serial yet.
    pub fn seat_input_serial(
        &self,
        seat: crate::SeatId,
        source: crate::InputSerialSource,
    ) -> Option<crate::InputSerial> {
        let rec = self.state.seats.get(&seat.get())?;
        let serial = rec.last_input_serial?;
        Some(crate::InputSerial::new(rec.seat.clone(), serial, source))
    }

    /// Primary seat id (first bound / current primary), if any.
    pub fn primary_seat_id(&self) -> Option<crate::SeatId> {
        let primary = self.state.seat.as_ref()?;
        self.state
            .seat_objects
            .get(&primary.id().protocol_id())
            .copied()
            .map(crate::SeatId::from_raw)
    }

    /// Whether `seat` has a bound `wl_data_device` (clipboard / DnD).
    pub fn seat_has_data_device(&self, seat: crate::SeatId) -> bool {
        self.state
            .seats
            .get(&seat.get())
            .is_some_and(|s| s.data_device.is_some())
    }

    /// Whether `seat` has a bound primary selection device.
    pub fn seat_has_primary_device(&self, seat: crate::SeatId) -> bool {
        self.state
            .seats
            .get(&seat.get())
            .is_some_and(|s| s.primary_device.is_some())
    }

    /// Whether a keymap was applied via `wl_keyboard.keymap` (xkb ready).
    pub fn has_xkb(&self) -> bool {
        self.state.xkb.is_some()
    }

    /// Set the pointer cursor via `wp_cursor_shape` when available.
    pub fn set_cursor_shape(
        &mut self,
        shape: wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape,
    ) -> Result<(), NativeError> {
        self.set_cursor_shape_on_seat(shape, None)
    }

    /// Set the cursor shape on a specific seat's pointer (or auto-resolve).
    pub fn set_cursor_shape_on_seat(
        &mut self,
        shape: wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape,
        seat: Option<crate::SeatId>,
    ) -> Result<(), NativeError> {
        let (pointer, serial) = self.state.resolve_cursor_pointer_serial(seat)?;
        let manager = self.state.cursor_shape_manager.as_ref().ok_or_else(|| {
            NativeError::Protocol("wp_cursor_shape_manager_v1 missing".into())
        })?;
        let qh = self.queue.handle();
        let device = manager.get_pointer(&pointer, &qh, ());
        device.set_shape(serial, shape);
        device.destroy();
        self.connection.mark_dirty();
        Ok(())
    }

    /// Ensure every bound seat has data-device / primary selection proxies.
    ///
    /// Shell-wide `data_device` / `primary_device` always mirror the **primary**
    /// seat for single-seat APIs (`set_selection`, `start_drag`, …).
    pub(crate) fn ensure_all_seat_transfer_devices(&mut self) {
        let qh = self.queue.handle();
        let seat_globals: Vec<u32> = self.state.seats.keys().copied().collect();
        for global in seat_globals {
            let Some(rec) = self.state.seats.get(&global) else {
                continue;
            };
            let seat = rec.seat.clone();
            let need_data = rec.data_device.is_none();
            let need_primary = rec.primary_device.is_none();
            if need_data && let Some(manager) = self.state.data_device_manager.as_ref() {
                let device = manager.get_data_device(&seat, &qh, ());
                if let Some(rec) = self.state.seats.get_mut(&global) {
                    rec.data_device = Some(device);
                }
            }
            if need_primary && let Some(manager) = self.state.primary_selection_manager.as_ref() {
                let device = manager.get_device(&seat, &qh, ());
                if let Some(rec) = self.state.seats.get_mut(&global) {
                    rec.primary_device = Some(device);
                }
            }
        }
        self.mirror_primary_transfer_devices();
        self.connection.mark_dirty();
    }

    /// Copy primary-seat transfer proxies onto shell-wide fields.
    pub(crate) fn mirror_primary_transfer_devices(&mut self) {
        let Some(primary_id) = self.primary_seat_id() else {
            return;
        };
        let Some(rec) = self.state.seats.get(&primary_id.get()) else {
            return;
        };
        self.state.data_device = rec.data_device.clone();
        self.state.primary_device = rec.primary_device.clone();
    }

    /// Re-create seat-scoped protocol objects after primary seat changes.
    pub(crate) fn rebind_primary_seat_devices(&mut self) {
        let qh = self.queue.handle();
        self.ensure_all_seat_transfer_devices();
        let Some(seat) = self.state.seat.clone() else {
            return;
        };
        // Force primary mirror even if shell-wide fields were stale.
        self.mirror_primary_transfer_devices();
        if self.state.text_input.is_none()
            && let Some(tim) = self.state.text_input_manager.as_ref()
        {
            self.state.text_input = Some(tim.get_text_input(&seat, &qh, ()));
        }
        // Recreate pointer gestures for every seat that has a pointer.
        self.ensure_all_seat_gestures();
        // Mirror primary seat gestures onto shell-wide fields for compatibility.
        if let Some(primary_id) = self.primary_seat_id()
            && let Some(rec) = self.state.seats.get(&primary_id.get())
        {
            self.state.swipe_gesture = rec.swipe_gesture.clone();
            self.state.pinch_gesture = rec.pinch_gesture.clone();
            self.state.hold_gesture = rec.hold_gesture.clone();
        }
        let _ = qh;
        self.connection.mark_dirty();
    }

    /// Bind swipe/pinch/hold on every seat that has a pointer and the global.
    pub(crate) fn ensure_all_seat_gestures(&mut self) {
        let qh = self.queue.handle();
        let primary = self.primary_seat_id().map(|id| id.get());
        let seat_globals: Vec<(u32, wl_pointer::WlPointer)> = self
            .state
            .seats
            .iter()
            .filter_map(|(global, rec)| {
                rec.pointer
                    .as_ref()
                    .map(|p| (*global, p.clone()))
            })
            .collect();
        for (global, pointer) in seat_globals {
            let is_primary = primary == Some(global);
            self.state
                .bind_gestures_for_seat(global, &pointer, &qh, is_primary);
        }
        // Keep shell-wide mirrors on the primary seat.
        if let Some(primary_id) = primary
            && let Some(rec) = self.state.seats.get(&primary_id)
        {
            self.state.swipe_gesture = rec.swipe_gesture.clone();
            self.state.pinch_gesture = rec.pinch_gesture.clone();
            self.state.hold_gesture = rec.hold_gesture.clone();
        }
        self.connection.mark_dirty();
    }
}

//! Seat lifecycle: bind/unbind, focus/serial bookkeeping, transfer-device rebind.
//!
//! Kept out of `types` / `api` so multi-seat state machines stay in one place.

use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_seat, wl_touch};
use wayland_client::Proxy;

use super::api::NativeShell;
use super::types::{NativeShellEvent, NativeShellState, NativeSurfaceId, SeatRecord};

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

        self.push(NativeShellEvent::SeatRemoved { seat: global_name });
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

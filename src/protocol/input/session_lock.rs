use super::*;

impl RuntimeState {
    pub(super) fn process_session_lock_input(&mut self, event: LibinputEvent) {
        match event {
            LibinputEvent::Input(BackendInputEvent::Keyboard(event)) => {
                self.forward_session_lock_keyboard(event)
            }
            LibinputEvent::Input(BackendInputEvent::PointerMotion(event)) => {
                let Some(current) = self.input_seat.pointer_location() else {
                    return;
                };
                let Some(location) =
                    self.relative_pointer_location(current, (event.delta_x, event.delta_y).into())
                else {
                    return;
                };
                self.forward_session_lock_pointer_location(location, event.time_ns);
            }
            LibinputEvent::Input(BackendInputEvent::PointerMotionAbsolute(event)) => {
                let Some(bounds) = self.pointer_coordinate_space() else {
                    return;
                };
                let current = self
                    .input_seat
                    .pointer_location()
                    .unwrap_or_else(|| center_pointer_location(bounds));
                let location = Point::from((
                    event.x * f64::from(bounds.size.w),
                    event.y * f64::from(bounds.size.h),
                )) + bounds.loc.to_f64();
                self.forward_session_lock_pointer_location(
                    constrain_pointer_location(
                        replace_non_finite_pointer_location(location, current),
                        bounds,
                    ),
                    event.time_ns,
                );
            }
            LibinputEvent::Input(BackendInputEvent::PointerButton(event)) => {
                self.forward_session_lock_button(event)
            }
            LibinputEvent::Input(BackendInputEvent::PointerAxis(event)) => {
                self.forward_session_lock_axis(event)
            }
            LibinputEvent::Input(BackendInputEvent::PointerGesture(event)) => {
                self.forward_pointer_gesture(event)
            }
            LibinputEvent::Input(BackendInputEvent::Activity) | LibinputEvent::Device(_) => {}
        }
    }

    fn forward_session_lock_keyboard(&mut self, event: KeyboardEvent) {
        if !self.input_seat.keyboard_enabled() {
            return;
        }
        let serial = next_serial();
        let Some(update) = self.input_seat.update_key(event.key, event.pressed, serial) else {
            return;
        };
        if !update.transition {
            return;
        }
        let mut intercepted = false;
        if let Some(vt) = virtual_terminal_for_keysym(update.keysym) {
            if update.pressed {
                self.request_virtual_terminal(vt);
            }
            intercepted = true;
        }
        if !update.pressed && !self.input_seat.key_was_forwarded(update.evdev_key) {
            intercepted = true;
        }
        if !intercepted {
            if update.modifiers_changed {
                self.protocol_globals
                    .seat
                    .modifiers(update.modifiers, serial);
            }
            self.protocol_globals.seat.key(
                update.evdev_key,
                update.pressed,
                serial,
                event.time_msec(),
            );
            self.input_seat
                .set_key_forwarded(update.evdev_key, update.pressed);
        }
    }

    fn forward_session_lock_pointer_location(
        &mut self,
        location: Point<f64, Logical>,
        time_ns: u64,
    ) {
        if !self.input_seat.pointer_enabled() {
            return;
        }
        let focus = self.session_lock_pointer_focus(location);
        let serial = next_serial();
        let time = (time_ns / 1_000_000) as u32;
        self.deliver_pointer_motion(focus, location, serial, time);
        self.protocol_globals.seat.pointer_frame();
        self.protocol_globals
            .activation
            .sync_pointer_focus(self.input_seat.pointer_focus());
        let _ = self.cursor.note_pointer_activity();
        self.request_redraw_at(location);
    }

    fn session_lock_pointer_focus(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let output = self.space.output_under(location).next()?;
        let geometry = self.space.output_geometry(output)?;
        let surface = self
            .protocol_globals
            .session_lock
            .surface_for_output(output.id())?;
        surface_tree_under(surface, location, geometry.loc)
            .map(|(surface, surface_location)| (surface, surface_location.to_f64()))
    }

    fn forward_session_lock_button(&mut self, event: PointerButtonEvent) {
        let Some(focused) = self.input_seat.pointer_focus_owned() else {
            return;
        };
        let serial = next_serial();
        if event.pressed {
            let mut surface = focused;
            while let Some(parent) = crate::protocol::globals::compositor::get_parent(&surface) {
                surface = parent;
            }
            if self
                .protocol_globals
                .session_lock
                .contains_active_surface(&surface)
            {
                self.focus_session_lock_surface(&surface);
            }
        }
        if self
            .input_seat
            .set_button(event.button, event.pressed, serial)
        {
            self.protocol_globals.seat.pointer_button(
                serial,
                event.time_msec(),
                event.button,
                event.pressed,
            );
            self.protocol_globals.seat.pointer_frame();
        }
    }

    fn forward_session_lock_axis(&mut self, event: PointerAxisEvent) {
        if !self.input_seat.pointer_enabled() {
            return;
        }
        let scale = self
            .input_seat
            .pointer_focus()
            .and_then(Resource::client)
            .map(|client| self.client_scale(&client))
            .unwrap_or(1.0);
        self.protocol_globals.seat.pointer_axis(event, scale);
        self.protocol_globals.seat.pointer_frame();
    }
}

use super::*;

impl RuntimeState {
    pub(super) fn process_session_lock_input(&mut self, event: LibinputEvent) {
        match event {
            LibinputEvent::Input(BackendInputEvent::Keyboard(event)) => {
                self.forward_session_lock_keyboard(event)
            }
            LibinputEvent::Input(BackendInputEvent::PointerMotion(event)) => {
                let Some(pointer) = self.seat.get_pointer() else {
                    return;
                };
                let Some(location) = self.relative_pointer_location(
                    pointer.current_location(),
                    (event.delta_x, event.delta_y).into(),
                ) else {
                    return;
                };
                self.forward_session_lock_pointer_location(location, event.time_ns);
            }
            LibinputEvent::Input(BackendInputEvent::PointerMotionAbsolute(event)) => {
                let Some(bounds) = self.pointer_coordinate_space() else {
                    return;
                };
                let current = self
                    .seat
                    .get_pointer()
                    .map(|pointer| pointer.current_location())
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
            LibinputEvent::Input(BackendInputEvent::Activity)
            | LibinputEvent::Tablet { .. }
            | LibinputEvent::Device { .. } => {}
        }
    }

    fn forward_session_lock_keyboard(&mut self, event: KeyboardEvent) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let key_state = smithay_key_state(event.pressed);
        keyboard.input::<(), _>(
            self,
            xkb_keycode(event.key),
            key_state,
            SERIAL_COUNTER.next_serial(),
            event.time_msec(),
            move |state, _, handle| {
                if let Some(vt) = virtual_terminal_for_keysym(handle.modified_sym().raw()) {
                    if key_state == SmithayKeyState::Pressed {
                        state.request_virtual_terminal(vt);
                    }
                    FilterResult::Intercept(())
                } else {
                    FilterResult::Forward
                }
            },
        );
    }

    fn forward_session_lock_pointer_location(
        &mut self,
        location: Point<f64, Logical>,
        time_ns: u64,
    ) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let focus = self.session_lock_pointer_focus(location);
        pointer.motion(
            self,
            focus,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: (time_ns / 1_000_000) as u32,
            },
        );
        pointer.frame(self);
        self.protocol_globals.activation.sync_pointer_focus(
            pointer
                .current_focus()
                .as_ref()
                .map(SurfaceFocusTarget::surface),
        );
        let _ = self.cursor.note_pointer_activity();
        self.request_redraw_at(location);
    }

    fn session_lock_pointer_focus(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<(SurfaceFocusTarget, Point<f64, Logical>)> {
        let output = self.space.output_under(location).next()?;
        let geometry = self.space.output_geometry(output)?;
        let surface = self
            .protocol_globals
            .session_lock
            .surface_for_output(output.id())?;
        surface_tree_under(surface, location, geometry.loc)
            .map(|(surface, surface_location)| (surface.into(), surface_location.to_f64()))
    }

    fn forward_session_lock_button(&mut self, event: PointerButtonEvent) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        let state = smithay_button_state(event.pressed);
        if state == ButtonState::Pressed
            && let Some(surface) = pointer.current_focus()
        {
            let mut surface = surface.into_surface();
            while let Some(parent) = smithay::wayland::compositor::get_parent(&surface) {
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
        pointer.button(
            self,
            &ButtonEvent {
                serial,
                time: event.time_msec(),
                button: event.button,
                state,
            },
        );
        pointer.frame(self);
    }

    fn forward_session_lock_axis(&mut self, event: PointerAxisEvent) {
        use smithay::input::pointer::AxisFrame;

        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let mut frame = AxisFrame::new(event.time_msec())
            .source(smithay_axis_source(event.source))
            .relative_direction(
                Axis::Horizontal,
                smithay_axis_direction(event.horizontal_direction),
            )
            .relative_direction(
                Axis::Vertical,
                smithay_axis_direction(event.vertical_direction),
            );
        for (axis, amount, v120) in [
            (
                Axis::Horizontal,
                event.horizontal(),
                event.horizontal_v120(),
            ),
            (Axis::Vertical, event.vertical(), event.vertical_v120()),
        ] {
            if let Some(amount) = amount {
                frame = frame.value(axis, amount);
            }
            if let Some(steps) = v120 {
                frame = frame.v120(axis, steps);
            }
        }
        if event.horizontal_stopped() {
            frame = frame.stop(Axis::Horizontal);
        }
        if event.vertical_stopped() {
            frame = frame.stop(Axis::Vertical);
        }
        pointer.axis(self, frame);
        pointer.frame(self);
    }
}

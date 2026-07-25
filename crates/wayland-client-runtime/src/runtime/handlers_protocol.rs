impl ActivationHandler for RuntimeState {
    fn activation_token_done(&mut self, purpose: ActivationTokenPurpose, token: String) {
        match purpose {
            ActivationTokenPurpose::Export { request, surface } => {
                self.events
                    .push(Event::Activation(ActivationEvent::TokenDone {
                        request,
                        requesting_surface: surface,
                        token: ActivationToken::from_raw(token),
                    }));
            }
            ActivationTokenPurpose::Attention { surface } => {
                self.pending_attention.remove(&surface);
                if let Some(shared) = self.surfaces.get(&surface)
                    && let Some(activation) = self.xdg_activation.as_ref()
                {
                    activation.activate(shared.wl_surface(), ActivationToken::from_raw(token));
                }
            }
        }
    }
}

impl FractionalScaleHandler for RuntimeState {
    fn preferred_scale(&mut self, surface: &wl_surface::WlSurface, factor: f64) {
        if let Some(surface) = self.surface_id(surface) {
            self.events
                .push(Event::Surface(SurfaceEvent::ScaleFactorChanged {
                    surface,
                    factor,
                }));
        }
    }
}

impl TouchHandler for RuntimeState {
    fn touch_frame_event(&mut self, seat: &wl_seat::WlSeat, event: wl_touch::Event) {
        self.dispatch_touch_event(seat, event);
    }

    fn touch_cancelled(&mut self, seat: &wl_seat::WlSeat) {
        self.touch_cancel(seat);
    }
}

impl PointerGestureHandler for RuntimeState {
    fn pointer_gesture_surface(&mut self, surface: &wl_surface::WlSurface) -> Option<SurfaceId> {
        self.surface_id(surface)
            .filter(|surface| self.pointer_gesture_subscriptions.contains(*surface))
    }

    fn pointer_gesture_event(&mut self, event: PointerGestureEvent) {
        let input = event
            .serial()
            .map(|serial| (serial.seat.id().protocol_id(), serial.serial));
        if let Some((seat_id, serial)) = input {
            self.record_selection_serial(seat_id, serial);
        }
        self.events.push(Event::PointerGesture(event));
    }
}

impl PointerConstraintsHandler for RuntimeState {
    fn confined(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        confined_pointer: &ZwpConfinedPointerV1,
        _: &wl_surface::WlSurface,
        pointer: &wl_pointer::WlPointer,
    ) {
        self.pointer_constraint_changed(pointer, |session| {
            session.confined_changed(confined_pointer, true)
        });
    }

    fn unconfined(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        confined_pointer: &ZwpConfinedPointerV1,
        _: &wl_surface::WlSurface,
        pointer: &wl_pointer::WlPointer,
    ) {
        self.pointer_constraint_changed(pointer, |session| {
            session.confined_changed(confined_pointer, false)
        });
    }

    fn locked(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        locked_pointer: &ZwpLockedPointerV1,
        _: &wl_surface::WlSurface,
        pointer: &wl_pointer::WlPointer,
    ) {
        self.pointer_constraint_changed(pointer, |session| {
            session.locked_changed(locked_pointer, true)
        });
    }

    fn unlocked(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        locked_pointer: &ZwpLockedPointerV1,
        _: &wl_surface::WlSurface,
        pointer: &wl_pointer::WlPointer,
    ) {
        self.pointer_constraint_changed(pointer, |session| {
            session.locked_changed(locked_pointer, false)
        });
    }
}

impl RelativePointerHandler for RuntimeState {
    fn relative_pointer_motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        relative_pointer: &ZwpRelativePointerV1,
        pointer: &wl_pointer::WlPointer,
        event: RelativeMotionEvent,
    ) {
        let Some(data) = pointer.data::<PointerData<()>>() else {
            return;
        };
        let seat_id = data.seat().id().protocol_id();
        let Some(session) = self
            .seats
            .get(&seat_id)
            .map(|objects| &objects.pointer_session)
            .filter(|session| session.relative_matches(relative_pointer))
        else {
            return;
        };
        let Some(surface) = session.focus() else {
            return;
        };
        if !session.should_emit_relative() {
            return;
        }
        self.events
            .push(Event::RelativePointer(RelativePointerEvent {
                surface,
                time_micros: event.utime,
                delta: event.delta,
                delta_unaccelerated: event.delta_unaccel,
            }));
    }
}

impl RuntimeState {
    fn pointer_constraint_changed(
        &mut self,
        pointer: &wl_pointer::WlPointer,
        update: impl FnOnce(&mut SeatPointerSession) -> Option<crate::PointerConstraintEvent>,
    ) {
        let Some(data) = pointer.data::<PointerData<()>>() else {
            return;
        };
        let seat_id = data.seat().id().protocol_id();
        let Some(event) = self
            .seats
            .get_mut(&seat_id)
            .and_then(|objects| update(&mut objects.pointer_session))
        else {
            return;
        };
        self.events.push(Event::PointerConstraint(event));
    }
}

impl TextInputHandler for RuntimeState {
    fn text_input_entered(
        &mut self,
        seat_id: u32,
        text_input: &ZwpTextInputV3,
        surface: &wl_surface::WlSurface,
    ) {
        let Some(surface) = self.surface_id(surface) else {
            return;
        };
        let desired = self.surfaces.get(&surface).and_then(|shared| {
            shared
                .text_input
                .lock()
                .expect("surface text input mutex poisoned")
                .clone()
        });
        let Some(session) = self
            .seats
            .get_mut(&seat_id)
            .and_then(|objects| objects.text_input.as_mut())
            .filter(|session| session.matches(text_input))
        else {
            return;
        };
        session.enter(surface, desired.as_ref());
        self.events
            .push(Event::TextInput(TextInputEvent::Entered { surface }));
    }

    fn text_input_left(
        &mut self,
        seat_id: u32,
        text_input: &ZwpTextInputV3,
        surface: &wl_surface::WlSurface,
    ) {
        let surface = self.surface_id(surface);
        let Some(session) = self
            .seats
            .get_mut(&seat_id)
            .and_then(|objects| objects.text_input.as_mut())
            .filter(|session| session.matches(text_input))
        else {
            return;
        };
        session.leave();
        if let Some(surface) = surface {
            self.events
                .push(Event::TextInput(TextInputEvent::Left { surface }));
        }
    }

    fn text_input_done(
        &mut self,
        seat_id: u32,
        text_input: &ZwpTextInputV3,
        surface: &wl_surface::WlSurface,
        serial: u32,
        batch: PendingBatch,
    ) {
        let Some(surface) = self.surface_id(surface) else {
            return;
        };
        let enabled = self
            .seats
            .get(&seat_id)
            .and_then(|objects| objects.text_input.as_ref())
            .is_some_and(|session| session.accepts_done(text_input, surface));
        if enabled {
            self.events.push(Event::TextInput(TextInputEvent::Done(
                batch.into_done(surface, serial),
            )));
        }
    }
}

delegate_registry!(RuntimeState);
smithay_client_toolkit::delegate_dispatch2!(RuntimeState);


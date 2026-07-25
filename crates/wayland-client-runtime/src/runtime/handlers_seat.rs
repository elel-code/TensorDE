impl SeatHandler for RuntimeState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.ensure_seat_data_device(qh, &seat);
        self.ensure_seat_text_input(qh, &seat);
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        let seat_id = seat.id().protocol_id();
        // SeatState binds seats already present in the initial registry without
        // invoking new_seat. Capability callbacks are therefore also an
        // initialization path for per-seat data devices.
        self.ensure_seat_data_device(qh, &seat);
        self.ensure_seat_text_input(qh, &seat);
        let objects = self.seats.entry(seat_id).or_default();
        match capability {
            Capability::Keyboard if objects.keyboard.is_none() => {
                objects.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
            }
            Capability::Pointer if objects.pointer.is_none() => {
                let cursor_surface = self.compositor.create_surface(qh);
                objects.pointer = self
                    .seat_state
                    .get_pointer_with_theme::<RuntimeState, ()>(
                        qh,
                        &seat,
                        self.shm.wl_shm(),
                        cursor_surface,
                        ThemeSpec::System,
                    )
                    .ok();
                if objects.pointer.is_some() {
                    objects.pointer_session.attach();
                    if self.pointer_gesture_subscriptions.is_active()
                        && let Some(manager) = self.pointer_gesture_manager.as_ref()
                    {
                        objects.ensure_pointer_gestures(manager, qh);
                    }
                }
            }
            Capability::Touch if objects.touch.is_none() => {
                objects.touch = Some(seat.get_touch(qh, TouchData::new(seat.clone())));
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        let mut cancelled_touch_surfaces = Vec::new();
        let Some(objects) = self.seats.get_mut(&seat.id().protocol_id()) else {
            return;
        };
        match capability {
            Capability::Keyboard => {
                if let Some(keyboard) = objects.keyboard.take() {
                    self.keyboard_focus.remove(&keyboard.id().protocol_id());
                    objects.keyboard_focus = None;
                    if keyboard.version() >= 3 {
                        keyboard.release();
                    }
                }
            }
            Capability::Pointer => {
                objects.pointer_gestures.take();
                objects.pointer_session.detach();
                objects.pointer.take();
                objects.pointer_presses.clear();
            }
            Capability::Touch => {
                if let Some(touch) = objects.touch.take()
                    && touch.version() >= 3
                {
                    touch.release();
                }
                cancelled_touch_surfaces = objects.touch_points.drain_surfaces();
            }
            _ => {}
        }
        for surface in cancelled_touch_surfaces {
            self.events.push(Event::Touch(TouchEvent {
                surface: Some(surface),
                kind: TouchEventKind::Cancelled,
            }));
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        let Some(mut objects) = self.seats.remove(&seat.id().protocol_id()) else {
            return;
        };
        for surface in objects.touch_points.drain_surfaces() {
            self.events.push(Event::Touch(TouchEvent {
                surface: Some(surface),
                kind: TouchEventKind::Cancelled,
            }));
        }
        if let Some(device) = objects.data_device.as_ref()
            && let Some(offer) = self.active_dnd_by_device.remove(&device.inner().id())
            && let Some(record) = self.incoming_dnd.remove(&offer)
        {
            record.offer.destroy();
        }
    }
}

impl RuntimeState {
    fn ensure_seat_text_input(
        &mut self,
        qh: &QueueHandle<Self>,
        seat: &wl_seat::WlSeat,
    ) {
        let seat_id = seat.id().protocol_id();
        if self
            .seats
            .get(&seat_id)
            .is_some_and(|objects| objects.text_input.is_some())
        {
            return;
        }
        let Some(manager) = self.text_input_manager.as_ref() else {
            return;
        };
        let text_input = manager.get_text_input(seat, qh);
        self.seats.entry(seat_id).or_default().text_input =
            Some(SeatTextInput::new(text_input));
    }

    fn ensure_seat_data_device(
        &mut self,
        qh: &QueueHandle<Self>,
        seat: &wl_seat::WlSeat,
    ) {
        let seat_id = seat.id().protocol_id();
        if self
            .seats
            .get(&seat_id)
            .is_none_or(|objects| objects.data_device.is_none())
        {
            let device = self.data_device_manager.get_data_device(qh, seat);
            self.seats.entry(seat_id).or_default().data_device = Some(device);
        }
    }
}

impl ShmHandler for RuntimeState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl PointerHandler for RuntimeState {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        pointer: &wl_pointer::WlPointer,
        events: &[SctkPointerEvent],
    ) {
        let Some(data) = pointer.data::<PointerData<()>>() else {
            return;
        };
        let seat = data.seat().clone();
        let seat_id = seat.id().protocol_id();
        for event in events {
            if let SctkPointerEventKind::Release { button, serial, .. } = &event.kind {
                // End the implicit grab even when the target surface was
                // destroyed earlier in this dispatch cycle.
                self.record_pointer_release(seat_id, *button, *serial);
            }
            let Some(surface) = self.surface_id(&event.surface) else {
                continue;
            };
            let kind = match &event.kind {
                SctkPointerEventKind::Enter { serial } => {
                    self.record_selection_serial(seat_id, *serial);
                    let capture = self
                        .surfaces
                        .get(&surface)
                        .map(|shared| {
                            shared
                                .pointer_capture
                                .lock()
                                .expect("surface pointer capture mutex poisoned")
                                .clone()
                        })
                        .unwrap_or_default();
                    let region = if capture.constraint == PointerConstraint::None {
                        Ok(None)
                    } else {
                        make_pointer_constraint_region(&self.compositor, &capture.region)
                    };
                    if let Some(objects) = self.seats.get_mut(&seat_id) {
                        match region {
                            Ok(region) => {
                                let _ = objects.pointer_session.enter(
                                    PointerCaptureTarget::new(
                                        surface,
                                        &event.surface,
                                        pointer,
                                        region.as_ref().map(Region::wl_region),
                                    ),
                                    &capture,
                                    &self.pointer_protocols,
                                    qh,
                                );
                            }
                            Err(_) => objects.pointer_session.focus_enter(surface),
                        }
                    }
                    PointerEventKind::Enter {
                        serial: InputSerial::new(
                            seat.clone(),
                            *serial,
                            InputSerialSource::PointerEnter,
                        ),
                    }
                }
                SctkPointerEventKind::Leave { .. } => {
                    if let Some(objects) = self.seats.get_mut(&seat_id) {
                        objects.pointer_session.leave(surface);
                    }
                    PointerEventKind::Leave
                }
                SctkPointerEventKind::Motion { time } => PointerEventKind::Motion { time: *time },
                SctkPointerEventKind::Press {
                    time,
                    button,
                    serial,
                } => {
                    self.record_pointer_press(seat_id, surface, *button, *serial);
                    PointerEventKind::Press {
                        time: *time,
                        button: *button,
                        serial: InputSerial::new(
                            seat.clone(),
                            *serial,
                            InputSerialSource::PointerPress,
                        ),
                    }
                }
                SctkPointerEventKind::Release {
                    time,
                    button,
                    serial,
                } => PointerEventKind::Release {
                    time: *time,
                    button: *button,
                    serial: InputSerial::new(
                        seat.clone(),
                        *serial,
                        InputSerialSource::PointerRelease,
                    ),
                },
                SctkPointerEventKind::Axis {
                    time,
                    horizontal,
                    vertical,
                    source,
                } => PointerEventKind::Axis {
                    time: *time,
                    horizontal: map_axis_value(*horizontal),
                    vertical: map_axis_value(*vertical),
                    source: map_axis_source(*source),
                },
            };
            self.events.push(Event::Pointer(PointerEvent {
                surface,
                position: event.position,
                kind,
            }));
        }
    }
}

impl RuntimeState {
    fn dispatch_touch_event(&mut self, seat: &wl_seat::WlSeat, event: wl_touch::Event) {
        match event {
            wl_touch::Event::Down {
                serial,
                time,
                surface,
                id,
                x,
                y,
            } => self.touch_down(seat, serial, time, surface, id, (x, y)),
            wl_touch::Event::Up { serial, time, id } => {
                self.touch_up(seat, serial, time, id)
            }
            wl_touch::Event::Motion { time, id, x, y } => {
                self.touch_motion(seat, time, id, (x, y))
            }
            wl_touch::Event::Shape { id, major, minor } => {
                self.touch_shape(seat, id, major, minor)
            }
            wl_touch::Event::Orientation { id, orientation } => {
                self.touch_orientation(seat, id, orientation)
            }
            _ => {}
        }
    }

    fn touch_down(
        &mut self,
        seat: &wl_seat::WlSeat,
        serial: u32,
        time: u32,
        surface: wl_surface::WlSurface,
        id: i32,
        position: (f64, f64),
    ) {
        let Some(surface) = self.surface_id(&surface) else {
            return;
        };
        let seat_id = seat.id().protocol_id();
        self.record_selection_serial(seat_id, serial);
        let Some(objects) = self.seats.get_mut(&seat_id) else {
            return;
        };
        objects.touch_points.insert(id, surface, serial);
        self.events.push(Event::Touch(TouchEvent {
            surface: Some(surface),
            kind: TouchEventKind::Down {
                time,
                id,
                position,
                serial: InputSerial::new(seat.clone(), serial, InputSerialSource::TouchDown),
            },
        }));
    }

    fn touch_up(
        &mut self,
        seat: &wl_seat::WlSeat,
        serial: u32,
        time: u32,
        id: i32,
    ) {
        let seat_id = seat.id().protocol_id();
        self.record_selection_serial(seat_id, serial);
        let surface = self
            .seats
            .get_mut(&seat_id)
            .and_then(|objects| objects.touch_points.remove(id));
        self.events.push(Event::Touch(TouchEvent {
            surface,
            kind: TouchEventKind::Up {
                time,
                id,
                serial: InputSerial::new(seat.clone(), serial, InputSerialSource::TouchUp),
            },
        }));
    }

    fn touch_motion(
        &mut self,
        seat: &wl_seat::WlSeat,
        time: u32,
        id: i32,
        position: (f64, f64),
    ) {
        let Some(surface) = self
            .seats
            .get(&seat.id().protocol_id())
            .and_then(|objects| objects.touch_points.surface(id))
        else {
            return;
        };
        self.events.push(Event::Touch(TouchEvent {
            surface: Some(surface),
            kind: TouchEventKind::Motion {
                time,
                id,
                position,
            },
        }));
    }

    fn touch_shape(
        &mut self,
        seat: &wl_seat::WlSeat,
        id: i32,
        major: f64,
        minor: f64,
    ) {
        let Some(surface) = self
            .seats
            .get(&seat.id().protocol_id())
            .and_then(|objects| objects.touch_points.surface(id))
        else {
            return;
        };
        self.events.push(Event::Touch(TouchEvent {
            surface: Some(surface),
            kind: TouchEventKind::Shape { id, major, minor },
        }));
    }

    fn touch_orientation(
        &mut self,
        seat: &wl_seat::WlSeat,
        id: i32,
        degrees: f64,
    ) {
        let Some(surface) = self
            .seats
            .get(&seat.id().protocol_id())
            .and_then(|objects| objects.touch_points.surface(id))
        else {
            return;
        };
        self.events.push(Event::Touch(TouchEvent {
            surface: Some(surface),
            kind: TouchEventKind::Orientation { id, degrees },
        }));
    }

    fn touch_cancel(&mut self, seat: &wl_seat::WlSeat) {
        let Some(objects) = self.seats.get_mut(&seat.id().protocol_id()) else {
            return;
        };
        let surfaces = objects.touch_points.drain_surfaces();
        if surfaces.is_empty() {
            self.events.push(Event::Touch(TouchEvent {
                surface: None,
                kind: TouchEventKind::Cancelled,
            }));
            return;
        }
        for surface in surfaces {
            self.events.push(Event::Touch(TouchEvent {
                surface: Some(surface),
                kind: TouchEventKind::Cancelled,
            }));
        }
    }
}

impl KeyboardHandler for RuntimeState {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        serial: u32,
        raw: &[u32],
        _: &[smithay_client_toolkit::seat::keyboard::Keysym],
    ) {
        let Some(surface) = self.surface_id(surface) else {
            return;
        };
        let Some(data) = keyboard.data::<KeyboardData<Self, ()>>() else {
            return;
        };
        self.keyboard_focus
            .insert(keyboard.id().protocol_id(), surface);
        let seat_id = data.seat().id().protocol_id();
        self.record_selection_serial(seat_id, serial);
        if let Some(objects) = self.seats.get_mut(&seat_id) {
            objects.keyboard_focus = Some(surface);
        }
        self.events.push(Event::Keyboard(KeyboardEvent::Enter {
            surface,
            serial: InputSerial::new(
                data.seat().clone(),
                serial,
                InputSerialSource::KeyboardEnter,
            ),
            pressed_raw_codes: raw.to_vec(),
        }));
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        let surface = self.surface_id(surface);
        self.keyboard_focus.remove(&keyboard.id().protocol_id());
        if let Some(data) = keyboard.data::<KeyboardData<Self, ()>>()
            && let Some(objects) = self.seats.get_mut(&data.seat().id().protocol_id())
            && objects.keyboard_focus == surface
        {
            objects.keyboard_focus = None;
        }
        if let Some(surface) = surface {
            self.events
                .push(Event::Keyboard(KeyboardEvent::Leave { surface }));
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.push_key(keyboard, KeyState::Pressed, serial, event);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.push_key(keyboard, KeyState::Repeated, serial, event);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.push_key(keyboard, KeyState::Released, serial, event);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        modifiers: SctkModifiers,
        _: RawModifiers,
        _: u32,
    ) {
        if let Some(data) = keyboard.data::<KeyboardData<Self, ()>>() {
            self.record_selection_serial(data.seat().id().protocol_id(), serial);
        }
        let Some(surface) = self
            .keyboard_focus
            .get(&keyboard.id().protocol_id())
            .copied()
        else {
            return;
        };
        self.events
            .push(Event::Keyboard(KeyboardEvent::Modifiers {
                surface,
                modifiers: Modifiers {
                    ctrl: modifiers.ctrl,
                    alt: modifiers.alt,
                    shift: modifiers.shift,
                    caps_lock: modifiers.caps_lock,
                    logo: modifiers.logo,
                    num_lock: modifiers.num_lock,
                },
            }));
    }
}

impl RuntimeState {
    fn drag_offer_for_device(&self, device: &wl_data_device::WlDataDevice) -> Option<DragOffer> {
        self.seats
            .values()
            .filter_map(|objects| objects.data_device.as_ref())
            .find(|candidate| candidate.inner() == device)
            .and_then(|device| device.data().drag_offer())
    }

    fn write_data_source(
        &self,
        source: &wl_data_source::WlDataSource,
        mime: &str,
        pipe: WritePipe,
    ) {
        let Some(bytes) = self
            .outgoing_dnd
            .get(&source.id())
            .map(|record| &record.content)
            .or_else(|| {
                self.selection_sources
                    .get(&source.id())
                    .map(|record| &record.content)
            })
            .and_then(|content| content.bytes_for_mime(mime))
        else {
            return;
        };
        crate::data_transfer::spawn_write_pipe("wayland-transfer-write", pipe, bytes);
    }
}


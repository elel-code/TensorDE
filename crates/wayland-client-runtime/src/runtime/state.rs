struct RuntimeState {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    background_effect_state: BackgroundEffectState,
    data_device_manager: DataDeviceManagerState,
    compositor: CompositorState,
    shm: Shm,
    xdg_shell: XdgShell,
    xdg_activation: Option<ActivationManager>,
    toplevel_icon_manager: Option<ToplevelIconManager>,
    layer_shell_manager: Option<LayerShellManager>,
    text_input_manager: Option<TextInputManager>,
    fractional_scale_manager: Option<FractionalScaleManager>,
    pointer_gesture_manager: Option<PointerGestureManager>,
    pointer_protocols: PointerProtocols,
    pointer_gesture_subscriptions: PointerGestureSubscriptions,
    surfaces: HashMap<SurfaceId, Arc<SurfaceShared>>,
    surface_ids: HashMap<ObjectId, SurfaceId>,
    children: HashMap<SurfaceId, Vec<SurfaceId>>,
    seats: HashMap<u32, SeatObjects>,
    keyboard_focus: HashMap<u32, SurfaceId>,
    incoming_dnd: HashMap<DndOfferId, IncomingDndOffer>,
    active_dnd_by_device: HashMap<ObjectId, DndOfferId>,
    outgoing_dnd: HashMap<ObjectId, OutgoingDndSource>,
    selection_sources: HashMap<ObjectId, SelectionSource>,
    pending_attention: HashSet<SurfaceId>,
    events: EventBuffer,
    next_surface_id: u64,
    next_dnd_id: u64,
    next_input_order: u64,
    next_activation_request_id: u64,
}

impl Drop for RuntimeState {
    fn drop(&mut self) {
        // Protocol leaves must disappear before the resources they reference:
        // data sources/offers before data devices, pointer constraints and text
        // inputs before their seats, and seat-scoped objects before surfaces.
        self.outgoing_dnd.clear();
        self.incoming_dnd.clear();
        self.selection_sources.clear();
        self.seats.clear();
        self.surfaces.clear();
    }
}

struct IncomingDndOffer {
    id: DndOfferId,
    offer: DragOffer,
    surface: SurfaceId,
}

struct OutgoingDndSource {
    id: DndSourceId,
    _source: DragSource,
    content: TransferContent,
    selected_action: Option<DndAction>,
    _icon: Option<DndIconSurface>,
}

struct SelectionSource {
    _source: CopyPasteSource,
    content: TransferContent,
}

struct DndIconSurface {
    surface: wl_surface::WlSurface,
    _buffer: ShmBuffer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SelectionSerial {
    serial: u32,
    order: u64,
}

impl Drop for DndIconSurface {
    fn drop(&mut self) {
        if self.surface.is_alive() {
            self.surface.destroy();
        }
    }
}

impl RuntimeState {
    fn surface_id(&self, surface: &wl_surface::WlSurface) -> Option<SurfaceId> {
        self.surface_ids.get(&surface.id()).copied()
    }

    fn remove_surface(&mut self, id: SurfaceId) {
        let Some(shared) = self.surfaces.remove(&id) else {
            return;
        };
        let gesture_change = self.pointer_gesture_subscriptions.remove_surface(id);
        self.surface_ids.remove(&shared.wl_surface().id());
        self.pending_attention.remove(&id);
        self.children.remove(&id);
        if let Some(parent) = shared.parent.as_ref()
            && let Some(children) = self.children.get_mut(&parent.id)
        {
            children.retain(|child| *child != id);
        }
        self.keyboard_focus.retain(|_, focused| *focused != id);
        for objects in self.seats.values_mut() {
            objects.pointer_session.remove_surface(id);
            match gesture_change {
                GestureSubscriptionChange::DetachSeats => {
                    objects.pointer_gestures.take();
                }
                GestureSubscriptionChange::Unchanged | GestureSubscriptionChange::KeepSeats => {
                    if let Some(gestures) = objects.pointer_gestures.as_ref() {
                        gestures.remove_surface(id);
                    }
                }
                GestureSubscriptionChange::AttachSeats => {
                    unreachable!("removing a gesture subscription cannot activate it")
                }
            }
            objects.pointer_presses.remove_surface(id);
            if objects.keyboard_focus == Some(id) {
                objects.keyboard_focus = None;
            }
            if let Some(text_input) = objects.text_input.as_mut() {
                text_input.remove_surface(id);
            }
            objects.touch_points.remove_surface(id);
        }
    }

    fn clear_pointer_gesture_surface(&self, surface: SurfaceId) {
        for objects in self.seats.values() {
            if let Some(gestures) = objects.pointer_gestures.as_ref() {
                gestures.remove_surface(surface);
            }
        }
    }

    fn record_pointer_press(&mut self, seat_id: u32, surface: SurfaceId, button: u32, serial: u32) {
        let order = self.take_input_order();
        if let Some(objects) = self.seats.get_mut(&seat_id) {
            objects
                .pointer_presses
                .press(button, surface, serial, order);
            objects.latest_selection_serial = Some(SelectionSerial { serial, order });
        }
    }

    fn record_pointer_release(&mut self, seat_id: u32, button: u32, serial: u32) {
        let order = self.take_input_order();
        if let Some(objects) = self.seats.get_mut(&seat_id) {
            objects.pointer_presses.release(button);
            objects.latest_selection_serial = Some(SelectionSerial { serial, order });
        }
    }

    fn record_selection_serial(&mut self, seat_id: u32, serial: u32) {
        let order = self.take_input_order();
        if let Some(objects) = self.seats.get_mut(&seat_id) {
            objects.latest_selection_serial = Some(SelectionSerial { serial, order });
        }
    }

    fn take_input_order(&mut self) -> u64 {
        let order = self.next_input_order;
        self.next_input_order = self.next_input_order.saturating_add(1);
        order
    }

    fn apply_pointer_gesture_subscription_change(
        &mut self,
        change: GestureSubscriptionChange,
        queue_handle: &QueueHandle<Self>,
    ) {
        match change {
            GestureSubscriptionChange::Unchanged | GestureSubscriptionChange::KeepSeats => {}
            GestureSubscriptionChange::AttachSeats => {
                let Some(manager) = self.pointer_gesture_manager.as_ref() else {
                    return;
                };
                for objects in self.seats.values_mut() {
                    objects.ensure_pointer_gestures(manager, queue_handle);
                }
            }
            GestureSubscriptionChange::DetachSeats => {
                for objects in self.seats.values_mut() {
                    objects.pointer_gestures.take();
                }
            }
        }
    }

    fn push_key(
        &mut self,
        keyboard: &wl_keyboard::WlKeyboard,
        state: KeyState,
        serial: u32,
        event: KeyEvent,
    ) {
        let keyboard_id = keyboard.id().protocol_id();
        let Some(surface) = self.keyboard_focus.get(&keyboard_id).copied() else {
            return;
        };
        let Some(data) = keyboard.data::<KeyboardData<Self, ()>>() else {
            return;
        };
        self.record_selection_serial(data.seat().id().protocol_id(), serial);
        let serial = InputSerial::new(data.seat().clone(), serial, InputSerialSource::KeyboardKey);
        self.events.push(Event::Keyboard(KeyboardEvent::Key {
            surface,
            state,
            time: event.time,
            raw_code: event.raw_code,
            keysym: event.keysym.raw(),
            text: event.utf8,
            serial,
        }));
    }
}

fn is_current_popup_grab(objects: &SeatObjects, source: InputSerialSource, serial: u32) -> bool {
    match source {
        InputSerialSource::PointerPress => objects.pointer_presses.contains_serial(serial),
        InputSerialSource::TouchDown => objects.touch_points.contains_serial(serial),
        _ => false,
    }
}

fn collect_post_order(
    children: &HashMap<SurfaceId, Vec<SurfaceId>>,
    id: SurfaceId,
    order: &mut Vec<SurfaceId>,
) {
    if let Some(direct_children) = children.get(&id) {
        for child in direct_children.iter().copied() {
            collect_post_order(children, child, order);
        }
    }
    order.push(id);
}

#[derive(Default)]
struct SeatObjects {
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<ThemedPointer>,
    pointer_gestures: Option<SeatPointerGestures>,
    touch: Option<wl_touch::WlTouch>,
    pointer_session: SeatPointerSession,
    text_input: Option<SeatTextInput>,
    data_device: Option<DataDevice>,
    pointer_presses: PointerPressTracker,
    latest_selection_serial: Option<SelectionSerial>,
    keyboard_focus: Option<SurfaceId>,
    touch_points: TouchPoints,
}

impl SeatObjects {
    fn has_focus(&self) -> bool {
        self.pointer_session.focus().is_some() || self.keyboard_focus.is_some()
    }

    fn ensure_pointer_gestures(
        &mut self,
        manager: &PointerGestureManager,
        queue_handle: &QueueHandle<RuntimeState>,
    ) {
        if self.pointer_gestures.is_some() {
            return;
        }
        let Some(pointer) = self.pointer.as_ref().map(ThemedPointer::pointer) else {
            return;
        };
        let Some(data) = pointer.data::<PointerData<()>>() else {
            return;
        };
        self.pointer_gestures =
            Some(manager.create_seat_gestures(pointer, data.seat(), queue_handle));
    }
}

impl Drop for SeatObjects {
    fn drop(&mut self) {
        if let Some(keyboard) = self.keyboard.take()
            && keyboard.version() >= 3
        {
            keyboard.release();
        }
        self.pointer_gestures.take();
        self.pointer_session.detach();
        self.pointer.take();
        if let Some(touch) = self.touch.take()
            && touch.version() >= 3
        {
            touch.release();
        }
        self.text_input.take();
    }
}


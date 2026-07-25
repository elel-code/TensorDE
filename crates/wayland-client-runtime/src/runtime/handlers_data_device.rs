impl DataDeviceHandler for RuntimeState {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        data_device: &wl_data_device::WlDataDevice,
        x: f64,
        y: f64,
        wl_surface: &wl_surface::WlSurface,
    ) {
        let Some(surface) = self.surface_id(wl_surface) else {
            return;
        };
        let Some(offer) = self.drag_offer_for_device(data_device) else {
            return;
        };
        let id = DndOfferId(self.next_dnd_id);
        self.next_dnd_id += 1;
        let mime_types = offer.with_mime_types(ToOwned::to_owned);
        let source_actions = dnd_actions(offer.source_actions);
        self.active_dnd_by_device.insert(data_device.id(), id);
        self.incoming_dnd.insert(
            id,
            IncomingDndOffer {
                id,
                offer,
                surface,
            },
        );
        self.events.push(Event::Dnd(DndEvent::Enter {
            offer: id,
            surface,
            position: LogicalPosition::new(x.round() as i32, y.round() as i32),
            mime_types,
            source_actions,
        }));
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        data_device: &wl_data_device::WlDataDevice,
    ) {
        let Some(id) = self.active_dnd_by_device.remove(&data_device.id()) else {
            return;
        };
        let Some(record) = self.incoming_dnd.get(&id) else {
            return;
        };
        let surface = record.surface;
        self.events.push(Event::Dnd(DndEvent::Leave {
            offer: id,
            surface,
        }));
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        data_device: &wl_data_device::WlDataDevice,
        x: f64,
        y: f64,
    ) {
        let Some(id) = self.active_dnd_by_device.get(&data_device.id()).copied() else {
            return;
        };
        let Some(record) = self.incoming_dnd.get(&id) else {
            return;
        };
        self.events.push(Event::Dnd(DndEvent::Motion {
            offer: id,
            surface: record.surface,
            position: LogicalPosition::new(x.round() as i32, y.round() as i32),
        }));
    }

    fn selection(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_device::WlDataDevice,
    ) {
    }

    fn drop_performed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        data_device: &wl_data_device::WlDataDevice,
    ) {
        let Some(id) = self.active_dnd_by_device.get(&data_device.id()).copied() else {
            return;
        };
        let Some(current) = self.drag_offer_for_device(data_device) else {
            return;
        };
        let Some(record) = self.incoming_dnd.get_mut(&id) else {
            return;
        };
        record.offer = current;
        self.events.push(Event::Dnd(DndEvent::Drop {
            offer: id,
            surface: record.surface,
            action: dnd_action(record.offer.selected_action),
        }));
    }
}

impl DataOfferHandler for RuntimeState {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: WlDndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: WlDndAction,
    ) {
    }
}

impl DataSourceHandler for RuntimeState {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_source::WlDataSource,
        _: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
        mime: String,
        pipe: WritePipe,
    ) {
        self.write_data_source(source, &mime, pipe);
    }

    fn cancelled(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
    ) {
        if self.selection_sources.remove(&source.id()).is_some() {
            return;
        }
        if let Some(record) = self.outgoing_dnd.remove(&source.id()) {
            self.events
                .push(Event::Dnd(DndEvent::SourceCancelled { source: record.id }));
        }
    }

    fn dnd_dropped(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
    ) {
        if let Some(record) = self.outgoing_dnd.get(&source.id()) {
            self.events.push(Event::Dnd(DndEvent::SourceDropped {
                source: record.id,
                action: record.selected_action,
            }));
        }
    }

    fn dnd_finished(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
    ) {
        if let Some(record) = self.outgoing_dnd.remove(&source.id()) {
            self.events
                .push(Event::Dnd(DndEvent::SourceFinished {
                    source: record.id,
                    action: record.selected_action,
                }));
        }
    }

    fn action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &wl_data_source::WlDataSource,
        action: WlDndAction,
    ) {
        if let Some(record) = self.outgoing_dnd.get_mut(&source.id()) {
            record.selected_action = dnd_action(action);
        }
    }
}

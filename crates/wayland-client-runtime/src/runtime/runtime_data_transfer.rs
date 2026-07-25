impl Runtime {
    /// Start a drag using the origin's focused pointer seat.
    ///
    /// Call this while handling the pointer gesture which activated the drag.
    /// The runtime owns the compositor serial and selects the newest matching
    /// seat, so applications do not need to retain protocol serials.
    pub fn start_drag(
        &mut self,
        origin: SurfaceId,
        content: TransferContent,
        actions: DndActions,
        icon: Option<DndIcon>,
    ) -> Result<DndSourceId, RuntimeError> {
        let origin_surface = self.surface_shared(origin)?;
        let candidates = self.state.seats.iter().map(|(seat_id, objects)| {
            (
                *seat_id,
                objects.pointer_session.focus(),
                objects.data_device.is_some(),
                objects.pointer_presses.latest_for_surface(origin),
            )
        });
        let (seat_id, press) = select_active_pointer_press(origin, candidates)
            .ok_or(RuntimeError::InvalidDragSerial)?;
        let serial = press.serial;
        let icon = icon
            .map(|icon| prepare_dnd_icon_surface(&mut self.state, &self.queue_handle, icon))
            .transpose()?;
        let data_device = self
            .state
            .seats
            .get(&seat_id)
            .and_then(|objects| objects.data_device.as_ref())
            .ok_or(RuntimeError::Unsupported("wl_data_device"))?;
        let source = self.state.data_device_manager.create_drag_and_drop_source(
            &self.queue_handle,
            content.mime_types(),
            map_dnd_actions(actions),
        );
        let id = DndSourceId(self.state.next_dnd_id);
        self.state.next_dnd_id += 1;
        source.start_drag(
            data_device,
            origin_surface.wl_surface(),
            icon.as_ref().map(|icon| &icon.surface),
            serial,
        );
        // Match winit #4571: on KDE, committing the icon before start_drag can
        // prevent its offset from taking effect.
        if let Some(icon) = icon.as_ref() {
            icon.surface.commit();
        }
        self.state.outgoing_dnd.insert(
            source.inner().id(),
            OutgoingDndSource {
                id,
                _source: source,
                content,
                selected_action: None,
                _icon: icon,
            },
        );
        Ok(id)
    }

    /// Make `content` the clipboard selection for the most recently active seat.
    pub fn store_selection(&mut self, content: TransferContent) -> Result<(), RuntimeError> {
        let (seat_id, serial) = select_selection_seat(self.state.seats.iter().map(
            |(seat_id, objects)| {
                (
                    *seat_id,
                    objects.has_focus(),
                    objects.data_device.is_some(),
                    objects.latest_selection_serial,
                )
            },
        ))
        .ok_or(RuntimeError::InvalidSelectionSerial)?;
        let data_device = self
            .state
            .seats
            .get(&seat_id)
            .and_then(|objects| objects.data_device.as_ref())
            .ok_or(RuntimeError::Unsupported("wl_data_device"))?;
        let source = self
            .state
            .data_device_manager
            .create_copy_paste_source(&self.queue_handle, content.mime_types());
        source.set_selection(data_device, serial);
        self.state.selection_sources.insert(
            source.inner().id(),
            SelectionSource {
                _source: source,
                content,
            },
        );
        Ok(())
    }

    /// Receive the first clipboard MIME type supported by the caller.
    pub fn receive_selection(
        &self,
        preferred_mimes: &[&str],
    ) -> Result<TransferReadPipe, RuntimeError> {
        let (_, data_device) = self
            .state
            .seats
            .iter()
            .filter(|(_, objects)| objects.has_focus())
            .filter_map(|(seat_id, objects)| {
                Some((
                    (objects.latest_selection_serial?.order, *seat_id),
                    objects.data_device.as_ref()?,
                ))
            })
            .max_by_key(|(key, _)| *key)
            .ok_or(RuntimeError::SelectionUnavailable)?;
        let selection = data_device
            .data()
            .selection_offer()
            .ok_or(RuntimeError::SelectionUnavailable)?;
        let mime = selection
            .with_mime_types(|offered| {
                preferred_mimes
                    .iter()
                    .find(|mime| offered.iter().any(|item| item == **mime))
                    .map(|mime| (*mime).to_string())
            })
            .ok_or(RuntimeError::SelectionMimeNotFound)?;
        selection
            .receive(mime.clone())
            .map(|pipe| TransferReadPipe::new(mime, pipe))
            .map_err(|error| RuntimeError::Protocol(error.to_string()))
    }

    pub fn set_dnd_offer_actions(
        &self,
        offer: DndOfferId,
        accepted_mime: Option<&str>,
        actions: DndActions,
        preferred: Option<DndAction>,
    ) -> Result<(), RuntimeError> {
        let offer = self
            .state
            .incoming_dnd
            .get(&offer)
            .ok_or(RuntimeError::DndOfferNotFound(offer))?;
        offer
            .offer
            .accept_mime_type(offer.offer.serial, accepted_mime.map(str::to_string));
        offer.offer.set_actions(
            map_dnd_actions(actions),
            preferred
                .map(map_dnd_action)
                .unwrap_or_else(WlDndAction::empty),
        );
        Ok(())
    }

    pub fn receive_dnd(
        &self,
        offer: DndOfferId,
        mime: impl Into<String>,
    ) -> Result<DndReadPipe, RuntimeError> {
        let offer = self
            .state
            .incoming_dnd
            .get(&offer)
            .ok_or(RuntimeError::DndOfferNotFound(offer))?;
        let mime = mime.into();
        offer
            .offer
            .receive(mime.clone())
            .map(|pipe| DndReadPipe::new(mime, pipe))
            .map_err(|error| RuntimeError::Protocol(error.to_string()))
    }

    pub fn finish_dnd_offer(&mut self, offer: DndOfferId) -> Result<(), RuntimeError> {
        let offer = self.take_dnd_offer(offer)?;
        offer.offer.finish();
        offer.offer.destroy();
        Ok(())
    }

    /// Discard an offer that left without a successful drop.
    ///
    /// This cleanup operation is idempotent because a compositor may queue a
    /// leave after the application has already finished the dropped offer.
    pub fn discard_dnd_offer(&mut self, offer: DndOfferId) -> Result<(), RuntimeError> {
        let offer = match self.take_dnd_offer(offer) {
            Ok(offer) => offer,
            Err(RuntimeError::DndOfferNotFound(_)) => return Ok(()),
            Err(error) => return Err(error),
        };
        offer.offer.destroy();
        Ok(())
    }

    fn take_dnd_offer(&mut self, offer: DndOfferId) -> Result<IncomingDndOffer, RuntimeError> {
        let offer = self
            .state
            .incoming_dnd
            .remove(&offer)
            .ok_or(RuntimeError::DndOfferNotFound(offer))?;
        self.state
            .active_dnd_by_device
            .retain(|_, active| *active != offer.id);
        Ok(offer)
    }
}

fn select_selection_seat(
    candidates: impl IntoIterator<Item = (u32, bool, bool, Option<SelectionSerial>)>,
) -> Option<(u32, u32)> {
    candidates
        .into_iter()
        .filter_map(|(seat_id, has_focus, has_data_device, input)| {
            let input = input?;
            (has_focus && has_data_device).then_some((seat_id, input))
        })
        .max_by_key(|(_, input)| input.order)
        .map(|(seat_id, input)| (seat_id, input.serial))
}

impl NativeShellState {
    pub(crate) fn alloc_id(&mut self) -> NativeSurfaceId {
        let id = NativeSurfaceId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    pub(crate) fn alloc_transfer_id(&mut self) -> u64 {
        let id = self.next_transfer_id;
        self.next_transfer_id = self.next_transfer_id.saturating_add(1);
        id
    }

    pub(crate) fn push(&mut self, event: NativeShellEvent) {
        self.events.push(event);
    }

    /// Drop protocol bookkeeping that references a destroyed content surface.
    ///
    /// Clears pending `wl_surface.frame` callbacks and presentation-feedback
    /// records so destroy does not leave stale object-id maps.
    pub(crate) fn clear_surface_protocol_state(&mut self, id: NativeSurfaceId) {
        self.frame_callbacks.retain(|_, surface| *surface != id);
        self.frame_pending.remove(&id);
        self.presentation_feedbacks
            .retain(|_, rec| rec.surface != id);
        self.presentation_pending.remove(&id);
    }

    pub(crate) fn is_presentation_pending(&self, id: NativeSurfaceId) -> bool {
        self.presentation_pending.contains(&id)
    }

    // Seat lifecycle helpers live in `seat.rs`.

    /// Borrow the content `wl_surface` for any role (toplevel / popup / layer).
    pub(crate) fn wl_surface(
        &self,
        id: NativeSurfaceId,
    ) -> Option<&wl_surface::WlSurface> {
        self.toplevels
            .get(&id)
            .map(|r| &r.wl)
            .or_else(|| self.popups.get(&id).map(|r| &r.wl))
            .or_else(|| self.layers.get(&id).map(|r| &r.wl))
    }

    /// Logical size tracked for the surface (configure / client set).
    pub(crate) fn logical_size(
        &self,
        id: NativeSurfaceId,
    ) -> Option<(u32, u32)> {
        if let Some(t) = self.toplevels.get(&id) {
            return Some((t.logical_w, t.logical_h));
        }
        if let Some(p) = self.popups.get(&id) {
            return Some((p.logical_w, p.logical_h));
        }
        if let Some(l) = self.layers.get(&id) {
            return Some((l.logical_w, l.logical_h));
        }
        None
    }

    /// Fractional / integer scale factor tracked for the surface.
    pub(crate) fn scale_factor(&self, id: NativeSurfaceId) -> Option<f64> {
        self.toplevels
            .get(&id)
            .map(|t| t.scale_factor)
            .or_else(|| self.layers.get(&id).map(|l| l.scale_factor))
            // Popups inherit parent scale; report 1.0 when mapped so callers
            // can treat any live surface uniformly.
            .or_else(|| self.popups.get(&id).map(|_| 1.0))
    }

    pub(crate) fn is_frame_pending(&self, id: NativeSurfaceId) -> bool {
        self.frame_pending.contains(&id)
    }
}


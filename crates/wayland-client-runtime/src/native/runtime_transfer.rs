//! Clipboard and DnD methods on [`NativeRuntime`].

use crate::data_transfer::{TransferContent, TransferReadPipe};
use crate::dnd::{DndAction, DndActions, DndOfferId, DndReadPipe, DndSourceId};
use crate::native::connection::NativeError;
use crate::runtime_common::RuntimeError;
use crate::surface::SurfaceId;

use super::runtime_facade::{NativeRuntime, map_native_error};

impl NativeRuntime {
    pub fn store_selection(&mut self, content: TransferContent) -> Result<(), RuntimeError> {
        self.store_selection_on_seat(content, None)
    }

    /// Store clipboard content on a specific seat (or primary when `None`).
    pub fn store_selection_on_seat(
        &mut self,
        content: TransferContent,
        seat: Option<crate::SeatId>,
    ) -> Result<(), RuntimeError> {
        self.shell
            .set_selection_content_on_seat(content, seat)
            .map_err(map_native_error)
    }

    pub fn receive_selection(
        &mut self,
        preferred_mimes: &[&str],
    ) -> Result<TransferReadPipe, RuntimeError> {
        // Returns a live pipe immediately; callers (clipboard worker threads)
        // perform the blocking read so the display loop can still handle
        // data-source Send events.
        self.shell
            .receive_selection_preferred_pipe(preferred_mimes)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("mime not found") => {
                    RuntimeError::SelectionMimeNotFound
                }
                NativeError::Protocol(msg) if msg.contains("no selection") => {
                    RuntimeError::SelectionUnavailable
                }
                other => map_native_error(other),
            })
    }

    /// Set primary selection only (middle-click paste buffer).
    pub fn store_primary_selection(
        &mut self,
        content: TransferContent,
    ) -> Result<(), RuntimeError> {
        self.store_primary_selection_on_seat(content, None)
    }

    /// Set primary selection on a specific seat (or primary when `None`).
    pub fn store_primary_selection_on_seat(
        &mut self,
        content: TransferContent,
        seat: Option<crate::SeatId>,
    ) -> Result<(), RuntimeError> {
        if !self.shell.has_primary_selection() {
            return Err(RuntimeError::Unsupported(
                "zwp_primary_selection_device_manager_v1",
            ));
        }
        self.shell
            .set_primary_selection_content_on_seat(content, seat)
            .map_err(map_native_error)
    }

    /// Receive primary selection (middle-click paste).
    pub fn receive_primary_selection(
        &mut self,
        preferred_mimes: &[&str],
    ) -> Result<TransferReadPipe, RuntimeError> {
        if !self.shell.has_primary_selection() {
            return Err(RuntimeError::Unsupported(
                "zwp_primary_selection_device_manager_v1",
            ));
        }
        self.shell
            .receive_primary_selection_preferred_pipe(preferred_mimes)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("mime not found") => {
                    RuntimeError::SelectionMimeNotFound
                }
                NativeError::Protocol(msg) if msg.contains("no primary") => {
                    RuntimeError::SelectionUnavailable
                }
                other => map_native_error(other),
            })
    }

    pub fn start_drag(
        &mut self,
        origin: SurfaceId,
        content: TransferContent,
        _actions: DndActions,
        icon: Option<crate::DndIcon>,
    ) -> Result<DndSourceId, RuntimeError> {
        self.start_drag_on_seat(origin, content, _actions, icon, None)
    }

    /// Start a drag using a specific seat's data device and serial.
    pub fn start_drag_on_seat(
        &mut self,
        origin: SurfaceId,
        content: TransferContent,
        _actions: DndActions,
        icon: Option<crate::DndIcon>,
        seat: Option<crate::SeatId>,
    ) -> Result<DndSourceId, RuntimeError> {
        let native = self.native(origin)?;
        let id = self
            .shell
            .start_drag_content_with_icon_on_seat(native, content, icon, seat)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("serial") => {
                    RuntimeError::InvalidDragSerial
                }
                other => map_native_error(other),
            })?;
        Ok(DndSourceId(id))
    }

    pub fn set_dnd_offer_actions(
        &mut self,
        offer: DndOfferId,
        accepted_mime: Option<&str>,
        actions: DndActions,
        preferred: Option<DndAction>,
    ) -> Result<(), RuntimeError> {
        if self.shell.dnd_offer_id() != Some(offer.get()) {
            return Err(RuntimeError::DndOfferNotFound(offer));
        }
        self.shell
            .set_dnd_actions(
                accepted_mime,
                actions.contains(DndActions::COPY),
                actions.contains(DndActions::MOVE),
                matches!(preferred, Some(DndAction::Copy) | None),
            )
            .map_err(map_native_error)
    }

    pub fn receive_dnd(
        &mut self,
        offer: DndOfferId,
        mime: impl Into<String>,
    ) -> Result<DndReadPipe, RuntimeError> {
        if self.shell.dnd_offer_id() != Some(offer.get()) {
            return Err(RuntimeError::DndOfferNotFound(offer));
        }
        let mime = mime.into();
        // Non-blocking setup; Fika reads the pipe on a worker thread.
        self.shell.receive_dnd_pipe(&mime).map_err(map_native_error)
    }

    pub fn finish_dnd_offer(&mut self, offer: DndOfferId) -> Result<(), RuntimeError> {
        if self.shell.dnd_offer_id() != Some(offer.get()) {
            // Idempotent: leave-after-drop, double-finish, or a superseded offer.
            return Ok(());
        }
        self.shell.finish_dnd().map_err(map_native_error)
    }

    pub fn discard_dnd_offer(&mut self, offer: DndOfferId) -> Result<(), RuntimeError> {
        if self.shell.dnd_offer_id() != Some(offer.get()) {
            // Idempotent: leave may race with finish/discard.
            return Ok(());
        }
        self.shell.discard_dnd().map_err(map_native_error)
    }
}

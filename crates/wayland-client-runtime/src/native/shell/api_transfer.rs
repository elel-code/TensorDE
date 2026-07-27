//! Clipboard, drag-and-drop, and text-input methods for [`NativeShell`].

use std::sync::Arc;

use wayland_client::Proxy;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    self, ContentHint as WireContentHint, ContentPurpose as WireContentPurpose,
};

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::data_transfer::TransferContent;
use crate::native::connection::NativeError;
use crate::native::protocols::core::shm;
use crate::text_input::{
    TextInputChangeCause, TextInputContentHint, TextInputContentPurpose, TextInputState,
};

impl NativeShell {
    pub fn has_text_input(&self) -> bool {
        self.state.text_input.is_some()
    }

    /// Enable text-input-v3 and push the full client editor state.
    ///
    /// Applies surrounding text, content type, change cause, and cursor
    /// rectangle (surface-local logical coordinates). The hard-coded
    /// `(0,0,1,1)` placeholder used during early native bring-up is gone —
    /// compositors were docking the IME popup at the surface origin.
    pub fn set_text_input_state(
        &mut self,
        surface: NativeSurfaceId,
        state: &TextInputState,
    ) -> Result<(), NativeError> {
        let ti = self
            .state
            .text_input
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("text_input_v3 missing".into()))?
            .clone();
        let wl = self
            .state
            .toplevels
            .get(&surface)
            .map(|t| t.wl.clone())
            .or_else(|| self.state.popups.get(&surface).map(|p| p.wl.clone()))
            .or_else(|| self.state.layers.get(&surface).map(|l| l.wl.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {surface:?}")))?;

        ti.enable();
        self.state.text_input_surface = Some(surface);

        if let Some(surrounding) = state.surrounding_text() {
            ti.set_surrounding_text(
                surrounding.text().to_string(),
                surrounding.cursor() as i32,
                surrounding.anchor() as i32,
            );
        }
        ti.set_text_change_cause(match state.change_cause() {
            TextInputChangeCause::InputMethod => zwp_text_input_v3::ChangeCause::InputMethod,
            TextInputChangeCause::Other => zwp_text_input_v3::ChangeCause::Other,
        });
        if let Some(content) = state.content_type() {
            ti.set_content_type(
                content_hint_to_wire(content.hints),
                content_purpose_to_wire(content.purpose),
            );
        }
        if let Some(rect) = state.cursor_rectangle() {
            ti.set_cursor_rectangle(
                rect.origin.x,
                rect.origin.y,
                rect.size.width.max(1) as i32,
                rect.size.height.max(1) as i32,
            );
        }

        // Double-buffered: text-input commit applies state; on protocol v2+
        // the cursor rectangle is further applied on the next wl_surface.commit.
        ti.commit();
        wl.commit();
        self.connection.mark_dirty();
        Ok(())
    }

    /// Enable text-input-v3 on `surface` without editor state (legacy helper).
    pub fn enable_text_input(&mut self, surface: NativeSurfaceId) -> Result<(), NativeError> {
        self.set_text_input_state(surface, &TextInputState::new())
    }

    /// Disable text-input-v3 for the seat.
    pub fn disable_text_input(&mut self) -> Result<(), NativeError> {
        let ti = self
            .state
            .text_input
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("text_input_v3 missing".into()))?;
        ti.disable();
        ti.commit();
        self.state.text_input_surface = None;
        self.connection.mark_dirty();
        Ok(())
    }

    /// MIME types on the current drag offer.
    pub fn dnd_mimes(&self) -> &[String] {
        &self.state.dnd_mimes
    }

    pub fn dnd_offer_id(&self) -> Option<u64> {
        self.state.dnd_offer_id
    }

    /// Accept a mime on the current drag (call after [`NativeShellEvent::DndEnter`]).
    pub fn accept_dnd(&mut self, mime: Option<&str>) -> Result<(), NativeError> {
        let serial = self
            .state
            .dnd_serial
            .ok_or_else(|| NativeError::Protocol("no dnd serial".into()))?;
        let offer = self
            .state
            .dnd_offer
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("no dnd offer".into()))?;
        offer.accept(serial, mime.map(str::to_string));
        self.connection.mark_dirty();
        Ok(())
    }

    /// Set source/preferred actions on the current drag offer.
    pub fn set_dnd_actions(
        &mut self,
        accepted_mime: Option<&str>,
        copy: bool,
        move_action: bool,
        prefer_copy: bool,
    ) -> Result<(), NativeError> {
        let serial = self
            .state
            .dnd_serial
            .ok_or_else(|| NativeError::Protocol("no dnd serial".into()))?;
        let offer = self
            .state
            .dnd_offer
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("no dnd offer".into()))?;
        offer.accept(serial, accepted_mime.map(str::to_string));
        use wayland_client::protocol::wl_data_device_manager::DndAction;
        let mut source = DndAction::empty();
        if copy {
            source |= DndAction::Copy;
        }
        if move_action {
            source |= DndAction::Move;
        }
        if source.is_empty() {
            source = DndAction::Copy;
        }
        let preferred = if prefer_copy && source.contains(DndAction::Copy) {
            DndAction::Copy
        } else if source.contains(DndAction::Move) {
            DndAction::Move
        } else {
            DndAction::Copy
        };
        offer.set_actions(source, preferred);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Begin a drag-offer receive; returns a pipe the caller must read **off**
    /// the display thread (source `Send` needs the event loop to keep running).
    pub fn receive_dnd_pipe(
        &mut self,
        mime: &str,
    ) -> Result<crate::data_transfer::TransferReadPipe, NativeError> {
        use std::os::fd::AsFd;
        use std::os::unix::net::UnixStream;

        let offer = self
            .state
            .dnd_offer
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("no dnd offer".into()))?;
        if !self.state.dnd_mimes.iter().any(|m| m == mime) {
            return Err(NativeError::Protocol(format!("dnd has no mime {mime}")));
        }
        let (reader, writer) = UnixStream::pair().map_err(NativeError::from)?;
        // Keep the write end blocking for the source; reader may block off-thread.
        writer.set_nonblocking(false).ok();
        reader.set_nonblocking(false).ok();
        // One owned String for protocol + pipe metadata.
        let mime = mime.to_string();
        offer.receive(mime.clone(), writer.as_fd());
        drop(writer);
        self.connection.flush()?;
        let _ = self.dispatch_pending();
        Ok(crate::data_transfer::TransferReadPipe::from_stream(mime, reader))
    }

    /// Receive drag-offer bytes (blocks). Prefer [`Self::receive_dnd_pipe`] on
    /// the UI thread and read the pipe in a worker.
    pub fn receive_dnd(&mut self, mime: &str) -> Result<Vec<u8>, NativeError> {
        use std::io::Read;
        let mut pipe = self.receive_dnd_pipe(mime)?;
        let mut buf = Vec::new();
        pipe.read_to_end(&mut buf)
            .map_err(|e| NativeError::Io(e.to_string()))?;
        Ok(buf)
    }

    /// Finish and destroy the current drag offer after a successful drop transfer.
    pub fn finish_dnd(&mut self) -> Result<(), NativeError> {
        if let Some(offer) = self.state.dnd_offer.take() {
            let old_id = offer.id().protocol_id();
            self.state.offer_mimes.remove(&old_id);
            // Protocol: finish then destroy. Only valid after drop.
            offer.finish();
            offer.destroy();
        }
        self.state.dnd_offer_id = None;
        self.state.dnd_dropped = false;
        self.state.dnd_mimes.clear();
        self.state.dnd_focus = None;
        self.state.dnd_serial = None;
        self.connection.mark_dirty();
        Ok(())
    }

    /// Discard the current drag offer without finishing (leave / cancel).
    pub fn discard_dnd(&mut self) -> Result<(), NativeError> {
        if let Some(offer) = self.state.dnd_offer.take() {
            let old_id = offer.id().protocol_id();
            self.state.offer_mimes.remove(&old_id);
            offer.destroy();
        }
        self.state.dnd_offer_id = None;
        self.state.dnd_dropped = false;
        self.state.dnd_mimes.clear();
        self.state.dnd_focus = None;
        self.state.dnd_serial = None;
        self.connection.mark_dirty();
        Ok(())
    }

    /// Start an outgoing drag with multi-mime content and optional drag icon.
    pub fn start_drag_content(
        &mut self,
        origin: NativeSurfaceId,
        content: TransferContent,
    ) -> Result<u64, NativeError> {
        self.start_drag_content_with_icon(origin, content, None)
    }

    pub fn start_drag_content_with_icon(
        &mut self,
        origin: NativeSurfaceId,
        content: TransferContent,
        icon: Option<crate::DndIcon>,
    ) -> Result<u64, NativeError> {
        self.start_drag_content_with_icon_on_seat(origin, content, icon, None)
    }

    /// Start a drag on a specific seat (or the primary seat when `seat` is `None`).
    ///
    /// Uses that seat's `wl_data_device` and serial when available; falls back to
    /// shell-wide primary fields for single-seat clients.
    pub fn start_drag_content_with_icon_on_seat(
        &mut self,
        origin: NativeSurfaceId,
        content: TransferContent,
        icon: Option<crate::DndIcon>,
        seat: Option<crate::SeatId>,
    ) -> Result<u64, NativeError> {
        let (serial, device) = self.transfer_serial_and_data_device(seat, "start_drag")?;
        let origin_wl = self
            .state
            .toplevels
            .get(&origin)
            .map(|t| t.wl.clone())
            .or_else(|| self.state.popups.get(&origin).map(|p| p.wl.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {origin:?}")))?;
        let qh = self.queue.handle();
        if let Some(old) = self.state.dnd_source.take() {
            old.destroy();
        }
        // Drop previous icon before creating a new one.
        self.state.dnd_icon = None;

        let icon_surface = match icon {
            Some(icon) => Some(prepare_native_dnd_icon(self, &qh, icon)?),
            None => None,
        };

        let manager = self
            .state
            .data_device_manager
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("wl_data_device_manager missing".into()))?;
        let source = manager.create_data_source(&qh, ());
        for mime in content.mime_types() {
            source.offer(mime.to_string());
        }
        source.set_actions(
            wayland_client::protocol::wl_data_device_manager::DndAction::Copy
                | wayland_client::protocol::wl_data_device_manager::DndAction::Move,
        );
        let icon_wl = icon_surface.as_ref().map(|i| &i.wl);
        device.start_drag(Some(&source), &origin_wl, icon_wl, serial);
        // Match winit/KDE: commit icon after start_drag so offset applies.
        if let Some(icon) = icon_surface.as_ref() {
            icon.wl.commit();
        }
        let id = self.state.alloc_transfer_id();
        self.state.dnd_source = Some(source);
        self.state.dnd_source_id = Some(id);
        self.state.dnd_source_content = Some(content);
        self.state.dnd_icon = icon_surface;
        self.connection.mark_dirty();
        Ok(id)
    }

    /// Start an outgoing drag with text payload (requires recent pointer serial).
    pub fn start_drag_text(
        &mut self,
        origin: NativeSurfaceId,
        text: impl Into<String>,
    ) -> Result<u64, NativeError> {
        self.start_drag_content(origin, TransferContent::text(text.into()))
    }

    pub fn start_drag_bytes(
        &mut self,
        origin: NativeSurfaceId,
        bytes: Arc<[u8]>,
        mimes: &[&str],
    ) -> Result<u64, NativeError> {
        let payloads = mimes
            .iter()
            .map(|mime| {
                crate::data_transfer::MimePayload::new(*mime, bytes.clone())
                    .map_err(|e| NativeError::Protocol(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let content = TransferContent::new(payloads)
            .map_err(|e| NativeError::Protocol(e.to_string()))?;
        self.start_drag_content(origin, content)
    }

    /// Advertise multi-mime content on the clipboard.
    ///
    /// When primary selection is available, the same content is dual-written so
    /// middle-click paste sees the latest copy (common desktop expectation).
    pub fn set_selection_content(
        &mut self,
        content: TransferContent,
    ) -> Result<(), NativeError> {
        self.set_selection_content_on_seat(content, None)
    }

    /// Advertise clipboard content on a specific seat (or primary when `None`).
    pub fn set_selection_content_on_seat(
        &mut self,
        content: TransferContent,
        seat: Option<crate::SeatId>,
    ) -> Result<(), NativeError> {
        let (serial, device) = self.transfer_serial_and_data_device(seat, "set_selection")?;
        let manager = self
            .state
            .data_device_manager
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("wl_data_device_manager missing".into()))?;
        let qh = self.queue.handle();
        if let Some(old) = self.state.selection_source.take() {
            old.destroy();
        }
        let source = manager.create_data_source(&qh, ());
        for mime in content.mime_types() {
            source.offer(mime.to_string());
        }
        device.set_selection(Some(&source), serial);
        self.state.selection_source = Some(source);
        self.state.selection_content = Some(content.clone());
        // Dual-write primary selection when the global exists (SCTK apps often
        // keep both selections in sync for middle-click paste).
        let _ = self.set_primary_selection_content_inner(content, serial, seat);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Set primary selection only (does not touch the regular clipboard).
    pub fn set_primary_selection_content(
        &mut self,
        content: TransferContent,
    ) -> Result<(), NativeError> {
        self.set_primary_selection_content_on_seat(content, None)
    }

    /// Set primary selection on a specific seat (or primary when `None`).
    pub fn set_primary_selection_content_on_seat(
        &mut self,
        content: TransferContent,
        seat: Option<crate::SeatId>,
    ) -> Result<(), NativeError> {
        let serial = self
            .transfer_serial(seat)
            .ok_or_else(|| NativeError::Protocol("no input serial for primary selection".into()))?;
        self.set_primary_selection_content_inner(content, serial, seat)?;
        self.connection.mark_dirty();
        Ok(())
    }

    /// Resolve serial + data device for transfer APIs.
    fn transfer_serial_and_data_device(
        &self,
        seat: Option<crate::SeatId>,
        op: &str,
    ) -> Result<(u32, wayland_client::protocol::wl_data_device::WlDataDevice), NativeError> {
        let serial = self.transfer_serial(seat).ok_or_else(|| {
            NativeError::Protocol(format!("no input serial for {op}"))
        })?;
        let device = self
            .transfer_data_device(seat)
            .ok_or_else(|| NativeError::Protocol("wl_data_device missing".into()))?;
        Ok((serial, device))
    }

    fn transfer_serial(&self, seat: Option<crate::SeatId>) -> Option<u32> {
        if let Some(id) = seat {
            self.state
                .seats
                .get(&id.get())
                .and_then(|s| s.last_input_serial)
                .or(self.state.last_input_serial)
        } else {
            self.state.last_input_serial
        }
    }

    fn transfer_data_device(
        &self,
        seat: Option<crate::SeatId>,
    ) -> Option<wayland_client::protocol::wl_data_device::WlDataDevice> {
        if let Some(id) = seat {
            if let Some(dev) = self
                .state
                .seats
                .get(&id.get())
                .and_then(|s| s.data_device.clone())
            {
                return Some(dev);
            }
        }
        self.state.data_device.clone()
    }

    fn transfer_primary_device(
        &self,
        seat: Option<crate::SeatId>,
    ) -> Option<
        wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
    > {
        if let Some(id) = seat {
            if let Some(dev) = self
                .state
                .seats
                .get(&id.get())
                .and_then(|s| s.primary_device.clone())
            {
                return Some(dev);
            }
        }
        self.state.primary_device.clone()
    }

    fn set_primary_selection_content_inner(
        &mut self,
        content: TransferContent,
        serial: u32,
        seat: Option<crate::SeatId>,
    ) -> Result<(), NativeError> {
        let Some(manager) = self.state.primary_selection_manager.as_ref() else {
            return Ok(());
        };
        let Some(device) = self.transfer_primary_device(seat) else {
            return Ok(());
        };
        let qh = self.queue.handle();
        if let Some(old) = self.state.primary_source.take() {
            old.destroy();
        }
        let source = manager.create_source(&qh, ());
        for mime in content.mime_types() {
            source.offer(mime.to_string());
        }
        device.set_selection(Some(&source), serial);
        self.state.primary_source = Some(source);
        self.state.primary_content = Some(content);
        Ok(())
    }

    /// MIME types on the current primary selection offer.
    pub fn primary_selection_mimes(&self) -> &[String] {
        &self.state.primary_mimes
    }

    /// Begin a primary-selection receive (read off the display thread).
    pub fn receive_primary_selection_pipe(
        &mut self,
        mime: &str,
    ) -> Result<crate::data_transfer::TransferReadPipe, NativeError> {
        use std::os::fd::AsFd;
        use std::os::unix::net::UnixStream;

        let offer = self
            .state
            .primary_offer
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("no primary selection offer".into()))?;
        if !self.state.primary_mimes.iter().any(|m| m == mime) {
            return Err(NativeError::Protocol(format!(
                "primary selection has no mime {mime}"
            )));
        }
        let (reader, writer) = UnixStream::pair().map_err(NativeError::from)?;
        writer.set_nonblocking(false).ok();
        reader.set_nonblocking(false).ok();
        let mime = mime.to_string();
        offer.receive(mime.clone(), writer.as_fd());
        drop(writer);
        self.connection.flush()?;
        let _ = self.dispatch_pending();
        Ok(crate::data_transfer::TransferReadPipe::from_stream(mime, reader))
    }

    /// Open a pipe for the first preferred mime from primary selection.
    pub fn receive_primary_selection_preferred_pipe(
        &mut self,
        preferred_mimes: &[&str],
    ) -> Result<crate::data_transfer::TransferReadPipe, NativeError> {
        let mime = preferred_mimes
            .iter()
            .find(|m| self.state.primary_mimes.iter().any(|offered| offered == *m))
            .ok_or_else(|| NativeError::Protocol("primary selection mime not found".into()))?;
        self.receive_primary_selection_pipe(mime)
    }

    /// Advertise UTF-8 text on the clipboard (requires a recent input serial).
    pub fn set_selection_text(&mut self, text: impl Into<String>) -> Result<(), NativeError> {
        self.set_selection_content(TransferContent::text(text.into()))
    }

    pub fn set_selection_bytes(
        &mut self,
        bytes: Arc<[u8]>,
        mimes: &[&str],
    ) -> Result<(), NativeError> {
        let payloads = mimes
            .iter()
            .map(|mime| {
                crate::data_transfer::MimePayload::new(*mime, bytes.clone())
                    .map_err(|e| NativeError::Protocol(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let content = TransferContent::new(payloads)
            .map_err(|e| NativeError::Protocol(e.to_string()))?;
        self.set_selection_content(content)
    }

    /// MIME types advertised by the current incoming selection, if any.
    pub fn selection_mimes(&self) -> &[String] {
        &self.state.incoming_mimes
    }

    /// Whether a keymap was applied via `wl_keyboard.keymap` (xkb ready).
    pub fn has_xkb(&self) -> bool {
        self.state.xkb.is_some()
    }

    /// Begin a clipboard receive; returns a pipe read **off** the display thread.
    pub fn receive_selection_pipe(
        &mut self,
        mime: &str,
    ) -> Result<crate::data_transfer::TransferReadPipe, NativeError> {
        use std::os::fd::AsFd;
        use std::os::unix::net::UnixStream;

        let offer = self
            .state
            .incoming_offer
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("no selection offer".into()))?;
        if !self.state.incoming_mimes.iter().any(|m| m == mime) {
            return Err(NativeError::Protocol(format!(
                "selection has no mime {mime}"
            )));
        }
        let (reader, writer) = UnixStream::pair().map_err(NativeError::from)?;
        writer.set_nonblocking(false).ok();
        reader.set_nonblocking(false).ok();
        let mime = mime.to_string();
        offer.receive(mime.clone(), writer.as_fd());
        drop(writer);
        self.connection.flush()?;
        let _ = self.dispatch_pending();
        Ok(crate::data_transfer::TransferReadPipe::from_stream(mime, reader))
    }

    /// Receive the current selection as bytes for `mime` (blocks on the pipe).
    pub fn receive_selection(&mut self, mime: &str) -> Result<Vec<u8>, NativeError> {
        use std::io::Read;
        let mut pipe = self.receive_selection_pipe(mime)?;
        let mut buf = Vec::new();
        pipe.read_to_end(&mut buf)
            .map_err(|e| NativeError::Io(e.to_string()))?;
        Ok(buf)
    }
}

fn content_hint_to_wire(hints: TextInputContentHint) -> WireContentHint {
    let mut wire = WireContentHint::empty();
    if hints.contains(TextInputContentHint::COMPLETION) {
        wire |= WireContentHint::Completion;
    }
    if hints.contains(TextInputContentHint::SPELLCHECK) {
        wire |= WireContentHint::Spellcheck;
    }
    if hints.contains(TextInputContentHint::AUTO_CAPITALIZATION) {
        wire |= WireContentHint::AutoCapitalization;
    }
    if hints.contains(TextInputContentHint::LOWERCASE) {
        wire |= WireContentHint::Lowercase;
    }
    if hints.contains(TextInputContentHint::UPPERCASE) {
        wire |= WireContentHint::Uppercase;
    }
    if hints.contains(TextInputContentHint::TITLECASE) {
        wire |= WireContentHint::Titlecase;
    }
    if hints.contains(TextInputContentHint::HIDDEN_TEXT) {
        wire |= WireContentHint::HiddenText;
    }
    if hints.contains(TextInputContentHint::SENSITIVE_DATA) {
        wire |= WireContentHint::SensitiveData;
    }
    if hints.contains(TextInputContentHint::LATIN) {
        wire |= WireContentHint::Latin;
    }
    if hints.contains(TextInputContentHint::MULTILINE) {
        wire |= WireContentHint::Multiline;
    }
    wire
}

fn content_purpose_to_wire(purpose: TextInputContentPurpose) -> WireContentPurpose {
    match purpose {
        TextInputContentPurpose::Normal => WireContentPurpose::Normal,
        TextInputContentPurpose::Alpha => WireContentPurpose::Alpha,
        TextInputContentPurpose::Digits => WireContentPurpose::Digits,
        TextInputContentPurpose::Number => WireContentPurpose::Number,
        TextInputContentPurpose::Phone => WireContentPurpose::Phone,
        TextInputContentPurpose::Url => WireContentPurpose::Url,
        TextInputContentPurpose::Email => WireContentPurpose::Email,
        TextInputContentPurpose::Name => WireContentPurpose::Name,
        TextInputContentPurpose::Password => WireContentPurpose::Password,
        TextInputContentPurpose::Pin => WireContentPurpose::Pin,
        TextInputContentPurpose::Date => WireContentPurpose::Date,
        TextInputContentPurpose::Time => WireContentPurpose::Time,
        TextInputContentPurpose::DateTime => WireContentPurpose::Datetime,
        TextInputContentPurpose::Terminal => WireContentPurpose::Terminal,
    }
}

impl NativeShell {
    /// Open a pipe for the first preferred mime from the current selection.
    pub fn receive_selection_preferred_pipe(
        &mut self,
        preferred_mimes: &[&str],
    ) -> Result<crate::data_transfer::TransferReadPipe, NativeError> {
        let mime = preferred_mimes
            .iter()
            .find(|m| self.state.incoming_mimes.iter().any(|offered| offered == *m))
            .ok_or_else(|| NativeError::Protocol("selection mime not found".into()))?;
        self.receive_selection_pipe(mime)
    }

    /// Receive the first preferred mime from the current selection (blocks).
    pub fn receive_selection_preferred(
        &mut self,
        preferred_mimes: &[&str],
    ) -> Result<(String, Vec<u8>), NativeError> {
        use std::io::Read;
        let mut pipe = self.receive_selection_preferred_pipe(preferred_mimes)?;
        let mime = pipe.mime().to_string();
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)
            .map_err(|e| NativeError::Io(e.to_string()))?;
        Ok((mime, bytes))
    }
}

fn prepare_native_dnd_icon(
    shell: &mut NativeShell,
    qh: &wayland_client::QueueHandle<super::types::NativeShellState>,
    icon: crate::DndIcon,
) -> Result<super::types::NativeDndIconSurface, NativeError> {
    use wayland_client::Proxy;

    let (rgba, width, height, buffer_scale, offset) = icon.into_parts();
    let compositor = shell
        .state
        .compositor
        .as_ref()
        .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?;
    let shm = shell
        .state
        .shm
        .as_ref()
        .ok_or_else(|| NativeError::Registry("wl_shm".into()))?;
    let (file, pool, buffer) =
        shm::create_rgba_buffer(shm, qh, width, height, &rgba).map_err(|e| {
            NativeError::Io(e.to_string())
        })?;
    let wl = compositor.create_surface(qh, ());
    wl.set_buffer_scale(buffer_scale.max(1));
    if offset != crate::geometry::LogicalPosition::ZERO {
        if wl.version() >= 5 {
            wl.offset(offset.x, offset.y);
            wl.attach(Some(&buffer), 0, 0);
        } else {
            wl.attach(Some(&buffer), offset.x, offset.y);
        }
    } else {
        wl.attach(Some(&buffer), 0, 0);
    }
    wl.damage_buffer(0, 0, i32::MAX, i32::MAX);
    Ok(super::types::NativeDndIconSurface {
        wl,
        buffer,
        pool,
        _file: file,
    })
}

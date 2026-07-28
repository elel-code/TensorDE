//! Clipboard and primary-selection methods for [`NativeShell`].

use std::sync::Arc;

use super::api::NativeShell;
use crate::data_transfer::TransferContent;
use crate::native::connection::NativeError;

impl NativeShell {
    /// Advertise multi-mime content on the clipboard.
    ///
    /// When primary selection is available, the same content is dual-written so
    /// middle-click paste sees the latest copy (common desktop expectation).
    pub fn set_selection_content(&mut self, content: TransferContent) -> Result<(), NativeError> {
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
    pub(crate) fn transfer_serial_and_data_device(
        &self,
        seat: Option<crate::SeatId>,
        op: &str,
    ) -> Result<(u32, wayland_client::protocol::wl_data_device::WlDataDevice), NativeError> {
        let serial = self
            .transfer_serial(seat)
            .ok_or_else(|| NativeError::Protocol(format!("no input serial for {op}")))?;
        let device = self
            .transfer_data_device(seat)
            .ok_or_else(|| NativeError::Protocol("wl_data_device missing".into()))?;
        Ok((serial, device))
    }

    fn transfer_serial(&self, seat: Option<crate::SeatId>) -> Option<u32> {
        if let Some(id) = seat {
            return self
                .state
                .seats
                .get(&id.get())
                .and_then(|s| s.last_input_serial)
                .or(self.state.last_input_serial);
        }
        // Match grab resolution: prefer the seat that owns the last-wins serial.
        if let Some(serial) = self.state.last_input_serial {
            for rec in self.state.seats.values() {
                if rec.last_input_serial == Some(serial) {
                    return Some(serial);
                }
            }
            return Some(serial);
        }
        None
    }

    /// Seat whose serial is used for transfer when `seat` is `None`.
    fn transfer_seat_global(&self, seat: Option<crate::SeatId>) -> Option<u32> {
        if let Some(id) = seat {
            return Some(id.get());
        }
        let serial = self.state.last_input_serial?;
        self.state
            .seats
            .values()
            .find(|rec| rec.last_input_serial == Some(serial))
            .map(|rec| rec.global_name)
            .or_else(|| self.primary_seat_id().map(|id| id.get()))
    }

    fn transfer_data_device(
        &self,
        seat: Option<crate::SeatId>,
    ) -> Option<wayland_client::protocol::wl_data_device::WlDataDevice> {
        if let Some(global) = self.transfer_seat_global(seat)
            && let Some(dev) = self
                .state
                .seats
                .get(&global)
                .and_then(|s| s.data_device.clone())
        {
            return Some(dev);
        }
        self.state.data_device.clone()
    }

    fn transfer_primary_device(
        &self,
        seat: Option<crate::SeatId>,
    ) -> Option<
        wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
    >{
        if let Some(global) = self.transfer_seat_global(seat)
            && let Some(dev) = self
                .state
                .seats
                .get(&global)
                .and_then(|s| s.primary_device.clone())
        {
            return Some(dev);
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
        Ok(crate::data_transfer::TransferReadPipe::from_stream(
            mime, reader,
        ))
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
        let content =
            TransferContent::new(payloads).map_err(|e| NativeError::Protocol(e.to_string()))?;
        self.set_selection_content(content)
    }

    /// MIME types advertised by the current incoming selection, if any.
    pub fn selection_mimes(&self) -> &[String] {
        &self.state.incoming_mimes
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
        Ok(crate::data_transfer::TransferReadPipe::from_stream(
            mime, reader,
        ))
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

impl NativeShell {
    /// Open a pipe for the first preferred mime from the current selection.
    pub fn receive_selection_preferred_pipe(
        &mut self,
        preferred_mimes: &[&str],
    ) -> Result<crate::data_transfer::TransferReadPipe, NativeError> {
        let mime = preferred_mimes
            .iter()
            .find(|m| {
                self.state
                    .incoming_mimes
                    .iter()
                    .any(|offered| offered == *m)
            })
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

//! Clipboard, drag-and-drop, and text-input methods for [`NativeShell`].

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::native::connection::NativeError;

impl NativeShell {
    pub fn has_text_input(&self) -> bool {
        self.state.text_input.is_some()
    }

    /// Enable text-input-v3 on `surface` (IME / on-screen keyboard).
    pub fn enable_text_input(&mut self, surface: NativeSurfaceId) -> Result<(), NativeError> {
        let ti = self
            .state
            .text_input
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("text_input_v3 missing".into()))?;
        let _wl = self
            .state
            .toplevels
            .get(&surface)
            .map(|t| t.wl.clone())
            .or_else(|| self.state.popups.get(&surface).map(|p| p.wl.clone()))
            .or_else(|| self.state.layers.get(&surface).map(|l| l.wl.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {surface:?}")))?;
        ti.enable();
        ti.set_cursor_rectangle(0, 0, 1, 1);
        ti.commit();
        self.connection.flush()?;
        Ok(())
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
        self.connection.flush()?;
        Ok(())
    }

    /// MIME types on the current drag offer.
    pub fn dnd_mimes(&self) -> &[String] {
        &self.state.dnd_mimes
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
        self.connection.flush()?;
        Ok(())
    }

    /// Receive bytes from the current drag offer.
    pub fn receive_dnd(&mut self, mime: &str) -> Result<Vec<u8>, NativeError> {
        use std::io::Read;
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
        writer.set_nonblocking(false).ok();
        reader.set_nonblocking(false).ok();
        offer.receive(mime.to_string(), writer.as_fd());
        drop(writer);
        self.connection.flush()?;
        let _ = self.dispatch_pending();
        let mut reader = reader;
        let mut buf = Vec::new();
        reader
            .read_to_end(&mut buf)
            .map_err(|e| NativeError::Io(e.to_string()))?;
        // Finish the offer after successful receive.
        if let Some(offer) = self.state.dnd_offer.as_ref() {
            offer.finish();
        }
        Ok(buf)
    }

    /// Start an outgoing drag with text payload (requires recent pointer serial).
    pub fn start_drag_text(
        &mut self,
        origin: NativeSurfaceId,
        text: impl Into<String>,
    ) -> Result<(), NativeError> {
        let bytes: std::sync::Arc<[u8]> = text.into().into_bytes().into();
        self.start_drag_bytes(
            origin,
            bytes,
            &[
                "text/plain;charset=utf-8",
                "text/plain",
                "UTF8_STRING",
            ],
        )
    }

    pub fn start_drag_bytes(
        &mut self,
        origin: NativeSurfaceId,
        bytes: std::sync::Arc<[u8]>,
        mimes: &[&str],
    ) -> Result<(), NativeError> {
        let serial = self
            .state
            .last_input_serial
            .ok_or_else(|| NativeError::Protocol("no input serial for start_drag".into()))?;
        let manager = self
            .state
            .data_device_manager
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("wl_data_device_manager missing".into()))?;
        let device = self
            .state
            .data_device
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("wl_data_device missing".into()))?;
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
        let source = manager.create_data_source(&qh, ());
        for mime in mimes {
            source.offer((*mime).to_string());
        }
        source.set_actions(
            wayland_client::protocol::wl_data_device_manager::DndAction::Copy
                | wayland_client::protocol::wl_data_device_manager::DndAction::Move,
        );
        device.start_drag(Some(&source), &origin_wl, None, serial);
        self.state.dnd_source = Some(source);
        self.state.dnd_source_bytes = Some(bytes);
        self.state.dnd_source_mimes = mimes.iter().map(|m| (*m).to_string()).collect();
        self.connection.flush()?;
        Ok(())
    }

    /// Advertise UTF-8 text on the clipboard (requires a recent input serial).
    pub fn set_selection_text(&mut self, text: impl Into<String>) -> Result<(), NativeError> {
        let bytes: std::sync::Arc<[u8]> = text.into().into_bytes().into();
        self.set_selection_bytes(
            bytes,
            &[
                "text/plain;charset=utf-8",
                "text/plain",
                "UTF8_STRING",
            ],
        )
    }

    pub fn set_selection_bytes(
        &mut self,
        bytes: std::sync::Arc<[u8]>,
        mimes: &[&str],
    ) -> Result<(), NativeError> {
        let serial = self
            .state
            .last_input_serial
            .ok_or_else(|| NativeError::Protocol("no input serial for set_selection".into()))?;
        let manager = self
            .state
            .data_device_manager
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("wl_data_device_manager missing".into()))?;
        let device = self
            .state
            .data_device
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("wl_data_device missing".into()))?;
        let qh = self.queue.handle();
        if let Some(old) = self.state.selection_source.take() {
            old.destroy();
        }
        let source = manager.create_data_source(&qh, ());
        for mime in mimes {
            source.offer((*mime).to_string());
        }
        device.set_selection(Some(&source), serial);
        self.state.selection_source = Some(source);
        self.state.selection_bytes = Some(bytes);
        self.state.selection_mimes = mimes.iter().map(|m| (*m).to_string()).collect();
        self.connection.flush()?;
        Ok(())
    }

    /// MIME types advertised by the current incoming selection, if any.
    pub fn selection_mimes(&self) -> &[String] {
        &self.state.incoming_mimes
    }

    /// Whether a keymap was applied via `wl_keyboard.keymap` (xkb ready).
    pub fn has_xkb(&self) -> bool {
        self.state.xkb.is_some()
    }

    /// Receive the current selection as bytes for `mime` (blocking pipe read).
    pub fn receive_selection(&mut self, mime: &str) -> Result<Vec<u8>, NativeError> {
        use std::io::Read;
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
        offer.receive(mime.to_string(), writer.as_fd());
        // Drop writer so the peer sees EOF after compositor writes.
        drop(writer);
        self.connection.flush()?;
        // Allow compositor to complete the transfer.
        let _ = self.dispatch_pending();
        let mut reader = reader;
        let mut buf = Vec::new();
        reader
            .read_to_end(&mut buf)
            .map_err(|e| NativeError::Io(e.to_string()))?;
        Ok(buf)
    }

}

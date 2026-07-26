//! Clipboard, drag-and-drop, and text-input methods for [`NativeShell`].

use std::sync::Arc;

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::data_transfer::TransferContent;
use crate::native::connection::NativeError;
use crate::native::protocols::core::shm;

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
        self.connection.flush()?;
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
        self.connection.flush()?;
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
        offer.receive(mime.to_string(), writer.as_fd());
        drop(writer);
        self.connection.flush()?;
        let _ = self.dispatch_pending();
        Ok(crate::data_transfer::TransferReadPipe::from_stream(
            mime.to_string(),
            reader,
        ))
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
            offer.finish();
            offer.destroy();
        }
        self.state.dnd_offer_id = None;
        self.state.dnd_mimes.clear();
        self.state.dnd_focus = None;
        self.state.dnd_serial = None;
        self.connection.flush()?;
        Ok(())
    }

    /// Discard the current drag offer without finishing (leave / cancel).
    pub fn discard_dnd(&mut self) -> Result<(), NativeError> {
        if let Some(offer) = self.state.dnd_offer.take() {
            offer.destroy();
        }
        self.state.dnd_offer_id = None;
        self.state.dnd_mimes.clear();
        self.state.dnd_focus = None;
        self.state.dnd_serial = None;
        self.connection.flush()?;
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
        let serial = self
            .state
            .last_input_serial
            .ok_or_else(|| NativeError::Protocol("no input serial for start_drag".into()))?;
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
        let device = self
            .state
            .data_device
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("wl_data_device missing".into()))?;
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
        self.connection.flush()?;
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
    pub fn set_selection_content(
        &mut self,
        content: TransferContent,
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
        for mime in content.mime_types() {
            source.offer(mime.to_string());
        }
        device.set_selection(Some(&source), serial);
        self.state.selection_source = Some(source);
        self.state.selection_content = Some(content);
        self.connection.flush()?;
        Ok(())
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
        offer.receive(mime.to_string(), writer.as_fd());
        drop(writer);
        self.connection.flush()?;
        let _ = self.dispatch_pending();
        Ok(crate::data_transfer::TransferReadPipe::from_stream(
            mime.to_string(),
            reader,
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

//! Drag-and-drop methods for [`NativeShell`].

use std::sync::Arc;

use wayland_client::Proxy;

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::data_transfer::TransferContent;
use crate::native::connection::NativeError;

impl NativeShell {
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
        Ok(crate::data_transfer::TransferReadPipe::from_stream(
            mime, reader,
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
        let content =
            TransferContent::new(payloads).map_err(|e| NativeError::Protocol(e.to_string()))?;
        self.start_drag_content(origin, content)
    }
}

fn prepare_native_dnd_icon(
    shell: &mut NativeShell,
    qh: &wayland_client::QueueHandle<super::types::NativeShellState>,
    icon: crate::DndIcon,
) -> Result<super::types::NativeDndIconSurface, NativeError> {
    use wayland_client::Proxy;

    let (params, _width, _height, buffer_scale, offset) = icon.into_parts();
    let compositor = shell
        .state
        .compositor
        .as_ref()
        .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?;
    use std::os::fd::AsFd as _;
    use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::Flags;
    let dmabuf = shell
        .state
        .linux_dmabuf
        .as_ref()
        .ok_or_else(|| NativeError::Registry("zwp_linux_dmabuf_v1".into()))?;
    let params_proxy = dmabuf.create_params(qh, ());
    for plane in &params.planes {
        params_proxy.add(
            plane.fd.as_fd(),
            plane.plane_idx,
            plane.offset,
            plane.stride,
            (plane.modifier >> 32) as u32,
            plane.modifier as u32,
        );
    }
    let buffer = params_proxy.create_immed(
        params.width,
        params.height,
        params.format,
        Flags::from_bits_truncate(params.flags.bits()),
        qh,
        (),
    );
    params_proxy.destroy();
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
    Ok(super::types::NativeDndIconSurface { wl, buffer })
}

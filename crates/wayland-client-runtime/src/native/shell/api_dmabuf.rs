//! Linux dmabuf client helpers on [`NativeShell`].

use wayland_client::Proxy;

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::native::connection::NativeError;

impl NativeShell {
    pub fn has_linux_dmabuf(&self) -> bool {
        self.state.linux_dmabuf.is_some()
    }

    /// Bound `zwp_linux_dmabuf_v1` version, if any.
    pub fn linux_dmabuf_version(&self) -> Option<u32> {
        self.state
            .linux_dmabuf
            .as_ref()
            .map(|_| self.state.linux_dmabuf_version)
    }

    /// Legacy format/modifier pairs (protocol version &lt; 4 only).
    ///
    /// On v4+, use [`Self::dmabuf_default_feedback`] / surface feedback instead.
    pub fn dmabuf_modifiers(&self) -> &[crate::dmabuf::DmabufFormat] {
        &self.state.dmabuf_modifiers
    }

    /// Latest completed default dmabuf feedback (v4+), if received.
    pub fn dmabuf_default_feedback(&self) -> Option<&crate::dmabuf::DmabufFeedback> {
        self.state.dmabuf_default_feedback.as_ref()
    }

    /// Latest completed surface-scoped dmabuf feedback, if any.
    pub fn dmabuf_surface_feedback(
        &self,
        id: NativeSurfaceId,
    ) -> Option<&crate::dmabuf::DmabufFeedback> {
        self.state.dmabuf_surface_feedback.get(&id)
    }

    /// Request default dmabuf feedback (requires protocol version ≥ 4).
    ///
    /// Feedback arrives as [`NativeShellEvent::DmabufFeedback`] with
    /// `surface: None`. Idempotent if already requested.
    pub fn request_dmabuf_default_feedback(&mut self) -> Result<(), NativeError> {
        if self.state.dmabuf_default_feedback_obj.is_some() {
            return Ok(());
        }
        let dmabuf = self
            .state
            .linux_dmabuf
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("zwp_linux_dmabuf_v1 missing".into()))?;
        if dmabuf.version() < 4 {
            return Err(NativeError::Protocol(
                "zwp_linux_dmabuf_v1 feedback requires version >= 4".into(),
            ));
        }
        let qh = self.queue.handle();
        let feedback = dmabuf.get_default_feedback(&qh, ());
        self.state.dmabuf_default_feedback_obj = Some(feedback);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Request surface-scoped dmabuf feedback (requires protocol version ≥ 4).
    ///
    /// Replaces any previous feedback object for this surface.
    pub fn request_dmabuf_surface_feedback(
        &mut self,
        id: NativeSurfaceId,
    ) -> Result<(), NativeError> {
        let dmabuf = self
            .state
            .linux_dmabuf
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("zwp_linux_dmabuf_v1 missing".into()))?;
        if dmabuf.version() < 4 {
            return Err(NativeError::Protocol(
                "zwp_linux_dmabuf_v1 feedback requires version >= 4".into(),
            ));
        }
        let wl = self
            .state
            .wl_surface(id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?
            .clone();
        if let Some(old) = self.state.dmabuf_surface_feedback_objs.remove(&id) {
            let pid = old.id().protocol_id();
            self.state.dmabuf_feedback_surfaces.remove(&pid);
            self.state.dmabuf_feedback_pending.remove(&pid);
            self.state.dmabuf_tranche_pending.remove(&pid);
            old.destroy();
        }
        let qh = self.queue.handle();
        let feedback = dmabuf.get_surface_feedback(&wl, &qh, ());
        let pid = feedback.id().protocol_id();
        self.state.dmabuf_feedback_surfaces.insert(pid, id);
        self.state.dmabuf_surface_feedback_objs.insert(id, feedback);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Create a dmabuf-backed `wl_buffer` asynchronously.
    ///
    /// Success/failure is delivered as [`NativeShellEvent::DmabufBufferCreated`]
    /// or [`NativeShellEvent::DmabufBufferFailed`]. Prefer this over
    /// [`Self::create_dmabuf_buffer_immed`] when the compositor may reject the
    /// import without a fatal protocol error.
    pub fn create_dmabuf_buffer(
        &mut self,
        params: crate::dmabuf::DmabufBufferParams,
    ) -> Result<(), NativeError> {
        use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::Flags;

        let proxy = self.begin_dmabuf_params(&params)?;
        let flags = Flags::from_bits_truncate(params.flags.bits());
        proxy.create(params.width, params.height, params.format, flags);
        let pid = proxy.id().protocol_id();
        self.state.dmabuf_params.insert(pid, proxy);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Create a dmabuf-backed `wl_buffer` immediately (protocol `create_immed`).
    ///
    /// On failure the compositor may raise a protocol error or later emit
    /// `failed`. The returned id is valid only if the import succeeds.
    pub fn create_dmabuf_buffer_immed(
        &mut self,
        params: crate::dmabuf::DmabufBufferParams,
    ) -> Result<crate::dmabuf::DmabufBufferId, NativeError> {
        use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::Flags;

        let proxy = self.begin_dmabuf_params(&params)?;
        let qh = self.queue.handle();
        let flags = Flags::from_bits_truncate(params.flags.bits());
        let buffer = proxy.create_immed(params.width, params.height, params.format, flags, &qh, ());
        let id = self.state.next_dmabuf_buffer_id;
        self.state.next_dmabuf_buffer_id = self.state.next_dmabuf_buffer_id.saturating_add(1);
        let buffer_proto = buffer.id().protocol_id();
        self.state.dmabuf_buffers.insert(
            id,
            super::types::DmabufBufferRecord {
                buffer,
                params_proto: None,
            },
        );
        self.state.dmabuf_buffer_by_proto.insert(buffer_proto, id);
        // Params object is no longer needed after create_immed.
        proxy.destroy();
        self.connection.mark_dirty();
        Ok(crate::dmabuf::DmabufBufferId(id))
    }

    /// Validate params and create a populated `zwp_linux_buffer_params_v1`.
    fn begin_dmabuf_params(
        &self,
        params: &crate::dmabuf::DmabufBufferParams,
    ) -> Result<
        wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        NativeError,
    > {
        use std::os::fd::AsFd;

        let dmabuf = self
            .state
            .linux_dmabuf
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("zwp_linux_dmabuf_v1 missing".into()))?;
        if params.planes.is_empty() {
            return Err(NativeError::Protocol(
                "dmabuf buffer requires at least one plane".into(),
            ));
        }
        if params.width <= 0 || params.height <= 0 {
            return Err(NativeError::Protocol(
                "dmabuf buffer dimensions must be positive".into(),
            ));
        }
        let qh = self.queue.handle();
        let proxy = dmabuf.create_params(&qh, ());
        for plane in &params.planes {
            let modifier_hi = (plane.modifier >> 32) as u32;
            let modifier_lo = (plane.modifier & 0xffff_ffff) as u32;
            proxy.add(
                plane.fd.as_fd(),
                plane.plane_idx,
                plane.offset,
                plane.stride,
                modifier_hi,
                modifier_lo,
            );
        }
        Ok(proxy)
    }

    /// Attach a previously imported dmabuf buffer to a surface (no commit).
    pub fn attach_dmabuf_buffer(
        &mut self,
        id: NativeSurfaceId,
        buffer: crate::dmabuf::DmabufBufferId,
        x: i32,
        y: i32,
    ) -> Result<(), NativeError> {
        let wl = self
            .state
            .wl_surface(id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?
            .clone();
        let record = self
            .state
            .dmabuf_buffers
            .get(&buffer.0)
            .ok_or_else(|| NativeError::Protocol(format!("unknown dmabuf buffer {buffer:?}")))?;
        wl.attach(Some(&record.buffer), x, y);
        Ok(())
    }

    /// Destroy an imported dmabuf buffer.
    pub fn destroy_dmabuf_buffer(
        &mut self,
        buffer: crate::dmabuf::DmabufBufferId,
    ) -> Result<(), NativeError> {
        let Some(record) = self.state.dmabuf_buffers.remove(&buffer.0) else {
            return Err(NativeError::Protocol(format!(
                "unknown dmabuf buffer {buffer:?}"
            )));
        };
        self.state
            .dmabuf_buffer_by_proto
            .remove(&record.buffer.id().protocol_id());
        record.buffer.destroy();
        self.connection.mark_dirty();
        Ok(())
    }
}

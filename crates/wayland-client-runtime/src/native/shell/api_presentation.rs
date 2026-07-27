//! Frame callback and presentation-time helpers on [`NativeShell`].

use wayland_client::Proxy;

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::native::connection::NativeError;

impl NativeShell {
    /// Request a `wl_surface.frame` callback; emits [`NativeShellEvent::Frame`].
    ///
    /// Works for toplevel, popup, and layer surfaces (needed for GPU present pacing).
    ///
    /// Coalesces: if a frame callback is already outstanding for `id`, this is
    /// a no-op success (avoids stacking callbacks when the app arms every redraw).
    pub fn request_frame(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        if self.state.frame_pending.contains(&id) {
            return Ok(());
        }
        let qh = self.queue.handle();
        let wl = self
            .state
            .wl_surface(id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        let callback = wl.frame(&qh, ());
        self.state
            .frame_callbacks
            .insert(callback.id().protocol_id(), id);
        self.state.frame_pending.insert(id);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Whether a `wl_surface.frame` callback is outstanding for `id`.
    #[inline]
    pub fn is_frame_pending(&self, id: NativeSurfaceId) -> bool {
        self.state.is_frame_pending(id)
    }

    /// Logical size last known for the surface (configure / client updates).
    pub fn logical_size(&self, id: NativeSurfaceId) -> Option<crate::geometry::LogicalSize> {
        self.state
            .logical_size(id)
            .map(|(w, h)| crate::geometry::LogicalSize::new(w, h))
    }

    /// Request `wp_presentation.feedback` for the next commit on `id`.
    ///
    /// Associates with the **next** `wl_surface.commit` (call before or with
    /// the content submission). Emits [`NativeShellEvent::Presented`] or
    /// [`NativeShellEvent::PresentationDiscarded`]. No-ops cleanly when the
    /// global is missing (returns `Ok` so callers can always arm feedback).
    ///
    /// Coalesces: if feedback is already outstanding for `id`, this is a no-op
    /// success (avoids stacking feedback objects when arming every redraw).
    pub fn request_presentation_feedback(
        &mut self,
        id: NativeSurfaceId,
    ) -> Result<(), NativeError> {
        let Some(presentation) = self.state.presentation.clone() else {
            return Ok(());
        };
        if self.state.presentation_pending.contains(&id) {
            return Ok(());
        }
        let qh = self.queue.handle();
        let wl = self
            .state
            .wl_surface(id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?
            .clone();
        let feedback = presentation.feedback(&wl, &qh, ());
        let obj = feedback.id().protocol_id();
        self.state.presentation_feedbacks.insert(
            obj,
            super::types::PresentationFeedbackRecord {
                surface: id,
                sync_output: None,
            },
        );
        self.state.presentation_pending.insert(id);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Whether presentation feedback is outstanding for `id`.
    #[inline]
    pub fn is_presentation_pending(&self, id: NativeSurfaceId) -> bool {
        self.state.is_presentation_pending(id)
    }
}

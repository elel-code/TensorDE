// Tensor Files uses NativeRuntime (Compio io_uring completion waits, no SCTK).
/// Active protocol backend for the Tensor Files event loop.
struct WindowRuntime {
    inner: NativeRuntime,
}

impl WindowRuntime {
    fn connect() -> Result<Self, RuntimeError> {
        use std::sync::atomic::{AtomicBool, Ordering};
        static WARNED_LEGACY_BACKEND: AtomicBool = AtomicBool::new(false);
        let backend = std::env::var("TENSOR_FILES_WAYLAND_BACKEND")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(backend.as_str(), "sctk" | "smithay" | "legacy")
            && !WARNED_LEGACY_BACKEND.swap(true, Ordering::Relaxed)
        {
            eprintln!(
                "[tensor-files-wayland] TENSOR_FILES_WAYLAND_BACKEND={backend:?} ignored: native Compio backend only"
            );
        }
        Ok(Self {
            inner: NativeRuntime::connect()?,
        })
    }

    fn wake_handle(&self) -> WakeHandle {
        self.inner.wake_handle()
    }

    fn capabilities(&self) -> wayland_client_runtime::RuntimeCapabilities {
        self.inner.capabilities()
    }

    fn drain_events_into(&mut self, target: &mut Vec<Event>) {
        self.inner.drain_events_into(target)
    }

    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<(), RuntimeError> {
        self.inner.dispatch(timeout)
    }

    fn create_toplevel(
        &mut self,
        attributes: ToplevelAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        self.inner.create_toplevel(attributes)
    }

    fn create_dialog(
        &mut self,
        parent: SurfaceId,
        attributes: DialogAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        self.inner.create_dialog(parent, attributes)
    }

    #[allow(dead_code)]
    fn create_popup(
        &mut self,
        parent: SurfaceId,
        attributes: wayland_client_runtime::PopupAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        self.inner.create_popup(parent, attributes)
    }

    #[allow(dead_code)]
    fn reposition_popup(
        &mut self,
        surface: SurfaceId,
        positioner: &wayland_client_runtime::PopupPositioner,
        token: u32,
    ) -> Result<(), RuntimeError> {
        self.inner.reposition_popup(surface, positioner, token)
    }

    #[allow(dead_code)]
    fn create_layer_surface(
        &mut self,
        attributes: wayland_client_runtime::LayerSurfaceAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        self.inner.create_layer_surface(attributes)
    }

    #[allow(dead_code)]
    fn set_layer_surface_state(
        &mut self,
        surface: SurfaceId,
        state: wayland_client_runtime::LayerSurfaceState,
    ) -> Result<(), RuntimeError> {
        self.inner.set_layer_surface_state(surface, state)
    }

    #[allow(dead_code)]
    fn layer_surface_state(
        &self,
        surface: SurfaceId,
    ) -> Result<wayland_client_runtime::LayerSurfaceState, RuntimeError> {
        self.inner.layer_surface_state(surface)
    }

    #[allow(dead_code)]
    fn outputs(&self) -> Vec<wayland_client_runtime::OutputInfo> {
        self.inner.outputs()
    }

    #[allow(dead_code)]
    fn set_app_id(
        &mut self,
        surface: SurfaceId,
        app_id: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        self.inner.set_app_id(surface, app_id)
    }

    #[allow(dead_code)]
    fn request_activation_token(
        &mut self,
        surface: SurfaceId,
        attributes: wayland_client_runtime::ActivationTokenAttributes,
    ) -> Result<wayland_client_runtime::ActivationRequestId, RuntimeError> {
        self.inner.request_activation_token(surface, attributes)
    }

    #[allow(dead_code)]
    fn activate_surface(
        &mut self,
        surface: SurfaceId,
        token: wayland_client_runtime::ActivationToken,
    ) -> Result<(), RuntimeError> {
        self.inner.activate_surface(surface, token)
    }

    fn surface_handle(&self, surface: SurfaceId) -> Option<SurfaceHandle> {
        self.inner.surface_handle(surface)
    }

    fn logical_size(&self, surface: SurfaceId) -> Option<LogicalSize> {
        self.inner.logical_size(surface)
    }

    fn set_title(&mut self, surface: SurfaceId, title: String) -> Result<(), RuntimeError> {
        self.inner.set_title(surface, title)
    }

    fn set_min_size(
        &mut self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        self.inner.set_min_size(surface, size)
    }

    fn set_max_size(
        &mut self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        self.inner.set_max_size(surface, size)
    }

    fn set_blur(&mut self, surface: SurfaceId, state: BlurState) -> Result<(), RuntimeError> {
        self.inner.set_blur(surface, state)
    }

    fn set_cursor_on_seat(
        &mut self,
        icon: RuntimeCursorIcon,
        seat: Option<wayland_client_runtime::SeatId>,
    ) -> Result<(), RuntimeError> {
        self.inner.set_cursor_on_seat(icon, seat)
    }

    fn set_text_input_state(
        &mut self,
        surface: SurfaceId,
        state: Option<RuntimeTextInputState>,
    ) -> Result<(), RuntimeError> {
        self.inner.set_text_input_state(surface, state.as_ref())
    }

    fn request_user_attention(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        self.inner.request_user_attention(surface)
    }

    fn arm_present_notify(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        self.inner.arm_present_notify(surface)
    }

    fn flush(&mut self) -> Result<(), RuntimeError> {
        self.inner.flush()
    }

    fn is_present_pending(&self, surface: SurfaceId) -> bool {
        self.inner.is_present_pending(surface)
    }

    fn request_dmabuf_default_feedback(&mut self) -> Result<(), RuntimeError> {
        self.inner.request_dmabuf_default_feedback()
    }

    fn request_dmabuf_surface_feedback(
        &mut self,
        surface: SurfaceId,
    ) -> Result<(), RuntimeError> {
        self.inner.request_dmabuf_surface_feedback(surface)
    }

    fn has_linux_dmabuf(&self) -> bool {
        self.inner.has_linux_dmabuf()
    }

    #[allow(dead_code)] // available for GPU format negotiation / diagnostics
    fn dmabuf_default_feedback(
        &self,
    ) -> Option<wayland_client_runtime::DmabufFeedback> {
        self.inner.dmabuf_default_feedback().cloned()
    }

    fn commit(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        self.inner.commit(surface)
    }

    fn destroy_surface(&mut self, surface: SurfaceId) -> Result<Vec<SurfaceId>, RuntimeError> {
        self.inner.destroy_surface(surface)
    }

    fn set_toplevel_icon(
        &mut self,
        surface: SurfaceId,
        icon: Option<RuntimeToplevelIcon>,
    ) -> Result<(), RuntimeError> {
        self.inner.set_toplevel_icon(surface, icon)
    }

    fn set_pointer_gestures_enabled(
        &mut self,
        surface: SurfaceId,
        enabled: bool,
    ) -> Result<(), RuntimeError> {
        self.inner.set_pointer_gestures_enabled(surface, enabled)
    }

    fn set_window_geometry(
        &mut self,
        surface: SurfaceId,
        origin: LogicalPosition,
        size: LogicalSize,
    ) -> Result<(), RuntimeError> {
        self.inner.set_window_geometry(surface, origin, size)
    }

    fn set_buffer_scale(&mut self, surface: SurfaceId, factor: i32) -> Result<(), RuntimeError> {
        self.inner.set_buffer_scale(surface, factor)
    }

    fn set_viewport_destination(
        &mut self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        self.inner.set_viewport_destination(surface, size)
    }

    fn discard_dnd_offer(&mut self, offer: DndOfferId) -> Result<(), RuntimeError> {
        self.inner.discard_dnd_offer(offer)
    }

    fn finish_dnd_offer(&mut self, offer: DndOfferId) -> Result<(), RuntimeError> {
        self.inner.finish_dnd_offer(offer)
    }

    fn store_selection(&mut self, content: TransferContent) -> Result<(), RuntimeError> {
        self.inner.store_selection(content)
    }

    fn receive_selection(
        &mut self,
        preferred_mimes: &[&str],
    ) -> Result<wayland_client_runtime::TransferReadPipe, RuntimeError> {
        self.inner.receive_selection(preferred_mimes)
    }

    fn receive_primary_selection(
        &mut self,
        preferred_mimes: &[&str],
    ) -> Result<wayland_client_runtime::TransferReadPipe, RuntimeError> {
        self.inner.receive_primary_selection(preferred_mimes)
    }

    fn start_drag(
        &mut self,
        origin: SurfaceId,
        content: TransferContent,
        actions: RuntimeDndActions,
        icon: Option<RuntimeDndIcon>,
    ) -> Result<DndSourceId, RuntimeError> {
        self.inner.start_drag(origin, content, actions, icon)
    }

    fn set_dnd_offer_actions(
        &mut self,
        offer: DndOfferId,
        accepted_mime: Option<&str>,
        actions: RuntimeDndActions,
        preferred: Option<RuntimeDndAction>,
    ) -> Result<(), RuntimeError> {
        self.inner
            .set_dnd_offer_actions(offer, accepted_mime, actions, preferred)
    }

    fn receive_dnd(
        &mut self,
        offer: DndOfferId,
        mime: &str,
    ) -> Result<wayland_client_runtime::DndReadPipe, RuntimeError> {
        self.inner.receive_dnd(offer, mime)
    }

    #[allow(dead_code)]
    fn set_pointer_capture_state(
        &mut self,
        surface: SurfaceId,
        state: wayland_client_runtime::PointerCaptureState,
    ) -> Result<(), RuntimeError> {
        self.inner.set_pointer_capture_state(surface, state)
    }

    #[allow(dead_code)]
    fn set_pointer_constraint(
        &mut self,
        surface: SurfaceId,
        constraint: wayland_client_runtime::PointerConstraint,
    ) -> Result<(), RuntimeError> {
        self.inner.set_pointer_constraint(surface, constraint)
    }

    #[allow(dead_code)]
    fn set_relative_pointer_enabled(
        &mut self,
        surface: SurfaceId,
        enabled: bool,
    ) -> Result<(), RuntimeError> {
        self.inner.set_relative_pointer_enabled(surface, enabled)
    }

    #[allow(dead_code)]
    fn begin_interactive_move(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        self.inner.begin_interactive_move(surface)
    }

    #[allow(dead_code)]
    fn begin_interactive_resize(
        &mut self,
        surface: SurfaceId,
        edge: wayland_client_runtime::ResizeEdge,
    ) -> Result<(), RuntimeError> {
        self.inner.begin_interactive_resize(surface, edge)
    }

    #[allow(dead_code)]
    fn show_window_menu(
        &mut self,
        surface: SurfaceId,
        position: LogicalPosition,
    ) -> Result<(), RuntimeError> {
        self.inner.show_window_menu(surface, position)
    }

    #[allow(dead_code)]
    fn preferred_toplevel_icon_sizes(&self) -> Vec<u32> {
        self.inner.preferred_toplevel_icon_sizes()
    }

    fn is_native(&self) -> bool {
        true
    }
}

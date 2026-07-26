// Dual Wayland backend: SCTK Runtime (default) or NativeRuntime.
// Select with FIKA_WAYLAND_BACKEND=native (or sctk, default).

/// Active protocol backend for the Fika event loop.
enum PlatformBackend {
    Sctk(Runtime),
    Native(NativeRuntime),
}

impl PlatformBackend {
    fn connect() -> Result<Self, RuntimeError> {
        let backend = std::env::var("FIKA_WAYLAND_BACKEND")
            .unwrap_or_default()
            .to_ascii_lowercase();
        match backend.as_str() {
            "native" | "nativeshell" | "compio" => {
                eprintln!("[fika-wayland] backend=native (NativeShell, no SCTK)");
                Ok(Self::Native(NativeRuntime::connect()?))
            }
            "" | "sctk" | "smithay" | "default" => {
                Ok(Self::Sctk(Runtime::connect(
                    wayland_client_runtime::RuntimeOptions::default(),
                )?))
            }
            other => {
                eprintln!(
                    "[fika-wayland] unknown FIKA_WAYLAND_BACKEND={other:?}, using sctk"
                );
                Ok(Self::Sctk(Runtime::connect(
                    wayland_client_runtime::RuntimeOptions::default(),
                )?))
            }
        }
    }

    fn wake_handle(&self) -> WakeHandle {
        match self {
            Self::Sctk(rt) => rt.wake_handle(),
            Self::Native(rt) => rt.wake_handle(),
        }
    }

    fn capabilities(&self) -> wayland_client_runtime::RuntimeCapabilities {
        match self {
            Self::Sctk(rt) => rt.capabilities(),
            Self::Native(rt) => rt.capabilities(),
        }
    }

    fn drain_events_into(&mut self, target: &mut Vec<Event>) {
        match self {
            Self::Sctk(rt) => rt.drain_events_into(target),
            Self::Native(rt) => rt.drain_events_into(target),
        }
    }

    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.dispatch(timeout),
            Self::Native(rt) => rt.dispatch(timeout),
        }
    }

    fn create_toplevel(
        &mut self,
        attributes: ToplevelAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.create_toplevel(attributes),
            Self::Native(rt) => rt.create_toplevel(attributes),
        }
    }

    fn create_dialog(
        &mut self,
        parent: SurfaceId,
        attributes: DialogAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.create_dialog(parent, attributes),
            Self::Native(rt) => rt.create_dialog(parent, attributes),
        }
    }

    fn surface_handle(&self, surface: SurfaceId) -> Option<SurfaceHandle> {
        match self {
            Self::Sctk(rt) => rt.surface_handle(surface),
            Self::Native(rt) => rt.surface_handle(surface),
        }
    }

    fn set_title(&mut self, surface: SurfaceId, title: String) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_title(surface, title),
            Self::Native(rt) => rt.set_title(surface, title),
        }
    }

    fn set_min_size(
        &mut self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_min_size(surface, size),
            Self::Native(rt) => rt.set_min_size(surface, size),
        }
    }

    fn set_max_size(
        &mut self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_max_size(surface, size),
            Self::Native(rt) => rt.set_max_size(surface, size),
        }
    }

    fn set_blur(&mut self, surface: SurfaceId, state: BlurState) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_blur(surface, state),
            Self::Native(rt) => rt.set_blur(surface, state),
        }
    }

    fn set_cursor(&mut self, icon: RuntimeCursorIcon) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_cursor(icon),
            Self::Native(rt) => rt.set_cursor(icon),
        }
    }

    fn set_text_input_state(
        &mut self,
        surface: SurfaceId,
        state: Option<RuntimeTextInputState>,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_text_input_state(surface, state),
            Self::Native(rt) => rt.set_text_input_state(surface, state.as_ref()),
        }
    }

    fn request_user_attention(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.request_user_attention(surface),
            Self::Native(rt) => rt.request_user_attention(surface),
        }
    }

    fn request_frame(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.request_frame(surface),
            Self::Native(rt) => rt.request_frame(surface),
        }
    }

    fn commit(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.commit(surface),
            Self::Native(rt) => rt.commit(surface),
        }
    }

    fn destroy_surface(&mut self, surface: SurfaceId) -> Result<Vec<SurfaceId>, RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.destroy_surface(surface),
            Self::Native(rt) => rt.destroy_surface(surface),
        }
    }

    fn set_toplevel_icon(
        &mut self,
        surface: SurfaceId,
        icon: Option<RuntimeToplevelIcon>,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_toplevel_icon(surface, icon),
            Self::Native(rt) => rt.set_toplevel_icon(surface, icon),
        }
    }

    fn set_pointer_gestures_enabled(
        &mut self,
        surface: SurfaceId,
        enabled: bool,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_pointer_gestures_enabled(surface, enabled),
            Self::Native(rt) => rt.set_pointer_gestures_enabled(surface, enabled),
        }
    }

    fn set_window_geometry(
        &mut self,
        surface: SurfaceId,
        origin: LogicalPosition,
        size: LogicalSize,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_window_geometry(surface, origin, size),
            Self::Native(rt) => rt.set_window_geometry(surface, origin, size),
        }
    }

    fn set_buffer_scale(&mut self, surface: SurfaceId, factor: i32) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_buffer_scale(surface, factor),
            Self::Native(rt) => rt.set_buffer_scale(surface, factor),
        }
    }

    fn set_viewport_destination(
        &mut self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_viewport_destination(surface, size),
            Self::Native(rt) => rt.set_viewport_destination(surface, size),
        }
    }

    fn discard_dnd_offer(&mut self, offer: DndOfferId) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.discard_dnd_offer(offer),
            Self::Native(rt) => rt.discard_dnd_offer(offer),
        }
    }

    fn finish_dnd_offer(&mut self, offer: DndOfferId) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.finish_dnd_offer(offer),
            Self::Native(rt) => rt.finish_dnd_offer(offer),
        }
    }

    fn store_selection(
        &mut self,
        content: TransferContent,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.store_selection(content),
            Self::Native(rt) => rt.store_selection(content),
        }
    }

    fn receive_selection(
        &mut self,
        preferred_mimes: &[&str],
    ) -> Result<wayland_client_runtime::TransferReadPipe, RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.receive_selection(preferred_mimes),
            Self::Native(rt) => rt.receive_selection(preferred_mimes),
        }
    }

    fn start_drag(
        &mut self,
        origin: SurfaceId,
        content: TransferContent,
        actions: RuntimeDndActions,
        icon: Option<RuntimeDndIcon>,
    ) -> Result<DndSourceId, RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.start_drag(origin, content, actions, icon),
            Self::Native(rt) => rt.start_drag(origin, content, actions, icon),
        }
    }

    fn set_dnd_offer_actions(
        &mut self,
        offer: DndOfferId,
        accepted_mime: Option<&str>,
        actions: RuntimeDndActions,
        preferred: Option<RuntimeDndAction>,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => {
                rt.set_dnd_offer_actions(offer, accepted_mime, actions, preferred)
            }
            Self::Native(rt) => {
                rt.set_dnd_offer_actions(offer, accepted_mime, actions, preferred)
            }
        }
    }

    fn receive_dnd(
        &mut self,
        offer: DndOfferId,
        mime: &str,
    ) -> Result<wayland_client_runtime::DndReadPipe, RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.receive_dnd(offer, mime),
            Self::Native(rt) => rt.receive_dnd(offer, mime),
        }
    }

    fn set_pointer_capture_state(
        &mut self,
        surface: SurfaceId,
        state: wayland_client_runtime::PointerCaptureState,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_pointer_capture_state(surface, state),
            Self::Native(rt) => rt.set_pointer_capture_state(surface, state),
        }
    }

    fn set_pointer_constraint(
        &mut self,
        surface: SurfaceId,
        constraint: wayland_client_runtime::PointerConstraint,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_pointer_constraint(surface, constraint),
            Self::Native(rt) => rt.set_pointer_constraint(surface, constraint),
        }
    }

    fn set_relative_pointer_enabled(
        &mut self,
        surface: SurfaceId,
        enabled: bool,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.set_relative_pointer_enabled(surface, enabled),
            Self::Native(rt) => rt.set_relative_pointer_enabled(surface, enabled),
        }
    }

    fn begin_interactive_move(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.begin_interactive_move(surface),
            Self::Native(rt) => rt.begin_interactive_move(surface),
        }
    }

    fn begin_interactive_resize(
        &mut self,
        surface: SurfaceId,
        edge: wayland_client_runtime::ResizeEdge,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.begin_interactive_resize(surface, edge),
            Self::Native(rt) => rt.begin_interactive_resize(surface, edge),
        }
    }

    fn show_window_menu(
        &mut self,
        surface: SurfaceId,
        position: LogicalPosition,
    ) -> Result<(), RuntimeError> {
        match self {
            Self::Sctk(rt) => rt.show_window_menu(surface, position),
            Self::Native(rt) => rt.show_window_menu(surface, position),
        }
    }

    fn preferred_toplevel_icon_sizes(&self) -> Vec<u32> {
        match self {
            Self::Sctk(rt) => rt.preferred_toplevel_icon_sizes(),
            Self::Native(rt) => rt.preferred_toplevel_icon_sizes(),
        }
    }

    fn is_native(&self) -> bool {
        matches!(self, Self::Native(_))
    }
}

//! Fika-facing Compio adapter over the protocol-only [`NativeShell`].
//!
//! Protocol I/O uses the ordinary non-blocking Wayland display fd. Compio only
//! waits for **readiness** on cloned fds ([`CompioFdReady`]) via io_uring
//! completions — it never performs the Wayland `read` itself.
//!
//! The public [`Self::dispatch`] API stays synchronous by driving a dedicated
//! Compio runtime with `block_on`.

use std::collections::HashMap;
use std::future::poll_fn;
use std::task::Poll;
use std::time::Duration;

use crate::display_io::CompioFdReady;
use crate::event::Event;
use crate::geometry::{LogicalPosition, LogicalSize};
use crate::layer_shell::{LayerSurfaceAttributes, LayerSurfaceState};
use crate::native::event_map::{NativeEventMapState, SurfaceIdMap};
use crate::native::shell::{NativePopupPositioner, NativeShell, NativeSurfaceId};
use crate::output::OutputInfo;
use crate::runtime_common::{RuntimeCapabilities, RuntimeError, WakeHandle};
use crate::surface::{
    PopupAttributes, PopupPositioner, SurfaceHandle, SurfaceId, ToplevelAttributes,
};
use crate::wake_fd::EventFdWake;
use crate::{
    ActivationRequestId, ActivationToken, ActivationTokenAttributes, BlurState, NativeError,
    TextInputState, ToplevelIcon,
};

/// Which readiness source completed a Compio wait.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WaitSource {
    Display,
    Wake,
    Timeout,
}

/// Compio event-loop adapter over the protocol-only [`NativeShell`].
///
/// Other projects can ignore this type and drive [`NativeShell`] with their own
/// reactor (`default-features = false`).
pub struct NativeRuntime {
    pub(crate) shell: NativeShell,
    pub(crate) surfaces: SurfaceIdMap,
    map_state: NativeEventMapState,
    pub(crate) native_ids: HashMap<SurfaceId, NativeSurfaceId>,
    /// Pending activation export requests: native surface → public request id.
    activation_pending: HashMap<NativeSurfaceId, ActivationRequestId>,
    next_activation_request_id: u64,
    /// Reused scratch for shell → public event mapping (avoids per-dispatch alloc).
    drain_scratch: Vec<crate::NativeShellEvent>,
    wake: std::sync::Arc<EventFdWake>,
    /// Compio readiness watch on a clone of the wake eventfd (long-lived).
    /// Display readiness reuses [`NativeShell::display_ready`].
    wake_ready: CompioFdReady,
    wake_handle: WakeHandle,
    /// Owns the io_uring proactor used for display/wake waits.
    compio: compio::runtime::Runtime,
    pub(crate) capabilities: RuntimeCapabilities,
}

impl NativeRuntime {
    pub fn connect() -> Result<Self, RuntimeError> {
        let shell = NativeShell::connect_to_env().map_err(map_native_error)?;
        let caps = shell.capabilities();
        let popup_reposition = shell.supports_popup_reposition();
        let layer_ver = shell.layer_shell_version();
        let wake = std::sync::Arc::new(
            EventFdWake::new().map_err(|e| RuntimeError::EventLoop(e.to_string()))?,
        );
        let wake_ready = CompioFdReady::watch(wake.as_ref())
            .map_err(|e| RuntimeError::EventLoop(e.to_string()))?;
        let wake_handle = WakeHandle::from_event_fd(wake.clone());
        let compio = compio::runtime::Runtime::new()
            .map_err(|e| RuntimeError::EventLoop(e.to_string()))?;
        Ok(Self {
            shell,
            surfaces: SurfaceIdMap::new(),
            map_state: NativeEventMapState::default(),
            native_ids: HashMap::new(),
            activation_pending: HashMap::new(),
            next_activation_request_id: 1,
            drain_scratch: Vec::with_capacity(64),
            wake,
            wake_ready,
            wake_handle,
            compio,
            capabilities: RuntimeCapabilities {
                fractional_scale: caps.fractional_scale && caps.viewporter,
                cursor_shape: caps.cursor_shape,
                text_input_v3: caps.text_input,
                layer_shell_v1: caps.layer_shell,
                // set_layer since v2; on_demand keyboard since v4; exclusive_edge since v5.
                layer_shell_dynamic_layer: caps.layer_shell && layer_ver >= 2,
                layer_shell_on_demand_keyboard: caps.layer_shell && layer_ver >= 4,
                layer_shell_exclusive_edge: caps.layer_shell && layer_ver >= 5,
                xdg_activation_v1: caps.activation,
                pointer_gestures_v1: caps.pointer_gestures,
                pointer_gesture_hold_v1: caps.pointer_gesture_hold,
                relative_pointer_v1: caps.relative_pointer,
                pointer_constraints_v1: caps.pointer_constraints,
                xdg_dialog_v1: caps.xdg_dialog,
                xdg_toplevel_icon_v1: caps.toplevel_icon,
                ext_background_effect: caps.background_blur,
                // Server decorations available when zxdg_decoration_manager_v1 is bound.
                // Client/None still use app chrome; no separate capability bit exists.
                popup_reposition,
                presentation: caps.presentation,
                primary_selection: caps.primary_selection,
                idle_inhibit: caps.idle_inhibit,
                linux_dmabuf: caps.linux_dmabuf,
                linux_dmabuf_version: caps.linux_dmabuf_version,
            },
        })
    }

    pub fn wake_handle(&self) -> WakeHandle {
        self.wake_handle.clone()
    }

    pub fn capabilities(&self) -> RuntimeCapabilities {
        self.capabilities
    }

    pub fn drain_events_into(&mut self, target: &mut Vec<Event>) {
        // Map shell events into `target`. Scratch reuses capacity across pumps
        // so a quiet frame does not allocate.
        let seat = self.shell.seat().cloned();
        if let Some(serial) = self.shell.last_input_serial() {
            self.map_state.last_serial = serial;
        }
        // Collect into a local buffer first so mapping can borrow `self.shell`
        // (and other fields) without fighting a field-drain borrow.
        self.shell.drain_events_into(&mut self.drain_scratch);
        let mut native_events = std::mem::take(&mut self.drain_scratch);
        target.reserve(native_events.len());
        for event in native_events.drain(..) {
            if let crate::NativeShellEvent::ActivationToken { surface, token } = event {
                if let Some(request) = self.activation_pending.remove(&surface) {
                    let public = self.surfaces.intern(surface);
                    // Move the token string; no clone.
                    target.push(Event::Activation(crate::ActivationEvent::TokenDone {
                        request,
                        requesting_surface: public,
                        token: ActivationToken::from_raw(token),
                    }));
                    continue;
                }
                // Uncorrelated token: fall through to the generic mapper.
                if let Some(mapped) = crate::map_native_event_full(
                    crate::NativeShellEvent::ActivationToken { surface, token },
                    &mut self.surfaces,
                    seat.as_ref(),
                    &mut self.map_state,
                ) {
                    target.push(mapped);
                }
                continue;
            }
            // Prefer full output snapshot after mode/geometry/name settle.
            if let crate::NativeShellEvent::OutputDone { output } = event
                && let Some(info) = self.shell.output_info(output)
            {
                target.push(Event::Output(crate::OutputEvent::Updated(info)));
                continue;
            }
            if let Some(mapped) = crate::map_native_event_full(
                event,
                &mut self.surfaces,
                seat.as_ref(),
                &mut self.map_state,
            ) {
                target.push(mapped);
            }
        }
        // Return capacity to the field for the next pump.
        self.drain_scratch = native_events;
    }

    /// Drive Wayland I/O with Compio completion waits.
    ///
    /// * `None` — wait until the display is readable or a [`WakeHandle`] fires.
    /// * `Some(0)` — flush/dispatch only; never block on the proactor.
    /// * `Some(d)` — wait up to `d` via Compio timers + io_uring readiness.
    ///
    /// After any wait (including wake/timeout), the display socket is always
    /// drained when readable. Skipping the read on wake left `wl_surface.frame`
    /// (and other) messages buffered while `frame_pending` stayed true, which
    /// froze the UI after the first presented frame.
    pub fn dispatch(&mut self, timeout: Option<Duration>) -> Result<(), RuntimeError> {
        // Drain already-buffered socket data before sleeping (edge-triggered
        // io_uring readiness). Shares the protocol shell’s read path.
        let _ = self
            .shell
            .try_read_and_dispatch()
            .map_err(map_native_error)?;

        // Zero-timeout: only process already-queued protocol state.
        if matches!(timeout, Some(d) if d.is_zero()) {
            return Ok(());
        }

        // Classic client pattern: prepare_read before waiting so the queue is
        // locked against concurrent dispatch while the proactor sleeps.
        let prepared = self.shell.connection().connection().prepare_read();
        let Some(guard) = prepared else {
            let _ = self.shell.dispatch_pending().map_err(map_native_error)?;
            return Ok(());
        };

        // Partial borrows: shell display watch + wake watch + Compio runtime.
        let source = {
            let display = self.shell.display_ready();
            let wake = &self.wake_ready;
            let compio = &self.compio;
            compio.block_on(async {
                let race = race_display_or_wake(display, wake);
                match timeout {
                    None => race.await,
                    Some(duration) => match compio::runtime::time::timeout(duration, race).await {
                        Ok(result) => result,
                        Err(_elapsed) => Ok(WaitSource::Timeout),
                    },
                }
            })?
        };

        if source == WaitSource::Wake {
            self.wake.drain();
        }

        // Always consume the display read guard. Dropping without `read` on
        // wake/timeout left compositor replies (notably frame callbacks) sitting
        // in the socket while the app believed it was idle.
        match guard.read() {
            Ok(_) => {}
            Err(wayland_client::backend::WaylandError::Io(err))
                if err.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => {
                return Err(RuntimeError::EventLoop(error.to_string()));
            }
        }
        let _ = self.shell.dispatch_pending().map_err(map_native_error)?;
        Ok(())
    }

    pub fn create_toplevel(
        &mut self,
        attributes: ToplevelAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        let width = attributes
            .initial_size
            .map(|s| s.width)
            .filter(|&w| w > 0)
            .or_else(|| attributes.min_size.map(|s| s.width).filter(|&w| w > 0))
            .unwrap_or(800)
            .max(1);
        let height = attributes
            .initial_size
            .map(|s| s.height)
            .filter(|&h| h > 0)
            .or_else(|| attributes.min_size.map(|s| s.height).filter(|&h| h > 0))
            .unwrap_or(600)
            .max(1);
        let native = self
            .shell
            .create_toplevel_gpu(attributes.title, attributes.app_id, width, height)
            .map_err(map_native_error)?;
        if let Some(min) = attributes.min_size {
            let _ = self.shell.set_min_size(native, Some(min));
        }
        if let Some(max) = attributes.max_size {
            let _ = self.shell.set_max_size(native, Some(max));
        }
        let _ = self
            .shell
            .set_decorations(native, attributes.decorations);
        let public = self.surfaces.intern(native);
        self.native_ids.insert(public, native);
        Ok(public)
    }

    /// Parent a transient toplevel; uses `xdg_dialog_v1` modality when available.
    pub fn create_dialog(
        &mut self,
        parent: SurfaceId,
        attributes: crate::DialogAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        let parent_native = self.native(parent)?;
        let width = attributes
            .toplevel
            .initial_size
            .map(|s| s.width)
            .filter(|&w| w > 0)
            .or_else(|| {
                attributes
                    .toplevel
                    .min_size
                    .map(|s| s.width)
                    .filter(|&w| w > 0)
            })
            .unwrap_or(480)
            .max(1);
        let height = attributes
            .toplevel
            .initial_size
            .map(|s| s.height)
            .filter(|&h| h > 0)
            .or_else(|| {
                attributes
                    .toplevel
                    .min_size
                    .map(|s| s.height)
                    .filter(|&h| h > 0)
            })
            .unwrap_or(360)
            .max(1);
        let native = self
            .shell
            .create_dialog_gpu(
                parent_native,
                attributes.toplevel.title,
                attributes.toplevel.app_id,
                width,
                height,
                attributes.modal,
            )
            .map_err(map_native_error)?;
        if let Some(min) = attributes.toplevel.min_size {
            let _ = self.shell.set_min_size(native, Some(min));
        }
        if let Some(max) = attributes.toplevel.max_size {
            let _ = self.shell.set_max_size(native, Some(max));
        }
        let _ = self
            .shell
            .set_decorations(native, attributes.toplevel.decorations);
        let public = self.surfaces.intern(native);
        self.native_ids.insert(public, native);
        Ok(public)
    }

    pub fn surface_handle(&self, surface: SurfaceId) -> Option<SurfaceHandle> {
        let native = *self.native_ids.get(&surface)?;
        self.shell.public_surface_handle(native).ok()
    }

    pub fn outputs(&self) -> Vec<OutputInfo> {
        self.shell.outputs()
    }

    /// Fill `out` with current outputs (reuses capacity).
    pub fn outputs_into(&self, out: &mut Vec<OutputInfo>) {
        self.shell.outputs_into(out)
    }


    pub fn create_popup(
        &mut self,
        parent: SurfaceId,
        attributes: PopupAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        let parent_native = self.native(parent)?;
        let positioner = to_native_positioner(&attributes.positioner)?;
        let grab = attributes.grab.as_ref().filter(|s| s.is_popup_grab());
        let native = self
            .shell
            .create_popup_gpu(parent_native, &positioner, grab)
            .map_err(map_native_error)?;
        let public = self.surfaces.intern(native);
        self.native_ids.insert(public, native);
        Ok(public)
    }

    pub fn reposition_popup(
        &mut self,
        surface: SurfaceId,
        positioner: &PopupPositioner,
        token: u32,
    ) -> Result<(), RuntimeError> {
        if !self.capabilities.popup_reposition {
            return Err(RuntimeError::Unsupported("xdg-popup reposition"));
        }
        let native = self.native(surface)?;
        let pos = to_native_positioner(positioner)?;
        self.shell
            .reposition_popup(native, &pos, token)
            .map_err(map_native_error)
    }

    pub fn create_layer_surface(
        &mut self,
        attributes: LayerSurfaceAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        if !self.shell.has_layer_shell() {
            return Err(RuntimeError::Unsupported("layer-shell-v1"));
        }
        let output = attributes.output.map(|o| o.get());
        let native = self
            .shell
            .create_layer_surface_full(attributes.namespace, output, attributes.state)
            .map_err(map_native_error)?;
        let public = self.surfaces.intern(native);
        self.native_ids.insert(public, native);
        Ok(public)
    }

    /// Bufferless layer surface for Vulkan / wgpu swapchain present (no SHM fill).
    ///
    /// Prefer this over [`Self::create_layer_surface`] when the client owns
    /// GPU present (e.g. wallpaper engines, custom Vulkan WSI).
    pub fn create_layer_surface_gpu(
        &mut self,
        attributes: LayerSurfaceAttributes,
    ) -> Result<SurfaceId, RuntimeError> {
        if !self.shell.has_layer_shell() {
            return Err(RuntimeError::Unsupported("layer-shell-v1"));
        }
        let output = attributes.output.map(|o| o.get());
        let native = self
            .shell
            .create_layer_surface_gpu(attributes.namespace, output, attributes.state)
            .map_err(map_native_error)?;
        let public = self.surfaces.intern(native);
        self.native_ids.insert(public, native);
        Ok(public)
    }

    pub fn set_layer_surface_state(
        &mut self,
        surface: SurfaceId,
        state: LayerSurfaceState,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_layer_surface_state(native, state)
            .map_err(|e| match e {
                NativeError::Protocol(msg) if msg.contains("unknown layer") => {
                    RuntimeError::InvalidLayerSurfaceTarget(surface)
                }
                other => map_native_error(other),
            })
    }

    pub fn layer_surface_state(
        &self,
        surface: SurfaceId,
    ) -> Result<LayerSurfaceState, RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .layer_surface_state(native)
            .map_err(|_| RuntimeError::InvalidLayerSurfaceTarget(surface))
    }

    pub fn set_title(&mut self, surface: SurfaceId, title: String) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_title(native, title)
            .map_err(map_native_error)
    }

    pub fn set_app_id(
        &mut self,
        surface: SurfaceId,
        app_id: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_app_id(native, app_id)
            .map_err(map_native_error)
    }

    pub fn request_activation_token(
        &mut self,
        surface: SurfaceId,
        attributes: ActivationTokenAttributes,
    ) -> Result<ActivationRequestId, RuntimeError> {
        if !self.shell.has_activation() {
            return Err(RuntimeError::Unsupported("xdg_activation_v1"));
        }
        let native = self.native(surface)?;
        self.shell
            .request_activation_token(native, attributes.app_id.as_deref())
            .map_err(map_native_error)?;
        let request = ActivationRequestId(self.next_activation_request_id);
        self.next_activation_request_id = self.next_activation_request_id.saturating_add(1).max(1);
        self.activation_pending.insert(native, request);
        Ok(request)
    }

    pub fn activate_surface(
        &mut self,
        surface: SurfaceId,
        token: ActivationToken,
    ) -> Result<(), RuntimeError> {
        if !self.shell.has_activation() {
            return Err(RuntimeError::Unsupported("xdg_activation_v1"));
        }
        let native = self.native(surface)?;
        self.shell
            .activate_with_token(native, token.into_raw())
            .map_err(map_native_error)
    }


    pub fn set_min_size(
        &mut self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_min_size(native, size)
            .map_err(map_native_error)
    }

    pub fn set_max_size(
        &mut self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_max_size(native, size)
            .map_err(map_native_error)
    }


    pub fn request_frame(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .request_frame(native)
            .map_err(map_native_error)
    }

    /// Whether a `wl_surface.frame` callback is still outstanding.
    pub fn is_frame_pending(&self, surface: SurfaceId) -> bool {
        self.native(surface)
            .map(|n| self.shell.is_frame_pending(n))
            .unwrap_or(false)
    }

    /// Arm `wp_presentation.feedback` for the next commit (no-op if unsupported).
    pub fn request_presentation_feedback(
        &mut self,
        surface: SurfaceId,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .request_presentation_feedback(native)
            .map_err(map_native_error)
    }

    /// Whether presentation feedback is still outstanding for `surface`.
    pub fn is_presentation_pending(&self, surface: SurfaceId) -> bool {
        self.native(surface)
            .map(|n| self.shell.is_presentation_pending(n))
            .unwrap_or(false)
    }

    /// Presentation clock id from `wp_presentation.clock_id`, if advertised.
    pub fn presentation_clock_id(&self) -> Option<u32> {
        self.shell.presentation_clock_id()
    }

    /// Logical size last known for the surface.
    pub fn logical_size(&self, surface: SurfaceId) -> Option<LogicalSize> {
        let native = self.native(surface).ok()?;
        self.shell.logical_size(native)
    }

    /// Fractional / integer scale factor for the surface.
    pub fn scale_factor(&self, surface: SurfaceId) -> Option<f64> {
        let native = self.native(surface).ok()?;
        self.shell.scale_factor(native)
    }

    /// Physical buffer size suggestion (`ceil(logical × scale)`).
    pub fn buffer_size(&self, surface: SurfaceId) -> Option<(u32, u32)> {
        let native = self.native(surface).ok()?;
        self.shell.buffer_size(native)
    }

    /// Surface role (toplevel / dialog / popup / layer).
    pub fn surface_kind(&self, surface: SurfaceId) -> Option<crate::surface::SurfaceKind> {
        let native = self.native(surface).ok()?;
        self.shell.surface_kind(native)
    }

    /// Double-buffered opaque region (`wl_surface.set_opaque_region`).
    pub fn set_opaque_region(
        &mut self,
        surface: SurfaceId,
        region: crate::SurfaceRegion,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_opaque_region(native, region)
            .map_err(map_native_error)
    }

    /// Double-buffered input region (`wl_surface.set_input_region`).
    ///
    /// [`crate::SurfaceRegion::Empty`] enables pointer passthrough.
    pub fn set_input_region(
        &mut self,
        surface: SurfaceId,
        region: crate::SurfaceRegion,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_input_region(native, region)
            .map_err(map_native_error)
    }

    /// Look up an output by compositor-advertised name (`wl_output.name`).
    pub fn find_output_by_name(&self, name: &str) -> Option<OutputInfo> {
        self.shell.find_output_by_name(name)
    }

    pub fn commit(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .commit_surface(native)
            .map_err(map_native_error)
    }

    pub fn set_buffer_scale(&mut self, surface: SurfaceId, factor: i32) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_buffer_scale(native, factor)
            .map_err(map_native_error)
    }

    pub fn set_viewport_destination(
        &mut self,
        surface: SurfaceId,
        size: Option<LogicalSize>,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        if let Some(size) = size {
            self.shell
                .set_viewport_destination(native, size.width as i32, size.height as i32)
                .map_err(map_native_error)?;
        }
        Ok(())
    }

    pub fn set_window_geometry(
        &mut self,
        surface: SurfaceId,
        origin: LogicalPosition,
        size: LogicalSize,
    ) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell
            .set_window_geometry(native, origin, size)
            .map_err(map_native_error)?;
        // Keep viewporter destination in sync for fractional-scale clients.
        self.set_viewport_destination(surface, Some(size))
    }

    pub fn destroy_surface(&mut self, surface: SurfaceId) -> Result<Vec<SurfaceId>, RuntimeError> {
        let Some(native) = self.native_ids.remove(&surface) else {
            return Err(RuntimeError::SurfaceNotFound(surface));
        };
        self.surfaces.remove(native);
        self.activation_pending.remove(&native);
        // Try role-specific destroy: toplevel, popup, then layer.
        if self.shell.destroy_toplevel(native).is_ok() {
            return Ok(vec![surface]);
        }
        if self.shell.destroy_popup(native).is_ok() {
            return Ok(vec![surface]);
        }
        self.shell
            .destroy_layer_surface(native)
            .map_err(map_native_error)?;
        Ok(vec![surface])
    }

    pub fn set_text_input_state(
        &mut self,
        surface: SurfaceId,
        state: Option<&TextInputState>,
    ) -> Result<(), RuntimeError> {
        if !self.shell.has_text_input() {
            return Err(RuntimeError::Unsupported("text_input_v3"));
        }
        let native = self.native(surface)?;
        match state {
            Some(state) => self
                .shell
                .set_text_input_state(native, state)
                .map_err(map_native_error),
            None => self.shell.disable_text_input().map_err(map_native_error),
        }
    }

    pub fn request_user_attention(&mut self, surface: SurfaceId) -> Result<(), RuntimeError> {
        if !self.shell.has_activation() {
            return Err(RuntimeError::Unsupported("xdg_activation_v1"));
        }
        let native = self.native(surface)?;
        self.shell
            .request_activation_token(native, None)
            .map_err(map_native_error)
    }

    pub fn set_blur(&mut self, surface: SurfaceId, state: BlurState) -> Result<(), RuntimeError> {
        let native = self.native(surface)?;
        self.shell.set_blur(native, state).map_err(|e| match e {
            NativeError::Protocol(msg) if msg.contains("blur capability") => {
                RuntimeError::Unsupported("ext-background-effect-v1 blur")
            }
            other => map_native_error(other),
        })
    }

    pub fn set_toplevel_icon(
        &mut self,
        surface: SurfaceId,
        icon: Option<ToplevelIcon>,
    ) -> Result<(), RuntimeError> {
        if !self.shell.has_toplevel_icon() {
            return Err(RuntimeError::Unsupported("xdg-toplevel-icon-v1"));
        }
        let native = self.native(surface)?;
        self.shell
            .set_toplevel_icon(native, icon)
            .map_err(map_native_error)
    }



    pub(crate) fn native(&self, surface: SurfaceId) -> Result<NativeSurfaceId, RuntimeError> {
        self.native_ids
            .get(&surface)
            .copied()
            .ok_or(RuntimeError::SurfaceNotFound(surface))
    }
}

pub(crate) fn map_native_error(error: NativeError) -> RuntimeError {
    match error {
        NativeError::Connect(msg) => RuntimeError::Connect(msg),
        NativeError::Registry(msg) => RuntimeError::Registry(msg),
        NativeError::Protocol(msg) => RuntimeError::Protocol(msg),
        NativeError::Io(msg) => RuntimeError::EventLoop(msg),
    }
}

/// Race display readability against the wake eventfd on Compio's proactor.
async fn race_display_or_wake(
    display: &CompioFdReady,
    wake: &CompioFdReady,
) -> Result<WaitSource, RuntimeError> {
    poll_fn(|cx| {
        // Prefer display so we drain compositor traffic when both are ready.
        match display.poll_read_ready(cx) {
            Poll::Ready(Ok(())) => return Poll::Ready(Ok(WaitSource::Display)),
            Poll::Ready(Err(err)) => {
                return Poll::Ready(Err(RuntimeError::EventLoop(err.to_string())));
            }
            Poll::Pending => {}
        }
        match wake.poll_read_ready(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(WaitSource::Wake)),
            Poll::Ready(Err(err)) => Poll::Ready(Err(RuntimeError::EventLoop(err.to_string()))),
            Poll::Pending => Poll::Pending,
        }
    })
    .await
}

fn to_native_positioner(p: &PopupPositioner) -> Result<NativePopupPositioner, RuntimeError> {
    if p.size.width == 0 || p.size.height == 0 {
        return Err(RuntimeError::Protocol(
            "popup positioner size must be non-zero".into(),
        ));
    }
    Ok(NativePopupPositioner {
        size: p.size,
        anchor_rect: p.anchor_rect,
        anchor: p.anchor,
        gravity: p.gravity,
        constraints: p.constraints,
        offset: p.offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::LogicalSize;
    use crate::surface::{DecorationPreference, ToplevelAttributes};

    #[test]
    fn native_runtime_connects_and_creates_toplevel_when_display_present() {
        let Ok(mut runtime) = NativeRuntime::connect() else {
            return;
        };
        let caps = runtime.capabilities();
        // Core desktop stack should always be present on a real compositor.
        let _ = (
            caps.fractional_scale,
            caps.cursor_shape,
            caps.text_input_v3,
            caps.pointer_gestures_v1,
            caps.pointer_constraints_v1,
            caps.xdg_dialog_v1,
            caps.xdg_toplevel_icon_v1,
            caps.ext_background_effect,
        );

        let surface = runtime
            .create_toplevel(ToplevelAttributes {
                title: "native-runtime-smoke".into(),
                app_id: "dev.fika.NativeRuntimeSmoke".into(),
                initial_size: Some(LogicalSize::new(320, 240)),
                min_size: Some(LogicalSize::new(160, 120)),
                max_size: None,
                decorations: DecorationPreference::Server,
            })
            .expect("create toplevel");
        assert!(runtime.surface_handle(surface).is_some());
        runtime
            .set_title(surface, "native-runtime-retitled".into())
            .expect("set title");
        runtime.request_frame(surface).expect("frame");
        assert!(runtime.is_frame_pending(surface));
        // Coalesce while pending.
        runtime.request_frame(surface).expect("frame again");
        assert!(runtime.is_frame_pending(surface));
        assert_eq!(
            runtime.logical_size(surface),
            Some(LogicalSize::new(320, 240))
        );
        assert!(runtime.buffer_size(surface).is_some());
        assert_eq!(
            runtime.surface_kind(surface),
            Some(crate::surface::SurfaceKind::Toplevel)
        );
        runtime.commit(surface).expect("commit");
        // Non-blocking poll should not hang.
        runtime
            .dispatch(Some(Duration::from_millis(0)))
            .expect("dispatch");
        let mut events = Vec::new();
        runtime.drain_events_into(&mut events);
        runtime.destroy_surface(surface).expect("destroy");
    }

    #[test]
    fn native_runtime_interactive_apis_require_serial() {
        let Ok(mut runtime) = NativeRuntime::connect() else {
            return;
        };
        let surface = runtime
            .create_toplevel(ToplevelAttributes {
                title: "serial".into(),
                app_id: "dev.fika.Serial".into(),
                ..Default::default()
            })
            .expect("toplevel");
        assert!(runtime.begin_interactive_move(surface).is_err());
        assert!(runtime
            .begin_interactive_resize(surface, crate::ResizeEdge::Bottom)
            .is_err());
        let _ = runtime.destroy_surface(surface);
    }

    #[test]
    fn native_runtime_popup_layer_outputs_and_app_id() {
        use crate::layer_shell::{LayerSurfaceAttributes, LayerSurfaceState};
        use crate::surface::{PopupAttributes, PopupPositioner};

        let Ok(mut runtime) = NativeRuntime::connect() else {
            return;
        };
        let _ = runtime.outputs();
        let parent = runtime
            .create_toplevel(ToplevelAttributes {
                title: "parent".into(),
                app_id: "dev.fika.Parent".into(),
                initial_size: Some(LogicalSize::new(400, 300)),
                ..Default::default()
            })
            .expect("parent");
        runtime
            .set_app_id(parent, "dev.fika.ParentRenamed")
            .expect("app_id");

        let mut positioner = PopupPositioner::default();
        positioner.size = LogicalSize::new(120, 80);
        positioner.anchor_rect =
            crate::geometry::LogicalRect::new(0, 0, 40, 20);
        let popup = runtime
            .create_popup(
                parent,
                PopupAttributes {
                    positioner: positioner.clone(),
                    grab: None,
                },
            )
            .expect("popup");
        if runtime.capabilities().popup_reposition {
            let _ = runtime.reposition_popup(popup, &positioner, 1);
        }
        runtime.destroy_surface(popup).expect("destroy popup");

        if runtime.capabilities().layer_shell_v1 {
            let layer = runtime
                .create_layer_surface(LayerSurfaceAttributes {
                    namespace: "fika-native-layer".into(),
                    output: None,
                    state: LayerSurfaceState {
                        size: LogicalSize::new(200, 32),
                        ..Default::default()
                    },
                })
                .expect("layer");
            let st = runtime.layer_surface_state(layer).expect("state");
            assert_eq!(st.size.width, 200);
            runtime.destroy_surface(layer).expect("destroy layer");
        }

        if runtime.capabilities().xdg_activation_v1 {
            let _ = runtime.request_activation_token(
                parent,
                crate::ActivationTokenAttributes::default(),
            );
        }

        runtime.destroy_surface(parent).expect("destroy parent");
    }

    #[test]
    fn dispatch_compio_wait_returns_on_timeout_and_zero() {
        let Ok(mut runtime) = NativeRuntime::connect() else {
            return;
        };
        let surface = runtime
            .create_toplevel(ToplevelAttributes {
                title: "dispatch-wait".into(),
                app_id: "dev.fika.DispatchWait".into(),
                initial_size: Some(LogicalSize::new(320, 240)),
                ..Default::default()
            })
            .expect("toplevel");
        // Non-blocking must return.
        runtime
            .dispatch(Some(Duration::from_millis(0)))
            .expect("zero");
        // Short timeout must return (Compio timer), not hang forever.
        let start = std::time::Instant::now();
        runtime
            .dispatch(Some(Duration::from_millis(50)))
            .expect("timeout");
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(500),
            "dispatch timeout took {elapsed:?}"
        );
        // Infinite wait would hang; only exercise with wake from another thread.
        let wake = runtime.wake_handle();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            wake.wake();
        });
        let start = std::time::Instant::now();
        runtime.dispatch(None).expect("wake");
        let elapsed = start.elapsed();
        handle.join().unwrap();
        assert!(
            elapsed < Duration::from_millis(500),
            "dispatch None + wake took {elapsed:?}"
        );
        let _ = runtime.destroy_surface(surface);
    }

}
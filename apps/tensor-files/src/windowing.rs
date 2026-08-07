//! Tensor Files window, input, clipboard, and event-loop integration.
//!
//! The reusable protocol and event machinery lives in `wayland-client-runtime`.
//! This module only translates those Wayland-native events into Tensor Files's input
//! vocabulary and owns the application's scheduling policy.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::rc::Rc;
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(test)]
use wayland_client_runtime::PointerAxisValue;
use wayland_client_runtime::{
    BlurRegion, BlurState, CursorIcon as RuntimeCursorIcon, DecorationPreference, DialogAttributes,
    DndAction as RuntimeDndAction, DndActions as RuntimeDndActions, DndEvent,
    DndIcon as RuntimeDndIcon, DndOfferId, DndSourceId, Event, KeyboardEvent, LogicalPosition,
    LogicalSize, MimePayload, NativeRuntime, PointerEventKind, PointerGestureEvent,
    PointerPinchEvent, PointerSwipeEvent, RuntimeError, SeatId, SurfaceEvent, SurfaceHandle,
    SurfaceId, TextInputChangeCause as RuntimeTextInputChangeCause,
    TextInputContentHint as RuntimeTextInputContentHint,
    TextInputContentPurpose as RuntimeTextInputContentPurpose,
    TextInputContentType as RuntimeTextInputContentType, TextInputEvent as RuntimeTextInputEvent,
    TextInputState as RuntimeTextInputState,
    TextInputSurroundingText as RuntimeTextInputSurroundingText, ToplevelAttributes,
    ToplevelIcon as RuntimeToplevelIcon, TransferContent, WakeHandle,
};
include!("windowing_runtime.rs");
include!("windowing_types.rs");
include!("windowing_text_input.rs");
include!("windowing_clipboard.rs");
#[derive(Clone, Debug)]
pub struct WindowAttributes {
    title: String,
    app_id: String,
    surface_size: PhysicalSize<u32>,
    min_surface_size: Option<PhysicalSize<u32>>,
    max_surface_size: Option<PhysicalSize<u32>>,
    decorations: DecorationPreference,
    dialog: bool,
    modal: bool,
}

impl Default for WindowAttributes {
    fn default() -> Self {
        Self {
            title: String::new(),
            app_id: "tensor-files".to_string(),
            surface_size: PhysicalSize::new(1, 1),
            min_surface_size: None,
            max_surface_size: None,
            decorations: DecorationPreference::Server,
            dialog: false,
            modal: false,
        }
    }
}

impl WindowAttributes {
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_app_id(mut self, app_id: impl Into<String>) -> Self {
        self.app_id = app_id.into();
        self
    }

    pub fn with_transparent(self, _transparent: bool) -> Self {
        self
    }

    pub fn with_surface_size(mut self, size: PhysicalSize<u32>) -> Self {
        self.surface_size = size;
        self
    }

    pub fn with_min_surface_size(mut self, size: PhysicalSize<u32>) -> Self {
        self.min_surface_size = Some(size);
        self
    }

    pub fn with_max_surface_size(mut self, size: PhysicalSize<u32>) -> Self {
        self.max_surface_size = Some(size);
        self
    }

    pub fn with_resizable(self, _resizable: bool) -> Self {
        self
    }

    pub fn with_theme(self, _theme: Option<Theme>) -> Self {
        self
    }

    pub fn with_dialog(mut self, modal: bool) -> Self {
        self.dialog = true;
        self.modal = modal;
        self
    }
}

struct WindowState {
    logical_size: LogicalSize,
    physical_size: PhysicalSize<u32>,
    scale_factor: f64,
    configured: bool,
    redraw_requested: bool,
    destroy_requested: bool,
}

impl WindowState {
    fn mark_destroy_requested(&mut self) -> bool {
        if self.destroy_requested {
            false
        } else {
            self.destroy_requested = true;
            true
        }
    }
}

enum RuntimeCommand {
    SetTitle(SurfaceId, String),
    SetMinSize(SurfaceId, Option<LogicalSize>),
    SetMaxSize(SurfaceId, Option<LogicalSize>),
    SetBlur(SurfaceId, BlurState),
    SetCursor(CursorIcon),
    SetIme(SurfaceId, Option<ImeState>),
    RequestUserAttention(SurfaceId),
    Destroy(SurfaceId),
}

struct LoopShared {
    wake: WakeHandle,
    commands: Mutex<Vec<RuntimeCommand>>,
    synthetic_events: Mutex<Vec<SyntheticEvent>>,
}

struct SyntheticEvent {
    window: WindowId,
    event: WindowEvent,
    completed_offer: Option<DndOfferId>,
}

struct ActiveDndTransfer {
    offer: DndOfferId,
    window: WindowId,
    hints: Vec<TypeHint>,
    dropped: bool,
    read_complete: bool,
}

impl LoopShared {
    fn push(&self, command: RuntimeCommand) {
        self.commands
            .lock()
            .expect("Wayland command queue mutex poisoned")
            .push(command);
        self.wake.wake();
    }

    fn push_ime(&self, surface: SurfaceId, state: Option<ImeState>) {
        let mut commands = self
            .commands
            .lock()
            .expect("Wayland command queue mutex poisoned");
        queue_ime_command(&mut commands, surface, state);
        drop(commands);
        self.wake.wake();
    }
}

fn queue_ime_command(
    commands: &mut Vec<RuntimeCommand>,
    surface: SurfaceId,
    state: Option<ImeState>,
) {
    for command in commands.iter_mut().rev() {
        match command {
            RuntimeCommand::SetIme(candidate, pending) if *candidate == surface => {
                *pending = state;
                return;
            }
            RuntimeCommand::Destroy(candidate) if *candidate == surface => break,
            _ => {}
        }
    }
    commands.push(RuntimeCommand::SetIme(surface, state));
}

pub struct Window {
    id: SurfaceId,
    handle: SurfaceHandle,
    state: Mutex<WindowState>,
    shared: Arc<LoopShared>,
}

impl fmt::Debug for Window {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Window")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Window {
    pub fn id(&self) -> WindowId {
        self.id
    }

    pub fn surface_handle(&self) -> SurfaceHandle {
        self.handle.clone()
    }

    pub fn surface_size(&self) -> PhysicalSize<u32> {
        self.state
            .lock()
            .expect("Wayland window state mutex poisoned")
            .physical_size
    }

    pub fn scale_factor(&self) -> f64 {
        self.state
            .lock()
            .expect("Wayland window state mutex poisoned")
            .scale_factor
    }

    pub fn request_redraw(&self) {
        self.state
            .lock()
            .expect("Wayland window state mutex poisoned")
            .redraw_requested = true;
        self.shared.wake.wake();
    }

    pub fn set_title(&self, title: &str) {
        self.shared
            .push(RuntimeCommand::SetTitle(self.id, title.to_string()));
    }

    pub fn set_blur(&self, enabled: bool) {
        let state = if enabled {
            BlurState::Enabled(BlurRegion::EntireSurface)
        } else {
            BlurState::Disabled
        };
        self.set_blur_state(state);
    }

    pub fn set_blur_state(&self, state: BlurState) {
        self.shared.push(RuntimeCommand::SetBlur(self.id, state));
    }

    pub fn set_min_surface_size(&self, size: Option<PhysicalSize<u32>>) {
        let scale = self.scale_factor();
        self.shared.push(RuntimeCommand::SetMinSize(
            self.id,
            size.map(|s| physical_to_logical_rounded(s, scale)),
        ));
    }

    pub fn set_max_surface_size(&self, size: Option<PhysicalSize<u32>>) {
        let scale = self.scale_factor();
        self.shared.push(RuntimeCommand::SetMaxSize(
            self.id,
            size.map(|s| physical_to_logical_rounded(s, scale)),
        ));
    }

    pub fn request_surface_size(&self, size: PhysicalSize<u32>) -> Option<PhysicalSize<u32>> {
        let mut state = self
            .state
            .lock()
            .expect("Wayland window state mutex poisoned");
        state.logical_size = physical_to_logical_rounded(size, state.scale_factor);
        state.physical_size = size;
        Some(size)
    }

    pub fn set_resizable(&self, _resizable: bool) {}

    pub fn set_theme(&self, _theme: Option<Theme>) {}

    pub fn set_cursor(&self, cursor: CursorIcon) {
        self.shared.push(RuntimeCommand::SetCursor(cursor));
    }

    pub fn set_ime_state(&self, state: Option<ImeState>) {
        self.shared.push_ime(self.id, state);
    }

    pub fn focus_window(&self) {}

    pub fn request_user_attention(&self) {
        self.shared
            .push(RuntimeCommand::RequestUserAttention(self.id));
    }

    /// Queue destruction of the native surface exactly once.
    ///
    /// Explicit shutdown paths and `Window::drop` converge here. Callers must
    /// release renderer-owned Vulkan surfaces before requesting native surface
    /// destruction.
    pub fn destroy_surface(&self) {
        let mut state = self
            .state
            .lock()
            .expect("Wayland window state mutex poisoned");
        if !state.mark_destroy_requested() {
            return;
        }
        drop(state);
        self.shared.push(RuntimeCommand::Destroy(self.id));
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.destroy_surface();
    }
}

#[derive(Clone)]
pub struct EventLoopProxy {
    wake: WakeHandle,
}

impl EventLoopProxy {
    pub fn wake_up(&self) {
        self.wake.wake();
    }
}

pub trait ApplicationHandler {
    fn proxy_wake_up(&mut self, _event_loop: &ActiveEventLoop) {}
    fn can_create_surfaces(&mut self, event_loop: &ActiveEventLoop);
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop);
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    );
    /// Compositor dmabuf feedback updated (default or surface-scoped).
    ///
    /// Default no-op; Tensor Files logs import readiness when Vulkan + feedback align.
    fn dmabuf_feedback_updated(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _surface: Option<WindowId>,
    ) {
    }
}

pub struct ActiveEventLoop {
    runtime: Rc<RefCell<WindowRuntime>>,
    shared: Arc<LoopShared>,
    windows: Rc<RefCell<HashMap<SurfaceId, Weak<Window>>>>,
    primary_surface: Cell<Option<SurfaceId>>,
    dnd_transfers: RefCell<HashMap<DataTransferId, ActiveDndTransfer>>,
    dnd_sources: RefCell<HashMap<DndSourceId, WindowId>>,
    next_async_serial: Cell<u64>,
    control_flow: Cell<ControlFlow>,
    exiting: Cell<bool>,
    /// Seat that last produced pointer enter/motion (for cursor shape routing).
    last_pointer_seat: Cell<Option<SeatId>>,
    /// Latest default `zwp_linux_dmabuf` feedback (format table + tranches).
    dmabuf_default_feedback: RefCell<Option<wayland_client_runtime::DmabufFeedback>>,
    /// Per-surface feedback snapshots.
    dmabuf_surface_feedback: RefCell<HashMap<SurfaceId, wayland_client_runtime::DmabufFeedback>>,
}

impl ActiveEventLoop {
    /// Whether the event loop is using the SCTK-free native backend.
    #[allow(dead_code)] // public diagnostic API for smoke / future UI
    pub fn uses_native_backend(&self) -> bool {
        self.runtime.borrow().is_native()
    }

    /// Preferred square icon edge sizes advertised by the compositor, if any.
    #[allow(dead_code)] // public for icon pickers / smoke
    pub fn preferred_toplevel_icon_sizes(&self) -> Vec<u32> {
        self.runtime.borrow().preferred_toplevel_icon_sizes()
    }

    /// Latest default linux-dmabuf feedback, if the compositor has sent any.
    #[allow(dead_code)]
    pub fn dmabuf_default_feedback(&self) -> Option<wayland_client_runtime::DmabufFeedback> {
        self.dmabuf_default_feedback.borrow().clone()
    }

    /// Latest surface-scoped linux-dmabuf feedback, falling back to default.
    #[allow(dead_code)]
    pub fn dmabuf_feedback_for(
        &self,
        surface: SurfaceId,
    ) -> Option<wayland_client_runtime::DmabufFeedback> {
        self.dmabuf_surface_feedback
            .borrow()
            .get(&surface)
            .cloned()
            .or_else(|| self.dmabuf_default_feedback.borrow().clone())
    }

    /// Prefer a Vulkan-importable format from cached feedback (surface then default).
    #[allow(dead_code)]
    pub fn preferred_dmabuf_import_format(
        &self,
        surface: Option<SurfaceId>,
    ) -> Option<wayland_client_runtime::DmabufFormat> {
        let feedback = match surface {
            Some(id) => self.dmabuf_feedback_for(id),
            None => self.dmabuf_default_feedback(),
        }?;
        crate::ui::render::dmabuf::pick_import_format(&feedback)
    }

    /// Whether `zwp_linux_dmabuf_v1` is bound on the connection.
    pub fn has_linux_dmabuf(&self) -> bool {
        self.runtime.borrow().has_linux_dmabuf()
    }

    /// Arm present pacing before the renderer's next buffer commit/present.
    ///
    /// Prefers `wp_presentation` feedback, falls back to `wl_surface.frame`,
    /// then flushes so the compositor sees the callback on the same commit.
    pub fn pre_present_notify(&self, surface: WindowId) {
        let mut runtime = self.runtime.borrow_mut();
        if let Err(error) = runtime.arm_present_notify(surface) {
            eprintln!("[tensor-files-wayland] present notification failed: {error}");
            return;
        }
        if let Err(error) = runtime.flush() {
            eprintln!("[tensor-files-wayland] present flush failed: {error}");
        }
    }

    pub fn create_window(&self, attributes: WindowAttributes) -> Result<Arc<Window>, RuntimeError> {
        let WindowAttributes {
            title,
            app_id,
            surface_size,
            min_surface_size,
            max_surface_size,
            decorations,
            dialog,
            modal,
        } = attributes;
        let window_icon = RuntimeToplevelIcon::from_name(app_id.clone())
            .map_err(|error| RuntimeError::Protocol(error.to_string()))?;
        // `surface_size` is physical (pixels). Convert to Wayland logical units
        // with the best scale we know so far. Prefer the primary window's scale
        // (dialogs inherit the main display scale); otherwise max output scale.
        let scale_factor = self.best_initial_scale_factor();
        let logical_size = physical_to_logical_rounded(surface_size, scale_factor);
        let min_size = min_surface_size.map(|s| physical_to_logical_rounded(s, scale_factor));
        let max_size = max_surface_size.map(|s| physical_to_logical_rounded(s, scale_factor));
        let physical_size = logical_to_physical_rounded(logical_size, scale_factor);
        let toplevel = ToplevelAttributes {
            title,
            app_id,
            initial_size: Some(logical_size),
            min_size,
            max_size,
            decorations,
        };
        let id = if dialog {
            let parent = self.primary_surface.get().ok_or_else(|| {
                RuntimeError::Protocol("dialog has no parent surface".to_string())
            })?;
            self.runtime
                .borrow_mut()
                .create_dialog(parent, DialogAttributes { toplevel, modal })?
        } else {
            let id = self.runtime.borrow_mut().create_toplevel(toplevel)?;
            if self.primary_surface.get().is_none() {
                self.primary_surface.set(Some(id));
            }
            // Subscribe main toplevels to touchpad gestures (pinch → zoom).
            // Unsupported compositors leave the surface at zero gesture overhead.
            match self
                .runtime
                .borrow_mut()
                .set_pointer_gestures_enabled(id, true)
            {
                Ok(()) | Err(RuntimeError::Unsupported(_)) => {}
                Err(error) => {
                    eprintln!("[tensor-files-wayland] pointer-gestures enable failed: {error}");
                }
            }
            // Request dmabuf feedback early so GPU import can negotiate formats
            // with the compositor (v4+). Best-effort: missing global is fine.
            {
                let mut runtime = self.runtime.borrow_mut();
                if runtime.has_linux_dmabuf() {
                    match runtime.request_dmabuf_default_feedback() {
                        Ok(()) | Err(RuntimeError::Unsupported(_)) => {}
                        Err(error) => {
                            eprintln!(
                                "[tensor-files-wayland] dmabuf default feedback failed: {error}"
                            );
                        }
                    }
                    match runtime.request_dmabuf_surface_feedback(id) {
                        Ok(()) | Err(RuntimeError::Unsupported(_)) => {}
                        Err(error) => {
                            eprintln!(
                                "[tensor-files-wayland] dmabuf surface feedback failed: {error}"
                            );
                        }
                    }
                }
            }
            id
        };
        match self
            .runtime
            .borrow_mut()
            .set_toplevel_icon(id, Some(window_icon))
        {
            Ok(()) | Err(RuntimeError::Unsupported(_)) => {}
            Err(error) => return Err(error),
        }
        let handle = self
            .runtime
            .borrow()
            .surface_handle(id)
            .ok_or(RuntimeError::SurfaceNotFound(id))?;
        let window = Arc::new(Window {
            id,
            handle,
            state: Mutex::new(WindowState {
                logical_size,
                physical_size,
                scale_factor,
                configured: false,
                redraw_requested: true,
                destroy_requested: false,
            }),
            shared: self.shared.clone(),
        });
        self.windows
            .borrow_mut()
            .insert(id, Arc::downgrade(&window));
        Ok(window)
    }

    /// Best-effort scale for a newly created surface before fractional-scale
    /// events arrive. Dialogs inherit the primary window's scale; otherwise
    /// use the highest advertised output scale (integer).
    fn best_initial_scale_factor(&self) -> f64 {
        if let Some(primary) = self.primary_surface.get()
            && let Some(window) = self
                .windows
                .borrow()
                .get(&primary)
                .and_then(|weak| weak.upgrade())
        {
            let scale = window.scale_factor();
            if scale.is_finite() && scale > 0.0 {
                return scale;
            }
        }
        self.runtime
            .borrow()
            .outputs()
            .into_iter()
            .map(|output| f64::from(output.scale_factor.max(1)))
            .fold(1.0_f64, f64::max)
    }

    pub fn set_control_flow(&self, control_flow: ControlFlow) {
        self.control_flow.set(control_flow);
    }

    pub fn exit(&self) {
        self.exiting.set(true);
        self.shared.wake.wake();
    }

    pub fn start_drag(
        &self,
        window: WindowId,
        data: DataTransferSend,
        actions: &[DndAction],
        icon: Option<DragIcon>,
    ) -> Result<DataTransferId, String> {
        self.windows
            .borrow()
            .get(&window)
            .and_then(Weak::upgrade)
            .ok_or_else(|| "drag origin surface no longer exists".to_string())?;
        let content = TransferContent::new(
            data.payloads
                .into_iter()
                .map(|(hint, bytes)| {
                    MimePayload::new(hint.mime(), bytes).map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(|error| error.to_string())?;
        let icon = icon
            .map(|icon| {
                let offset = LogicalPosition::new(icon.offset_x, icon.offset_y);
                RuntimeDndIcon::from_dmabuf(icon.buffer, icon.buffer_scale, offset)
                    .map_err(str::to_string)
            })
            .transpose()?;
        let source = self
            .runtime
            .borrow_mut()
            .start_drag(window, content, runtime_dnd_actions(actions), icon)
            .map_err(|error| error.to_string())?;
        self.dnd_sources.borrow_mut().insert(source, window);
        Ok(DataTransferId(source.get()))
    }

    pub fn data_transfer(&self, id: DataTransferId) -> Result<DataTransfer, String> {
        self.dnd_transfers
            .borrow()
            .get(&id)
            .map(|transfer| DataTransfer {
                hints: transfer.hints.clone(),
            })
            .ok_or_else(|| format!("DnD transfer {} does not exist", id.into_raw()))
    }

    pub fn fetch_data_transfer(
        &self,
        id: DataTransferId,
        hint: &TypeHint,
    ) -> Result<AsyncRequestSerial, String> {
        let (offer, window) = self
            .dnd_transfers
            .borrow()
            .get(&id)
            .map(|transfer| (transfer.offer, transfer.window))
            .ok_or_else(|| format!("DnD transfer {} does not exist", id.into_raw()))?;
        let mut pipe = self
            .runtime
            .borrow_mut()
            .receive_dnd(offer, hint.mime())
            .map_err(|error| error.to_string())?;
        let serial = AsyncRequestSerial(self.next_async_serial.get());
        self.next_async_serial
            .set(self.next_async_serial.get().wrapping_add(1));
        let shared = self.shared.clone();
        let hint = hint.clone();
        thread::Builder::new()
            .name("tensor-files-wayland-dnd-read".to_string())
            .spawn(move || {
                let mut bytes = Vec::new();
                let result = pipe
                    .read_to_end(&mut bytes)
                    .map(|_| bytes)
                    .map_err(|error| error.to_string());
                let value: Arc<dyn TypedData> = Arc::new(ReceivedTypedData { hint, result });
                shared
                    .synthetic_events
                    .lock()
                    .expect("Wayland synthetic event queue mutex poisoned")
                    .push(SyntheticEvent {
                        window,
                        event: WindowEvent::DataTransferReceived { id, serial, value },
                        completed_offer: Some(offer),
                    });
                shared.wake.wake();
            })
            .map_err(|error| error.to_string())?;
        Ok(serial)
    }

    pub fn set_valid_dnd_actions(
        &self,
        id: DataTransferId,
        actions: &[DndAction],
    ) -> Result<(), String> {
        let (offer, accepted_mime) = {
            let transfers = self.dnd_transfers.borrow();
            let transfer = transfers
                .get(&id)
                .ok_or_else(|| format!("DnD transfer {} does not exist", id.into_raw()))?;
            // TypeHint::mime is &'static str — no Vec clone of hints.
            let accepted_mime = (!actions.is_empty())
                .then(|| {
                    transfer
                        .hints
                        .iter()
                        .find(|hint| **hint == TypeHint::UriList)
                })
                .flatten()
                .map(TypeHint::mime);
            (transfer.offer, accepted_mime)
        };
        self.runtime
            .borrow_mut()
            .set_dnd_offer_actions(
                offer,
                accepted_mime,
                runtime_dnd_actions(actions),
                preferred_runtime_dnd_action(actions),
            )
            .map_err(|error| error.to_string())
    }
}

pub struct EventLoop {
    active: ActiveEventLoop,
}

include!("windowing_event_loop.rs");

mod event_map;
use event_map::*;

#[cfg(test)]
include!("windowing_scaling_tests.rs");

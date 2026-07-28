//! Fika's Wayland-facing platform adapter.
//!
//! The reusable protocol and event machinery lives in `wayland-client-runtime`.
//! This module only translates those Wayland-native events into Fika's input
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

use wayland_client_runtime::{
    BlurRegion, BlurState, CursorIcon as RuntimeCursorIcon, DecorationPreference, DialogAttributes,
    DndAction as RuntimeDndAction, DndActions as RuntimeDndActions, DndEvent,
    DndIcon as RuntimeDndIcon, DndOfferId, DndSourceId, Event, KeyState, KeyboardEvent,
    LogicalPosition, LogicalSize, MimePayload, NativeRuntime, PointerAxisValue, PointerEventKind,
    PointerGestureEvent, PointerPinchEvent, PointerSwipeEvent, RuntimeError, SeatId, SurfaceEvent,
    SurfaceHandle, SurfaceId, TextInputChangeCause as RuntimeTextInputChangeCause,
    TextInputContentHint as RuntimeTextInputContentHint,
    TextInputContentPurpose as RuntimeTextInputContentPurpose,
    TextInputContentType as RuntimeTextInputContentType, TextInputEvent as RuntimeTextInputEvent,
    TextInputState as RuntimeTextInputState,
    TextInputSurroundingText as RuntimeTextInputSurroundingText, ToplevelAttributes,
    ToplevelIcon as RuntimeToplevelIcon, TransferContent, WakeHandle,
};
include!("platform_backend.rs");
include!("platform_types.rs");
include!("platform_text_input.rs");
include!("platform_clipboard.rs");
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
            app_id: "fika".to_string(),
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

pub struct WaylandWindow {
    id: SurfaceId,
    handle: SurfaceHandle,
    state: Mutex<WindowState>,
    shared: Arc<LoopShared>,
}

impl fmt::Debug for WaylandWindow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WaylandWindow")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl WaylandWindow {
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
}

impl Drop for WaylandWindow {
    fn drop(&mut self) {
        self.shared.push(RuntimeCommand::Destroy(self.id));
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
    /// Default no-op; Fika logs import readiness when Vulkan + feedback align.
    fn dmabuf_feedback_updated(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _surface: Option<WindowId>,
    ) {
    }
}

pub struct ActiveEventLoop {
    runtime: Rc<RefCell<PlatformBackend>>,
    shared: Arc<LoopShared>,
    windows: Rc<RefCell<HashMap<SurfaceId, Weak<WaylandWindow>>>>,
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

    /// Prefer a wgpu-importable format from cached feedback (surface then default).
    #[allow(dead_code)]
    pub fn preferred_dmabuf_import_format(
        &self,
        surface: Option<SurfaceId>,
    ) -> Option<wayland_client_runtime::DmabufFormat> {
        let feedback = match surface {
            Some(id) => self.dmabuf_feedback_for(id),
            None => self.dmabuf_default_feedback(),
        }?;
        crate::shell::render::dmabuf::pick_import_format(&feedback)
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
            eprintln!("[fika-wayland] present notification failed: {error}");
            return;
        }
        if let Err(error) = runtime.flush() {
            eprintln!("[fika-wayland] present flush failed: {error}");
        }
    }

    pub fn create_window(
        &self,
        attributes: WindowAttributes,
    ) -> Result<Arc<WaylandWindow>, RuntimeError> {
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
                    eprintln!("[fika-wayland] pointer-gestures enable failed: {error}");
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
                            eprintln!("[fika-wayland] dmabuf default feedback failed: {error}");
                        }
                    }
                    match runtime.request_dmabuf_surface_feedback(id) {
                        Ok(()) | Err(RuntimeError::Unsupported(_)) => {}
                        Err(error) => {
                            eprintln!("[fika-wayland] dmabuf surface feedback failed: {error}");
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
        let window = Arc::new(WaylandWindow {
            id,
            handle,
            state: Mutex::new(WindowState {
                logical_size,
                physical_size,
                scale_factor,
                configured: false,
                redraw_requested: true,
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
        if let Some(primary) = self.primary_surface.get() {
            if let Some(window) = self
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
            .name("fika-wayland-dnd-read".to_string())
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

include!("platform_event_loop.rs");

fn normalize_wayland_scale_factor(scale_factor: f64) -> f64 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

fn logical_to_physical_rounded(size: LogicalSize, scale_factor: f64) -> PhysicalSize<u32> {
    let scale_factor = normalize_wayland_scale_factor(scale_factor);
    PhysicalSize::new(
        scaled_dimension(size.width, scale_factor),
        scaled_dimension(size.height, scale_factor),
    )
}

fn apply_configured_logical_size(
    state: &mut WindowState,
    logical_size: LogicalSize,
) -> (PhysicalSize<u32>, bool, bool) {
    let physical_size = logical_to_physical_rounded(logical_size, state.scale_factor);
    let surface_state_changed = !state.configured
        || logical_size != state.logical_size
        || physical_size != state.physical_size;
    let resized = !state.configured || physical_size != state.physical_size;
    state.logical_size = logical_size;
    state.physical_size = physical_size;
    state.configured = true;
    state.redraw_requested = true;
    (physical_size, surface_state_changed, resized)
}

fn physical_to_logical_rounded(size: PhysicalSize<u32>, scale_factor: f64) -> LogicalSize {
    let scale_factor = normalize_wayland_scale_factor(scale_factor);
    LogicalSize::new(
        scaled_dimension(size.width, scale_factor.recip()),
        scaled_dimension(size.height, scale_factor.recip()),
    )
}

fn scaled_dimension(value: u32, scale_factor: f64) -> u32 {
    (f64::from(value) * scale_factor)
        .round()
        .clamp(1.0, f64::from(u32::MAX)) as u32
}

fn integer_buffer_scale(scale_factor: f64) -> i32 {
    normalize_wayland_scale_factor(scale_factor)
        .round()
        .clamp(1.0, f64::from(i32::MAX)) as i32
}

fn scale_dnd_position(position: LogicalPosition, scale: f64) -> PhysicalPosition<f64> {
    PhysicalPosition::new(position.x as f64 * scale, position.y as f64 * scale)
}

fn runtime_dnd_actions(actions: &[DndAction]) -> RuntimeDndActions {
    let mut mapped = RuntimeDndActions::empty();
    for action in actions {
        mapped |= match action {
            DndAction::Copy => RuntimeDndActions::COPY,
            DndAction::Move => RuntimeDndActions::MOVE,
            DndAction::Ask => RuntimeDndActions::ASK,
        };
    }
    mapped
}

fn preferred_runtime_dnd_action(actions: &[DndAction]) -> Option<RuntimeDndAction> {
    if actions.contains(&DndAction::Ask) {
        Some(RuntimeDndAction::Ask)
    } else if actions.contains(&DndAction::Move) {
        Some(RuntimeDndAction::Move)
    } else if actions.contains(&DndAction::Copy) {
        Some(RuntimeDndAction::Copy)
    } else {
        None
    }
}

fn platform_dnd_action(action: RuntimeDndAction) -> DndAction {
    match action {
        RuntimeDndAction::Copy => DndAction::Copy,
        RuntimeDndAction::Move => DndAction::Move,
        RuntimeDndAction::Ask => DndAction::Ask,
    }
}

fn runtime_cursor_icon(icon: CursorIcon) -> RuntimeCursorIcon {
    match icon {
        CursorIcon::ColResize => RuntimeCursorIcon::ColResize,
        CursorIcon::Default => RuntimeCursorIcon::Default,
        CursorIcon::Pointer => RuntimeCursorIcon::Pointer,
        CursorIcon::Text => RuntimeCursorIcon::Text,
    }
}

/// Map a framed Wayland pointer axis into Fika scroll vocabulary.
///
/// Prefer `axis_value120` / discrete logical steps (high-resolution wheels). Fall
/// back to continuous compositor coordinates scaled into physical pixels
/// (touchpads and continuous devices). Sign matches the historical continuous
/// path: UI consumers negate again to obtain content scroll direction.
fn map_pointer_axis_to_scroll_delta(
    horizontal: PointerAxisValue,
    vertical: PointerAxisValue,
    scale_factor: f64,
) -> MouseScrollDelta {
    let scale_factor = normalize_wayland_scale_factor(scale_factor);
    let horizontal_steps = horizontal.logical_steps();
    let vertical_steps = vertical.logical_steps();
    if horizontal_steps.is_some() || vertical_steps.is_some() {
        return MouseScrollDelta::LineDelta {
            x: -horizontal_steps.unwrap_or(0.0),
            y: -vertical_steps.unwrap_or(0.0),
        };
    }
    MouseScrollDelta::PixelDelta(PhysicalPosition::new(
        -horizontal.continuous * scale_factor,
        -vertical.continuous * scale_factor,
    ))
}

fn linux_button(button: u32) -> ButtonSource {
    let button = match button {
        0x110 => MouseButton::Left,
        0x111 => MouseButton::Right,
        0x112 => MouseButton::Middle,
        0x113 => MouseButton::Back,
        0x114 => MouseButton::Forward,
        value => return ButtonSource::Unknown(value),
    };
    ButtonSource::Mouse(button)
}

fn translate_key_event(
    state: KeyState,
    raw_code: u32,
    keysym: u32,
    text: Option<String>,
) -> KeyEvent {
    let logical_key = logical_key(keysym, text.as_deref());
    KeyEvent {
        physical_key: physical_key(raw_code),
        key_without_modifiers: logical_key.clone(),
        logical_key,
        state: match state {
            KeyState::Pressed | KeyState::Repeated => ElementState::Pressed,
            KeyState::Released => ElementState::Released,
        },
        repeat: state == KeyState::Repeated,
        text,
    }
}

fn logical_key(keysym: u32, text: Option<&str>) -> Key {
    use xkeysym::key;

    let named = match keysym {
        key::BackSpace => Some(NamedKey::Backspace),
        key::Tab | key::ISO_Left_Tab => Some(NamedKey::Tab),
        key::Return | key::KP_Enter => Some(NamedKey::Enter),
        key::Escape => Some(NamedKey::Escape),
        key::Delete | key::KP_Delete => Some(NamedKey::Delete),
        key::Home | key::KP_Home => Some(NamedKey::Home),
        key::Left | key::KP_Left => Some(NamedKey::ArrowLeft),
        key::Up | key::KP_Up => Some(NamedKey::ArrowUp),
        key::Right | key::KP_Right => Some(NamedKey::ArrowRight),
        key::Down | key::KP_Down => Some(NamedKey::ArrowDown),
        key::Page_Up | key::KP_Page_Up => Some(NamedKey::PageUp),
        key::Page_Down | key::KP_Page_Down => Some(NamedKey::PageDown),
        key::End | key::KP_End => Some(NamedKey::End),
        key::F1 => Some(NamedKey::F1),
        key::F2 => Some(NamedKey::F2),
        key::F3 => Some(NamedKey::F3),
        key::F5 => Some(NamedKey::F5),
        key::F6 => Some(NamedKey::F6),
        _ => None,
    };
    if let Some(named) = named {
        Key::Named(named)
    } else if let Some(text) = text.filter(|value| !value.is_empty()) {
        Key::Character(text.to_string())
    } else if let Some(character) = xkeysym::Keysym::new(keysym).key_char() {
        Key::Character(character.to_string())
    } else {
        Key::Unidentified(NativeKey::Unidentified)
    }
}

fn physical_key(raw_code: u32) -> PhysicalKey {
    // SCTK / wl_keyboard deliver Linux evdev keycodes. Do not subtract 8 here:
    // that offset is only used when converting *to* XKB keycodes (see SCTK's
    // `KeyCode::new(raw_code + 8)`). Subtracting maps Ctrl+C (46) to KeyL (38)
    // and steals the address-bar shortcut.
    let code = match raw_code {
        1 => KeyCode::Escape,
        2 => KeyCode::Digit1,
        3 => KeyCode::Digit2,
        4 => KeyCode::Digit3,
        14 => KeyCode::Backspace,
        15 => KeyCode::Tab,
        19 => KeyCode::KeyR,
        30 => KeyCode::KeyA,
        32 => KeyCode::KeyD,
        33 => KeyCode::KeyF,
        35 => KeyCode::KeyH,
        38 => KeyCode::KeyL,
        45 => KeyCode::KeyX,
        46 => KeyCode::KeyC,
        47 => KeyCode::KeyV,
        59 => KeyCode::F1,
        60 => KeyCode::F2,
        61 => KeyCode::F3,
        63 => KeyCode::F5,
        64 => KeyCode::F6,
        79 => KeyCode::Numpad1,
        80 => KeyCode::Numpad2,
        81 => KeyCode::Numpad3,
        102 => KeyCode::Home,
        103 => KeyCode::ArrowUp,
        105 => KeyCode::ArrowLeft,
        106 => KeyCode::ArrowRight,
        107 => KeyCode::End,
        108 => KeyCode::ArrowDown,
        111 => KeyCode::Delete,
        _ => return PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
    };
    PhysicalKey::Code(code)
}

#[cfg(test)]
mod scaling_tests {
    use super::*;

    #[test]
    fn physical_key_uses_linux_evdev_codes_without_xkb_offset() {
        // wl_keyboard / SCTK raw_code values are already Linux keycodes.
        assert_eq!(physical_key(46), PhysicalKey::Code(KeyCode::KeyC));
        assert_eq!(physical_key(38), PhysicalKey::Code(KeyCode::KeyL));
        assert_eq!(physical_key(30), PhysicalKey::Code(KeyCode::KeyA));
        assert_eq!(physical_key(47), PhysicalKey::Code(KeyCode::KeyV));
        assert_eq!(physical_key(45), PhysicalKey::Code(KeyCode::KeyX));
        // X11-style keycodes (evdev + 8) must not silently remap onto neighbors.
        assert!(matches!(physical_key(54), PhysicalKey::Unidentified(_)));
    }

    #[test]
    fn fractional_scale_rounds_toplevel_sizes_half_away_from_zero() {
        let logical = LogicalSize::new(801, 641);

        assert_eq!(
            logical_to_physical_rounded(logical, 1.25),
            PhysicalSize::new(1001, 801)
        );
        assert_eq!(
            logical_to_physical_rounded(logical, 1.5),
            PhysicalSize::new(1202, 962)
        );
        assert_eq!(
            logical_to_physical_rounded(LogicalSize::new(800, 640), 0.75),
            PhysicalSize::new(600, 480)
        );
    }

    #[test]
    fn physical_size_requests_use_the_fractional_scale() {
        assert_eq!(
            physical_to_logical_rounded(PhysicalSize::new(1001, 801), 1.25),
            LogicalSize::new(801, 641)
        );
        assert_eq!(
            physical_to_logical_rounded(PhysicalSize::new(1202, 962), 1.5),
            LogicalSize::new(801, 641)
        );
    }

    #[test]
    fn repeated_same_size_configure_skips_surface_state_commit_and_resize() {
        let logical = LogicalSize::new(847, 1015);
        let physical = PhysicalSize::new(1271, 1523);
        let mut state = WindowState {
            logical_size: logical,
            physical_size: physical,
            scale_factor: 1.5,
            configured: true,
            redraw_requested: false,
        };

        let (next_physical, surface_state_changed, resized) =
            apply_configured_logical_size(&mut state, logical);

        assert_eq!(next_physical, physical);
        assert!(!surface_state_changed);
        assert!(!resized);
        assert!(state.redraw_requested);
    }

    #[test]
    fn initial_and_resized_configures_update_surface_state() {
        let initial_logical = LogicalSize::new(847, 1015);
        let mut state = WindowState {
            logical_size: initial_logical,
            physical_size: PhysicalSize::new(1271, 1523),
            scale_factor: 1.5,
            configured: false,
            redraw_requested: false,
        };

        let (_, surface_state_changed, resized) =
            apply_configured_logical_size(&mut state, initial_logical);
        assert!(surface_state_changed);
        assert!(resized);

        let resized_logical = LogicalSize::new(900, 700);
        let (physical, surface_state_changed, resized) =
            apply_configured_logical_size(&mut state, resized_logical);
        assert_eq!(physical, PhysicalSize::new(1350, 1050));
        assert!(surface_state_changed);
        assert!(resized);
    }

    #[test]
    fn pointer_axis_prefers_value120_steps_over_continuous_pixels() {
        let horizontal = PointerAxisValue::default();
        let vertical = PointerAxisValue {
            continuous: 12.0,
            value120: 30,
            discrete: 1,
            ..Default::default()
        };
        assert_eq!(
            map_pointer_axis_to_scroll_delta(horizontal, vertical, 1.25),
            MouseScrollDelta::LineDelta { x: 0.0, y: -0.25 }
        );
    }

    #[test]
    fn pointer_axis_falls_back_to_scaled_continuous_pixels() {
        let horizontal = PointerAxisValue {
            continuous: -2.0,
            ..Default::default()
        };
        let vertical = PointerAxisValue {
            continuous: 4.0,
            ..Default::default()
        };
        assert_eq!(
            map_pointer_axis_to_scroll_delta(horizontal, vertical, 1.5),
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(3.0, -6.0))
        );
    }

    #[test]
    fn pointer_axis_uses_deprecated_discrete_when_value120_is_absent() {
        let vertical = PointerAxisValue {
            discrete: -2,
            continuous: 8.0,
            ..Default::default()
        };
        assert_eq!(
            map_pointer_axis_to_scroll_delta(PointerAxisValue::default(), vertical, 2.0),
            MouseScrollDelta::LineDelta { x: 0.0, y: 2.0 }
        );
    }
}

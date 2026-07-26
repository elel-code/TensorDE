//! Native shell types and dispatch state.

use std::collections::{HashMap, HashSet};
use std::fs::File;

use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
    wl_keyboard, wl_pointer, wl_seat, wl_shm, wl_shm_pool, wl_surface, wl_touch,
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_v1,
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols::xdg::shell::client::{
    xdg_popup, xdg_surface, xdg_toplevel, xdg_wm_base,
};

use crate::geometry::{LogicalPosition, LogicalRect, LogicalSize, SuggestedSize};
use crate::surface::{ConstraintAdjustments, Gravity, PopupAnchor};

/// Opaque id for a native toplevel surface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeSurfaceId(pub(crate) u32);

/// Events emitted by the native shell (grows toward the public crate Event model).
#[derive(Clone, Debug)]
pub enum NativeShellEvent {
    ToplevelConfigure {
        surface: NativeSurfaceId,
        suggested_size: SuggestedSize,
        /// Decoded `xdg_toplevel.configure` state bits.
        state: crate::ToplevelState,
        /// Configure serial from the matching `xdg_surface.configure`.
        serial: u32,
    },
    ToplevelClose {
        surface: NativeSurfaceId,
    },
    /// Preferred scale from `wp_fractional_scale_v1` (decoded: protocol / 120).
    ScaleFactorChanged {
        surface: NativeSurfaceId,
        factor: f64,
    },
    PointerEnter {
        surface: NativeSurfaceId,
        x: f64,
        y: f64,
    },
    PointerLeave {
        surface: NativeSurfaceId,
    },
    PointerMotion {
        surface: NativeSurfaceId,
        x: f64,
        y: f64,
    },
    PointerButton {
        surface: Option<NativeSurfaceId>,
        button: u32,
        pressed: bool,
    },
    PointerAxis {
        surface: Option<NativeSurfaceId>,
        horizontal: f64,
        vertical: f64,
        /// High-resolution wheel units (120 = one notch); 0 if not reported.
        horizontal_value120: i32,
        vertical_value120: i32,
    },
    SeatKeyboardEnter {
        surface: Option<NativeSurfaceId>,
    },
    SeatKeyboardLeave {
        surface: Option<NativeSurfaceId>,
    },
    SeatKeyboardKey {
        /// Linux evdev keycode (Wayland `wl_keyboard.key`).
        key: u32,
        pressed: bool,
        /// XKB keysym when a keymap is available; otherwise 0.
        keysym: u32,
        /// UTF-8 text produced on press (empty/control keys → `None`).
        text: Option<String>,
    },
    SeatModifiers {
        mods_depressed: u32,
        mods_latched: u32,
        mods_locked: u32,
        group: u32,
    },
    /// `wl_surface.frame` callback fired (time is compositor milliseconds).
    Frame {
        surface: NativeSurfaceId,
        time: u32,
    },
    /// `wp_presentation_feedback.presented` — content became visible.
    Presented {
        surface: NativeSurfaceId,
        /// Presentation clock seconds (from `tv_sec_hi`/`tv_sec_lo`).
        tv_sec: u64,
        /// Nanoseconds fraction of the presentation timestamp.
        tv_nsec: u32,
        /// Nominal refresh period in nanoseconds (`0` if unknown).
        refresh_ns: u32,
        /// Frame sequence counter when available.
        seq: u64,
        /// `wp_presentation_feedback.kind` bits.
        flags_bits: u32,
        /// Registry name of the synchronized `wl_output`, if known.
        sync_output: Option<u32>,
    },
    /// `wp_presentation_feedback.discarded` — update never shown.
    PresentationDiscarded {
        surface: NativeSurfaceId,
    },
    TouchDown {
        surface: NativeSurfaceId,
        id: i32,
        x: f64,
        y: f64,
        serial: u32,
        time: u32,
    },
    TouchUp {
        id: i32,
        serial: u32,
        time: u32,
    },
    TouchMotion {
        id: i32,
        x: f64,
        y: f64,
        time: u32,
    },
    /// Ellipse axes approximating the contact (optional; compositor-dependent).
    TouchShape {
        id: i32,
        major: f64,
        minor: f64,
    },
    /// Clockwise angle of the major axis from positive surface-local Y (degrees).
    TouchOrientation {
        id: i32,
        degrees: f64,
    },
    /// End of a frame-buffered touch batch (after pending events are flushed).
    TouchFrame,
    TouchCancel,
    OutputGeometry {
        output: u32,
        x: i32,
        y: i32,
        physical_width: i32,
        physical_height: i32,
        make: String,
        model: String,
    },
    OutputMode {
        output: u32,
        width: i32,
        height: i32,
        refresh: i32,
        current: bool,
    },
    OutputScale {
        output: u32,
        factor: i32,
    },
    OutputDone {
        output: u32,
    },
    /// Registry `global_remove` for a previously bound `wl_output`.
    OutputRemoved {
        output: u32,
    },
    SurfaceOutputEnter {
        surface: NativeSurfaceId,
        output: u32,
    },
    SurfaceOutputLeave {
        surface: NativeSurfaceId,
        output: u32,
    },
    /// Clipboard selection offer updated (mime list may be empty when cleared).
    Selection {
        mimes: Vec<String>,
    },
    /// Outgoing clipboard source was cancelled by the compositor.
    SelectionCancelled,
    /// Primary selection (middle-click paste) offer updated.
    PrimarySelection {
        mimes: Vec<String>,
    },
    /// Outgoing primary selection source was cancelled.
    PrimarySelectionCancelled,
    /// `xdg_popup.configure` (geometry relative to parent).
    PopupConfigure {
        surface: NativeSurfaceId,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        serial: u32,
        /// Reposition token when this configure follows `xdg_popup.repositioned`.
        reposition_token: Option<u32>,
    },
    /// `xdg_popup.popup_done` — popup was dismissed.
    PopupDone {
        surface: NativeSurfaceId,
    },
    /// Incoming drag entered a surface.
    DndEnter {
        /// Stable offer id for Fika / public [`crate::DndOfferId`].
        offer: u64,
        surface: NativeSurfaceId,
        x: f64,
        y: f64,
        mimes: Vec<String>,
    },
    /// Drag left the surface (or was cancelled before drop).
    DndLeave {
        offer: u64,
        surface: Option<NativeSurfaceId>,
    },
    /// Drag motion over the focused surface.
    DndMotion {
        offer: u64,
        x: f64,
        y: f64,
    },
    /// Drop performed; call [`crate::NativeShell::accept_dnd`] / receive as needed.
    DndDrop {
        offer: u64,
    },
    /// Outgoing drag finished (source side).
    DndFinished {
        source: u64,
        cancelled: bool,
    },
    /// text-input-v3 entered a surface.
    TextInputEnter {
        surface: NativeSurfaceId,
    },
    /// text-input-v3 left a surface.
    TextInputLeave {
        surface: NativeSurfaceId,
    },
    /// text-input-v3 `done` batch (commit / preedit / delete).
    TextInputDone {
        surface: NativeSurfaceId,
        serial: u32,
        commit: Option<String>,
        preedit: Option<String>,
        delete_before: u32,
        delete_after: u32,
    },
    /// `zwlr_layer_surface_v1.configure`.
    LayerConfigure {
        surface: NativeSurfaceId,
        suggested_size: SuggestedSize,
        serial: u32,
    },
    /// `zwlr_layer_surface_v1.closed`.
    LayerClosed {
        surface: NativeSurfaceId,
    },
    /// Activation token ready (`xdg_activation_token_v1.done`).
    ActivationToken {
        surface: NativeSurfaceId,
        token: String,
    },
    /// Touchpad swipe (`zwp_pointer_gesture_swipe_v1`).
    GestureSwipeBegin {
        surface: NativeSurfaceId,
        fingers: u32,
        time: u32,
    },
    GestureSwipeUpdate {
        dx: f64,
        dy: f64,
        time: u32,
    },
    GestureSwipeEnd {
        cancelled: bool,
        time: u32,
    },
    /// Touchpad pinch (`zwp_pointer_gesture_pinch_v1`).
    GesturePinchBegin {
        surface: NativeSurfaceId,
        fingers: u32,
        time: u32,
    },
    GesturePinchUpdate {
        dx: f64,
        dy: f64,
        scale: f64,
        rotation: f64,
        time: u32,
    },
    GesturePinchEnd {
        cancelled: bool,
        time: u32,
    },
    /// Touchpad hold (`zwp_pointer_gesture_hold_v1`, v3+).
    GestureHoldBegin {
        surface: NativeSurfaceId,
        fingers: u32,
        time: u32,
    },
    GestureHoldEnd {
        cancelled: bool,
        time: u32,
    },
    /// `zwp_relative_pointer_v1.relative_motion`.
    RelativePointer {
        utime: u64,
        dx: f64,
        dy: f64,
        dx_unaccel: f64,
        dy_unaccel: f64,
    },
    /// Pointer constraint activated or deactivated.
    PointerConstraint {
        surface: NativeSurfaceId,
        /// 0 = none/cleared, 1 = confined, 2 = locked.
        kind: u8,
        active: bool,
    },
    /// Default or surface-scoped dmabuf feedback (`zwp_linux_dmabuf_feedback_v1.done`).
    DmabufFeedback {
        /// `None` = default feedback; `Some` = surface-scoped feedback.
        surface: Option<NativeSurfaceId>,
        feedback: crate::dmabuf::DmabufFeedback,
    },
    /// Async `zwp_linux_buffer_params_v1.created`.
    DmabufBufferCreated {
        id: crate::dmabuf::DmabufBufferId,
    },
    /// Async `zwp_linux_buffer_params_v1.failed`.
    DmabufBufferFailed,
    /// Compositor released a dmabuf-backed `wl_buffer` (may be reused / destroyed).
    DmabufBufferReleased {
        id: crate::dmabuf::DmabufBufferId,
    },
}

/// Positioner inputs for native `xdg_popup` creation.
#[derive(Clone, Debug)]
pub struct NativePopupPositioner {
    pub size: LogicalSize,
    pub anchor_rect: LogicalRect,
    pub anchor: PopupAnchor,
    pub gravity: Gravity,
    pub constraints: ConstraintAdjustments,
    pub offset: LogicalPosition,
}

impl Default for NativePopupPositioner {
    fn default() -> Self {
        Self {
            size: LogicalSize::new(200, 120),
            anchor_rect: LogicalRect::new(0, 0, 1, 1),
            anchor: PopupAnchor::BottomLeft,
            gravity: Gravity::BottomRight,
            constraints: ConstraintAdjustments::SLIDE_X
                | ConstraintAdjustments::SLIDE_Y
                | ConstraintAdjustments::FLIP_X
                | ConstraintAdjustments::FLIP_Y,
            offset: LogicalPosition::ZERO,
        }
    }
}

/// Capability snapshot for the native shell connection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeCapabilities {
    pub fractional_scale: bool,
    pub viewporter: bool,
    pub cursor_shape: bool,
    pub seat: bool,
    pub pointer: bool,
    pub keyboard: bool,
    pub touch: bool,
    pub output_count: u32,
    pub data_device: bool,
    /// Keymap received and compiled with libxkbcommon.
    pub xkb: bool,
    pub text_input: bool,
    pub layer_shell: bool,
    pub activation: bool,
    pub pointer_gestures: bool,
    pub pointer_gesture_hold: bool,
    pub relative_pointer: bool,
    pub xdg_dialog: bool,
    pub toplevel_icon: bool,
    pub background_blur: bool,
    pub xdg_decoration: bool,
    pub pointer_constraints: bool,
    pub subcompositor: bool,
    /// `wp_presentation` bound (stable presentation-time).
    pub presentation: bool,
    /// `zwp_primary_selection_device_manager_v1` (middle-click paste).
    pub primary_selection: bool,
    /// `zwp_idle_inhibit_manager_v1` (screensaver / idle inhibit).
    pub idle_inhibit: bool,
    /// `zwp_linux_dmabuf_v1` (GPU zero-copy buffers).
    pub linux_dmabuf: bool,
    /// Bound linux-dmabuf protocol version (0 if unbound).
    pub linux_dmabuf_version: u32,
}

/// In-flight presentation feedback object metadata.
pub(crate) struct PresentationFeedbackRecord {
    pub(crate) surface: NativeSurfaceId,
    pub(crate) sync_output: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct OutputRecord {
    pub(crate) scale: i32,
    pub(crate) make: String,
    pub(crate) model: String,
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) physical_width: i32,
    pub(crate) physical_height: i32,
    /// Current mode size when advertised.
    pub(crate) mode_width: i32,
    pub(crate) mode_height: i32,
    /// Current mode refresh in millihertz (`wl_output.mode.refresh`).
    pub(crate) mode_refresh_mhz: i32,
    pub(crate) done: bool,
}

pub(crate) struct ToplevelRecord {
    pub(crate) wl: wl_surface::WlSurface,
    #[allow(dead_code)]
    pub(crate) xdg: xdg_surface::XdgSurface,
    pub(crate) toplevel: xdg_toplevel::XdgToplevel,
    /// Optional `xdg_dialog_v1` role (staging); destroyed with the toplevel.
    pub(crate) dialog: Option<
        wayland_protocols::xdg::dialog::v1::client::xdg_dialog_v1::XdgDialogV1,
    >,
    pub(crate) parent: Option<NativeSurfaceId>,
    pub(crate) buffer: Option<wl_buffer::WlBuffer>,
    pub(crate) _pool: Option<wl_shm_pool::WlShmPool>,
    pub(crate) _file: Option<File>,
    pub(crate) viewport: Option<wp_viewport::WpViewport>,
    pub(crate) fractional: Option<wp_fractional_scale_v1::WpFractionalScaleV1>,
    /// Retained SHM icon buffers until replaced (compositor may read async).
    pub(crate) icon_shm: Vec<(File, wl_shm_pool::WlShmPool, wl_buffer::WlBuffer)>,
    pub(crate) blur_effect: Option<
        wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
    >,
    /// Desired blur state. Re-applied when the compositor advertises the blur
    /// capability after the first enable request (common on cold start).
    pub(crate) pending_blur: Option<crate::blur::BlurState>,
    pub(crate) decoration: Option<
        wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1,
    >,
    /// Last mode from `zxdg_toplevel_decoration_v1.configure` (if any).
    pub(crate) decoration_mode: Option<crate::surface::DecorationPreference>,
    /// User preference for decorations (drives CSD enable/hide).
    pub(crate) decorations_preference: crate::surface::DecorationPreference,
    /// Desired pointer capture while this surface has pointer focus.
    pub(crate) pointer_capture: crate::PointerCaptureState,
    pub(crate) configured: bool,
    pub(crate) pending_size: Option<(i32, i32)>,
    pub(crate) pending_states: crate::ToplevelState,
    pub(crate) last_configure_serial: u32,
    /// Logical destination size for viewporter (surface-local).
    pub(crate) logical_w: u32,
    pub(crate) logical_h: u32,
    pub(crate) scale_factor: f64,
    pub(crate) title: String,
}

/// In-progress feedback object assembly (one per feedback proxy).
#[derive(Clone, Debug, Default)]
pub(crate) struct DmabufFeedbackBuild {
    pub(crate) main_device: u64,
    pub(crate) formats: Vec<crate::dmabuf::DmabufFormat>,
    pub(crate) tranches: Vec<crate::dmabuf::DmabufFeedbackTranche>,
}

/// In-progress tranche for a feedback proxy.
#[derive(Clone, Debug, Default)]
pub(crate) struct PendingDmabufTranche {
    pub(crate) device: u64,
    pub(crate) flags: crate::dmabuf::DmabufTrancheFlags,
    pub(crate) formats: Vec<u16>,
}

/// Live dmabuf-backed `wl_buffer` owned by the shell.
pub(crate) struct DmabufBufferRecord {
    pub(crate) buffer: wl_buffer::WlBuffer,
    /// Params object that created this buffer (destroyed after create/failed).
    #[allow(dead_code)]
    pub(crate) params_proto: Option<u32>,
}

/// One compositor touch event held until `wl_touch.frame` (or Weston workaround).
#[derive(Clone, Debug)]
pub(crate) enum PendingTouchEvent {
    Down {
        surface: NativeSurfaceId,
        id: i32,
        x: f64,
        y: f64,
        serial: u32,
        time: u32,
    },
    Up {
        id: i32,
        serial: u32,
        time: u32,
    },
    Motion {
        id: i32,
        x: f64,
        y: f64,
        time: u32,
    },
    Shape {
        id: i32,
        major: f64,
        minor: f64,
    },
    Orientation {
        id: i32,
        degrees: f64,
    },
}

pub(crate) struct PopupRecord {
    pub(crate) wl: wl_surface::WlSurface,
    #[allow(dead_code)]
    pub(crate) xdg: xdg_surface::XdgSurface,
    pub(crate) popup: xdg_popup::XdgPopup,
    pub(crate) parent: NativeSurfaceId,
    pub(crate) buffer: Option<wl_buffer::WlBuffer>,
    pub(crate) _pool: Option<wl_shm_pool::WlShmPool>,
    pub(crate) _file: Option<File>,
    pub(crate) configured: bool,
    pub(crate) pending_geom: Option<(i32, i32, i32, i32)>,
    pub(crate) last_configure_serial: u32,
    pub(crate) pending_reposition_token: Option<u32>,
    pub(crate) logical_w: u32,
    pub(crate) logical_h: u32,
}

pub(crate) struct LayerRecord {
    pub(crate) wl: wl_surface::WlSurface,
    pub(crate) layer: wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    pub(crate) buffer: Option<wl_buffer::WlBuffer>,
    pub(crate) _pool: Option<wl_shm_pool::WlShmPool>,
    pub(crate) _file: Option<File>,
    pub(crate) viewport: Option<wp_viewport::WpViewport>,
    pub(crate) fractional: Option<wp_fractional_scale_v1::WpFractionalScaleV1>,
    pub(crate) scale_factor: f64,
    pub(crate) configured: bool,
    pub(crate) pending_size: Option<(u32, u32)>,
    pub(crate) logical_w: u32,
    pub(crate) logical_h: u32,
    /// Last applied double-buffered layer state (client-side mirror).
    pub(crate) state: crate::layer_shell::LayerSurfaceState,
}

/// Temporary role-less SHM surface used as a drag icon.
pub(crate) struct NativeDndIconSurface {
    pub(crate) wl: wl_surface::WlSurface,
    pub(crate) buffer: wl_buffer::WlBuffer,
    pub(crate) pool: wl_shm_pool::WlShmPool,
    pub(crate) _file: File,
}

impl Drop for NativeDndIconSurface {
    fn drop(&mut self) {
        self.buffer.destroy();
        self.pool.destroy();
        self.wl.destroy();
    }
}

/// Dispatch state for the native shell event queue.
pub struct NativeShellState {
    pub(crate) compositor: Option<wl_compositor::WlCompositor>,
    pub(crate) shm: Option<wl_shm::WlShm>,
    pub(crate) wm_base: Option<xdg_wm_base::XdgWmBase>,
    /// Bound `xdg_wm_base` version (for popup reposition etc.).
    pub(crate) wm_base_version: u32,
    pub(crate) seat: Option<wl_seat::WlSeat>,
    pub(crate) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(crate) pointer: Option<wl_pointer::WlPointer>,
    pub(crate) touch: Option<wl_touch::WlTouch>,
    /// Frame-buffered touch events (SCTK/Weston-compatible; flushed on `Frame`
    /// or when the last active point goes up without a trailing frame).
    pub(crate) touch_pending: Vec<PendingTouchEvent>,
    /// Sorted active touch point ids (binary search).
    pub(crate) touch_active: Vec<i32>,
    /// Live touch point id → surface (for cancellation on surface destroy).
    pub(crate) touch_points: HashMap<i32, NativeSurfaceId>,
    pub(crate) data_device_manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    pub(crate) data_device: Option<wl_data_device::WlDataDevice>,
    /// Primary selection (X11-style middle-click paste), SCTK-compatible path.
    pub(crate) primary_selection_manager: Option<
        wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1,
    >,
    pub(crate) primary_device: Option<
        wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
    >,
    pub(crate) primary_offer: Option<
        wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1,
    >,
    pub(crate) primary_pending_offer: Option<
        wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1,
    >,
    pub(crate) primary_mimes: Vec<String>,
    pub(crate) primary_source: Option<
        wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
    >,
    pub(crate) primary_content: Option<crate::data_transfer::TransferContent>,
    /// offer protocol id → mimes for primary offers (parallel to offer_mimes).
    pub(crate) primary_offer_mimes: HashMap<u32, Vec<String>>,
    /// Idle inhibit manager (prevent screensaver / idle lock while active).
    pub(crate) idle_inhibit_manager: Option<
        wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1,
    >,
    /// Active inhibitors keyed by content surface.
    pub(crate) idle_inhibitors: HashMap<
        NativeSurfaceId,
        wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
    >,
    /// `zwp_linux_dmabuf_v1` global (version 3+; Mesa needs ≥3).
    pub(crate) linux_dmabuf: Option<
        wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
    >,
    /// Bound protocol version (0 if unbound).
    pub(crate) linux_dmabuf_version: u32,
    /// Legacy format/modifier pairs from v3 `modifier` events (empty on v4+).
    pub(crate) dmabuf_modifiers: Vec<crate::dmabuf::DmabufFormat>,
    /// Default feedback object (v4+), if requested.
    pub(crate) dmabuf_default_feedback_obj: Option<
        wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
    >,
    /// Latest completed default feedback snapshot.
    pub(crate) dmabuf_default_feedback: Option<crate::dmabuf::DmabufFeedback>,
    /// feedback protocol id → surface (None tracked separately as default).
    pub(crate) dmabuf_feedback_surfaces: HashMap<u32, NativeSurfaceId>,
    /// Live surface-scoped feedback proxies.
    pub(crate) dmabuf_surface_feedback_objs: HashMap<
        NativeSurfaceId,
        wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
    >,
    /// Latest completed surface feedback.
    pub(crate) dmabuf_surface_feedback: HashMap<NativeSurfaceId, crate::dmabuf::DmabufFeedback>,
    /// In-progress feedback builds keyed by feedback proxy protocol id.
    pub(crate) dmabuf_feedback_pending: HashMap<u32, DmabufFeedbackBuild>,
    /// In-progress tranche keyed by feedback proxy protocol id.
    pub(crate) dmabuf_tranche_pending: HashMap<u32, PendingDmabufTranche>,
    /// Live params objects (protocol id → proxy) awaiting create/failed.
    pub(crate) dmabuf_params: HashMap<
        u32,
        wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    >,
    /// Imported dmabuf buffers.
    pub(crate) dmabuf_buffers: HashMap<u64, DmabufBufferRecord>,
    /// `wl_buffer` protocol id → dmabuf buffer id.
    pub(crate) dmabuf_buffer_by_proto: HashMap<u32, u64>,
    pub(crate) next_dmabuf_buffer_id: u64,
    pub(crate) viewporter: Option<wp_viewporter::WpViewporter>,
    pub(crate) fractional_manager: Option<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1>,
    pub(crate) cursor_shape_manager:
        Option<wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1>,
    pub(crate) text_input_manager: Option<
        wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    >,
    pub(crate) text_input: Option<
        wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ZwpTextInputV3,
    >,
    pub(crate) text_input_surface: Option<NativeSurfaceId>,
    pub(crate) text_input_serial: u32,
    pub(crate) text_input_pending_commit: Option<String>,
    pub(crate) text_input_pending_preedit: Option<String>,
    pub(crate) text_input_pending_delete: (u32, u32),
    pub(crate) layer_shell: Option<
        wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1,
    >,
    /// Bound `zwlr_layer_shell_v1` version.
    pub(crate) layer_shell_version: u32,
    pub(crate) xdg_wm_dialog: Option<
        wayland_protocols::xdg::dialog::v1::client::xdg_wm_dialog_v1::XdgWmDialogV1,
    >,
    pub(crate) toplevel_icon_manager: Option<
        wayland_protocols::xdg::toplevel_icon::v1::client::xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1,
    >,
    pub(crate) preferred_icon_sizes: Vec<u32>,
    pub(crate) background_effect_manager: Option<
        wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    >,
    pub(crate) background_blur_capable: bool,
    /// Set when blur capability becomes true so after_dispatch can re-apply.
    pub(crate) pending_blur_replay: bool,
    pub(crate) activation: Option<
        wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1,
    >,
    /// Pending activation token proxies (kept alive until `done`).
    pub(crate) activation_tokens: HashMap<
        u32,
        (
            NativeSurfaceId,
            wayland_protocols::xdg::activation::v1::client::xdg_activation_token_v1::XdgActivationTokenV1,
        ),
    >,
    pub(crate) pointer_gestures: Option<
        wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gestures_v1::ZwpPointerGesturesV1,
    >,
    pub(crate) swipe_gesture: Option<
        wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1,
    >,
    pub(crate) pinch_gesture: Option<
        wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1,
    >,
    pub(crate) hold_gesture: Option<
        wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1,
    >,
    /// Surface that currently owns an in-progress swipe/pinch/hold (begin).
    pub(crate) gesture_surface: Option<NativeSurfaceId>,
    pub(crate) relative_pointer_manager: Option<
        wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
    >,
    pub(crate) relative_pointer: Option<
        wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::ZwpRelativePointerV1,
    >,
    pub(crate) pointer_constraints: Option<
        wayland_protocols::wp::pointer_constraints::zv1::client::zwp_pointer_constraints_v1::ZwpPointerConstraintsV1,
    >,
    pub(crate) locked_pointer: Option<(
        NativeSurfaceId,
        wayland_protocols::wp::pointer_constraints::zv1::client::zwp_locked_pointer_v1::ZwpLockedPointerV1,
    )>,
    pub(crate) confined_pointer: Option<(
        NativeSurfaceId,
        wayland_protocols::wp::pointer_constraints::zv1::client::zwp_confined_pointer_v1::ZwpConfinedPointerV1,
    )>,
    pub(crate) toplevels: HashMap<NativeSurfaceId, ToplevelRecord>,
    pub(crate) popups: HashMap<NativeSurfaceId, PopupRecord>,
    pub(crate) layers: HashMap<NativeSurfaceId, LayerRecord>,
    pub(crate) toplevel_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) popup_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) layer_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) xdg_surface_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) wl_surface_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) fractional_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) pointer_focus: Option<NativeSurfaceId>,
    pub(crate) keyboard_focus: Option<NativeSurfaceId>,
    pub(crate) pointer_enter_serial: Option<u32>,
    /// Latest serial usable for set_selection (keyboard key or pointer button).
    pub(crate) last_input_serial: Option<u32>,
    pub(crate) selection_source: Option<wl_data_source::WlDataSource>,
    pub(crate) selection_content: Option<crate::data_transfer::TransferContent>,
    pub(crate) incoming_offer: Option<wl_data_offer::WlDataOffer>,
    pub(crate) incoming_mimes: Vec<String>,
    /// Mimes collected per offer object id before `Selection` / drag attach.
    pub(crate) offer_mimes: HashMap<u32, Vec<String>>,
    /// Active drag offer (incoming DnD).
    pub(crate) dnd_offer: Option<wl_data_offer::WlDataOffer>,
    pub(crate) dnd_offer_id: Option<u64>,
    /// True after `wl_data_device.drop` until finish/discard.
    ///
    /// Compositors often send `leave` after a successful drop. Protocol says
    /// the destination must keep the offer for `receive`/`finish` after drop,
    /// so leave must not destroy the offer once this flag is set.
    pub(crate) dnd_dropped: bool,
    pub(crate) dnd_mimes: Vec<String>,
    pub(crate) dnd_focus: Option<NativeSurfaceId>,
    pub(crate) dnd_serial: Option<u32>,
    /// Outgoing drag source (if we started a drag).
    pub(crate) dnd_source: Option<wl_data_source::WlDataSource>,
    pub(crate) dnd_source_id: Option<u64>,
    pub(crate) dnd_source_content: Option<crate::data_transfer::TransferContent>,
    /// Drag icon surface kept alive until the drag finishes/cancels.
    pub(crate) dnd_icon: Option<NativeDndIconSurface>,
    /// Monotonic ids for public DndOfferId / DndSourceId.
    pub(crate) next_transfer_id: u64,
    pub(crate) decoration_manager: Option<
        wayland_protocols::xdg::decoration::zv1::client::zxdg_decoration_manager_v1::ZxdgDecorationManagerV1,
    >,
    /// `zxdg_toplevel_decoration_v1` object id → content surface.
    pub(crate) decoration_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) subcompositor: Option<
        wayland_client::protocol::wl_subcompositor::WlSubcompositor,
    >,
    /// Client-side decoration frames keyed by content toplevel id.
    pub(crate) csd_frames: HashMap<NativeSurfaceId, super::csd::ClientSideFrame>,
    /// CSD part surface id → (parent toplevel, part kind).
    pub(crate) csd_part_owners: HashMap<NativeSurfaceId, (NativeSurfaceId, super::csd::FramePartKind)>,
    /// CSD part surface id → parent toplevel (for focus rewrite).
    pub(crate) csd_surface_to_parent: HashMap<NativeSurfaceId, NativeSurfaceId>,
    /// Pointer is currently over a CSD part of this parent (if any).
    pub(crate) csd_pointer_part: Option<(NativeSurfaceId, super::csd::FramePartKind)>,
    /// Frame actions deferred until after dispatch (move/resize/close/…).
    pub(crate) pending_frame_actions: Vec<(NativeSurfaceId, super::csd::FrameAction)>,
    /// Cursor shape requested by CSD hover.
    pub(crate) pending_csd_cursor: Option<super::csd::FrameCursor>,
    /// Surfaces whose CSD must be re-synced after dispatch.
    pub(crate) pending_csd_refresh: HashSet<NativeSurfaceId>,
    /// XKB state from the latest `wl_keyboard.keymap` (optional).
    pub(crate) xkb: Option<crate::native::protocols::core::NativeXkb>,
    /// Accumulated axis values until frame (or immediate emit if no frame).
    pub(crate) axis_h: f64,
    pub(crate) axis_v: f64,
    pub(crate) axis_h120: i32,
    pub(crate) axis_v120: i32,
    pub(crate) next_id: u32,
    pub(crate) events: Vec<NativeShellEvent>,
    pub(crate) seat_capabilities: wl_seat::Capability,
    /// wl_callback object id → surface.
    pub(crate) frame_callbacks: HashMap<u32, NativeSurfaceId>,
    /// Surfaces that already have an outstanding `wl_surface.frame` callback.
    ///
    /// Used to coalesce duplicate [`NativeShell::request_frame`] calls before
    /// the previous `Done` (common when the app arms every redraw).
    pub(crate) frame_pending: HashSet<NativeSurfaceId>,
    /// `wp_presentation` global (presentation-time).
    pub(crate) presentation: Option<
        wayland_protocols::wp::presentation_time::client::wp_presentation::WpPresentation,
    >,
    /// Compositor presentation clock id (`clock_gettime` clockid on Linux).
    pub(crate) presentation_clock_id: Option<u32>,
    /// `wp_presentation_feedback` object id → pending feedback.
    pub(crate) presentation_feedbacks: HashMap<u32, PresentationFeedbackRecord>,
    pub(crate) outputs: HashMap<u32, OutputRecord>,
    /// protocol_id → registry global name
    pub(crate) output_objects: HashMap<u32, u32>,
    /// registry global name → live `wl_output` proxy
    pub(crate) output_proxies: HashMap<u32, wayland_client::protocol::wl_output::WlOutput>,
}

impl Default for NativeShellState {
    fn default() -> Self {
        Self {
            compositor: None,
            shm: None,
            wm_base: None,
            wm_base_version: 0,
            seat: None,
            keyboard: None,
            pointer: None,
            touch: None,
            touch_pending: Vec::new(),
            touch_active: Vec::new(),
            touch_points: HashMap::new(),
            data_device_manager: None,
            data_device: None,
            primary_selection_manager: None,
            primary_device: None,
            primary_offer: None,
            primary_pending_offer: None,
            primary_mimes: Vec::new(),
            primary_source: None,
            primary_content: None,
            primary_offer_mimes: HashMap::new(),
            idle_inhibit_manager: None,
            idle_inhibitors: HashMap::new(),
            linux_dmabuf: None,
            linux_dmabuf_version: 0,
            dmabuf_modifiers: Vec::new(),
            dmabuf_default_feedback_obj: None,
            dmabuf_default_feedback: None,
            dmabuf_feedback_surfaces: HashMap::new(),
            dmabuf_surface_feedback_objs: HashMap::new(),
            dmabuf_surface_feedback: HashMap::new(),
            dmabuf_feedback_pending: HashMap::new(),
            dmabuf_tranche_pending: HashMap::new(),
            dmabuf_params: HashMap::new(),
            dmabuf_buffers: HashMap::new(),
            dmabuf_buffer_by_proto: HashMap::new(),
            next_dmabuf_buffer_id: 1,
            viewporter: None,
            fractional_manager: None,
            cursor_shape_manager: None,
            text_input_manager: None,
            text_input: None,
            text_input_surface: None,
            text_input_serial: 0,
            text_input_pending_commit: None,
            text_input_pending_preedit: None,
            text_input_pending_delete: (0, 0),
            layer_shell: None,
            layer_shell_version: 0,
            xdg_wm_dialog: None,
            toplevel_icon_manager: None,
            preferred_icon_sizes: Vec::new(),
            background_effect_manager: None,
            background_blur_capable: false,
            pending_blur_replay: false,
            activation: None,
            activation_tokens: HashMap::new(),
            pointer_gestures: None,
            swipe_gesture: None,
            pinch_gesture: None,
            hold_gesture: None,
            gesture_surface: None,
            relative_pointer_manager: None,
            relative_pointer: None,
            pointer_constraints: None,
            locked_pointer: None,
            confined_pointer: None,
            toplevels: HashMap::new(),
            popups: HashMap::new(),
            layers: HashMap::new(),
            toplevel_objects: HashMap::new(),
            popup_objects: HashMap::new(),
            layer_objects: HashMap::new(),
            xdg_surface_objects: HashMap::new(),
            wl_surface_objects: HashMap::new(),
            fractional_objects: HashMap::new(),
            pointer_focus: None,
            keyboard_focus: None,
            pointer_enter_serial: None,
            last_input_serial: None,
            selection_source: None,
            selection_content: None,
            incoming_offer: None,
            incoming_mimes: Vec::new(),
            offer_mimes: HashMap::new(),
            dnd_offer: None,
            dnd_offer_id: None,
            dnd_dropped: false,
            dnd_mimes: Vec::new(),
            dnd_focus: None,
            dnd_serial: None,
            dnd_source: None,
            dnd_source_id: None,
            dnd_source_content: None,
            dnd_icon: None,
            next_transfer_id: 1,
            decoration_manager: None,
            decoration_objects: HashMap::new(),
            subcompositor: None,
            csd_frames: HashMap::new(),
            csd_part_owners: HashMap::new(),
            csd_surface_to_parent: HashMap::new(),
            csd_pointer_part: None,
            pending_frame_actions: Vec::new(),
            pending_csd_cursor: None,
            pending_csd_refresh: HashSet::new(),
            xkb: None,
            axis_h: 0.0,
            axis_v: 0.0,
            axis_h120: 0,
            axis_v120: 0,
            next_id: 1,
            // Hot path: avoid realloc on typical multi-event dispatch batches.
            events: Vec::with_capacity(64),
            seat_capabilities: wl_seat::Capability::empty(),
            frame_callbacks: HashMap::new(),
            frame_pending: HashSet::new(),
            presentation: None,
            presentation_clock_id: None,
            presentation_feedbacks: HashMap::new(),
            outputs: HashMap::new(),
            output_objects: HashMap::new(),
            output_proxies: HashMap::new(),
        }
    }
}

impl NativeShellState {
    pub(crate) fn alloc_id(&mut self) -> NativeSurfaceId {
        let id = NativeSurfaceId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    pub(crate) fn alloc_transfer_id(&mut self) -> u64 {
        let id = self.next_transfer_id;
        self.next_transfer_id = self.next_transfer_id.saturating_add(1);
        id
    }

    pub(crate) fn push(&mut self, event: NativeShellEvent) {
        self.events.push(event);
    }

    /// Drop protocol bookkeeping that references a destroyed content surface.
    ///
    /// Clears pending `wl_surface.frame` callbacks and presentation-feedback
    /// records so destroy does not leave stale object-id maps.
    pub(crate) fn clear_surface_protocol_state(&mut self, id: NativeSurfaceId) {
        self.frame_callbacks.retain(|_, surface| *surface != id);
        self.frame_pending.remove(&id);
        self.presentation_feedbacks
            .retain(|_, rec| rec.surface != id);
    }

    /// Borrow the content `wl_surface` for any role (toplevel / popup / layer).
    pub(crate) fn wl_surface(
        &self,
        id: NativeSurfaceId,
    ) -> Option<&wl_surface::WlSurface> {
        self.toplevels
            .get(&id)
            .map(|r| &r.wl)
            .or_else(|| self.popups.get(&id).map(|r| &r.wl))
            .or_else(|| self.layers.get(&id).map(|r| &r.wl))
    }

    /// Logical size tracked for the surface (configure / client set).
    pub(crate) fn logical_size(
        &self,
        id: NativeSurfaceId,
    ) -> Option<(u32, u32)> {
        if let Some(t) = self.toplevels.get(&id) {
            return Some((t.logical_w, t.logical_h));
        }
        if let Some(p) = self.popups.get(&id) {
            return Some((p.logical_w, p.logical_h));
        }
        if let Some(l) = self.layers.get(&id) {
            return Some((l.logical_w, l.logical_h));
        }
        None
    }

    /// Fractional / integer scale factor tracked for the surface.
    pub(crate) fn scale_factor(&self, id: NativeSurfaceId) -> Option<f64> {
        self.toplevels
            .get(&id)
            .map(|t| t.scale_factor)
            .or_else(|| self.layers.get(&id).map(|l| l.scale_factor))
            // Popups inherit parent scale; report 1.0 when mapped so callers
            // can treat any live surface uniformly.
            .or_else(|| self.popups.get(&id).map(|_| 1.0))
    }

    pub(crate) fn is_frame_pending(&self, id: NativeSurfaceId) -> bool {
        self.frame_pending.contains(&id)
    }
}


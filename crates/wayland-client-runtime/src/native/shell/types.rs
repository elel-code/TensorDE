//! Native shell types and dispatch state.

use std::collections::HashMap;
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
    TouchDown {
        surface: NativeSurfaceId,
        id: i32,
        x: f64,
        y: f64,
    },
    TouchUp {
        id: i32,
    },
    TouchMotion {
        id: i32,
        x: f64,
        y: f64,
    },
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
    /// `xdg_popup.configure` (geometry relative to parent).
    PopupConfigure {
        surface: NativeSurfaceId,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
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
}

#[derive(Clone, Debug, Default)]
pub(crate) struct OutputRecord {
    pub(crate) scale: i32,
    pub(crate) make: String,
    pub(crate) model: String,
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
    pub(crate) decoration: Option<
        wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1,
    >,
    /// Desired pointer capture while this surface has pointer focus.
    pub(crate) pointer_capture: crate::PointerCaptureState,
    pub(crate) configured: bool,
    pub(crate) pending_size: Option<(i32, i32)>,
    /// Logical destination size for viewporter (surface-local).
    pub(crate) logical_w: u32,
    pub(crate) logical_h: u32,
    pub(crate) scale_factor: f64,
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
    pub(crate) logical_w: u32,
    pub(crate) logical_h: u32,
}

pub(crate) struct LayerRecord {
    pub(crate) wl: wl_surface::WlSurface,
    pub(crate) layer: wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    pub(crate) buffer: Option<wl_buffer::WlBuffer>,
    pub(crate) _pool: Option<wl_shm_pool::WlShmPool>,
    pub(crate) _file: Option<File>,
    pub(crate) configured: bool,
    pub(crate) pending_size: Option<(u32, u32)>,
    pub(crate) logical_w: u32,
    pub(crate) logical_h: u32,
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
    pub(crate) seat: Option<wl_seat::WlSeat>,
    pub(crate) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(crate) pointer: Option<wl_pointer::WlPointer>,
    pub(crate) touch: Option<wl_touch::WlTouch>,
    pub(crate) data_device_manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    pub(crate) data_device: Option<wl_data_device::WlDataDevice>,
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
    pub(crate) outputs: HashMap<u32, OutputRecord>,
    pub(crate) output_objects: HashMap<u32, u32>,
}

impl Default for NativeShellState {
    fn default() -> Self {
        Self {
            compositor: None,
            shm: None,
            wm_base: None,
            seat: None,
            keyboard: None,
            pointer: None,
            touch: None,
            data_device_manager: None,
            data_device: None,
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
            xdg_wm_dialog: None,
            toplevel_icon_manager: None,
            preferred_icon_sizes: Vec::new(),
            background_effect_manager: None,
            background_blur_capable: false,
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
            dnd_mimes: Vec::new(),
            dnd_focus: None,
            dnd_serial: None,
            dnd_source: None,
            dnd_source_id: None,
            dnd_source_content: None,
            dnd_icon: None,
            next_transfer_id: 1,
            decoration_manager: None,
            xkb: None,
            axis_h: 0.0,
            axis_v: 0.0,
            axis_h120: 0,
            axis_v120: 0,
            next_id: 1,
            events: Vec::new(),
            seat_capabilities: wl_seat::Capability::empty(),
            frame_callbacks: HashMap::new(),
            outputs: HashMap::new(),
            output_objects: HashMap::new(),
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
}


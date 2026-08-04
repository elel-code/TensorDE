//! Native shell types and dispatch state.

use std::collections::{HashMap, HashSet};
use std::fs::File;

use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer,
    wl_data_source, wl_keyboard, wl_pointer, wl_seat, wl_shm, wl_shm_pool, wl_surface, wl_touch,
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_v1,
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols::xdg::shell::client::{xdg_popup, xdg_surface, xdg_toplevel, xdg_wm_base};

use crate::geometry::{LogicalPosition, LogicalRect, LogicalSize, SuggestedSize};
use crate::surface::{ConstraintAdjustments, Gravity, PopupAnchor};

include!("types_events.rs");

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
    /// Number of currently bound seats (multi-seat compositors may be > 1).
    pub seat_count: u32,
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
    /// `ext_idle_notifier_v1` (user idle / resume notifications).
    pub idle_notify: bool,
    /// `ext_idle_notifier_v1` version ≥ 2 (`get_input_idle_notification`).
    pub idle_notify_input: bool,
    /// `zwlr_output_power_manager_v1` (per-output DPMS control).
    pub output_power: bool,
    /// `zxdg_exporter_v2` / `zxdg_importer_v2` (cross-client surface handles).
    pub xdg_foreign: bool,
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

/// One bound `wl_seat` and its capability devices + input focus/serials.
///
/// Shell-wide `pointer_focus` / `last_input_serial` still track the most recent
/// input (last-wins) for single-seat APIs. Per-seat fields allow multi-seat
/// clients to query each seat independently via [`crate::NativeShell`].
pub(crate) struct SeatRecord {
    /// Registry global name (`wl_registry` name).
    pub(crate) global_name: u32,
    /// Live seat proxy (kept for seat-scoped grabs / data devices).
    pub(crate) seat: wl_seat::WlSeat,
    pub(crate) name: Option<String>,
    pub(crate) capabilities: wl_seat::Capability,
    pub(crate) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(crate) pointer: Option<wl_pointer::WlPointer>,
    pub(crate) touch: Option<wl_touch::WlTouch>,
    /// Surface with keyboard focus on this seat.
    pub(crate) keyboard_focus: Option<NativeSurfaceId>,
    /// Surface with pointer focus on this seat.
    pub(crate) pointer_focus: Option<NativeSurfaceId>,
    /// Latest input serial on this seat (key / button / enter).
    pub(crate) last_input_serial: Option<u32>,
    /// Serial from the latest pointer enter on this seat.
    pub(crate) pointer_enter_serial: Option<u32>,
    /// Seat-scoped clipboard / DnD device (`wl_data_device`).
    pub(crate) data_device: Option<wl_data_device::WlDataDevice>,
    /// Seat-scoped primary selection device (middle-click paste).
    pub(crate) primary_device: Option<
        wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
    >,
    /// Per-seat pointer-gesture objects (bound when this seat has a pointer).
    pub(crate) swipe_gesture: Option<
        wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1,
    >,
    pub(crate) pinch_gesture: Option<
        wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1,
    >,
    pub(crate) hold_gesture: Option<
        wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1,
    >,
    /// Axis accumulation until `wl_pointer.frame` for this seat's pointer.
    pub(crate) axis: crate::pointer_axis::PointerAxisFrameAccum,
    /// Per-seat relative pointer object (when enabled for this seat's pointer).
    pub(crate) relative_pointer: Option<
        wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::ZwpRelativePointerV1,
    >,
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

pub(crate) struct OutputPowerRecord {
    pub(crate) power: Option<
        wayland_protocols_wlr::output_power_management::v1::client::zwlr_output_power_v1::ZwlrOutputPowerV1,
    >,
    pub(crate) mode: Option<crate::output::OutputPowerMode>,
    pub(crate) failed: bool,
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
        seat: Option<u32>,
    },
    Up {
        id: i32,
        serial: u32,
        time: u32,
        seat: Option<u32>,
    },
    Motion {
        id: i32,
        x: f64,
        y: f64,
        time: u32,
        seat: Option<u32>,
    },
    Shape {
        id: i32,
        major: f64,
        minor: f64,
        seat: Option<u32>,
    },
    Orientation {
        id: i32,
        degrees: f64,
        seat: Option<u32>,
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
    pub(crate) layer:
        wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
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

/// Temporary role-less surface used as a drag icon.
pub(crate) struct NativeDndIconSurface {
    pub(crate) wl: wl_surface::WlSurface,
    pub(crate) buffer: wl_buffer::WlBuffer,
}

impl Drop for NativeDndIconSurface {
    fn drop(&mut self) {
        self.buffer.destroy();
        self.wl.destroy();
    }
}

include!("types_state.rs");

include!("types_methods.rs");

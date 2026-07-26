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
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use crate::geometry::SuggestedSize;

/// Opaque id for a native toplevel surface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeSurfaceId(u32);

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
    },
    SeatKeyboardEnter {
        surface: Option<NativeSurfaceId>,
    },
    SeatKeyboardLeave {
        surface: Option<NativeSurfaceId>,
    },
    SeatKeyboardKey {
        key: u32,
        pressed: bool,
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
    pub(crate) buffer: Option<wl_buffer::WlBuffer>,
    pub(crate) _pool: Option<wl_shm_pool::WlShmPool>,
    pub(crate) _file: Option<File>,
    pub(crate) viewport: Option<wp_viewport::WpViewport>,
    pub(crate) fractional: Option<wp_fractional_scale_v1::WpFractionalScaleV1>,
    pub(crate) configured: bool,
    pub(crate) pending_size: Option<(i32, i32)>,
    /// Logical destination size for viewporter (surface-local).
    pub(crate) logical_w: u32,
    pub(crate) logical_h: u32,
    pub(crate) scale_factor: f64,
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
    pub(crate) toplevels: HashMap<NativeSurfaceId, ToplevelRecord>,
    pub(crate) toplevel_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) xdg_surface_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) wl_surface_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) fractional_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) pointer_focus: Option<NativeSurfaceId>,
    pub(crate) keyboard_focus: Option<NativeSurfaceId>,
    pub(crate) pointer_enter_serial: Option<u32>,
    /// Latest serial usable for set_selection (keyboard key or pointer button).
    pub(crate) last_input_serial: Option<u32>,
    pub(crate) selection_source: Option<wl_data_source::WlDataSource>,
    pub(crate) selection_bytes: Option<std::sync::Arc<[u8]>>,
    pub(crate) selection_mimes: Vec<String>,
    pub(crate) incoming_offer: Option<wl_data_offer::WlDataOffer>,
    pub(crate) incoming_mimes: Vec<String>,
    /// Accumulated axis values until frame (or immediate emit if no frame).
    pub(crate) axis_h: f64,
    pub(crate) axis_v: f64,
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
            toplevels: HashMap::new(),
            toplevel_objects: HashMap::new(),
            xdg_surface_objects: HashMap::new(),
            wl_surface_objects: HashMap::new(),
            fractional_objects: HashMap::new(),
            pointer_focus: None,
            keyboard_focus: None,
            pointer_enter_serial: None,
            last_input_serial: None,
            selection_source: None,
            selection_bytes: None,
            selection_mimes: Vec::new(),
            incoming_offer: None,
            incoming_mimes: Vec::new(),
            axis_h: 0.0,
            axis_v: 0.0,
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

    pub(crate) fn push(&mut self, event: NativeShellEvent) {
        self.events.push(event);
    }
}


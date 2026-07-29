/// Opaque id for a native toplevel surface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeSurfaceId(pub(crate) u32);

impl NativeSurfaceId {
    /// Rebuild an id previously obtained from [`Self::get`].
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }

    /// Runtime-local numeric identity of this surface.
    pub const fn get(self) -> u32 {
        self.0
    }
}

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
        serial: u32,
        /// Registry global name of the seat that owns this pointer, if known.
        seat: Option<u32>,
    },
    PointerLeave {
        surface: NativeSurfaceId,
        serial: u32,
        seat: Option<u32>,
    },
    PointerMotion {
        surface: NativeSurfaceId,
        x: f64,
        y: f64,
        time: u32,
        seat: Option<u32>,
    },
    PointerButton {
        surface: Option<NativeSurfaceId>,
        button: u32,
        pressed: bool,
        serial: u32,
        time: u32,
        seat: Option<u32>,
    },
    PointerAxis {
        surface: Option<NativeSurfaceId>,
        horizontal: crate::pointer_axis::PointerAxisValue,
        vertical: crate::pointer_axis::PointerAxisValue,
        source: Option<crate::pointer_axis::PointerAxisSource>,
        time: u32,
        seat: Option<u32>,
    },
    SeatKeyboardEnter {
        surface: Option<NativeSurfaceId>,
        seat: Option<u32>,
    },
    SeatKeyboardLeave {
        surface: Option<NativeSurfaceId>,
        seat: Option<u32>,
    },
    SeatKeyboardKey {
        /// Linux evdev keycode (Wayland `wl_keyboard.key`).
        key: u32,
        pressed: bool,
        /// XKB keysym when a keymap is available; otherwise 0.
        keysym: u32,
        /// UTF-8 text produced on press (empty/control keys → `None`).
        text: Option<String>,
        seat: Option<u32>,
    },
    SeatModifiers {
        mods_depressed: u32,
        mods_latched: u32,
        mods_locked: u32,
        group: u32,
        seat: Option<u32>,
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
        /// Registry global name of the seat that owns this touch device.
        seat: Option<u32>,
    },
    TouchUp {
        id: i32,
        serial: u32,
        time: u32,
        seat: Option<u32>,
    },
    TouchMotion {
        id: i32,
        x: f64,
        y: f64,
        time: u32,
        seat: Option<u32>,
    },
    /// Ellipse axes approximating the contact (optional; compositor-dependent).
    TouchShape {
        id: i32,
        major: f64,
        minor: f64,
        seat: Option<u32>,
    },
    /// Clockwise angle of the major axis from positive surface-local Y (degrees).
    TouchOrientation {
        id: i32,
        degrees: f64,
        seat: Option<u32>,
    },
    /// End of a frame-buffered touch batch (after pending events are flushed).
    TouchFrame {
        seat: Option<u32>,
    },
    TouchCancel {
        seat: Option<u32>,
    },
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
    /// A `wl_seat` was bound (hotplug or late global).
    SeatAdded {
        seat: u32,
        name: Option<String>,
        has_keyboard: bool,
        has_pointer: bool,
        has_touch: bool,
    },
    /// Seat name or capability devices changed after the initial add.
    SeatChanged {
        seat: u32,
        name: Option<String>,
        has_keyboard: bool,
        has_pointer: bool,
        has_touch: bool,
    },
    /// Registry `global_remove` for a previously bound `wl_seat`.
    SeatRemoved {
        seat: u32,
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
        /// Registry global name of the seat that owns the gesture pointer.
        seat: Option<u32>,
    },
    GestureSwipeUpdate {
        dx: f64,
        dy: f64,
        time: u32,
        seat: Option<u32>,
    },
    GestureSwipeEnd {
        cancelled: bool,
        time: u32,
        seat: Option<u32>,
    },
    /// Touchpad pinch (`zwp_pointer_gesture_pinch_v1`).
    GesturePinchBegin {
        surface: NativeSurfaceId,
        fingers: u32,
        time: u32,
        seat: Option<u32>,
    },
    GesturePinchUpdate {
        dx: f64,
        dy: f64,
        scale: f64,
        rotation: f64,
        time: u32,
        seat: Option<u32>,
    },
    GesturePinchEnd {
        cancelled: bool,
        time: u32,
        seat: Option<u32>,
    },
    /// Touchpad hold (`zwp_pointer_gesture_hold_v1`, v3+).
    GestureHoldBegin {
        surface: NativeSurfaceId,
        fingers: u32,
        time: u32,
        seat: Option<u32>,
    },
    GestureHoldEnd {
        cancelled: bool,
        time: u32,
        seat: Option<u32>,
    },
    /// `zwp_relative_pointer_v1.relative_motion` for a seat's pointer stream.
    RelativePointer {
        utime: u64,
        dx: f64,
        dy: f64,
        dx_unaccel: f64,
        dy_unaccel: f64,
        seat: Option<u32>,
    },
    /// `ext_idle_notification_v1.idled` / `resumed`.
    IdleNotify {
        /// Client-assigned notification id from [`crate::NativeShell::create_idle_notification`].
        id: u64,
        idle: bool,
    },
    /// `zxdg_exported_v2.handle` for an export request.
    ForeignExported {
        surface: NativeSurfaceId,
        handle: String,
    },
    /// `zxdg_imported_v2.destroyed` — the remote export was revoked.
    ForeignImportedDestroyed {
        id: u64,
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

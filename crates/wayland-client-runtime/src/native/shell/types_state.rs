/// Dispatch state for the native shell event queue.
pub struct NativeShellState {
    pub(crate) compositor: Option<wl_compositor::WlCompositor>,
    pub(crate) shm: Option<wl_shm::WlShm>,
    pub(crate) wm_base: Option<xdg_wm_base::XdgWmBase>,
    /// Bound `xdg_wm_base` version (for popup reposition etc.).
    pub(crate) wm_base_version: u32,
    /// Primary / first seat (compat for APIs that need a single seat).
    pub(crate) seat: Option<wl_seat::WlSeat>,
    /// All bound seats keyed by registry global name.
    pub(crate) seats: HashMap<u32, SeatRecord>,
    /// `wl_seat` protocol id → registry global name.
    pub(crate) seat_objects: HashMap<u32, u32>,
    /// `wl_keyboard` protocol id → seat registry name.
    pub(crate) keyboard_objects: HashMap<u32, u32>,
    /// `wl_pointer` protocol id → seat registry name.
    pub(crate) pointer_objects: HashMap<u32, u32>,
    /// `wl_touch` protocol id → seat registry name.
    pub(crate) touch_objects: HashMap<u32, u32>,
    /// Primary seat changed (hotplug); rebind data-device / text-input after dispatch.
    pub(crate) pending_primary_seat_rebind: bool,
    /// Primary-seat keyboard (mirrors first seat that has keyboard).
    pub(crate) keyboard: Option<wl_keyboard::WlKeyboard>,
    /// Primary-seat pointer.
    pub(crate) pointer: Option<wl_pointer::WlPointer>,
    /// Primary-seat touch.
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
    /// `zwp_pointer_gesture_swipe_v1` protocol id → seat registry name.
    pub(crate) swipe_objects: HashMap<u32, u32>,
    /// `zwp_pointer_gesture_pinch_v1` protocol id → seat registry name.
    pub(crate) pinch_objects: HashMap<u32, u32>,
    /// `zwp_pointer_gesture_hold_v1` protocol id → seat registry name.
    pub(crate) hold_objects: HashMap<u32, u32>,
    /// Surface that currently owns an in-progress swipe/pinch/hold (begin).
    pub(crate) gesture_surface: Option<NativeSurfaceId>,
    pub(crate) relative_pointer_manager: Option<
        wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
    >,
    pub(crate) relative_pointer: Option<
        wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_v1::ZwpRelativePointerV1,
    >,
    /// `zwp_relative_pointer_v1` protocol id → seat registry name.
    pub(crate) relative_pointer_objects: HashMap<u32, u32>,
    /// Whether any client requested relative motion (keeps per-seat streams alive).
    pub(crate) relative_pointer_wanted: bool,
    pub(crate) idle_notifier: Option<
        wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1,
    >,
    pub(crate) idle_notifications: HashMap<
        u64,
        wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::ExtIdleNotificationV1,
    >,
    pub(crate) idle_notification_objects: HashMap<u32, u64>,
    pub(crate) next_idle_notification_id: u64,
    pub(crate) output_power_manager: Option<
        wayland_protocols_wlr::output_power_management::v1::client::zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1,
    >,
    /// Output registry global name → retained power control and latest state.
    pub(crate) output_powers: HashMap<u32, super::types::OutputPowerRecord>,
    /// `zwlr_output_power_v1` protocol id → output registry global name.
    pub(crate) output_power_objects: HashMap<u32, u32>,
    /// Controls whose destructor must run after dispatch.
    ///
    /// The bool retains a failed-state tombstone for diagnostics; output
    /// removal uses `false` and drops the record completely.
    pub(crate) pending_output_power_destroy: Vec<(u32, bool)>,
    pub(crate) xdg_exporter: Option<
        wayland_protocols::xdg::foreign::zv2::client::zxdg_exporter_v2::ZxdgExporterV2,
    >,
    pub(crate) xdg_importer: Option<
        wayland_protocols::xdg::foreign::zv2::client::zxdg_importer_v2::ZxdgImporterV2,
    >,
    pub(crate) foreign_exports: HashMap<
        NativeSurfaceId,
        wayland_protocols::xdg::foreign::zv2::client::zxdg_exported_v2::ZxdgExportedV2,
    >,
    pub(crate) foreign_export_objects: HashMap<u32, NativeSurfaceId>,
    pub(crate) foreign_imports: HashMap<
        u64,
        wayland_protocols::xdg::foreign::zv2::client::zxdg_imported_v2::ZxdgImportedV2,
    >,
    pub(crate) foreign_import_objects: HashMap<u32, u64>,
    pub(crate) next_foreign_import_id: u64,
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
    /// Shell-wide axis accumulation fallback when seat is unknown.
    pub(crate) axis: crate::pointer_axis::PointerAxisFrameAccum,
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
    /// Surfaces with an outstanding presentation feedback (coalesce arms).
    pub(crate) presentation_pending: HashSet<NativeSurfaceId>,
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
            seats: HashMap::new(),
            seat_objects: HashMap::new(),
            keyboard_objects: HashMap::new(),
            pointer_objects: HashMap::new(),
            touch_objects: HashMap::new(),
            pending_primary_seat_rebind: false,
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
            swipe_objects: HashMap::new(),
            pinch_objects: HashMap::new(),
            hold_objects: HashMap::new(),
            gesture_surface: None,
            relative_pointer_manager: None,
            relative_pointer: None,
            relative_pointer_objects: HashMap::new(),
            relative_pointer_wanted: false,
            idle_notifier: None,
            idle_notifications: HashMap::new(),
            idle_notification_objects: HashMap::new(),
            next_idle_notification_id: 1,
            output_power_manager: None,
            output_powers: HashMap::new(),
            output_power_objects: HashMap::new(),
            pending_output_power_destroy: Vec::new(),
            xdg_exporter: None,
            xdg_importer: None,
            foreign_exports: HashMap::new(),
            foreign_export_objects: HashMap::new(),
            foreign_imports: HashMap::new(),
            foreign_import_objects: HashMap::new(),
            next_foreign_import_id: 1,
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
            axis: crate::pointer_axis::PointerAxisFrameAccum::default(),
            next_id: 1,
            // Hot path: avoid realloc on typical multi-event dispatch batches.
            events: Vec::with_capacity(64),
            seat_capabilities: wl_seat::Capability::empty(),
            frame_callbacks: HashMap::new(),
            frame_pending: HashSet::new(),
            presentation: None,
            presentation_clock_id: None,
            presentation_feedbacks: HashMap::new(),
            presentation_pending: HashSet::new(),
            outputs: HashMap::new(),
            output_objects: HashMap::new(),
            output_proxies: HashMap::new(),
        }
    }
}

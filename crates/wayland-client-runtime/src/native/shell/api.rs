//! NativeShell public methods.

use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{
    wl_compositor, wl_data_device_manager, wl_output, wl_seat, wl_shm, wl_subcompositor,
};
use wayland_client::Proxy;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape as CursorShape;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1;
use wayland_protocols::wp::presentation_time::client::wp_presentation;
use wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibit_manager_v1;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1;
use wayland_protocols::wp::primary_selection::zv1::client::zwp_primary_selection_device_manager_v1;
use wayland_protocols::wp::viewporter::client::wp_viewporter;
use wayland_protocols::xdg::shell::client::xdg_wm_base;

use super::handle::NativeSurfaceHandle;
use super::types::{
    NativeCapabilities, NativePopupPositioner, NativeShellEvent, NativeShellState, NativeSurfaceId,
    PopupRecord,
};
use crate::surface::{ConstraintAdjustments, Gravity, PopupAnchor};
use wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gestures_v1;
use wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_manager_v1;
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1;
use wayland_protocols::xdg::shell::client::xdg_positioner;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1;
use crate::native::connection::{NativeConnection, NativeError};
use crate::native::protocols::core::shm;
use wayland_client::globals::GlobalList;
use wayland_client::EventQueue;

/// Protocol-only native shell (no event-loop executor).
///
/// Owns the Wayland connection, registry bindings, and surface/input state.
/// The display socket is a **plain non-blocking fd** ([`Self::display_fd`]).
///
/// Drive I/O with:
/// - [`Self::dispatch_pending`] / [`Self::try_read_and_dispatch`] (any loop)
/// - [`Self::pump_once`] when `feature = "compio"` (reuses a long-lived
///   Compio readiness watch; does not re-clone the fd every wait)
///
/// Register [`Self::display_fd`] with your own reactor if you do not use Compio.
///
/// # Request flush batching
///
/// Shell mutators mark the connection dirty instead of writing the socket on
/// every call. [`Self::dispatch_pending`], [`Self::try_read_and_dispatch`], and
/// [`Self::pump_once`] flush when needed so a burst of API calls becomes one
/// write. Paths that must be seen by the compositor before blocking pipe I/O
/// (clipboard / DnD receive) still flush immediately. Call [`Self::flush`] to
/// force a write outside the pump.
pub struct NativeShell {
    pub(crate) connection: NativeConnection,
    #[allow(dead_code)]
    pub(crate) globals: GlobalList,
    pub(crate) queue: EventQueue<NativeShellState>,
    pub(crate) state: NativeShellState,
    /// Compio readiness watch on a clone of the display fd (created once).
    #[cfg(feature = "compio")]
    pub(crate) display_ready: crate::display_io::CompioFdReady,
}

impl NativeShell {
    /// Connect and bind core + xdg + optional seat/scale globals.
    pub fn connect_to_env() -> Result<Self, NativeError> {
        let connection = NativeConnection::connect_to_env()?;
        let (globals, queue) = registry_queue_init::<NativeShellState>(connection.connection())
            .map_err(|error| NativeError::Registry(error.to_string()))?;
        let qh = queue.handle();
        let mut state = NativeShellState::default();

        if let Ok(compositor) = globals.bind::<wl_compositor::WlCompositor, _, _>(&qh, 1..=6, ()) {
            state.compositor = Some(compositor);
        }
        if let Ok(sub) =
            globals.bind::<wl_subcompositor::WlSubcompositor, _, _>(&qh, 1..=1, ())
        {
            state.subcompositor = Some(sub);
        }
        if let Ok(shm) = globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ()) {
            state.shm = Some(shm);
        }
        if let Ok(wm_base) = globals.bind::<xdg_wm_base::XdgWmBase, _, _>(&qh, 1..=6, ()) {
            state.wm_base_version = wm_base.version();
            state.wm_base = Some(wm_base);
        }
        // Bind every advertised wl_seat (multi-seat compositors).
        for global in globals.contents().clone_list() {
            if global.interface == "wl_seat" {
                let version = global.version.clamp(1, 9);
                let seat = globals
                    .registry()
                    .bind::<wl_seat::WlSeat, _, _>(global.name, version, &qh, ());
                state.register_seat(global.name, seat);
            }
        }
        if let Ok(ddm) =
            globals.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(&qh, 1..=3, ())
        {
            state.data_device_manager = Some(ddm);
        }
        // Bind every advertised wl_output (multi-instance).
        for global in globals.contents().clone_list() {
            if global.interface == "wl_output" {
                let version = global.version.clamp(1, 4);
                let output = globals
                    .registry()
                    .bind::<wl_output::WlOutput, _, _>(global.name, version, &qh, ());
                state
                    .output_objects
                    .insert(output.id().protocol_id(), global.name);
                state.output_proxies.insert(global.name, output);
                state.outputs.insert(
                    global.name,
                    super::types::OutputRecord {
                        scale: 1,
                        make: String::new(),
                        model: String::new(),
                        name: None,
                        description: None,
                        x: 0,
                        y: 0,
                        physical_width: 0,
                        physical_height: 0,
                        mode_width: 0,
                        mode_height: 0,
                        mode_refresh_mhz: 0,
                        done: false,
                    },
                );
            }
        }
        if let Ok(viewporter) = globals.bind::<wp_viewporter::WpViewporter, _, _>(&qh, 1..=1, ()) {
            state.viewporter = Some(viewporter);
        }
        if let Ok(presentation) =
            globals.bind::<wp_presentation::WpPresentation, _, _>(&qh, 1..=1, ())
        {
            state.presentation = Some(presentation);
        }
        if let Ok(psm) = globals.bind::<
            zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1,
            _,
            _,
        >(&qh, 1..=1, ())
        {
            state.primary_selection_manager = Some(psm);
        }
        if let Ok(idle) = globals.bind::<
            zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1,
            _,
            _,
        >(&qh, 1..=1, ())
        {
            state.idle_inhibit_manager = Some(idle);
        }
        // Mesa requires version ≥3; feedback needs ≥4. Prefer highest available.
        if let Ok(dmabuf) = globals.bind::<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1, _, _>(
            &qh,
            3..=5,
            (),
        ) {
            state.linux_dmabuf_version = dmabuf.version();
            state.linux_dmabuf = Some(dmabuf);
        }
        if let Ok(frac) = globals
            .bind::<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1, _, _>(
                &qh,
                1..=1,
                (),
            )
        {
            state.fractional_manager = Some(frac);
        }
        if let Ok(cursor) = globals.bind::<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1, _, _>(
            &qh,
            1..=1,
            (),
        ) {
            state.cursor_shape_manager = Some(cursor);
        }
        if let Ok(tim) = globals.bind::<
            wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3,
            _,
            _,
        >(&qh, 1..=1, ())
        {
            state.text_input_manager = Some(tim);
        }
        if let Ok(layer) = globals.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(
            &qh,
            1..=5,
            (),
        ) {
            state.layer_shell_version = layer.version();
            state.layer_shell = Some(layer);
        }
        if let Ok(dialog) = globals.bind::<
            wayland_protocols::xdg::dialog::v1::client::xdg_wm_dialog_v1::XdgWmDialogV1,
            _,
            _,
        >(&qh, 1..=1, ())
        {
            state.xdg_wm_dialog = Some(dialog);
        }
        if let Ok(icon_mgr) = globals.bind::<
            wayland_protocols::xdg::toplevel_icon::v1::client::xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1,
            _,
            _,
        >(&qh, 1..=1, ())
        {
            state.toplevel_icon_manager = Some(icon_mgr);
        }
        if let Ok(blur_mgr) = globals.bind::<
            wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
            _,
            _,
        >(&qh, 1..=1, ())
        {
            state.background_effect_manager = Some(blur_mgr);
        }
        if let Ok(deco) = globals.bind::<
            wayland_protocols::xdg::decoration::zv1::client::zxdg_decoration_manager_v1::ZxdgDecorationManagerV1,
            _,
            _,
        >(&qh, 1..=1, ())
        {
            state.decoration_manager = Some(deco);
        }
        if let Ok(act) =
            globals.bind::<xdg_activation_v1::XdgActivationV1, _, _>(&qh, 1..=1, ())
        {
            state.activation = Some(act);
        }
        if let Ok(gestures) =
            globals.bind::<zwp_pointer_gestures_v1::ZwpPointerGesturesV1, _, _>(&qh, 1..=3, ())
        {
            state.pointer_gestures = Some(gestures);
        }
        if let Ok(rel) = globals
            .bind::<zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1, _, _>(
                &qh,
                1..=1,
                (),
            )
        {
            state.relative_pointer_manager = Some(rel);
        }
        if let Ok(pc) = globals.bind::<
            wayland_protocols::wp::pointer_constraints::zv1::client::zwp_pointer_constraints_v1::ZwpPointerConstraintsV1,
            _,
            _,
        >(&qh, 1..=1, ())
        {
            state.pointer_constraints = Some(pc);
        }

        if state.compositor.is_none() {
            return Err(NativeError::Registry("wl_compositor missing".into()));
        }
        if state.shm.is_none() {
            return Err(NativeError::Registry("wl_shm missing".into()));
        }
        if state.wm_base.is_none() {
            return Err(NativeError::Registry("xdg_wm_base missing".into()));
        }

        if let (Some(tim), Some(seat)) = (state.text_input_manager.as_ref(), state.seat.as_ref()) {
            state.text_input = Some(tim.get_text_input(seat, &qh, ()));
        }

        #[cfg(feature = "compio")]
        let display_ready = crate::display_io::CompioFdReady::watch(connection.as_fd())
            .map_err(NativeError::from)?;

        let mut shell = Self {
            connection,
            globals,
            queue,
            state,
            #[cfg(feature = "compio")]
            display_ready,
        };
        // Bind data-device / primary selection on every seat (multi-seat ready).
        shell.ensure_all_seat_transfer_devices();
        // Flush binds so the compositor can reply with capability events
        // (e.g. ext-background-effect Capabilities) before the first set_blur.
        shell.connection.flush()?;
        let _ = shell.dispatch_pending()?;
        // One non-blocking drain is enough for in-socket replies; a full
        // blocking roundtrip is intentionally avoided here (would stall if
        // the compositor is silent). set_blur + pending_blur cover races.
        Ok(shell)
    }

    pub fn connection(&self) -> &NativeConnection {
        &self.connection
    }

    /// Force a display write of any queued requests (and clear the dirty flag).
    ///
    /// Prefer relying on the pump; use this when the next step blocks waiting
    /// for the compositor without going through [`Self::dispatch_pending`].
    pub fn flush(&self) -> Result<(), NativeError> {
        self.connection.flush()
    }

    /// Borrow the **non-blocking** display socket for external event loops.
    ///
    /// This is a normal fd: epoll/kqueue/calloop/tokio/`poll` all work. After
    /// it reports readable, call [`Self::try_read_and_dispatch`].
    pub fn display_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.connection.as_fd()
    }

    /// Long-lived Compio readiness watch on a clone of the display fd.
    #[cfg(feature = "compio")]
    pub(crate) fn display_ready(&self) -> &crate::display_io::CompioFdReady {
        &self.display_ready
    }

    pub fn has_fractional_scale(&self) -> bool {
        self.state.fractional_manager.is_some() && self.state.viewporter.is_some()
    }

    pub fn has_cursor_shape(&self) -> bool {
        self.state.cursor_shape_manager.is_some()
    }

    /// Bound `zwlr_layer_shell_v1` interface version (0 if unbound).
    pub fn layer_shell_version(&self) -> u32 {
        self.state.layer_shell_version
    }

    pub fn capabilities(&self) -> NativeCapabilities {
        NativeCapabilities {
            fractional_scale: self.state.fractional_manager.is_some(),
            viewporter: self.state.viewporter.is_some(),
            cursor_shape: self.state.cursor_shape_manager.is_some(),
            seat: self.state.seat.is_some() || !self.state.seats.is_empty(),
            seat_count: self.state.seats.len() as u32,
            pointer: self.state.pointer.is_some()
                || self.state.seats.values().any(|s| s.pointer.is_some())
                || self
                    .state
                    .seat_capabilities
                    .contains(wayland_client::protocol::wl_seat::Capability::Pointer),
            keyboard: self.state.keyboard.is_some()
                || self.state.seats.values().any(|s| s.keyboard.is_some())
                || self
                    .state
                    .seat_capabilities
                    .contains(wayland_client::protocol::wl_seat::Capability::Keyboard),
            touch: self.state.touch.is_some()
                || self.state.seats.values().any(|s| s.touch.is_some())
                || self
                    .state
                    .seat_capabilities
                    .contains(wayland_client::protocol::wl_seat::Capability::Touch),
            output_count: self.state.outputs.len() as u32,
            data_device: self.state.data_device.is_some(),
            xkb: self.state.xkb.is_some(),
            text_input: self.state.text_input.is_some(),
            layer_shell: self.state.layer_shell.is_some(),
            activation: self.state.activation.is_some(),
            pointer_gestures: self.state.pointer_gestures.is_some(),
            pointer_gesture_hold: self
                .state
                .pointer_gestures
                .as_ref()
                .is_some_and(|g| g.version() >= 3),
            relative_pointer: self.state.relative_pointer_manager.is_some(),
            xdg_dialog: self.state.xdg_wm_dialog.is_some(),
            toplevel_icon: self.state.toplevel_icon_manager.is_some(),
            background_blur: self.state.background_blur_capable,
            xdg_decoration: self.state.decoration_manager.is_some(),
            pointer_constraints: self.state.pointer_constraints.is_some(),
            subcompositor: self.state.subcompositor.is_some(),
            presentation: self.state.presentation.is_some(),
            primary_selection: self.state.primary_selection_manager.is_some(),
            idle_inhibit: self.state.idle_inhibit_manager.is_some(),
            linux_dmabuf: self.state.linux_dmabuf.is_some(),
            linux_dmabuf_version: self.state.linux_dmabuf_version,
        }
    }

    pub fn has_presentation(&self) -> bool {
        self.state.presentation.is_some()
    }

    pub fn has_primary_selection(&self) -> bool {
        self.state.primary_selection_manager.is_some()
    }

    pub fn has_idle_inhibit(&self) -> bool {
        self.state.idle_inhibit_manager.is_some()
    }

    pub fn set_idle_inhibit(
        &mut self,
        id: NativeSurfaceId,
        inhibit: bool,
    ) -> Result<(), NativeError> {
        if !inhibit {
            if let Some(inhibitor) = self.state.idle_inhibitors.remove(&id) {
                inhibitor.destroy();
                self.connection.mark_dirty();
            }
            return Ok(());
        }
        if self.state.idle_inhibitors.contains_key(&id) {
            return Ok(());
        }
        let manager = self
            .state
            .idle_inhibit_manager
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("zwp_idle_inhibit_manager_v1 missing".into()))?;
        let wl = self
            .state
            .wl_surface(id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?
            .clone();
        let qh = self.queue.handle();
        let inhibitor = manager.create_inhibitor(&wl, &qh, ());
        self.state.idle_inhibitors.insert(id, inhibitor);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Presentation clock id advertised by `wp_presentation.clock_id`, if any.
    pub fn presentation_clock_id(&self) -> Option<u32> {
        self.state.presentation_clock_id
    }

    pub fn has_pointer_constraints(&self) -> bool {
        self.state.pointer_constraints.is_some()
    }

    pub fn has_xdg_decoration(&self) -> bool {
        self.state.decoration_manager.is_some()
    }

    pub fn has_xdg_dialog(&self) -> bool {
        self.state.xdg_wm_dialog.is_some()
    }

    pub fn has_toplevel_icon(&self) -> bool {
        self.state.toplevel_icon_manager.is_some()
    }

    pub fn has_background_blur(&self) -> bool {
        self.state.background_blur_capable
    }

    pub fn preferred_icon_sizes(&self) -> &[u32] {
        &self.state.preferred_icon_sizes
    }

    /// Enable `zwp_relative_pointer_v1` for the seat pointer (unaccelerated motion).
    pub fn enable_relative_pointer(&mut self) -> Result<(), NativeError> {
        if self.state.relative_pointer.is_some() {
            return Ok(());
        }
        let manager = self
            .state
            .relative_pointer_manager
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("relative_pointer_manager missing".into()))?;
        let pointer = self
            .state
            .pointer
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("no pointer".into()))?;
        let qh = self.queue.handle();
        self.state.relative_pointer = Some(manager.get_relative_pointer(pointer, &qh, ()));
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn disable_relative_pointer(&mut self) -> Result<(), NativeError> {
        if let Some(rel) = self.state.relative_pointer.take() {
            rel.destroy();
            self.connection.mark_dirty();
        }
        Ok(())
    }

    pub fn has_layer_shell(&self) -> bool {
        self.state.layer_shell.is_some()
    }

    pub fn has_activation(&self) -> bool {
        self.state.activation.is_some()
    }

    /// Request an `xdg_activation_v1` token for `surface`.
    ///
    /// Completes with [`NativeShellEvent::ActivationToken`].
    pub fn request_activation_token(
        &mut self,
        surface: NativeSurfaceId,
        app_id: Option<&str>,
    ) -> Result<(), NativeError> {
        let activation = self
            .state
            .activation
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("xdg_activation_v1 missing".into()))?;
        let wl = self
            .state
            .toplevels
            .get(&surface)
            .map(|t| t.wl.clone())
            .or_else(|| self.state.popups.get(&surface).map(|p| p.wl.clone()))
            .or_else(|| self.state.layers.get(&surface).map(|l| l.wl.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {surface:?}")))?;
        let qh = self.queue.handle();
        let token = activation.get_activation_token(&qh, ());
        if let Some(app_id) = app_id {
            token.set_app_id(app_id.to_string());
        }
        if let (Some(serial), Some(seat)) =
            (self.state.last_input_serial, self.state.seat.as_ref())
        {
            token.set_serial(serial, seat);
        }
        token.set_surface(&wl);
        token.commit();
        let obj_id = token.id().protocol_id();
        self.state
            .activation_tokens
            .insert(obj_id, (surface, token));
        self.connection.mark_dirty();
        Ok(())
    }

    /// Activate `surface` with a previously obtained token string.
    pub fn activate_with_token(
        &mut self,
        surface: NativeSurfaceId,
        token: impl Into<String>,
    ) -> Result<(), NativeError> {
        let activation = self
            .state
            .activation
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("xdg_activation_v1 missing".into()))?;
        let wl = self
            .state
            .toplevels
            .get(&surface)
            .map(|t| t.wl.clone())
            .or_else(|| self.state.popups.get(&surface).map(|p| p.wl.clone()))
            .or_else(|| self.state.layers.get(&surface).map(|l| l.wl.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {surface:?}")))?;
        activation.activate(token.into(), &wl);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Number of bound seats.
    #[inline]
    pub fn seat_count(&self) -> usize {
        self.state.seats.len()
    }

    /// Snapshot of bound seats (registry name + optional compositor seat name).
    pub fn seats(&self) -> Vec<crate::SeatInfo> {
        let mut list: Vec<_> = self
            .state
            .seats
            .values()
            .map(|s| crate::SeatInfo {
                id: crate::SeatId::from_raw(s.global_name),
                name: s.name.clone(),
                has_keyboard: s.keyboard.is_some(),
                has_pointer: s.pointer.is_some(),
                has_touch: s.touch.is_some(),
            })
            .collect();
        list.sort_by_key(|s| s.id.get());
        list
    }

    /// Keyboard-focused surface on `seat`, if any.
    pub fn seat_keyboard_focus(
        &self,
        seat: crate::SeatId,
    ) -> Option<NativeSurfaceId> {
        self.state
            .seats
            .get(&seat.get())
            .and_then(|s| s.keyboard_focus)
    }

    /// Pointer-focused surface on `seat`, if any.
    pub fn seat_pointer_focus(
        &self,
        seat: crate::SeatId,
    ) -> Option<NativeSurfaceId> {
        self.state
            .seats
            .get(&seat.get())
            .and_then(|s| s.pointer_focus)
    }

    /// Latest input serial on `seat` (for seat-scoped grabs / selections).
    pub fn seat_last_input_serial(&self, seat: crate::SeatId) -> Option<u32> {
        self.state
            .seats
            .get(&seat.get())
            .and_then(|s| s.last_input_serial)
    }

    /// Pointer-enter serial on `seat` (cursor shape, etc.).
    pub fn seat_pointer_enter_serial(&self, seat: crate::SeatId) -> Option<u32> {
        self.state
            .seats
            .get(&seat.get())
            .and_then(|s| s.pointer_enter_serial)
    }

    /// Build an [`crate::InputSerial`] from the latest serial on `seat`.
    ///
    /// Returns `None` if the seat is unknown or has no serial yet.
    pub fn seat_input_serial(
        &self,
        seat: crate::SeatId,
        source: crate::InputSerialSource,
    ) -> Option<crate::InputSerial> {
        let rec = self.state.seats.get(&seat.get())?;
        let serial = rec.last_input_serial?;
        Some(crate::InputSerial::new(rec.seat.clone(), serial, source))
    }

    /// Primary seat id (first bound / current primary), if any.
    pub fn primary_seat_id(&self) -> Option<crate::SeatId> {
        let primary = self.state.seat.as_ref()?;
        self.state
            .seat_objects
            .get(&primary.id().protocol_id())
            .copied()
            .map(crate::SeatId::from_raw)
    }

    /// Whether `seat` has a bound `wl_data_device` (clipboard / DnD).
    pub fn seat_has_data_device(&self, seat: crate::SeatId) -> bool {
        self.state
            .seats
            .get(&seat.get())
            .is_some_and(|s| s.data_device.is_some())
    }

    /// Whether `seat` has a bound primary selection device.
    pub fn seat_has_primary_device(&self, seat: crate::SeatId) -> bool {
        self.state
            .seats
            .get(&seat.get())
            .is_some_and(|s| s.primary_device.is_some())
    }

    /// Create a bufferless GPU-friendly popup (no solid SHM fill).
    ///
    /// When `grab` is `Some`, the popup is grabbed with that seat+serial.
    /// When `None`, no grab is requested.
    pub fn create_popup_gpu(
        &mut self,
        parent: NativeSurfaceId,
        positioner: &NativePopupPositioner,
        grab: Option<&crate::InputSerial>,
    ) -> Result<NativeSurfaceId, NativeError> {
        if positioner.size.width == 0 || positioner.size.height == 0 {
            return Err(NativeError::Protocol("popup size must be non-zero".into()));
        }
        let parent_xdg = self
            .state
            .toplevels
            .get(&parent)
            .map(|t| t.xdg.clone())
            .or_else(|| self.state.popups.get(&parent).map(|p| p.xdg.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown parent {parent:?}")))?;

        let qh = self.queue.handle();
        let compositor = self
            .state
            .compositor
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?;
        let wm_base = self
            .state
            .wm_base
            .as_ref()
            .ok_or_else(|| NativeError::Registry("xdg_wm_base".into()))?;

        let pos = wm_base.create_positioner(&qh, ());
        apply_positioner(&pos, positioner);

        let wl = compositor.create_surface(&qh, ());
        wl.set_buffer_scale(1);
        let xdg = wm_base.get_xdg_surface(&wl, &qh, ());
        let popup = xdg.get_popup(Some(&parent_xdg), &pos, &qh, ());
        pos.destroy();

        if let Some(serial) = grab {
            popup.grab(serial.seat(), serial.serial());
        }

        let w = positioner.size.width;
        let h = positioner.size.height;
        wl.commit();

        let id = self.state.alloc_id();
        self.state
            .popup_objects
            .insert(popup.id().protocol_id(), id);
        self.state
            .xdg_surface_objects
            .insert(xdg.id().protocol_id(), id);
        self.state
            .wl_surface_objects
            .insert(wl.id().protocol_id(), id);
        self.state.popups.insert(
            id,
            PopupRecord {
                wl,
                xdg,
                popup,
                parent,
                buffer: None,
                _pool: None,
                _file: None,
                configured: false,
                pending_geom: None,
                last_configure_serial: 0,
                pending_reposition_token: None,
                logical_w: w,
                logical_h: h,
            },
        );
        self.connection.mark_dirty();
        Ok(id)
    }

    /// Request a `wl_surface.frame` callback; emits [`NativeShellEvent::Frame`].
    ///
    /// Works for toplevel, popup, and layer surfaces (needed for GPU present pacing).
    ///
    /// Coalesces: if a frame callback is already outstanding for `id`, this is
    /// a no-op success (avoids stacking callbacks when the app arms every redraw).
    pub fn request_frame(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        if self.state.frame_pending.contains(&id) {
            return Ok(());
        }
        let qh = self.queue.handle();
        let wl = self
            .state
            .wl_surface(id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        let callback = wl.frame(&qh, ());
        self.state
            .frame_callbacks
            .insert(callback.id().protocol_id(), id);
        self.state.frame_pending.insert(id);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Whether a `wl_surface.frame` callback is outstanding for `id`.
    #[inline]
    pub fn is_frame_pending(&self, id: NativeSurfaceId) -> bool {
        self.state.is_frame_pending(id)
    }

    /// Logical size last known for the surface (configure / client updates).
    pub fn logical_size(&self, id: NativeSurfaceId) -> Option<crate::geometry::LogicalSize> {
        self.state
            .logical_size(id)
            .map(|(w, h)| crate::geometry::LogicalSize::new(w, h))
    }

    /// Request `wp_presentation.feedback` for the next commit on `id`.
    ///
    /// Associates with the **next** `wl_surface.commit` (call before or with
    /// the content submission). Emits [`NativeShellEvent::Presented`] or
    /// [`NativeShellEvent::PresentationDiscarded`]. No-ops cleanly when the
    /// global is missing (returns `Ok` so callers can always arm feedback).
    ///
    /// Coalesces: if feedback is already outstanding for `id`, this is a no-op
    /// success (avoids stacking feedback objects when arming every redraw).
    pub fn request_presentation_feedback(
        &mut self,
        id: NativeSurfaceId,
    ) -> Result<(), NativeError> {
        let Some(presentation) = self.state.presentation.clone() else {
            return Ok(());
        };
        if self.state.presentation_pending.contains(&id) {
            return Ok(());
        }
        let qh = self.queue.handle();
        let wl = self
            .state
            .wl_surface(id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?
            .clone();
        let feedback = presentation.feedback(&wl, &qh, ());
        let obj = feedback.id().protocol_id();
        self.state.presentation_feedbacks.insert(
            obj,
            super::types::PresentationFeedbackRecord {
                surface: id,
                sync_output: None,
            },
        );
        self.state.presentation_pending.insert(id);
        self.connection.mark_dirty();
        Ok(())
    }

    /// Whether presentation feedback is outstanding for `id`.
    #[inline]
    pub fn is_presentation_pending(&self, id: NativeSurfaceId) -> bool {
        self.state.is_presentation_pending(id)
    }

    /// Set the pointer cursor via `wp_cursor_shape` when available.
    pub fn set_cursor_shape(&mut self, shape: CursorShape) -> Result<(), NativeError> {
        let serial = self
            .state
            .pointer_enter_serial
            .ok_or_else(|| NativeError::Protocol("no pointer enter serial".into()))?;
        let pointer = self
            .state
            .pointer
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("no pointer".into()))?;
        let manager = self
            .state
            .cursor_shape_manager
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("wp_cursor_shape_manager_v1 missing".into()))?;
        let qh = self.queue.handle();
        let device = manager.get_pointer(pointer, &qh, ());
        device.set_shape(serial, shape);
        device.destroy();
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn dispatch_pending(&mut self) -> Result<usize, NativeError> {
        // Coalesce any API requests queued since the last pump.
        self.connection.flush_if_needed()?;
        let n = self.queue.dispatch_pending(&mut self.state)?;
        self.after_dispatch()?;
        // CSD / blur work from after_dispatch may have marked dirty again.
        self.connection.flush_if_needed()?;
        Ok(n)
    }

    /// Apply deferred CSD work that cannot run inside `Dispatch` handlers.
    fn after_dispatch(&mut self) -> Result<(), NativeError> {
        if self.state.pending_primary_seat_rebind {
            self.state.pending_primary_seat_rebind = false;
            self.rebind_primary_seat_devices();
        }
        // Hotplugged seats may still lack transfer devices.
        if self
            .state
            .seats
            .values()
            .any(|s| s.data_device.is_none() && self.state.data_device_manager.is_some())
        {
            self.ensure_all_seat_transfer_devices();
        }
        if self.state.pending_blur_replay {
            self.state.pending_blur_replay = false;
            let _ = self.apply_pending_blur_all();
        }
        // Apply frame actions collected during pointer dispatch.
        if !self.state.pending_frame_actions.is_empty() {
            let actions = std::mem::take(&mut self.state.pending_frame_actions);
            for (id, action) in actions {
                let _ = self.apply_frame_action(id, action);
            }
        }
        if let Some(cursor) = self.state.pending_csd_cursor.take() {
            self.set_csd_cursor(cursor);
        }
        // Sync CSD after decoration mode / configure size changes.
        if !self.state.pending_csd_refresh.is_empty() {
            let refresh: Vec<_> = self.state.pending_csd_refresh.drain().collect();
            for id in refresh {
                let _ = self.sync_csd_for(id);
            }
        }
        // Redraw dirty frames (hover, title, state). Skip scan when no CSD.
        if !self.state.csd_frames.is_empty() {
            let dirty: Vec<_> = self
                .state
                .csd_frames
                .iter()
                .filter(|(_, f)| f.dirty())
                .map(|(&id, _)| id)
                .collect();
            for id in dirty {
                let _ = self.redraw_csd(id);
            }
        }
        Ok(())
    }

    /// Non-blocking: flush dirty requests, optionally read, and dispatch.
    ///
    /// Suitable for external event loops: call after the display fd is readable,
    /// or poll periodically. Returns the number of events dispatched.
    pub fn try_read_and_dispatch(&mut self) -> Result<usize, NativeError> {
        self.connection.flush_if_needed()?;
        let mut n = self.dispatch_pending()?;
        n += self.read_display_nonblocking()?;
        Ok(n)
    }

    /// Shared non-blocking display read (no wait). Used by protocol pumps and
    /// the Compio runtime after a readiness completion.
    pub(crate) fn read_display_nonblocking(&mut self) -> Result<usize, NativeError> {
        match self.connection.connection().prepare_read() {
            None => self.dispatch_pending(),
            Some(guard) => match guard.read() {
                Ok(_) => self.dispatch_pending(),
                Err(wayland_client::backend::WaylandError::Io(err))
                    if err.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    Ok(0)
                }
                Err(error) => Err(error.into()),
            },
        }
    }

    /// Compio-driven pump: wait until the display is readable, then dispatch.
    ///
    /// Reuses the shell’s long-lived Compio readiness watch (no per-wait fd
    /// clone). Requires `feature = "compio"` and a Compio executor.
    #[cfg(feature = "compio")]
    pub async fn pump_once(&mut self) -> Result<usize, NativeError> {
        self.connection.flush_if_needed()?;
        let mut n = self.dispatch_pending()?;
        // If the queue is empty, wait once then read; otherwise drain only.
        match self.connection.connection().prepare_read() {
            None => {
                n += self.dispatch_pending()?;
            }
            Some(guard) => {
                self.display_ready.wait_readable().await?;
                match guard.read() {
                    Ok(_) => {
                        n += self.dispatch_pending()?;
                    }
                    Err(wayland_client::backend::WaylandError::Io(err))
                        if err.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(n)
    }

    /// Number of queued shell events waiting to be drained.
    #[inline]
    pub fn pending_event_count(&self) -> usize {
        self.state.events.len()
    }

    // Seat transfer rebind helpers live in `seat.rs`.

    pub fn drain_events(&mut self) -> impl Iterator<Item = NativeShellEvent> + '_ {
        self.state.events.drain(..)
    }

    pub fn drain_events_into(&mut self, target: &mut Vec<NativeShellEvent>) {
        // `append` reuses both buffers' capacity (no per-event realloc dance).
        target.append(&mut self.state.events);
    }

    /// Drain native events mapped to the public [`crate::Event`] model.
    ///
    /// Uses the live seat proxy when present so keyboard/pointer serials are real
    /// `WlSeat` handles. Unmapped events (clipboard, modifiers, …) are dropped;
    /// use [`Self::drain_events`] when those are needed.
    pub fn drain_public_events(
        &mut self,
        surfaces: &mut crate::native::event_map::SurfaceIdMap,
        map_state: &mut crate::native::event_map::NativeEventMapState,
        out: &mut Vec<crate::Event>,
    ) {
        let seat = self.state.seat.clone();
        if let Some(serial) = self.state.last_input_serial {
            map_state.last_serial = serial;
        }
        // Reserve once for the common 1:1 map case (avoids grow mid-batch).
        out.reserve(self.state.events.len());
        for event in self.state.events.drain(..) {
            if let Some(mapped) = crate::native::event_map::map_native_event_full(
                event,
                surfaces,
                seat.as_ref(),
                map_state,
            ) {
                out.push(mapped);
            }
        }
    }

    /// Borrow the seat proxy (for building [`crate::InputSerial`] outside the shell).
    pub fn seat(&self) -> Option<&wayland_client::protocol::wl_seat::WlSeat> {
        self.state.seat.as_ref()
    }

    pub fn last_input_serial(&self) -> Option<u32> {
        self.state.last_input_serial
    }

    pub fn toplevel_count(&self) -> usize {
        self.state.toplevels.len()
    }

    pub fn is_configured(&self, id: NativeSurfaceId) -> bool {
        self.state.toplevels.get(&id).is_some_and(|t| t.configured)
    }

    pub fn scale_factor(&self, id: NativeSurfaceId) -> Option<f64> {
        self.state.scale_factor(id)
    }

    /// Suggested physical buffer size from logical size × scale (ceil).
    ///
    /// Useful for Vulkan / wgpu swapchain recreation without re-deriving
    /// fractional scale math in every client.
    pub fn buffer_size(&self, id: NativeSurfaceId) -> Option<(u32, u32)> {
        let (w, h) = self.state.logical_size(id)?;
        let scale = self.state.scale_factor(id).unwrap_or(1.0).max(0.01);
        let bw = ((w as f64) * scale).ceil().max(1.0) as u32;
        let bh = ((h as f64) * scale).ceil().max(1.0) as u32;
        Some((bw, bh))
    }

    /// Role of a live surface, if known.
    pub fn surface_kind(&self, id: NativeSurfaceId) -> Option<crate::surface::SurfaceKind> {
        use crate::surface::SurfaceKind;
        if let Some(record) = self.state.toplevels.get(&id) {
            return Some(if record.parent.is_some() || record.dialog.is_some() {
                SurfaceKind::Dialog
            } else {
                SurfaceKind::Toplevel
            });
        }
        if self.state.popups.contains_key(&id) {
            return Some(SurfaceKind::Popup);
        }
        if self.state.layers.contains_key(&id) {
            return Some(SurfaceKind::Layer);
        }
        None
    }

    /// Renderer lease for wgpu / Vulkan (`VK_KHR_wayland_surface`).
    ///
    /// Works for toplevels (including dialogs), popups, and layer surfaces.
    /// The returned handle keeps `Connection` + `wl_surface` alive; keep it
    /// for the lifetime of the GPU surface.
    pub fn surface_handle(
        &self,
        id: NativeSurfaceId,
    ) -> Result<NativeSurfaceHandle, NativeError> {
        use crate::surface::SurfaceKind;
        let conn = self.connection.connection().clone();
        if let Some(record) = self.state.toplevels.get(&id) {
            let kind = if record.parent.is_some() || record.dialog.is_some() {
                SurfaceKind::Dialog
            } else {
                SurfaceKind::Toplevel
            };
            return Ok(NativeSurfaceHandle::new(conn, record.wl.clone(), id, kind));
        }
        if let Some(record) = self.state.popups.get(&id) {
            return Ok(NativeSurfaceHandle::new(
                conn,
                record.wl.clone(),
                id,
                SurfaceKind::Popup,
            ));
        }
        if let Some(record) = self.state.layers.get(&id) {
            return Ok(NativeSurfaceHandle::new(
                conn,
                record.wl.clone(),
                id,
                SurfaceKind::Layer,
            ));
        }
        Err(NativeError::Protocol(format!("unknown surface {id:?}")))
    }

    pub fn set_title(
        &mut self,
        id: NativeSurfaceId,
        title: impl Into<String>,
    ) -> Result<(), NativeError> {
        let title = title.into();
        let record = self
            .state
            .toplevels
            .get_mut(&id)
            .ok_or(NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.title = title.clone();
        record.toplevel.set_title(title.clone());
        if let Some(frame) = self.state.csd_frames.get_mut(&id) {
            frame.set_title(title);
        }
        let _ = self.redraw_csd(id);
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn set_app_id(
        &mut self,
        id: NativeSurfaceId,
        app_id: impl Into<String>,
    ) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or(NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.toplevel.set_app_id(app_id.into());
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn destroy_toplevel(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        // Drop idle inhibitor before the surface goes away.
        let _ = self.set_idle_inhibit(id, false);
        // Cancel any live touch points on this surface before proxies die.
        self.state.cancel_touch_for_surface(id);
        self.state.clear_surface_protocol_state(id);
        // Drop surface-scoped dmabuf feedback.
        if let Some(fb) = self.state.dmabuf_surface_feedback_objs.remove(&id) {
            let pid = fb.id().protocol_id();
            self.state.dmabuf_feedback_surfaces.remove(&pid);
            self.state.dmabuf_feedback_pending.remove(&pid);
            self.state.dmabuf_tranche_pending.remove(&pid);
            fb.destroy();
        }
        self.state.dmabuf_surface_feedback.remove(&id);
        // Destroy child popups first (parent must outlive them for some compositors).
        let child_popups: Vec<_> = self
            .state
            .popups
            .iter()
            .filter(|(_, p)| p.parent == id)
            .map(|(&pid, _)| pid)
            .collect();
        for pid in child_popups {
            let _ = self.destroy_popup(pid);
        }

        // Also destroy child dialog toplevels that named us as parent.
        let child_dialogs: Vec<_> = self
            .state
            .toplevels
            .iter()
            .filter(|(_, t)| t.parent == Some(id))
            .map(|(&cid, _)| cid)
            .collect();
        for cid in child_dialogs {
            let _ = self.destroy_toplevel(cid);
        }

        self.destroy_csd(id);
        let Some(record) = self.state.toplevels.remove(&id) else {
            return Err(NativeError::Protocol(format!("unknown surface {id:?}")));
        };
        self.state.clear_live_constraints_for(id);
        self.state
            .toplevel_objects
            .remove(&record.toplevel.id().protocol_id());
        self.state
            .xdg_surface_objects
            .remove(&record.xdg.id().protocol_id());
        self.state
            .wl_surface_objects
            .remove(&record.wl.id().protocol_id());
        if let Some(dialog) = record.dialog {
            dialog.destroy();
        }
        if let Some(effect) = record.blur_effect {
            effect.destroy();
        }
        if let Some(deco) = record.decoration {
            self.state
                .decoration_objects
                .remove(&deco.id().protocol_id());
            deco.destroy();
        }
        for (_file, pool, buffer) in record.icon_shm {
            buffer.destroy();
            pool.destroy();
        }
        if let Some(ref frac) = record.fractional {
            self.state
                .fractional_objects
                .remove(&frac.id().protocol_id());
            frac.destroy();
        }
        if let Some(vp) = record.viewport {
            vp.destroy();
        }
        record.toplevel.destroy();
        record.xdg.destroy();
        if let Some(buffer) = record.buffer {
            buffer.destroy();
        }
        if let Some(pool) = record._pool {
            pool.destroy();
        }
        record.wl.destroy();
        self.connection.mark_dirty();
        Ok(())
    }

    /// Create an `xdg_popup` child of a configured toplevel (or another popup).
    ///
    /// When `grab` is `Some`, the popup is grabbed with that seat+serial.
    pub fn create_popup(
        &mut self,
        parent: NativeSurfaceId,
        positioner: &NativePopupPositioner,
        grab: Option<&crate::InputSerial>,
    ) -> Result<NativeSurfaceId, NativeError> {
        if positioner.size.width == 0 || positioner.size.height == 0 {
            return Err(NativeError::Protocol("popup size must be non-zero".into()));
        }
        let parent_xdg = self
            .state
            .toplevels
            .get(&parent)
            .map(|t| t.xdg.clone())
            .or_else(|| self.state.popups.get(&parent).map(|p| p.xdg.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown parent {parent:?}")))?;

        let qh = self.queue.handle();
        let compositor = self
            .state
            .compositor
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?;
        let shm = self
            .state
            .shm
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_shm".into()))?;
        let wm_base = self
            .state
            .wm_base
            .as_ref()
            .ok_or_else(|| NativeError::Registry("xdg_wm_base".into()))?;

        let pos = wm_base.create_positioner(&qh, ());
        apply_positioner(&pos, positioner);

        let wl = compositor.create_surface(&qh, ());
        wl.set_buffer_scale(1);
        let xdg = wm_base.get_xdg_surface(&wl, &qh, ());
        let popup = xdg.get_popup(Some(&parent_xdg), &pos, &qh, ());
        pos.destroy();

        if let Some(serial) = grab {
            popup.grab(serial.seat(), serial.serial());
        }

        let w = positioner.size.width;
        let h = positioner.size.height;
        let (file, pool, buffer) =
            shm::create_solid_buffer(shm, &qh, w, h, [0xff, 0x33, 0x33, 0x33])
                .map_err(|e| NativeError::Io(e.to_string()))?;
        wl.commit();

        let id = self.state.alloc_id();
        self.state
            .popup_objects
            .insert(popup.id().protocol_id(), id);
        self.state
            .xdg_surface_objects
            .insert(xdg.id().protocol_id(), id);
        self.state
            .wl_surface_objects
            .insert(wl.id().protocol_id(), id);
        self.state.popups.insert(
            id,
            PopupRecord {
                wl,
                xdg,
                popup,
                parent,
                buffer: Some(buffer),
                _pool: Some(pool),
                _file: Some(file),
                configured: false,
                pending_geom: None,
                last_configure_serial: 0,
                pending_reposition_token: None,
                logical_w: w,
                logical_h: h,
            },
        );
        self.connection.mark_dirty();
        Ok(id)
    }

    /// Reposition an existing popup (`xdg_popup.reposition`, requires xdg_wm_base ≥ 3).
    pub fn reposition_popup(
        &mut self,
        id: NativeSurfaceId,
        positioner: &NativePopupPositioner,
        token: u32,
    ) -> Result<(), NativeError> {
        if self.state.wm_base_version < 3 {
            return Err(NativeError::Protocol(
                "xdg_popup.reposition requires xdg_wm_base version 3+".into(),
            ));
        }
        if positioner.size.width == 0 || positioner.size.height == 0 {
            return Err(NativeError::Protocol("popup size must be non-zero".into()));
        }
        let qh = self.queue.handle();
        let wm_base = self
            .state
            .wm_base
            .as_ref()
            .ok_or_else(|| NativeError::Registry("xdg_wm_base".into()))?;
        let record = self
            .state
            .popups
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown popup {id:?}")))?;
        let pos = wm_base.create_positioner(&qh, ());
        apply_positioner(&pos, positioner);
        record.popup.reposition(&pos, token);
        pos.destroy();
        if let Some(record) = self.state.popups.get_mut(&id) {
            record.pending_reposition_token = Some(token);
            record.logical_w = positioner.size.width;
            record.logical_h = positioner.size.height;
        }
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn supports_popup_reposition(&self) -> bool {
        self.state.wm_base_version >= 3
    }

    pub fn destroy_popup(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        self.state.cancel_touch_for_surface(id);
        self.state.clear_surface_protocol_state(id);
        let Some(record) = self.state.popups.remove(&id) else {
            return Err(NativeError::Protocol(format!("unknown popup {id:?}")));
        };
        self.state
            .popup_objects
            .remove(&record.popup.id().protocol_id());
        self.state
            .xdg_surface_objects
            .remove(&record.xdg.id().protocol_id());
        self.state
            .wl_surface_objects
            .remove(&record.wl.id().protocol_id());
        record.popup.destroy();
        record.xdg.destroy();
        if let Some(buffer) = record.buffer {
            buffer.destroy();
        }
        if let Some(pool) = record._pool {
            pool.destroy();
        }
        record.wl.destroy();
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn popup_count(&self) -> usize {
        self.state.popups.len()
    }

    pub fn is_popup_configured(&self, id: NativeSurfaceId) -> bool {
        self.state.popups.get(&id).is_some_and(|p| p.configured)
    }
}

fn apply_positioner(pos: &xdg_positioner::XdgPositioner, value: &NativePopupPositioner) {
    pos.set_size(value.size.width as i32, value.size.height as i32);
    pos.set_anchor_rect(
        value.anchor_rect.origin.x,
        value.anchor_rect.origin.y,
        value.anchor_rect.size.width as i32,
        value.anchor_rect.size.height as i32,
    );
    pos.set_anchor(map_anchor(value.anchor));
    pos.set_gravity(map_gravity(value.gravity));
    pos.set_constraint_adjustment(map_constraints(value.constraints));
    pos.set_offset(value.offset.x, value.offset.y);
}

fn map_anchor(value: PopupAnchor) -> xdg_positioner::Anchor {
    match value {
        PopupAnchor::None => xdg_positioner::Anchor::None,
        PopupAnchor::Top => xdg_positioner::Anchor::Top,
        PopupAnchor::Bottom => xdg_positioner::Anchor::Bottom,
        PopupAnchor::Left => xdg_positioner::Anchor::Left,
        PopupAnchor::Right => xdg_positioner::Anchor::Right,
        PopupAnchor::TopLeft => xdg_positioner::Anchor::TopLeft,
        PopupAnchor::BottomLeft => xdg_positioner::Anchor::BottomLeft,
        PopupAnchor::TopRight => xdg_positioner::Anchor::TopRight,
        PopupAnchor::BottomRight => xdg_positioner::Anchor::BottomRight,
    }
}

fn map_gravity(value: Gravity) -> xdg_positioner::Gravity {
    match value {
        Gravity::None => xdg_positioner::Gravity::None,
        Gravity::Top => xdg_positioner::Gravity::Top,
        Gravity::Bottom => xdg_positioner::Gravity::Bottom,
        Gravity::Left => xdg_positioner::Gravity::Left,
        Gravity::Right => xdg_positioner::Gravity::Right,
        Gravity::TopLeft => xdg_positioner::Gravity::TopLeft,
        Gravity::BottomLeft => xdg_positioner::Gravity::BottomLeft,
        Gravity::TopRight => xdg_positioner::Gravity::TopRight,
        Gravity::BottomRight => xdg_positioner::Gravity::BottomRight,
    }
}

fn map_constraints(value: ConstraintAdjustments) -> xdg_positioner::ConstraintAdjustment {
    let mut result = xdg_positioner::ConstraintAdjustment::empty();
    if value.contains(ConstraintAdjustments::SLIDE_X) {
        result |= xdg_positioner::ConstraintAdjustment::SlideX;
    }
    if value.contains(ConstraintAdjustments::SLIDE_Y) {
        result |= xdg_positioner::ConstraintAdjustment::SlideY;
    }
    if value.contains(ConstraintAdjustments::FLIP_X) {
        result |= xdg_positioner::ConstraintAdjustment::FlipX;
    }
    if value.contains(ConstraintAdjustments::FLIP_Y) {
        result |= xdg_positioner::ConstraintAdjustment::FlipY;
    }
    if value.contains(ConstraintAdjustments::RESIZE_X) {
        result |= xdg_positioner::ConstraintAdjustment::ResizeX;
    }
    if value.contains(ConstraintAdjustments::RESIZE_Y) {
        result |= xdg_positioner::ConstraintAdjustment::ResizeY;
    }
    result
}


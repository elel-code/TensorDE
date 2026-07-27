//! Additional Wayland protocol handlers beyond the core shell path.

use smithay::{
    input::pointer::PointerHandle,
    utils::{IsAlive, Logical, Point, Serial},
    wayland::{
        foreign_toplevel_list::ForeignToplevelListHandler,
        idle_inhibit::IdleInhibitHandler,
        input_method::{InputMethodHandler, PopupSurface as ImPopupSurface},
        keyboard_shortcuts_inhibit::{
            KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState,
            KeyboardShortcutsInhibitor,
        },
        pointer_constraints::{PointerConstraintsHandler, with_pointer_constraint},
        pointer_warp::PointerWarpHandler,
        session_lock::{LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker},
        xdg_foreign::XdgForeignHandler,
        xdg_system_bell::XdgSystemBellHandler,
    },
};
use tracing::{debug, info, warn};
use wayland_server::{
    Resource,
    protocol::{wl_output::WlOutput, wl_surface::WlSurface},
};

use crate::protocol::extensions::security_context::{
    SecurityContextHandler, SecurityContextListener,
};
use crate::protocol::state::{ObjectKey, RuntimeState, SessionLockState};

impl RuntimeState {
    pub(crate) fn refresh_idle_inhibition(&mut self) {
        self.protocol_side
            .idle_inhibitors
            .retain(|surface| surface.alive());
        let inhibited = !self.protocol_side.idle_inhibitors.is_empty();
        self.protocol_globals
            .idle_notifier()
            .set_is_inhibited(inhibited);
    }

    pub(crate) fn publish_foreign_toplevel_from_surface(&mut self, surface: &WlSurface) {
        let (title, app_id) = toplevel_metadata(surface);
        self.publish_foreign_toplevel(
            surface,
            title.unwrap_or_default(),
            app_id.unwrap_or_default(),
        );
    }

    pub(crate) fn refresh_foreign_toplevel_metadata(
        &mut self,
        surface: &smithay::wayland::shell::xdg::ToplevelSurface,
    ) {
        let (title, app_id) = toplevel_metadata(surface.wl_surface());
        self.update_foreign_toplevel(surface.wl_surface(), title.as_deref(), app_id.as_deref());
    }

    /// Activate a pointer constraint when focus matches the constrained surface.
    pub(crate) fn maybe_activate_pointer_constraint(&mut self) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let Some(focus) = pointer.current_focus() else {
            return;
        };
        with_pointer_constraint(&focus, &pointer, |constraint| {
            if let Some(constraint) = constraint {
                constraint.activate();
            }
        });
    }

    pub(crate) fn publish_foreign_toplevel(
        &mut self,
        surface: &WlSurface,
        title: impl Into<String>,
        app_id: impl Into<String>,
    ) {
        let key = ObjectKey::from_surface(surface);
        if self.protocol_side.foreign_toplevels.contains_key(&key) {
            return;
        }
        let title = title.into();
        let app_id = app_id.into();
        let handle = self
            .protocol_globals
            .foreign_toplevel_list()
            .new_toplevel::<RuntimeState>(title.clone(), app_id.clone());
        // Stable surface key for capture/IPC without scanning the map by title.
        handle.user_data().insert_if_missing(|| key);
        self.protocol_side.foreign_toplevels.insert(key, handle);
        debug!(
            surface = surface.id().protocol_id(),
            title = %title,
            app_id = %app_id,
            "ext-foreign-toplevel-list: published"
        );
    }

    pub(crate) fn update_foreign_toplevel(
        &mut self,
        surface: &WlSurface,
        title: Option<&str>,
        app_id: Option<&str>,
    ) {
        let key = ObjectKey::from_surface(surface);
        let Some(handle) = self.protocol_side.foreign_toplevels.get(&key) else {
            return;
        };
        // Smithay skips no-op title/app_id events; only send `done` when either
        // field actually changes so idle clients are not woken every configure.
        let title_changed = title.is_some_and(|title| handle.title() != title);
        let app_id_changed = app_id.is_some_and(|app_id| handle.app_id() != app_id);
        if !title_changed && !app_id_changed {
            return;
        }
        if let Some(title) = title.filter(|_| title_changed) {
            handle.send_title(title);
        }
        if let Some(app_id) = app_id.filter(|_| app_id_changed) {
            handle.send_app_id(app_id);
        }
        handle.send_done();
    }

    pub(crate) fn close_foreign_toplevel(&mut self, surface: &WlSurface) {
        let key = ObjectKey::from_surface(surface);
        if let Some(handle) = self.protocol_side.foreign_toplevels.remove(&key) {
            handle.send_closed();
        }
    }

    pub(crate) fn session_is_locked(&self) -> bool {
        self.protocol_side.session_lock.is_some()
    }

    /// Whether compositor keybinds (except VT recovery) should be suppressed.
    #[allow(dead_code)] // reserved for the upcoming bind path
    pub(crate) fn shortcuts_inhibited_for(&self, surface: &WlSurface) -> bool {
        self.protocol_side
            .shortcut_inhibitors
            .get(&ObjectKey::from_surface(surface))
            .is_some_and(KeyboardShortcutsInhibitor::is_active)
    }
}

impl PointerConstraintsHandler for RuntimeState {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        let focused = pointer
            .current_focus()
            .is_some_and(|focus| focus.id() == surface.id());
        if focused {
            with_pointer_constraint(surface, pointer, |constraint| {
                if let Some(constraint) = constraint {
                    constraint.activate();
                }
            });
        }
    }

    fn remove_constraint(&mut self, _surface: &WlSurface, _pointer: &PointerHandle<Self>) {}

    fn cursor_position_hint(
        &mut self,
        surface: &WlSurface,
        pointer: &PointerHandle<Self>,
        location: Point<f64, Logical>,
    ) {
        let active = with_pointer_constraint(surface, pointer, |constraint| {
            constraint.is_some_and(|c| c.is_active())
        });
        if !active {
            return;
        }
        let Some(window) = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(surface))
        else {
            return;
        };
        let Some(geometry) = self.space.element_geometry(window) else {
            return;
        };
        let target = geometry.loc.to_f64() + location;
        #[cfg(feature = "tty")]
        if let Some(bounds) = self.pointer_coordinate_space() {
            pointer.set_location(crate::protocol::input::constrain_pointer_location(
                target, bounds,
            ));
        } else {
            pointer.set_location(target);
        }
        #[cfg(not(feature = "tty"))]
        pointer.set_location(target);
        #[cfg(feature = "tty")]
        self.request_redraw_at(pointer.current_location());
    }
}

impl IdleInhibitHandler for RuntimeState {
    fn inhibit(&mut self, surface: WlSurface) {
        self.protocol_side.idle_inhibitors.insert(surface);
        self.refresh_idle_inhibition();
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        self.protocol_side.idle_inhibitors.remove(&surface);
        self.refresh_idle_inhibition();
    }
}

impl KeyboardShortcutsInhibitHandler for RuntimeState {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        self.protocol_globals.keyboard_shortcuts_inhibit()
    }

    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        inhibitor.activate();
        self.protocol_side
            .shortcut_inhibitors
            .insert(ObjectKey::from_surface(inhibitor.wl_surface()), inhibitor);
    }

    fn inhibitor_destroyed(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        self.protocol_side
            .shortcut_inhibitors
            .remove(&ObjectKey::from_surface(inhibitor.wl_surface()));
    }
}

impl XdgSystemBellHandler for RuntimeState {
    fn ring(&mut self, surface: Option<WlSurface>) {
        info!(
            surface = surface.as_ref().map(|s| s.id().protocol_id()),
            "xdg-system-bell ring"
        );
    }
}

impl PointerWarpHandler for RuntimeState {
    fn warp_pointer(
        &mut self,
        surface: WlSurface,
        _pointer: wayland_server::protocol::wl_pointer::WlPointer,
        pos: Point<f64, Logical>,
        _serial: Serial,
    ) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let origin = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(&surface))
            .and_then(|window| self.space.element_geometry(window))
            .map(|geo| geo.loc.to_f64());
        #[cfg(feature = "tty")]
        let origin = origin.or_else(|| {
            self.layer_surface_origin(&surface)
                .map(|point| point.to_f64())
        });
        let Some(origin) = origin else {
            return;
        };
        let target = origin + pos;
        #[cfg(feature = "tty")]
        if let Some(bounds) = self.pointer_coordinate_space() {
            pointer.set_location(crate::protocol::input::constrain_pointer_location(
                target, bounds,
            ));
        } else {
            pointer.set_location(target);
        }
        #[cfg(not(feature = "tty"))]
        pointer.set_location(target);
        #[cfg(feature = "tty")]
        self.request_redraw_at(pointer.current_location());
    }
}

impl ForeignToplevelListHandler for RuntimeState {
    fn foreign_toplevel_list_state(
        &mut self,
    ) -> &mut smithay::wayland::foreign_toplevel_list::ForeignToplevelListState {
        self.protocol_globals.foreign_toplevel_list()
    }
}

impl XdgForeignHandler for RuntimeState {
    fn xdg_foreign_state(&mut self) -> &mut smithay::wayland::xdg_foreign::XdgForeignState {
        self.protocol_globals.xdg_foreign()
    }
}

impl SessionLockHandler for RuntimeState {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        self.protocol_globals.session_lock()
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        if self.protocol_side.session_lock.is_some() {
            return;
        }
        confirmation.lock();
        self.protocol_side.session_lock = Some(SessionLockState {
            surfaces: std::collections::HashMap::new(),
        });
        info!("session locked");
        #[cfg(feature = "tty")]
        self.request_redraw_all();
    }

    fn unlock(&mut self) {
        self.protocol_side.session_lock = None;
        info!("session unlocked");
        #[cfg(feature = "tty")]
        {
            self.restore_keyboard_focus();
            self.request_redraw_all();
        }
    }

    fn new_surface(&mut self, surface: LockSurface, output: WlOutput) {
        let Some(lock) = self.protocol_side.session_lock.as_mut() else {
            return;
        };
        let name = self
            .space
            .outputs()
            .find(|candidate| candidate.owns(&output))
            .map(|output| output.name())
            .unwrap_or_else(|| "unknown".to_owned());
        if let Some(output_obj) = self.space.outputs().find(|o| o.owns(&output))
            && let Some(geometry) = self.space.output_geometry(output_obj)
        {
            surface.with_pending_state(|state| {
                state.size = Some(
                    (
                        u32::try_from(geometry.size.w).unwrap_or(1).max(1),
                        u32::try_from(geometry.size.h).unwrap_or(1).max(1),
                    )
                        .into(),
                );
            });
            surface.send_configure();
        }
        lock.surfaces.insert(name, surface);
        #[cfg(feature = "tty")]
        self.request_redraw_all();
    }
}

impl SecurityContextHandler for RuntimeState {
    fn security_context_is_nested(&self, client: &wayland_server::Client) -> bool {
        client
            .get_data::<crate::protocol::state::WaylandClientState>()
            .is_some_and(|data| data.security_context.is_some())
    }

    fn context_created(&mut self, listener: SecurityContextListener) {
        let Some(submitter) = self.security_context_submitter() else {
            warn!("security-context completion runtime is not installed");
            return;
        };
        if let Err(error) = submitter.submit(listener) {
            warn!(?error, "security-context listener queue rejected a request");
        }
    }
}

impl InputMethodHandler for RuntimeState {
    fn new_popup(&mut self, surface: ImPopupSurface) {
        if let Err(error) = self
            .popups
            .track_popup(crate::protocol::state::PopupKind::InputMethod(surface))
        {
            warn!(%error, "failed to track input-method popup");
        }
        #[cfg(feature = "tty")]
        self.request_redraw_workspace();
    }

    fn dismiss_popup(&mut self, surface: ImPopupSurface) {
        let parent = surface.get_parent().map(|parent| parent.surface.clone());
        if let Some(parent) = parent {
            let _ = self.popups.dismiss_popup(
                &parent,
                &crate::protocol::state::PopupKind::InputMethod(surface),
            );
        }
        #[cfg(feature = "tty")]
        self.request_redraw_workspace();
    }

    fn popup_repositioned(&mut self, _surface: ImPopupSurface) {
        #[cfg(feature = "tty")]
        self.request_redraw_workspace();
    }

    fn parent_geometry(&self, parent: &WlSurface) -> smithay::utils::Rectangle<i32, Logical> {
        self.space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(parent))
            .and_then(|window| self.space.element_geometry(window))
            .unwrap_or_default()
    }
}

fn toplevel_metadata(surface: &WlSurface) -> (Option<String>, Option<String>) {
    use smithay::wayland::compositor::with_states;
    use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
    with_states(surface, |states| {
        let data = states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .map(|d| d.lock().unwrap());
        (
            data.as_ref().and_then(|d| d.title.clone()),
            data.as_ref().and_then(|d| d.app_id.clone()),
        )
    })
}

impl smithay::wayland::xdg_toplevel_icon::XdgToplevelIconHandler for RuntimeState {}
impl smithay::wayland::xdg_toplevel_tag::XdgToplevelTagHandler for RuntimeState {}

impl smithay::wayland::selection::wlr_data_control::DataControlHandler for RuntimeState {
    fn data_control_state(
        &mut self,
    ) -> &mut smithay::wayland::selection::wlr_data_control::DataControlState {
        self.protocol_globals.wlr_data_control()
    }
}

impl smithay::wayland::selection::ext_data_control::DataControlHandler for RuntimeState {
    fn data_control_state(
        &mut self,
    ) -> &mut smithay::wayland::selection::ext_data_control::DataControlState {
        self.protocol_globals.ext_data_control()
    }
}

impl smithay::wayland::background_effect::ExtBackgroundEffectHandler for RuntimeState {
    fn capabilities(&self) -> smithay::wayland::background_effect::Capability {
        // Advertise blur only when the scene can mark backdrop sampling.
        // Pixel blur still follows compositor policy radius until the GPU pass.
        smithay::wayland::background_effect::Capability::Blur
    }

    fn set_blur_region(
        &mut self,
        wl_surface: WlSurface,
        region: smithay::wayland::compositor::RegionAttributes,
    ) {
        // Pending state is already on the surface cache; commit applies to ECS.
        // Trace-level only: this runs on protocol traffic, not the flip path.
        debug!(
            surface = wl_surface.id().protocol_id(),
            rects = region.rects.len(),
            "ext-background-effect: blur region pending"
        );
    }

    fn unset_blur_region(&mut self, wl_surface: WlSurface) {
        debug!(
            surface = wl_surface.id().protocol_id(),
            "ext-background-effect: blur region cleared (pending)"
        );
    }
}

#[cfg(feature = "xwayland")]
impl smithay::wayland::xwayland_keyboard_grab::XWaylandKeyboardGrabHandler for RuntimeState {
    fn keyboard_focus_for_xsurface(
        &self,
        surface: &WlSurface,
    ) -> Option<crate::protocol::focus::KeyboardFocusTarget> {
        self.space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(surface))
            .and_then(crate::protocol::state::ProtocolWindow::x11_surface)
            .cloned()
            .map(crate::protocol::focus::KeyboardFocusTarget::from)
            .or_else(|| {
                Some(crate::protocol::focus::KeyboardFocusTarget::from(
                    surface.clone(),
                ))
            })
    }
}

impl crate::protocol::extensions::virtual_pointer::VirtualPointerHandler for RuntimeState {
    fn virtual_pointer_manager_state(
        &mut self,
    ) -> &mut crate::protocol::extensions::virtual_pointer::VirtualPointerManagerState {
        self.protocol_globals.virtual_pointer()
    }

    fn on_virtual_pointer_motion(&mut self, event: tensor_input::RelativeMotionEvent) {
        #[cfg(feature = "tty")]
        self.forward_pointer_motion(event);
        #[cfg(not(feature = "tty"))]
        let _ = event;
    }

    fn on_virtual_pointer_motion_absolute(&mut self, event: tensor_input::AbsoluteMotionEvent) {
        #[cfg(feature = "tty")]
        self.forward_pointer_motion_absolute(event);
        #[cfg(not(feature = "tty"))]
        let _ = event;
    }

    fn on_virtual_pointer_button(&mut self, event: tensor_input::PointerButtonEvent) {
        #[cfg(feature = "tty")]
        self.forward_pointer_button(event);
        #[cfg(not(feature = "tty"))]
        let _ = event;
    }

    fn on_virtual_pointer_axis(&mut self, event: tensor_input::PointerAxisEvent) {
        #[cfg(feature = "tty")]
        self.forward_pointer_axis(event);
        #[cfg(not(feature = "tty"))]
        let _ = event;
    }
}

impl crate::protocol::extensions::gamma_control::GammaControlHandler for RuntimeState {
    fn gamma_control_manager_state(
        &mut self,
    ) -> &mut crate::protocol::extensions::gamma_control::GammaControlManagerState {
        self.protocol_globals.gamma_control()
    }

    fn gamma_output_id(&self, output: &WlOutput) -> Option<tensor_host::ConnectorId> {
        #[cfg(feature = "tty")]
        {
            self.space
                .outputs()
                .find(|candidate| candidate.owns(output))?
                .user_data()
                .get::<crate::backend::BackendOutputId>()
                .copied()
        }
        #[cfg(not(feature = "tty"))]
        {
            let _ = output;
            None
        }
    }

    fn get_gamma_size(&mut self, output: tensor_host::ConnectorId) -> Option<u32> {
        #[cfg(feature = "tty")]
        {
            self.backend
                .as_ref()
                .and_then(|backend| backend.gamma_size(output))
        }
        #[cfg(not(feature = "tty"))]
        {
            let _ = output;
            None
        }
    }

    fn set_gamma(
        &mut self,
        output: tensor_host::ConnectorId,
        ramp: Option<Vec<u16>>,
    ) -> Option<()> {
        #[cfg(feature = "tty")]
        {
            self.backend
                .as_mut()
                .and_then(|backend| backend.set_gamma(output, ramp.as_deref()))
        }
        #[cfg(not(feature = "tty"))]
        {
            let _ = (output, ramp);
            None
        }
    }
}

impl smithay::wayland::image_capture_source::ImageCaptureSourceHandler for RuntimeState {
    fn source_destroyed(
        &mut self,
        _source: smithay::wayland::image_capture_source::ImageCaptureSource,
    ) {
    }
}

impl smithay::wayland::image_capture_source::OutputCaptureSourceHandler for RuntimeState {
    fn output_capture_source_state(
        &mut self,
    ) -> &mut smithay::wayland::image_capture_source::OutputCaptureSourceState {
        self.protocol_globals.output_capture_source()
    }

    fn output_source_created(
        &mut self,
        source: smithay::wayland::image_capture_source::ImageCaptureSource,
        output: &smithay::output::Output,
    ) {
        source.user_data().insert_if_missing(|| output.downgrade());
    }
}

impl smithay::wayland::image_capture_source::ToplevelCaptureSourceHandler for RuntimeState {
    fn toplevel_capture_source_state(
        &mut self,
    ) -> &mut smithay::wayland::image_capture_source::ToplevelCaptureSourceState {
        self.protocol_globals.toplevel_capture_source()
    }

    fn toplevel_source_created(
        &mut self,
        source: smithay::wayland::image_capture_source::ImageCaptureSource,
        toplevel: smithay::wayland::foreign_toplevel_list::ForeignToplevelHandle,
    ) {
        source
            .user_data()
            .insert_if_missing(|| toplevel.downgrade());
    }
}

impl crate::protocol::extensions::ext_workspace::ExtWorkspaceHandler for RuntimeState {
    fn ext_workspace_manager_state(
        &mut self,
    ) -> &mut crate::protocol::extensions::ext_workspace::ExtWorkspaceManagerState {
        self.protocol_globals.ext_workspace()
    }

    fn activate_workspace_id(&mut self, id: crate::ecs::WorkspaceId) {
        let _ = self.activate_workspace(id);
    }

    fn workspace_snapshot(
        &self,
    ) -> crate::protocol::extensions::ext_workspace::WorkspaceProtocolSnapshot {
        crate::protocol::extensions::ext_workspace::WorkspaceProtocolSnapshot {
            active: self.active_workspace(),
            count: self.workspace_count(),
        }
    }
}

impl crate::protocol::extensions::output_management::OutputManagementHandler for RuntimeState {
    fn output_management_state(
        &mut self,
    ) -> &mut crate::protocol::extensions::output_management::OutputManagementState {
        self.protocol_globals.output_management()
    }

    fn apply_output_configuration(
        &mut self,
        updates: Vec<(
            String,
            crate::protocol::extensions::output_management::OutputHeadUpdate,
        )>,
    ) -> Result<(), String> {
        RuntimeState::apply_output_configuration(self, updates)
    }

    fn current_output_heads(
        &self,
    ) -> Vec<crate::protocol::extensions::output_management::HeadSnapshot> {
        self.output_management_heads()
    }
}

impl smithay::wayland::image_copy_capture::ImageCopyCaptureHandler for RuntimeState {
    fn image_copy_capture_state(
        &mut self,
    ) -> &mut smithay::wayland::image_copy_capture::ImageCopyCaptureState {
        self.protocol_globals.image_copy_capture()
    }

    fn capture_constraints(
        &mut self,
        source: &smithay::wayland::image_capture_source::ImageCaptureSource,
    ) -> Option<smithay::wayland::image_copy_capture::BufferConstraints> {
        self.capture_constraints_for_source(source)
    }

    fn new_session(&mut self, session: smithay::wayland::image_copy_capture::Session) {
        self.store_capture_session(session);
    }

    fn new_cursor_session(&mut self, session: smithay::wayland::image_copy_capture::CursorSession) {
        self.store_cursor_capture_session(session);
    }

    fn frame(
        &mut self,
        session: &smithay::wayland::image_copy_capture::SessionRef,
        frame: smithay::wayland::image_copy_capture::Frame,
    ) {
        self.handle_capture_frame(session, frame);
    }

    fn session_destroyed(&mut self, session: smithay::wayland::image_copy_capture::SessionRef) {
        self.drop_capture_session(&session);
    }
}

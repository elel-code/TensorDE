//! Additional Wayland protocol handlers beyond the core shell path.

use smithay::{
    input::pointer::PointerHandle,
    utils::{Logical, Point},
    wayland::{
        input_method::{InputMethodHandler, PopupSurface as ImPopupSurface},
        pointer_constraints::{PointerConstraintsHandler, with_pointer_constraint},
        session_lock::{LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker},
        xdg_foreign::XdgForeignHandler,
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
    pub(crate) fn maybe_activate_pointer_constraint(&mut self, focus: Option<&WlSurface>) {
        let Some(focus) = focus else {
            return;
        };
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        with_pointer_constraint(focus, &pointer, |constraint| {
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
            .new_toplevel::<RuntimeState>(key, title.clone(), app_id.clone());
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
        handle.send_metadata(title, app_id);
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
            .map(|output| output.name().to_owned())
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
            crate::protocol::globals::output::Output::from_resource(output)
                .map(|output| output.id())
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

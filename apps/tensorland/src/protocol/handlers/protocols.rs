//! Additional Wayland protocol handlers beyond the core shell path.

use tracing::{debug, warn};
use wayland_server::{
    Resource,
    protocol::{wl_output::WlOutput, wl_surface::WlSurface},
};

use crate::protocol::extensions::security_context::{
    SecurityContextHandler, SecurityContextListener,
};
use crate::protocol::globals::xdg_shell::Toplevel;
use crate::protocol::state::{ObjectKey, RuntimeState};

impl RuntimeState {
    pub(crate) fn foreign_toplevel_identifier(&self, surface: &WlSurface) -> Option<String> {
        self.protocol_side
            .foreign_toplevels
            .get(&ObjectKey::from_surface(surface))
            .map(|handle| handle.identifier())
    }

    pub(crate) fn publish_foreign_toplevel_from_surface(&mut self, surface: &WlSurface) {
        let (title, app_id) = self
            .protocol_globals
            .xdg_shell
            .toplevel_for_surface(surface)
            .map(|toplevel| toplevel.metadata())
            .unwrap_or_default();
        self.publish_foreign_toplevel(
            surface,
            title.unwrap_or_default(),
            app_id.unwrap_or_default(),
        );
    }

    pub(crate) fn refresh_foreign_toplevel_metadata(&mut self, surface: &Toplevel) {
        let (title, app_id) = surface.metadata();
        self.update_foreign_toplevel(surface.wl_surface(), title.as_deref(), app_id.as_deref());
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

impl crate::protocol::extensions::virtual_pointer::VirtualPointerHandler for RuntimeState {
    fn virtual_pointer_manager_state(
        &mut self,
    ) -> &mut crate::protocol::extensions::virtual_pointer::VirtualPointerManagerState {
        self.protocol_globals.virtual_pointer()
    }

    fn create_virtual_pointer(
        &mut self,
        pointer: crate::protocol::extensions::virtual_pointer::VirtualPointer,
    ) {
        if let Some(crate::protocol::globals::seat::SeatOwner::Transient(id)) =
            self.virtual_pointer_seat_owner(&pointer)
        {
            self.protocol_globals.transient_seat.pointer_created(id);
        }
    }

    fn destroy_virtual_pointer(
        &mut self,
        pointer: crate::protocol::extensions::virtual_pointer::VirtualPointer,
    ) {
        if let Some(crate::protocol::globals::seat::SeatOwner::Transient(id)) =
            self.virtual_pointer_seat_owner(&pointer)
        {
            self.protocol_globals.transient_seat.pointer_destroyed(id);
        }
    }

    fn on_virtual_pointer_motion(
        &mut self,
        pointer: &crate::protocol::extensions::virtual_pointer::VirtualPointer,
        event: tensor_event::RelativeMotionEvent,
    ) {
        if self.route_transient_pointer(pointer) {
            return;
        }
        #[cfg(feature = "tty")]
        self.forward_pointer_motion(event);
        #[cfg(not(feature = "tty"))]
        let _ = event;
    }

    fn on_virtual_pointer_motion_absolute(
        &mut self,
        pointer: &crate::protocol::extensions::virtual_pointer::VirtualPointer,
        event: tensor_event::AbsoluteMotionEvent,
    ) {
        if self.route_transient_pointer(pointer) {
            return;
        }
        #[cfg(feature = "tty")]
        self.forward_pointer_motion_absolute(event);
        #[cfg(not(feature = "tty"))]
        let _ = event;
    }

    fn on_virtual_pointer_button(
        &mut self,
        pointer: &crate::protocol::extensions::virtual_pointer::VirtualPointer,
        event: tensor_event::PointerButtonEvent,
    ) {
        if self.route_transient_pointer(pointer) {
            return;
        }
        #[cfg(feature = "tty")]
        self.forward_pointer_button(event);
        #[cfg(not(feature = "tty"))]
        let _ = event;
    }

    fn on_virtual_pointer_axis(
        &mut self,
        pointer: &crate::protocol::extensions::virtual_pointer::VirtualPointer,
        event: tensor_event::PointerAxisEvent,
    ) {
        if self.route_transient_pointer(pointer) {
            return;
        }
        #[cfg(feature = "tty")]
        self.forward_pointer_axis(event);
        #[cfg(not(feature = "tty"))]
        let _ = event;
    }
}

impl RuntimeState {
    fn virtual_pointer_seat_owner(
        &self,
        pointer: &crate::protocol::extensions::virtual_pointer::VirtualPointer,
    ) -> Option<crate::protocol::globals::seat::SeatOwner> {
        pointer.seat().map_or(
            Some(crate::protocol::globals::seat::SeatOwner::Primary),
            |seat| self.protocol_globals.seat.owner(seat),
        )
    }

    fn route_transient_pointer(
        &mut self,
        pointer: &crate::protocol::extensions::virtual_pointer::VirtualPointer,
    ) -> bool {
        let Some(crate::protocol::globals::seat::SeatOwner::Transient(id)) =
            self.virtual_pointer_seat_owner(pointer)
        else {
            return false;
        };
        self.protocol_globals.transient_seat.pointer_event(id);
        true
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

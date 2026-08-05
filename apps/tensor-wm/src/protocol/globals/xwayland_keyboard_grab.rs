//! XWayland-only active keyboard grabs.
//!
//! Logical compositor focus remains Tensor policy. This owner selects a
//! separate core-keyboard wire focus while an X11 active grab exists, and
//! session lock always takes precedence over that selection.

use wayland_protocols::xwayland::keyboard_grab::zv1::server::{
    zwp_xwayland_keyboard_grab_manager_v1::{self, ZwpXwaylandKeyboardGrabManagerV1},
    zwp_xwayland_keyboard_grab_v1::{self, ZwpXwaylandKeyboardGrabV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    serial::next_serial,
    state::RuntimeState,
    xwayland::XWaylandClientData,
};

const VERSION: u32 = 1;
const MAX_XWAYLAND_KEYBOARD_GRABS: usize = 32;

#[derive(Clone, Debug)]
struct KeyboardGrab {
    resource: Weak<ZwpXwaylandKeyboardGrabV1>,
    surface: WlSurface,
}

#[derive(Debug)]
pub(crate) struct XWaylandKeyboardGrabProtocol {
    _global: GlobalId,
    grabs: Vec<KeyboardGrab>,
    active: Option<ObjectId>,
}

impl XWaylandKeyboardGrabProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, ZwpXwaylandKeyboardGrabManagerV1, _>(
                VERSION, GlobalData,
            ),
            grabs: Vec::with_capacity(MAX_XWAYLAND_KEYBOARD_GRABS),
            active: None,
        }
    }

    fn register(&mut self, resource: &ZwpXwaylandKeyboardGrabV1, surface: WlSurface) -> bool {
        self.grabs.retain(|grab| grab.resource.upgrade().is_ok());
        if self.grabs.len() == MAX_XWAYLAND_KEYBOARD_GRABS {
            return false;
        }
        self.grabs.push(KeyboardGrab {
            resource: resource.downgrade(),
            surface,
        });
        self.active = Some(resource.id());
        true
    }

    fn remove(&mut self, resource: &ZwpXwaylandKeyboardGrabV1) -> bool {
        self.grabs
            .retain(|grab| grab.resource.id() != resource.id());
        if self.active.as_ref() == Some(&resource.id()) {
            self.active = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn active_surface(&self) -> Option<WlSurface> {
        let active = self.active.as_ref()?;
        self.grabs
            .iter()
            .find(|grab| &grab.resource.id() == active && grab.resource.upgrade().is_ok())
            .map(|grab| grab.surface.clone())
            .filter(Resource::is_alive)
    }

    pub(crate) fn surface_destroyed(&mut self, surface: &WlSurface) -> bool {
        let Some(active) = self.active.as_ref() else {
            return false;
        };
        let destroyed_active = self
            .grabs
            .iter()
            .any(|grab| &grab.resource.id() == active && grab.surface.id() == surface.id());
        if destroyed_active {
            self.active = None;
        }
        destroyed_active
    }
}

#[derive(Clone, Copy, Debug)]
struct GlobalData;

#[derive(Clone, Copy, Debug)]
struct ManagerData;

#[derive(Clone, Copy, Debug)]
struct GrabData;

impl GlobalDispatchDelegate<ZwpXwaylandKeyboardGrabManagerV1, RuntimeState> for GlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpXwaylandKeyboardGrabManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, ManagerData);
    }

    fn can_view(&self, client: &Client) -> bool {
        client.get_data::<XWaylandClientData>().is_some()
    }
}

impl DispatchDelegate<ZwpXwaylandKeyboardGrabManagerV1, RuntimeState> for ManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &ZwpXwaylandKeyboardGrabManagerV1,
        request: zwp_xwayland_keyboard_grab_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_xwayland_keyboard_grab_manager_v1::Request::Destroy => {}
            zwp_xwayland_keyboard_grab_manager_v1::Request::GrabKeyboard { id, surface, seat } => {
                if !state.protocol_globals.seat.owns(&seat) {
                    resource.post_error(0_u32, "seat is not owned by Tensor");
                    return;
                }
                let grab = data_init.init(id, GrabData);
                if !state
                    .protocol_globals
                    .xwayland_keyboard_grab
                    .register(&grab, surface)
                {
                    resource.post_error(0_u32, "XWayland keyboard-grab capacity exceeded");
                    return;
                }
                state.sync_keyboard_wire_focus(next_serial());
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwpXwaylandKeyboardGrabV1, RuntimeState> for GrabData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &ZwpXwaylandKeyboardGrabV1,
        request: zwp_xwayland_keyboard_grab_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_xwayland_keyboard_grab_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: wayland_server::backend::ClientId,
        resource: &ZwpXwaylandKeyboardGrabV1,
    ) {
        if state
            .protocol_globals
            .xwayland_keyboard_grab
            .remove(resource)
        {
            state.sync_keyboard_wire_focus(next_serial());
        }
    }
}

delegate_global_dispatch!(RuntimeState, ZwpXwaylandKeyboardGrabManagerV1, GlobalData);
delegate_dispatch!(RuntimeState, ZwpXwaylandKeyboardGrabManagerV1, ManagerData);
delegate_dispatch!(RuntimeState, ZwpXwaylandKeyboardGrabV1, GrabData);

#[cfg(test)]
mod tests;

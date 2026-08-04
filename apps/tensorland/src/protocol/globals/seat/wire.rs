use tensor_util::Point;
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource,
    backend::ClientId,
    protocol::{
        wl_keyboard::{self, WlKeyboard},
        wl_pointer::{self, WlPointer},
        wl_seat::{self, WlSeat},
        wl_touch::{self, WlTouch},
    },
};

use super::{CURSOR_IMAGE_ROLE, CursorSurfaceState, SeatOwner, pointer::logical_hotspot};
#[cfg(feature = "tty")]
use crate::protocol::cursor::CursorImage;
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::compositor::{get_role, give_role, with_states},
    state::RuntimeState,
};

#[derive(Clone, Debug)]
pub(in crate::protocol::globals) struct SeatGlobalData {
    owner: SeatOwner,
    allowed_client: Option<ClientId>,
}

impl SeatGlobalData {
    pub(in crate::protocol::globals) const fn primary() -> Self {
        Self {
            owner: SeatOwner::Primary,
            allowed_client: None,
        }
    }

    pub(in crate::protocol::globals) fn transient(id: u64, allowed_client: ClientId) -> Self {
        Self {
            owner: SeatOwner::Transient(id),
            allowed_client: Some(allowed_client),
        }
    }
}

#[derive(Debug)]
pub(super) struct SeatData {
    pub(super) owner: SeatOwner,
}

#[derive(Debug)]
struct KeyboardData {
    owner: SeatOwner,
}

#[derive(Debug)]
pub(super) struct PointerData {
    pub(super) owner: SeatOwner,
}

#[derive(Debug)]
struct TouchData {
    owner: SeatOwner,
}

impl GlobalDispatchDelegate<WlSeat, RuntimeState> for SeatGlobalData {
    fn bind(
        &self,
        state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WlSeat>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let seat = data_init.init(resource, SeatData { owner: self.owner });
        if seat.version() >= 2 {
            let name = match self.owner {
                SeatOwner::Primary => "tensorland".to_owned(),
                SeatOwner::Transient(id) => format!("tensorland-transient-{id}"),
            };
            seat.name(name);
        }
        match self.owner {
            SeatOwner::Primary => {
                seat.capabilities(state.protocol_globals.seat.capabilities());
                state.protocol_globals.seat.seats.push(seat.downgrade());
            }
            SeatOwner::Transient(id) => {
                seat.capabilities(wl_seat::Capability::empty());
                state.protocol_globals.transient_seat.bound(id, &seat);
            }
        }
    }

    fn can_view(&self, client: &Client) -> bool {
        self.allowed_client
            .as_ref()
            .is_none_or(|allowed| *allowed == client.id())
    }
}

impl DispatchDelegate<WlSeat, RuntimeState> for SeatData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _seat: &WlSeat,
        request: wl_seat::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_seat::Request::GetPointer { id } => {
                let pointer = data_init.init(id, PointerData { owner: self.owner });
                if self.owner == SeatOwner::Primary {
                    state
                        .protocol_globals
                        .seat
                        .insert_pointer(client.id(), pointer);
                }
            }
            wl_seat::Request::GetKeyboard { id } => {
                let keyboard = data_init.init(id, KeyboardData { owner: self.owner });
                if self.owner == SeatOwner::Primary {
                    let client_id = client.id();
                    let snapshot = state.keyboard_snapshot(&client_id);
                    state
                        .protocol_globals
                        .seat
                        .insert_keyboard(client_id, keyboard, snapshot);
                }
            }
            wl_seat::Request::GetTouch { id } => {
                let touch = data_init.init(id, TouchData { owner: self.owner });
                if self.owner == SeatOwner::Primary {
                    state.protocol_globals.seat.insert_touch(client.id(), touch);
                }
            }
            wl_seat::Request::Release => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WlKeyboard, RuntimeState> for KeyboardData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _keyboard: &WlKeyboard,
        request: wl_keyboard::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_keyboard::Request::Release => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, client: ClientId, keyboard: &WlKeyboard) {
        if self.owner == SeatOwner::Primary {
            state
                .protocol_globals
                .seat
                .remove_keyboard(&client, keyboard);
        }
    }
}

impl DispatchDelegate<WlPointer, RuntimeState> for PointerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        pointer: &WlPointer,
        request: wl_pointer::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_pointer::Request::SetCursor {
                serial,
                surface,
                hotspot_x,
                hotspot_y,
            } => {
                #[cfg(feature = "tty")]
                let current_surface = surface
                    .as_ref()
                    .is_some_and(|surface| state.cursor.pointer_uses_surface(surface));
                #[cfg(not(feature = "tty"))]
                let current_surface = false;
                if !state.protocol_globals.seat.pointer_may_set_cursor(
                    serial.into(),
                    &pointer.id(),
                    current_surface,
                ) {
                    return;
                }
                #[cfg(feature = "tty")]
                let cursor_location = state.input_seat.pointer_location();
                #[cfg(feature = "tty")]
                if let Some(location) = cursor_location {
                    state.queue_cursor_redraw_between(0, location, location);
                }
                let surface = match surface {
                    Some(surface) => {
                        if give_role(&surface, CURSOR_IMAGE_ROLE).is_err()
                            && get_role(&surface) != Some(CURSOR_IMAGE_ROLE)
                        {
                            pointer.post_error(
                                wl_pointer::Error::Role,
                                "cursor surface already has another role",
                            );
                            return;
                        }
                        let scale = state.client_scale(client);
                        let hotspot = Point::new(
                            logical_hotspot(hotspot_x, scale),
                            logical_hotspot(hotspot_y, scale),
                        );
                        with_states(&surface, |states| {
                            let storage = states.data_map.get_or_insert(|| {
                                std::sync::Mutex::new(CursorSurfaceState::default())
                            });
                            storage.lock().unwrap().hotspot = hotspot;
                        });
                        Some(surface)
                    }
                    None => None,
                };
                #[cfg(feature = "tty")]
                {
                    state.cursor.set_image(match surface {
                        Some(surface) => CursorImage::Surface(surface),
                        None => CursorImage::Hidden,
                    });
                    state.refresh_cursor_surface_outputs();
                    if let Some(location) = cursor_location {
                        state.queue_cursor_redraw_between(0, location, location);
                        state.flush_queued_redraws();
                    } else {
                        state.request_redraw_workspace();
                    }
                }
                #[cfg(not(feature = "tty"))]
                let _ = surface;
            }
            wl_pointer::Request::Release => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, client: ClientId, pointer: &WlPointer) {
        if self.owner == SeatOwner::Primary {
            state.protocol_globals.seat.remove_pointer(&client, pointer);
        }
    }
}

impl DispatchDelegate<WlTouch, RuntimeState> for TouchData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _touch: &WlTouch,
        request: wl_touch::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_touch::Request::Release => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, client: ClientId, touch: &WlTouch) {
        if self.owner == SeatOwner::Primary {
            state.protocol_globals.seat.remove_touch(&client, touch);
        }
    }
}

delegate_global_dispatch!(RuntimeState, WlSeat, SeatGlobalData);
delegate_dispatch!(RuntimeState, WlSeat, SeatData);
delegate_dispatch!(RuntimeState, WlKeyboard, KeyboardData);
delegate_dispatch!(RuntimeState, WlPointer, PointerData);
delegate_dispatch!(RuntimeState, WlTouch, TouchData);

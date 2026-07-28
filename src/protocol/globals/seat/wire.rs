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

use super::{CURSOR_IMAGE_ROLE, CursorSurfaceState, pointer::logical_hotspot};
#[cfg(feature = "tty")]
use crate::protocol::cursor::CursorImage;
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::compositor::{get_role, give_role, with_states},
    state::RuntimeState,
};

#[derive(Debug)]
pub(super) struct SeatGlobalData;

#[derive(Debug)]
pub(super) struct SeatData;

#[derive(Debug)]
struct KeyboardData;

#[derive(Debug)]
pub(super) struct PointerData;

#[derive(Debug)]
struct TouchData;

impl GlobalDispatchDelegate<WlSeat, RuntimeState> for SeatGlobalData {
    fn bind(
        &self,
        state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WlSeat>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let seat = data_init.init(resource, SeatData);
        if seat.version() >= 2 {
            seat.name("tensor".into());
        }
        seat.capabilities(state.protocol_globals.seat.capabilities());
        state.protocol_globals.seat.seats.push(seat.downgrade());
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
                let pointer = data_init.init(id, PointerData);
                state
                    .protocol_globals
                    .seat
                    .insert_pointer(client.id(), pointer);
            }
            wl_seat::Request::GetKeyboard { id } => {
                let keyboard = data_init.init(id, KeyboardData);
                let client_id = client.id();
                let snapshot = state.keyboard_snapshot(&client_id);
                state
                    .protocol_globals
                    .seat
                    .insert_keyboard(client_id, keyboard, snapshot);
            }
            wl_seat::Request::GetTouch { id } => {
                let touch = data_init.init(id, TouchData);
                state.protocol_globals.seat.insert_touch(client.id(), touch);
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
        state
            .protocol_globals
            .seat
            .remove_keyboard(&client, keyboard);
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
                let grab_surface = state
                    .input_seat
                    .pointer_grab_start()
                    .and_then(|data| data.focus.as_ref())
                    .map(Resource::id);
                if !state.protocol_globals.seat.pointer_may_set_cursor(
                    serial.into(),
                    &pointer.id(),
                    grab_surface.as_ref(),
                ) {
                    return;
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
                    let changed = state.cursor.set_image(match surface {
                        Some(surface) => CursorImage::Surface(surface),
                        None => CursorImage::Hidden,
                    });
                    state.refresh_cursor_surface_outputs();
                    if changed {
                        if let Some(location) = state.input_seat.pointer_location() {
                            state.request_redraw_at(location);
                        } else {
                            state.request_redraw_workspace();
                        }
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
        state.protocol_globals.seat.remove_pointer(&client, pointer);
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
        state.protocol_globals.seat.remove_touch(&client, touch);
    }
}

delegate_global_dispatch!(RuntimeState, WlSeat, SeatGlobalData);
delegate_dispatch!(RuntimeState, WlSeat, SeatData);
delegate_dispatch!(RuntimeState, WlKeyboard, KeyboardData);
delegate_dispatch!(RuntimeState, WlPointer, PointerData);
delegate_dispatch!(RuntimeState, WlTouch, TouchData);

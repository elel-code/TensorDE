//! Tensor-owned relative-pointer wire state.

use std::collections::HashMap;

use wayland_protocols::wp::relative_pointer::zv1::server::{
    zwp_relative_pointer_manager_v1::{self, ZwpRelativePointerManagerV1},
    zwp_relative_pointer_v1::{self, ZwpRelativePointerV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource,
    backend::{ClientId, GlobalId},
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct RelativePointerProtocol {
    _global: GlobalId,
    clients: HashMap<ClientId, Vec<ZwpRelativePointerV1>>,
}

impl RelativePointerProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, ZwpRelativePointerManagerV1, _>(
                1,
                RelativePointerGlobalData,
            ),
            clients: HashMap::new(),
        }
    }

    fn insert(&mut self, client: ClientId, pointer: ZwpRelativePointerV1) {
        self.clients.entry(client).or_default().push(pointer);
    }

    fn remove(&mut self, client: &ClientId, pointer: &ZwpRelativePointerV1) {
        let mut remove_client = false;
        if let Some(pointers) = self.clients.get_mut(client) {
            if let Some(index) = pointers
                .iter()
                .position(|candidate| candidate.id() == pointer.id())
            {
                pointers.swap_remove(index);
            }
            remove_client = pointers.is_empty();
        }
        if remove_client {
            self.clients.remove(client);
        }
    }

    pub(crate) fn motion(
        &self,
        client: &ClientId,
        client_scale: f64,
        event: tensor_input::RelativeMotionEvent,
    ) {
        let Some(pointers) = self.clients.get(client) else {
            return;
        };
        if !client_scale.is_finite() || client_scale <= 0.0 {
            return;
        }
        let dx = event.delta_x * client_scale;
        let dy = event.delta_y * client_scale;
        if !dx.is_finite()
            || !dy.is_finite()
            || !event.unaccelerated_x.is_finite()
            || !event.unaccelerated_y.is_finite()
        {
            return;
        }
        let time_usec = event.time_ns / 1_000;
        let time_hi = (time_usec >> 32) as u32;
        let time_lo = time_usec as u32;
        for pointer in pointers {
            pointer.relative_motion(
                time_hi,
                time_lo,
                dx,
                dy,
                event.unaccelerated_x,
                event.unaccelerated_y,
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn pointer_count(&self) -> usize {
        self.clients.values().map(Vec::len).sum()
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct RelativePointerGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct RelativePointerManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct RelativePointerData {
    client: Option<ClientId>,
}

impl GlobalDispatchDelegate<ZwpRelativePointerManagerV1, RuntimeState>
    for RelativePointerGlobalData
{
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpRelativePointerManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, RelativePointerManagerData);
    }
}

impl DispatchDelegate<ZwpRelativePointerManagerV1, RuntimeState> for RelativePointerManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _manager: &ZwpRelativePointerManagerV1,
        request: zwp_relative_pointer_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_relative_pointer_manager_v1::Request::GetRelativePointer { id, pointer } => {
                let active = state.protocol_globals.seat.owns_pointer(&pointer);
                let client = active.then(|| client.id());
                let relative_pointer = data_init.init(
                    id,
                    RelativePointerData {
                        client: client.clone(),
                    },
                );
                if let Some(client) = client {
                    state
                        .protocol_globals
                        .relative_pointer
                        .insert(client, relative_pointer);
                }
            }
            zwp_relative_pointer_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwpRelativePointerV1, RuntimeState> for RelativePointerData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _pointer: &ZwpRelativePointerV1,
        request: zwp_relative_pointer_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_relative_pointer_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        pointer: &ZwpRelativePointerV1,
    ) {
        if let Some(client) = &self.client {
            state
                .protocol_globals
                .relative_pointer
                .remove(client, pointer);
        }
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ZwpRelativePointerManagerV1,
    RelativePointerGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ZwpRelativePointerManagerV1,
    RelativePointerManagerData
);
delegate_dispatch!(RuntimeState, ZwpRelativePointerV1, RelativePointerData);

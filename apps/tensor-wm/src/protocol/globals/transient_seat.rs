//! Privileged, creator-scoped `ext_transient_seat_v1` globals.

use std::sync::atomic::{AtomicU64, Ordering};

use wayland_protocols::ext::transient_seat::v1::server::{
    ext_transient_seat_manager_v1::{self, ExtTransientSeatManagerV1},
    ext_transient_seat_v1::{self, ExtTransientSeatV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId},
    protocol::wl_seat::{self, WlSeat},
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::seat::SeatGlobalData,
    state::{RuntimeState, WaylandClientState},
};

const VERSION: u32 = 1;
const MAX_TRANSIENT_SEATS: usize = 16;

struct TransientSeat {
    id: u64,
    global: GlobalId,
    resources: Vec<Weak<WlSeat>>,
    pointer_devices: u16,
    pointer_events: u64,
    keyboard_devices: u16,
    keyboard_events: u64,
}

pub(crate) struct TransientSeatProtocol {
    _global: GlobalId,
    seats: Vec<TransientSeat>,
    next_id: u64,
}

impl TransientSeatProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, ExtTransientSeatManagerV1, _>(
                VERSION,
                TransientSeatGlobalData,
            ),
            seats: Vec::with_capacity(MAX_TRANSIENT_SEATS),
            next_id: 1,
        }
    }

    fn create(&mut self, display: &DisplayHandle, client: &Client) -> Option<(u64, u32)> {
        self.seats.retain(|seat| {
            display
                .backend_handle()
                .global_info(seat.global.clone())
                .is_ok()
        });
        if self.seats.len() == MAX_TRANSIENT_SEATS {
            return None;
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1)?;
        let global = display.create_global::<RuntimeState, WlSeat, _>(
            9,
            SeatGlobalData::transient(id, client.id()),
        );
        let name = client.global_name(display, global.clone())?;
        self.seats.push(TransientSeat {
            id,
            global,
            resources: Vec::new(),
            pointer_devices: 0,
            pointer_events: 0,
            keyboard_devices: 0,
            keyboard_events: 0,
        });
        Some((id, name))
    }

    pub(crate) fn bound(&mut self, id: u64, seat: &WlSeat) {
        let Some(transient) = self.seats.iter_mut().find(|candidate| candidate.id == id) else {
            return;
        };
        seat.capabilities(transient.capabilities());
        transient.resources.push(seat.downgrade());
    }

    pub(crate) fn pointer_created(&mut self, id: u64) {
        let Some(seat) = self.seats.iter_mut().find(|seat| seat.id == id) else {
            return;
        };
        seat.pointer_devices = seat.pointer_devices.saturating_add(1);
        seat.send_capabilities();
    }

    pub(crate) fn pointer_destroyed(&mut self, id: u64) {
        let Some(seat) = self.seats.iter_mut().find(|seat| seat.id == id) else {
            return;
        };
        seat.pointer_devices = seat.pointer_devices.saturating_sub(1);
        seat.send_capabilities();
    }

    pub(crate) fn pointer_event(&mut self, id: u64) {
        if let Some(seat) = self.seats.iter_mut().find(|seat| seat.id == id) {
            seat.pointer_events = seat.pointer_events.saturating_add(1);
        }
    }

    pub(crate) fn keyboard_created(&mut self, id: u64) {
        let Some(seat) = self.seats.iter_mut().find(|seat| seat.id == id) else {
            return;
        };
        seat.keyboard_devices = seat.keyboard_devices.saturating_add(1);
        seat.send_capabilities();
    }

    pub(crate) fn keyboard_destroyed(&mut self, id: u64) {
        let Some(seat) = self.seats.iter_mut().find(|seat| seat.id == id) else {
            return;
        };
        seat.keyboard_devices = seat.keyboard_devices.saturating_sub(1);
        seat.send_capabilities();
    }

    pub(crate) fn keyboard_event(&mut self, id: u64) {
        if let Some(seat) = self.seats.iter_mut().find(|seat| seat.id == id) {
            seat.keyboard_events = seat.keyboard_events.saturating_add(1);
        }
    }

    fn remove(&mut self, display: &DisplayHandle, id: u64) -> bool {
        let Some(index) = self.seats.iter().position(|seat| seat.id == id) else {
            return false;
        };
        let mut seat = self.seats.swap_remove(index);
        seat.pointer_devices = 0;
        seat.keyboard_devices = 0;
        seat.send_capabilities();
        display.disable_global::<RuntimeState>(seat.global.clone());
        display.remove_global::<RuntimeState>(seat.global);
        true
    }

    #[cfg(test)]
    pub(crate) fn live_count(&self) -> usize {
        self.seats.len()
    }

    #[cfg(test)]
    pub(crate) fn pointer_snapshot(&self, id: u64) -> Option<(u16, u64)> {
        self.seats
            .iter()
            .find(|seat| seat.id == id)
            .map(|seat| (seat.pointer_devices, seat.pointer_events))
    }

    #[cfg(test)]
    pub(crate) fn keyboard_snapshot(&self, id: u64) -> Option<(u16, u64)> {
        self.seats
            .iter()
            .find(|seat| seat.id == id)
            .map(|seat| (seat.keyboard_devices, seat.keyboard_events))
    }
}

impl TransientSeat {
    fn capabilities(&self) -> wl_seat::Capability {
        let mut capabilities = wl_seat::Capability::empty();
        if self.pointer_devices > 0 {
            capabilities |= wl_seat::Capability::Pointer;
        }
        if self.keyboard_devices > 0 {
            capabilities |= wl_seat::Capability::Keyboard;
        }
        capabilities
    }

    fn send_capabilities(&mut self) {
        let capabilities = self.capabilities();
        self.resources.retain(|resource| {
            let Ok(resource) = resource.upgrade() else {
                return false;
            };
            resource.capabilities(capabilities);
            true
        });
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct TransientSeatGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct TransientSeatManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct TransientSeatData {
    id: AtomicU64,
}

impl TransientSeatData {
    const DENIED: u64 = 0;

    fn new(id: Option<u64>) -> Self {
        Self {
            id: AtomicU64::new(id.unwrap_or(Self::DENIED)),
        }
    }

    fn take_id(&self) -> Option<u64> {
        let id = self.id.swap(Self::DENIED, Ordering::AcqRel);
        (id != Self::DENIED).then_some(id)
    }
}

impl GlobalDispatchDelegate<ExtTransientSeatManagerV1, RuntimeState> for TransientSeatGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ExtTransientSeatManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, TransientSeatManagerData);
    }
}

impl DispatchDelegate<ExtTransientSeatManagerV1, RuntimeState> for TransientSeatManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _manager: &ExtTransientSeatManagerV1,
        request: ext_transient_seat_manager_v1::Request,
        display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_transient_seat_manager_v1::Request::Create { seat } => {
                let allowed = client
                    .get_data::<WaylandClientState>()
                    .is_none_or(|data| data.security_context.is_none());
                let created = allowed
                    .then(|| {
                        state
                            .protocol_globals
                            .transient_seat
                            .create(display, client)
                    })
                    .flatten();
                let resource = data_init.init(seat, TransientSeatData::new(created.map(|v| v.0)));
                match created {
                    Some((_, global_name)) => resource.ready(global_name),
                    None => resource.denied(),
                }
            }
            ext_transient_seat_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ExtTransientSeatV1, RuntimeState> for TransientSeatData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _resource: &ExtTransientSeatV1,
        request: ext_transient_seat_v1::Request,
        display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_transient_seat_v1::Request::Destroy => {
                if let Some(id) = self.take_id() {
                    state.protocol_globals.transient_seat.remove(display, id);
                }
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        _resource: &ExtTransientSeatV1,
    ) {
        if let Some(id) = self.take_id() {
            let display = state.display_handle.clone();
            state.protocol_globals.transient_seat.remove(&display, id);
        }
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ExtTransientSeatManagerV1,
    TransientSeatGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ExtTransientSeatManagerV1,
    TransientSeatManagerData
);
delegate_dispatch!(RuntimeState, ExtTransientSeatV1, TransientSeatData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_handle_has_no_removable_seat_identity() {
        let denied = TransientSeatData::new(None);
        assert_eq!(denied.take_id(), None);
    }

    #[test]
    fn handle_identity_is_consumed_exactly_once() {
        let ready = TransientSeatData::new(Some(7));
        assert_eq!(ready.take_id(), Some(7));
        assert_eq!(ready.take_id(), None);
    }
}

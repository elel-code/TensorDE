//! `zwp_primary_selection_*` wire handling.

use wayland_protocols::wp::primary_selection::zv1::server::{
    zwp_primary_selection_device_manager_v1::{self, ZwpPrimarySelectionDeviceManagerV1},
    zwp_primary_selection_device_v1::{self, ZwpPrimarySelectionDeviceV1},
    zwp_primary_selection_source_v1::{self, ZwpPrimarySelectionSourceV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource,
    backend::{ClientId, GlobalId},
};

use super::{SetSelectionError, SourceToken};
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(super) fn create_global(display: &DisplayHandle) -> GlobalId {
    display
        .create_global::<RuntimeState, ZwpPrimarySelectionDeviceManagerV1, _>(1, PrimaryGlobalData)
}

pub(in crate::protocol) struct PrimaryGlobalData;
pub(in crate::protocol) struct PrimaryManagerData;

pub(in crate::protocol) struct PrimarySourceData {
    token: SourceToken,
}

pub(in crate::protocol) struct PrimaryDeviceData {
    client: ClientId,
}

impl GlobalDispatchDelegate<ZwpPrimarySelectionDeviceManagerV1, RuntimeState>
    for PrimaryGlobalData
{
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpPrimarySelectionDeviceManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, PrimaryManagerData);
    }
}

impl DispatchDelegate<ZwpPrimarySelectionDeviceManagerV1, RuntimeState> for PrimaryManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _manager: &ZwpPrimarySelectionDeviceManagerV1,
        request: zwp_primary_selection_device_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_primary_selection_device_manager_v1::Request::CreateSource { id } => {
                let token = state.protocol_globals.selection.allocate_source();
                let source = data_init.init(id, PrimarySourceData { token });
                state.protocol_globals.selection.register_primary_source(
                    token,
                    client.id(),
                    &source,
                );
            }
            zwp_primary_selection_device_manager_v1::Request::GetDevice { id, seat } => {
                if !state.seat.owns(&seat) {
                    return;
                }
                let device = data_init.init(
                    id,
                    PrimaryDeviceData {
                        client: client.id(),
                    },
                );
                state
                    .protocol_globals
                    .selection
                    .add_primary_device(client, &device);
            }
            zwp_primary_selection_device_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwpPrimarySelectionSourceV1, RuntimeState> for PrimarySourceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _source: &ZwpPrimarySelectionSourceV1,
        request: zwp_primary_selection_source_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_primary_selection_source_v1::Request::Offer { mime_type } => {
                // The protocol has no late-offer error; keep a frozen snapshot.
                let _ = state
                    .protocol_globals
                    .selection
                    .offer_mime(self.token, mime_type);
            }
            zwp_primary_selection_source_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        _source: &ZwpPrimarySelectionSourceV1,
    ) {
        state.selection_source_destroyed(self.token);
    }
}

impl DispatchDelegate<ZwpPrimarySelectionDeviceV1, RuntimeState> for PrimaryDeviceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _device: &ZwpPrimarySelectionDeviceV1,
        request: zwp_primary_selection_device_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_primary_selection_device_v1::Request::SetSelection { source, .. } => {
                let source = source
                    .as_ref()
                    .and_then(|source| source.data::<PrimarySourceData>())
                    .map(|data| data.token);
                match state
                    .protocol_globals
                    .selection
                    .set_primary_selection(&client.id(), source)
                {
                    Ok(()) | Err(SetSelectionError::NotFocused) => {}
                    Err(
                        SetSelectionError::UnknownSource
                        | SetSelectionError::WrongSource
                        | SetSelectionError::UsedSource
                        | SetSelectionError::DndActions,
                    ) => {}
                }
            }
            zwp_primary_selection_device_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        device: &ZwpPrimarySelectionDeviceV1,
    ) {
        state
            .protocol_globals
            .selection
            .remove_primary_device(&self.client, &device.id());
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ZwpPrimarySelectionDeviceManagerV1,
    PrimaryGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ZwpPrimarySelectionDeviceManagerV1,
    PrimaryManagerData
);
delegate_dispatch!(RuntimeState, ZwpPrimarySelectionSourceV1, PrimarySourceData);
delegate_dispatch!(RuntimeState, ZwpPrimarySelectionDeviceV1, PrimaryDeviceData);

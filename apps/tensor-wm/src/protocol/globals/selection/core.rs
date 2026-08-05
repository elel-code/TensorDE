//! Core `wl_data_device_manager` clipboard wire handling.

use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource,
    backend::{ClientId, GlobalId},
    protocol::{
        wl_data_device::{self, WlDataDevice},
        wl_data_device_manager::{self, WlDataDeviceManager},
        wl_data_source::{self, WlDataSource},
    },
};

use super::{SetActionsError, SetSelectionError, SourceToken};
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(super) fn create_global(display: &DisplayHandle) -> GlobalId {
    display.create_global::<RuntimeState, WlDataDeviceManager, _>(3, CoreGlobalData)
}

pub(in crate::protocol) struct CoreGlobalData;
pub(in crate::protocol) struct CoreManagerData;

pub(in crate::protocol) struct CoreSourceData {
    token: SourceToken,
}

pub(in crate::protocol) struct CoreDeviceData {
    client: ClientId,
}

impl GlobalDispatchDelegate<WlDataDeviceManager, RuntimeState> for CoreGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WlDataDeviceManager>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, CoreManagerData);
    }
}

impl DispatchDelegate<WlDataDeviceManager, RuntimeState> for CoreManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _manager: &WlDataDeviceManager,
        request: wl_data_device_manager::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_data_device_manager::Request::CreateDataSource { id } => {
                let token = state.protocol_globals.selection.allocate_source();
                let source = data_init.init(id, CoreSourceData { token });
                state
                    .protocol_globals
                    .selection
                    .register_core_source(token, client.id(), &source);
            }
            wl_data_device_manager::Request::GetDataDevice { id, seat } => {
                if !state.protocol_globals.seat.owns(&seat) {
                    return;
                }
                let device = data_init.init(
                    id,
                    CoreDeviceData {
                        client: client.id(),
                    },
                );
                state
                    .protocol_globals
                    .selection
                    .add_core_device(client, &device);
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WlDataSource, RuntimeState> for CoreSourceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        source: &WlDataSource,
        request: wl_data_source::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_data_source::Request::Offer { mime_type } => {
                // Core has no late-offer error. Tensor freezes the MIME snapshot
                // at first use so every outstanding offer remains coherent.
                let _ = state
                    .protocol_globals
                    .selection
                    .offer_mime(self.token, mime_type);
            }
            wl_data_source::Request::SetActions { dnd_actions } => {
                match state
                    .protocol_globals
                    .selection
                    .set_core_actions(self.token, dnd_actions)
                {
                    Ok(()) => {}
                    Err(SetActionsError::InvalidMask) => source.post_error(
                        wl_data_source::Error::InvalidActionMask,
                        "drag action mask contains unknown bits",
                    ),
                    Err(SetActionsError::InvalidSource) => source.post_error(
                        wl_data_source::Error::InvalidSource,
                        "drag actions may be set once before source use",
                    ),
                }
            }
            wl_data_source::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, _source: &WlDataSource) {
        state.selection_source_destroyed(self.token);
    }
}

impl DispatchDelegate<WlDataDevice, RuntimeState> for CoreDeviceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        device: &WlDataDevice,
        request: wl_data_device::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_data_device::Request::SetSelection { source, .. } => {
                let source_token = source.as_ref().and_then(source_token);
                if source_token.is_some_and(|token| {
                    state.reject_toplevel_drag_selection_use(token, source.as_ref())
                }) {
                    return;
                }
                match state
                    .protocol_globals
                    .selection
                    .set_core_selection(&client.id(), source_token)
                {
                    Ok(()) | Err(SetSelectionError::NotFocused) => {}
                    Err(SetSelectionError::UsedSource) => device.post_error(
                        wl_data_device::Error::UsedSource,
                        "data source has already been used",
                    ),
                    Err(SetSelectionError::DndActions) => {
                        if let Some(source) = source.as_ref() {
                            source.post_error(
                                wl_data_source::Error::InvalidSource,
                                "a drag source cannot become a clipboard source",
                            );
                        }
                    }
                    Err(SetSelectionError::UnknownSource | SetSelectionError::WrongSource) => {}
                }
            }
            wl_data_device::Request::StartDrag {
                source,
                origin,
                icon,
                serial,
            } => {
                let source = source.as_ref().and_then(source_token);
                state.start_selection_drag(&client.id(), device, source, origin, icon, serial);
            }
            wl_data_device::Request::Release => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, device: &WlDataDevice) {
        state
            .protocol_globals
            .selection
            .remove_core_device(&self.client, &device.id());
    }
}

pub(super) fn source_token(source: &WlDataSource) -> Option<SourceToken> {
    source.data::<CoreSourceData>().map(|data| data.token)
}

delegate_global_dispatch!(RuntimeState, WlDataDeviceManager, CoreGlobalData);
delegate_dispatch!(RuntimeState, WlDataDeviceManager, CoreManagerData);
delegate_dispatch!(RuntimeState, WlDataSource, CoreSourceData);
delegate_dispatch!(RuntimeState, WlDataDevice, CoreDeviceData);

//! Privileged wlr-v2 and ext-v1 data-control wire handling.

use wayland_protocols::ext::data_control::v1::server::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::{self, ExtDataControlManagerV1},
    ext_data_control_source_v1::{self, ExtDataControlSourceV1},
};
use wayland_protocols_wlr::data_control::v1::server::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::{self, ZwlrDataControlManagerV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource,
    backend::{ClientId, GlobalId},
};

use super::{OfferMimeError, SelectionTarget, SetSelectionError, SourceToken};
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::{RuntimeState, WaylandClientState},
};

pub(super) fn create_wlr_global(display: &DisplayHandle) -> GlobalId {
    display.create_global::<RuntimeState, ZwlrDataControlManagerV1, _>(2, WlrGlobalData)
}

pub(super) fn create_ext_global(display: &DisplayHandle) -> GlobalId {
    display.create_global::<RuntimeState, ExtDataControlManagerV1, _>(1, ExtGlobalData)
}

pub(in crate::protocol) struct WlrGlobalData;
pub(in crate::protocol) struct WlrManagerData;
pub(in crate::protocol) struct ExtGlobalData;
pub(in crate::protocol) struct ExtManagerData;

pub(in crate::protocol) struct WlrSourceData {
    token: SourceToken,
}

pub(in crate::protocol) struct ExtSourceData {
    token: SourceToken,
}

pub(in crate::protocol) struct WlrDeviceData {
    client: ClientId,
}

pub(in crate::protocol) struct ExtDeviceData {
    client: ClientId,
}

fn unrestricted(client: &Client) -> bool {
    client
        .get_data::<WaylandClientState>()
        .is_none_or(|data| data.security_context.is_none())
}

impl GlobalDispatchDelegate<ZwlrDataControlManagerV1, RuntimeState> for WlrGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrDataControlManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, WlrManagerData);
    }

    fn can_view(&self, client: &Client) -> bool {
        unrestricted(client)
    }
}

impl GlobalDispatchDelegate<ExtDataControlManagerV1, RuntimeState> for ExtGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ExtDataControlManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, ExtManagerData);
    }

    fn can_view(&self, client: &Client) -> bool {
        unrestricted(client)
    }
}

impl DispatchDelegate<ZwlrDataControlManagerV1, RuntimeState> for WlrManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _manager: &ZwlrDataControlManagerV1,
        request: zwlr_data_control_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwlr_data_control_manager_v1::Request::CreateDataSource { id } => {
                let token = state.protocol_globals.selection.allocate_source();
                let source = data_init.init(id, WlrSourceData { token });
                state
                    .protocol_globals
                    .selection
                    .register_wlr_source(token, client.id(), &source);
            }
            zwlr_data_control_manager_v1::Request::GetDataDevice { id, seat } => {
                if !state.protocol_globals.seat.owns(&seat) {
                    return;
                }
                let device = data_init.init(
                    id,
                    WlrDeviceData {
                        client: client.id(),
                    },
                );
                state
                    .protocol_globals
                    .selection
                    .add_wlr_data_control_device(client, &device);
            }
            zwlr_data_control_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ExtDataControlManagerV1, RuntimeState> for ExtManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _manager: &ExtDataControlManagerV1,
        request: ext_data_control_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_data_control_manager_v1::Request::CreateDataSource { id } => {
                let token = state.protocol_globals.selection.allocate_source();
                let source = data_init.init(id, ExtSourceData { token });
                state
                    .protocol_globals
                    .selection
                    .register_ext_source(token, client.id(), &source);
            }
            ext_data_control_manager_v1::Request::GetDataDevice { id, seat } => {
                if !state.protocol_globals.seat.owns(&seat) {
                    return;
                }
                let device = data_init.init(
                    id,
                    ExtDeviceData {
                        client: client.id(),
                    },
                );
                state
                    .protocol_globals
                    .selection
                    .add_ext_data_control_device(client, &device);
            }
            ext_data_control_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwlrDataControlSourceV1, RuntimeState> for WlrSourceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        source: &ZwlrDataControlSourceV1,
        request: zwlr_data_control_source_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwlr_data_control_source_v1::Request::Offer { mime_type } => {
                if matches!(
                    state
                        .protocol_globals
                        .selection
                        .offer_mime(self.token, mime_type),
                    Err(OfferMimeError::Frozen)
                ) {
                    source.post_error(
                        zwlr_data_control_source_v1::Error::InvalidOffer,
                        "MIME types are immutable after first source use",
                    );
                }
            }
            zwlr_data_control_source_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        _source: &ZwlrDataControlSourceV1,
    ) {
        state.selection_source_destroyed(self.token);
    }
}

impl DispatchDelegate<ExtDataControlSourceV1, RuntimeState> for ExtSourceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        source: &ExtDataControlSourceV1,
        request: ext_data_control_source_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_data_control_source_v1::Request::Offer { mime_type } => {
                if matches!(
                    state
                        .protocol_globals
                        .selection
                        .offer_mime(self.token, mime_type),
                    Err(OfferMimeError::Frozen)
                ) {
                    source.post_error(
                        ext_data_control_source_v1::Error::InvalidOffer,
                        "MIME types are immutable after first source use",
                    );
                }
            }
            ext_data_control_source_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        _source: &ExtDataControlSourceV1,
    ) {
        state.selection_source_destroyed(self.token);
    }
}

impl DispatchDelegate<ZwlrDataControlDeviceV1, RuntimeState> for WlrDeviceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        device: &ZwlrDataControlDeviceV1,
        request: zwlr_data_control_device_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwlr_data_control_device_v1::Request::SetSelection { source } => {
                let source = wlr_source_token(source.as_ref());
                post_wlr_result(
                    device,
                    state.protocol_globals.selection.set_wlr_selection(
                        &client.id(),
                        source,
                        SelectionTarget::Clipboard,
                    ),
                );
            }
            zwlr_data_control_device_v1::Request::SetPrimarySelection { source } => {
                if device.version() < 2 {
                    return;
                }
                let source = wlr_source_token(source.as_ref());
                post_wlr_result(
                    device,
                    state.protocol_globals.selection.set_wlr_selection(
                        &client.id(),
                        source,
                        SelectionTarget::Primary,
                    ),
                );
            }
            zwlr_data_control_device_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        device: &ZwlrDataControlDeviceV1,
    ) {
        let _ = &self.client;
        state
            .protocol_globals
            .selection
            .remove_wlr_data_control_device(&device.id());
    }
}

impl DispatchDelegate<ExtDataControlDeviceV1, RuntimeState> for ExtDeviceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        device: &ExtDataControlDeviceV1,
        request: ext_data_control_device_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_data_control_device_v1::Request::SetSelection { source } => {
                let source = ext_source_token(source.as_ref());
                post_ext_result(
                    device,
                    state.protocol_globals.selection.set_ext_selection(
                        &client.id(),
                        source,
                        SelectionTarget::Clipboard,
                    ),
                );
            }
            ext_data_control_device_v1::Request::SetPrimarySelection { source } => {
                let source = ext_source_token(source.as_ref());
                post_ext_result(
                    device,
                    state.protocol_globals.selection.set_ext_selection(
                        &client.id(),
                        source,
                        SelectionTarget::Primary,
                    ),
                );
            }
            ext_data_control_device_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        device: &ExtDataControlDeviceV1,
    ) {
        let _ = &self.client;
        state
            .protocol_globals
            .selection
            .remove_ext_data_control_device(&device.id());
    }
}

fn wlr_source_token(source: Option<&ZwlrDataControlSourceV1>) -> Option<SourceToken> {
    source
        .and_then(|source| source.data::<WlrSourceData>())
        .map(|data| data.token)
}

fn ext_source_token(source: Option<&ExtDataControlSourceV1>) -> Option<SourceToken> {
    source
        .and_then(|source| source.data::<ExtSourceData>())
        .map(|data| data.token)
}

fn post_wlr_result(device: &ZwlrDataControlDeviceV1, result: Result<(), SetSelectionError>) {
    if matches!(result, Err(SetSelectionError::UsedSource)) {
        device.post_error(
            zwlr_data_control_device_v1::Error::UsedSource,
            "data-control source can be used only once",
        );
    }
}

fn post_ext_result(device: &ExtDataControlDeviceV1, result: Result<(), SetSelectionError>) {
    if matches!(result, Err(SetSelectionError::UsedSource)) {
        device.post_error(
            ext_data_control_device_v1::Error::UsedSource,
            "data-control source can be used only once",
        );
    }
}

delegate_global_dispatch!(RuntimeState, ZwlrDataControlManagerV1, WlrGlobalData);
delegate_dispatch!(RuntimeState, ZwlrDataControlManagerV1, WlrManagerData);
delegate_dispatch!(RuntimeState, ZwlrDataControlSourceV1, WlrSourceData);
delegate_dispatch!(RuntimeState, ZwlrDataControlDeviceV1, WlrDeviceData);
delegate_global_dispatch!(RuntimeState, ExtDataControlManagerV1, ExtGlobalData);
delegate_dispatch!(RuntimeState, ExtDataControlManagerV1, ExtManagerData);
delegate_dispatch!(RuntimeState, ExtDataControlSourceV1, ExtSourceData);
delegate_dispatch!(RuntimeState, ExtDataControlDeviceV1, ExtDeviceData);

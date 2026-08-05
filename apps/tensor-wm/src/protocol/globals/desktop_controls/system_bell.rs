use tracing::info;
use wayland_protocols::xdg::system_bell::v1::server::xdg_system_bell_v1::{self, XdgSystemBellV1};
use wayland_server::{Client, DataInit, DisplayHandle, New, Resource};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

#[derive(Debug)]
pub(super) struct SystemBellGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct SystemBellData;

impl GlobalDispatchDelegate<XdgSystemBellV1, RuntimeState> for SystemBellGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<XdgSystemBellV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, SystemBellData);
    }
}

impl DispatchDelegate<XdgSystemBellV1, RuntimeState> for SystemBellData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &XdgSystemBellV1,
        request: xdg_system_bell_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_system_bell_v1::Request::Ring { surface } => info!(
                surface = surface.as_ref().map(|surface| surface.id().protocol_id()),
                "xdg-system-bell ring"
            ),
            xdg_system_bell_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

delegate_global_dispatch!(RuntimeState, XdgSystemBellV1, SystemBellGlobalData);
delegate_dispatch!(RuntimeState, XdgSystemBellV1, SystemBellData);

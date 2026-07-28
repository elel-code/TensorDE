use wayland_protocols::xdg::toplevel_tag::v1::server::xdg_toplevel_tag_manager_v1::{
    self, XdgToplevelTagManagerV1,
};
use wayland_server::{Client, DataInit, DisplayHandle, New};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

#[derive(Debug)]
pub(super) struct ToplevelTagGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct ToplevelTagData;

impl GlobalDispatchDelegate<XdgToplevelTagManagerV1, RuntimeState> for ToplevelTagGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<XdgToplevelTagManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, ToplevelTagData);
    }
}

impl DispatchDelegate<XdgToplevelTagManagerV1, RuntimeState> for ToplevelTagData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _resource: &XdgToplevelTagManagerV1,
        request: xdg_toplevel_tag_manager_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            xdg_toplevel_tag_manager_v1::Request::SetToplevelTag { toplevel, tag } => {
                let Some(surface) = super::toplevel_surface(state, &toplevel) else {
                    return;
                };
                state
                    .protocol_globals
                    .desktop_controls
                    .set_tag(&surface, tag);
            }
            xdg_toplevel_tag_manager_v1::Request::SetToplevelDescription {
                toplevel,
                description,
            } => {
                let Some(surface) = super::toplevel_surface(state, &toplevel) else {
                    return;
                };
                state
                    .protocol_globals
                    .desktop_controls
                    .set_description(&surface, description);
            }
            xdg_toplevel_tag_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

delegate_global_dispatch!(RuntimeState, XdgToplevelTagManagerV1, ToplevelTagGlobalData);
delegate_dispatch!(RuntimeState, XdgToplevelTagManagerV1, ToplevelTagData);

//! Tensor-owned xdg-decoration wire state enforcing client-side decorations.

use std::collections::HashMap;

use wayland_protocols::xdg::{
    decoration::zv1::server::{
        zxdg_decoration_manager_v1::{self, ZxdgDecorationManagerV1},
        zxdg_toplevel_decoration_v1::{self, Mode, ZxdgToplevelDecorationV1},
    },
    shell::server::xdg_toplevel::XdgToplevel,
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, WEnum, Weak,
    backend::{ClientId, GlobalId, ObjectId},
};

use super::xdg_shell::Toplevel;
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct XdgDecorationProtocol {
    _global: GlobalId,
    decorations: HashMap<ObjectId, Weak<ZxdgToplevelDecorationV1>>,
}

impl XdgDecorationProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, ZxdgDecorationManagerV1, _>(
                1,
                XdgDecorationGlobalData,
            ),
            decorations: HashMap::new(),
        }
    }

    fn contains(&mut self, toplevel: &XdgToplevel) -> bool {
        let key = toplevel.id();
        let live = self
            .decorations
            .get(&key)
            .is_some_and(|decoration| decoration.upgrade().is_ok());
        if !live {
            self.decorations.remove(&key);
        }
        live
    }

    fn insert(&mut self, toplevel: &XdgToplevel, decoration: &ZxdgToplevelDecorationV1) {
        self.decorations
            .insert(toplevel.id(), decoration.downgrade());
    }

    fn remove(&mut self, toplevel: &XdgToplevel, decoration: &ZxdgToplevelDecorationV1) {
        let key = toplevel.id();
        if self
            .decorations
            .get(&key)
            .and_then(|resource| resource.upgrade().ok())
            .as_ref()
            == Some(decoration)
        {
            self.decorations.remove(&key);
        }
    }

    pub(super) fn toplevel_destroyed(&mut self, toplevel: &XdgToplevel) {
        if let Some(decoration) = self
            .decorations
            .remove(&toplevel.id())
            .and_then(|decoration| decoration.upgrade().ok())
        {
            decoration.post_error(
                zxdg_toplevel_decoration_v1::Error::Orphaned,
                "xdg_toplevel was destroyed before its decoration object",
            );
        }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct XdgDecorationGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct XdgDecorationManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct XdgDecorationData {
    toplevel: XdgToplevel,
}

impl GlobalDispatchDelegate<ZxdgDecorationManagerV1, RuntimeState> for XdgDecorationGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZxdgDecorationManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, XdgDecorationManagerData);
    }
}

impl DispatchDelegate<ZxdgDecorationManagerV1, RuntimeState> for XdgDecorationManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &ZxdgDecorationManagerV1,
        request: zxdg_decoration_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zxdg_decoration_manager_v1::Request::GetToplevelDecoration { id, toplevel } => {
                let Some(shell_toplevel) = state.protocol_globals.xdg_shell.toplevel(&toplevel)
                else {
                    manager.post_error(
                        zxdg_toplevel_decoration_v1::Error::Orphaned,
                        "cannot decorate a destroyed xdg_toplevel",
                    );
                    return;
                };
                if state.protocol_globals.xdg_decoration.contains(&toplevel) {
                    manager.post_error(
                        zxdg_toplevel_decoration_v1::Error::AlreadyConstructed,
                        "xdg_toplevel already has a decoration object",
                    );
                    return;
                }
                let decoration = data_init.init(
                    id,
                    XdgDecorationData {
                        toplevel: toplevel.clone(),
                    },
                );
                state
                    .protocol_globals
                    .xdg_decoration
                    .insert(&toplevel, &decoration);
                configure_client_side(&shell_toplevel, &decoration);
            }
            zxdg_decoration_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZxdgToplevelDecorationV1, RuntimeState> for XdgDecorationData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        decoration: &ZxdgToplevelDecorationV1,
        request: zxdg_toplevel_decoration_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zxdg_toplevel_decoration_v1::Request::SetMode { mode } => {
                if let WEnum::Unknown(mode) = mode {
                    decoration.post_error(
                        zxdg_toplevel_decoration_v1::Error::InvalidMode,
                        format!("unknown decoration mode {mode}"),
                    );
                    return;
                }
                configure_existing(state, &self.toplevel, decoration);
            }
            zxdg_toplevel_decoration_v1::Request::UnsetMode => {
                configure_existing(state, &self.toplevel, decoration);
            }
            zxdg_toplevel_decoration_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        decoration: &ZxdgToplevelDecorationV1,
    ) {
        state
            .protocol_globals
            .xdg_decoration
            .remove(&self.toplevel, decoration);
    }
}

fn configure_existing(
    state: &mut RuntimeState,
    toplevel: &XdgToplevel,
    decoration: &ZxdgToplevelDecorationV1,
) {
    let Some(shell_toplevel) = state.protocol_globals.xdg_shell.toplevel(toplevel) else {
        decoration.post_error(
            zxdg_toplevel_decoration_v1::Error::Orphaned,
            "xdg_toplevel no longer exists",
        );
        return;
    };
    configure_client_side(&shell_toplevel, decoration);
}

fn configure_client_side(toplevel: &Toplevel, decoration: &ZxdgToplevelDecorationV1) {
    decoration.configure(Mode::ClientSide);
    if toplevel.initial_configure_sent() {
        toplevel.send_configure();
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ZxdgDecorationManagerV1,
    XdgDecorationGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ZxdgDecorationManagerV1,
    XdgDecorationManagerData
);
delegate_dispatch!(RuntimeState, ZxdgToplevelDecorationV1, XdgDecorationData);

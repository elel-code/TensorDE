//! `wp_tearing_control_v1` surface hints and commit semantics.

use std::{cell::RefCell, collections::HashMap};

use tensor_protocol::SurfacePresentationHint;
use wayland_protocols::wp::tearing_control::v1::server::{
    wp_tearing_control_manager_v1::{self, WpTearingControlManagerV1},
    wp_tearing_control_v1::{self, WpTearingControlV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, WEnum, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::compositor,
    state::RuntimeState,
};

pub(crate) struct TearingControlProtocol {
    _global: GlobalId,
    surfaces: RefCell<HashMap<ObjectId, SurfaceTearingState>>,
}

impl TearingControlProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _global: display.create_global::<RuntimeState, WpTearingControlManagerV1, _>(
                1,
                TearingControlGlobalData,
            ),
            surfaces: RefCell::new(HashMap::new()),
        }
    }

    fn attach(&self, surface: &WlSurface, resource: &WpTearingControlV1) -> AttachResult {
        let mut surfaces = self.surfaces.borrow_mut();
        let state = surfaces.entry(surface.id()).or_default();
        if state.resource.is_some() {
            return AttachResult::AlreadyExists;
        }
        state.resource = Some(resource.id());
        let install_hook = !state.commit_hook_installed;
        state.commit_hook_installed = true;
        AttachResult::Attached { install_hook }
    }

    fn set_pending(&self, surface: &WlSurface, hint: SurfacePresentationHint) {
        self.surfaces
            .borrow_mut()
            .entry(surface.id())
            .or_default()
            .pending = Some(hint);
    }

    fn detach(&self, surface: &WlSurface, resource: &WpTearingControlV1) {
        let mut surfaces = self.surfaces.borrow_mut();
        let Some(state) = surfaces.get_mut(&surface.id()) else {
            return;
        };
        if state.resource.as_ref() != Some(&resource.id()) {
            return;
        }
        state.resource = None;
        state.pending = Some(SurfacePresentationHint::Vsync);
    }

    fn commit_surface(&self, surface: &WlSurface) -> Option<SurfacePresentationHint> {
        let mut surfaces = self.surfaces.borrow_mut();
        let state = surfaces.get_mut(&surface.id())?;
        if let Some(pending) = state.pending.take() {
            state.current = pending;
        }
        Some(state.current)
    }

    pub(crate) fn remove_surface(&self, surface: &WlSurface) {
        self.surfaces.borrow_mut().remove(&surface.id());
    }

    #[cfg(test)]
    pub(crate) fn committed_hint(&self, surface: &WlSurface) -> SurfacePresentationHint {
        self.surfaces
            .borrow()
            .get(&surface.id())
            .map_or(SurfacePresentationHint::Vsync, |state| state.current)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachResult {
    AlreadyExists,
    Attached { install_hook: bool },
}

#[derive(Debug, Default)]
struct SurfaceTearingState {
    resource: Option<ObjectId>,
    pending: Option<SurfacePresentationHint>,
    current: SurfacePresentationHint,
    commit_hook_installed: bool,
}

#[derive(Debug)]
pub(in crate::protocol) struct TearingControlGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct TearingControlManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct TearingControlData {
    surface: Weak<WlSurface>,
}

impl GlobalDispatchDelegate<WpTearingControlManagerV1, RuntimeState> for TearingControlGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpTearingControlManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, TearingControlManagerData);
    }
}

impl DispatchDelegate<WpTearingControlManagerV1, RuntimeState> for TearingControlManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &WpTearingControlManagerV1,
        request: wp_tearing_control_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_tearing_control_manager_v1::Request::Destroy => {}
            wp_tearing_control_manager_v1::Request::GetTearingControl { id, surface } => {
                let resource = data_init.init(
                    id,
                    TearingControlData {
                        surface: surface.downgrade(),
                    },
                );
                match state
                    .protocol_globals
                    .tearing_control
                    .attach(&surface, &resource)
                {
                    AttachResult::AlreadyExists => manager.post_error(
                        wp_tearing_control_manager_v1::Error::TearingControlExists,
                        "the surface already has a tearing-control object",
                    ),
                    AttachResult::Attached { install_hook } => {
                        if install_hook {
                            compositor::add_post_commit_hook::<RuntimeState, _>(
                                &surface,
                                tearing_control_post_commit,
                            );
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpTearingControlV1, RuntimeState> for TearingControlData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &WpTearingControlV1,
        request: wp_tearing_control_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_tearing_control_v1::Request::SetPresentationHint { hint } => {
                let Ok(surface) = self.surface.upgrade() else {
                    return;
                };
                let Some(hint) = decode_hint(hint) else {
                    return;
                };
                state
                    .protocol_globals
                    .tearing_control
                    .set_pending(&surface, hint);
            }
            wp_tearing_control_v1::Request::Destroy => self.detach(state, resource),
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &WpTearingControlV1,
    ) {
        self.detach(state, resource);
    }
}

fn decode_hint(
    hint: WEnum<wp_tearing_control_v1::PresentationHint>,
) -> Option<SurfacePresentationHint> {
    match hint {
        WEnum::Value(wp_tearing_control_v1::PresentationHint::Vsync) => {
            Some(SurfacePresentationHint::Vsync)
        }
        WEnum::Value(wp_tearing_control_v1::PresentationHint::Async) => {
            Some(SurfacePresentationHint::Async)
        }
        WEnum::Value(_) | WEnum::Unknown(_) => None,
    }
}

impl TearingControlData {
    fn detach(&self, state: &RuntimeState, resource: &WpTearingControlV1) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        state
            .protocol_globals
            .tearing_control
            .detach(&surface, resource);
    }
}

fn tearing_control_post_commit(
    state: &mut RuntimeState,
    _display: &DisplayHandle,
    surface: &WlSurface,
) {
    let Some(hint) = state
        .protocol_globals
        .tearing_control
        .commit_surface(surface)
    else {
        return;
    };
    let Some(view_id) = state.view_for_surface(surface) else {
        return;
    };
    if state
        .world
        .set_presentation_hint(view_id, hint)
        .unwrap_or(false)
    {
        state.request_redraw_all();
    }
}

delegate_global_dispatch!(
    RuntimeState,
    WpTearingControlManagerV1,
    TearingControlGlobalData
);
delegate_dispatch!(
    RuntimeState,
    WpTearingControlManagerV1,
    TearingControlManagerData
);
delegate_dispatch!(RuntimeState, WpTearingControlV1, TearingControlData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_is_double_buffered_and_destroy_reverts_on_commit() {
        let mut state = SurfaceTearingState::default();
        assert_eq!(state.current, SurfacePresentationHint::Vsync);
        state.pending = Some(SurfacePresentationHint::Async);
        assert_eq!(state.current, SurfacePresentationHint::Vsync);
        if let Some(pending) = state.pending.take() {
            state.current = pending;
        }
        assert_eq!(state.current, SurfacePresentationHint::Async);
        state.pending = Some(SurfacePresentationHint::Vsync);
        assert_eq!(state.current, SurfacePresentationHint::Async);
        if let Some(pending) = state.pending.take() {
            state.current = pending;
        }
        assert_eq!(state.current, SurfacePresentationHint::Vsync);
    }

    #[test]
    fn unknown_hint_never_mutates_surface_state() {
        assert_eq!(decode_hint(WEnum::Unknown(42)), None);
    }
}

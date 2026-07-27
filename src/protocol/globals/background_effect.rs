//! Tensor-owned `ext-background-effect-v1` wire and double-buffered state.

use std::{cell::RefCell, collections::HashMap};

use smithay::wayland::compositor::{self, RectangleKind, get_region_attributes};
use wayland_protocols::ext::background_effect::v1::server::{
    ext_background_effect_manager_v1::{self, ExtBackgroundEffectManagerV1},
    ext_background_effect_surface_v1::{self, ExtBackgroundEffectSurfaceV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::{wl_region::WlRegion, wl_surface::WlSurface},
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct BackgroundEffectProtocol {
    _global: GlobalId,
    surfaces: RefCell<HashMap<ObjectId, BackgroundSurfaceState>>,
}

impl BackgroundEffectProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        let global = display.create_global::<RuntimeState, ExtBackgroundEffectManagerV1, _>(
            1,
            BackgroundEffectGlobalData,
        );
        Self {
            _global: global,
            surfaces: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn commit_surface(&self, surface: &WlSurface) {
        if let Some(state) = self.surfaces.borrow_mut().get_mut(&surface.id()) {
            state.commit();
        }
    }

    pub(crate) fn remove_surface(&self, surface: &WlSurface) {
        self.surfaces.borrow_mut().remove(&surface.id());
    }

    pub(crate) fn committed_has_area(&self, surface: &WlSurface) -> bool {
        self.surfaces
            .borrow()
            .get(&surface.id())
            .is_some_and(|state| state.current_has_area)
    }

    fn has_resource(&self, surface: &WlSurface) -> bool {
        self.surfaces
            .borrow()
            .get(&surface.id())
            .is_some_and(BackgroundSurfaceState::has_resource)
    }

    fn attach(&self, surface: &WlSurface, resource: &ExtBackgroundEffectSurfaceV1) -> bool {
        let mut surfaces = self.surfaces.borrow_mut();
        let state = surfaces.entry(surface.id()).or_default();
        let install_hook = !state.commit_hook_installed;
        state.commit_hook_installed = true;
        state.resource = Some(EffectResource {
            id: resource.id(),
            weak: resource.downgrade(),
        });
        install_hook
    }

    fn set_pending(&self, surface: &WlSurface, has_area: bool) {
        self.surfaces
            .borrow_mut()
            .entry(surface.id())
            .or_default()
            .pending_has_area = Some(has_area);
    }

    fn detach(&self, surface: &WlSurface, resource: &ExtBackgroundEffectSurfaceV1) {
        let mut surfaces = self.surfaces.borrow_mut();
        let Some(state) = surfaces.get_mut(&surface.id()) else {
            return;
        };
        if state
            .resource
            .as_ref()
            .is_some_and(|attached| attached.id == resource.id())
        {
            state.resource = None;
            state.pending_has_area = Some(false);
        }
    }
}

#[derive(Debug, Default)]
struct BackgroundSurfaceState {
    resource: Option<EffectResource>,
    pending_has_area: Option<bool>,
    current_has_area: bool,
    commit_hook_installed: bool,
}

impl BackgroundSurfaceState {
    fn has_resource(&self) -> bool {
        self.resource
            .as_ref()
            .is_some_and(|resource| resource.weak.upgrade().is_ok())
    }

    fn commit(&mut self) {
        if let Some(has_area) = self.pending_has_area.take() {
            self.current_has_area = has_area;
        }
    }
}

#[derive(Debug)]
struct EffectResource {
    id: ObjectId,
    weak: Weak<ExtBackgroundEffectSurfaceV1>,
}

#[derive(Debug)]
pub(in crate::protocol) struct BackgroundEffectGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct BackgroundEffectManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct BackgroundEffectSurfaceData {
    surface: Weak<WlSurface>,
}

impl GlobalDispatchDelegate<ExtBackgroundEffectManagerV1, RuntimeState>
    for BackgroundEffectGlobalData
{
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ExtBackgroundEffectManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let manager = data_init.init(resource, BackgroundEffectManagerData);
        manager.capabilities(ext_background_effect_manager_v1::Capability::Blur);
    }
}

impl DispatchDelegate<ExtBackgroundEffectManagerV1, RuntimeState> for BackgroundEffectManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &ExtBackgroundEffectManagerV1,
        request: ext_background_effect_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_background_effect_manager_v1::Request::Destroy => {}
            ext_background_effect_manager_v1::Request::GetBackgroundEffect { id, surface } => {
                if state
                    .protocol_globals
                    .background_effect
                    .has_resource(&surface)
                {
                    manager.post_error(
                        ext_background_effect_manager_v1::Error::BackgroundEffectExists,
                        "the surface already has a background-effect object",
                    );
                    return;
                }
                let resource = data_init.init(
                    id,
                    BackgroundEffectSurfaceData {
                        surface: surface.downgrade(),
                    },
                );
                let install_hook = state
                    .protocol_globals
                    .background_effect
                    .attach(&surface, &resource);
                if install_hook {
                    compositor::add_post_commit_hook::<RuntimeState, _>(
                        &surface,
                        background_effect_post_commit,
                    );
                }
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ExtBackgroundEffectSurfaceV1, RuntimeState> for BackgroundEffectSurfaceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &ExtBackgroundEffectSurfaceV1,
        request: ext_background_effect_surface_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_background_effect_surface_v1::Request::SetBlurRegion { region } => {
                let Some(surface) = self.surface(resource) else {
                    return;
                };
                let has_area = region.as_ref().is_some_and(region_has_area);
                state
                    .protocol_globals
                    .background_effect
                    .set_pending(&surface, has_area);
            }
            ext_background_effect_surface_v1::Request::Destroy => self.detach(state, resource),
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &ExtBackgroundEffectSurfaceV1,
    ) {
        self.detach(state, resource);
    }
}

impl BackgroundEffectSurfaceData {
    fn surface(&self, resource: &ExtBackgroundEffectSurfaceV1) -> Option<WlSurface> {
        match self.surface.upgrade() {
            Ok(surface) => Some(surface),
            Err(_) => {
                resource.post_error(
                    ext_background_effect_surface_v1::Error::SurfaceDestroyed,
                    "the associated wl_surface was destroyed",
                );
                None
            }
        }
    }

    fn detach(&self, state: &RuntimeState, resource: &ExtBackgroundEffectSurfaceV1) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        state
            .protocol_globals
            .background_effect
            .detach(&surface, resource);
    }
}

fn region_has_area(region: &WlRegion) -> bool {
    let attributes = get_region_attributes(region);
    attributes.rects.iter().any(|(kind, rect)| {
        matches!(kind, RectangleKind::Add) && rect.size.w > 0 && rect.size.h > 0
    })
}

fn background_effect_post_commit(
    state: &mut RuntimeState,
    _display: &DisplayHandle,
    surface: &WlSurface,
) {
    state
        .protocol_globals
        .background_effect
        .commit_surface(surface);
}

delegate_global_dispatch!(
    RuntimeState,
    ExtBackgroundEffectManagerV1,
    BackgroundEffectGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ExtBackgroundEffectManagerV1,
    BackgroundEffectManagerData
);
delegate_dispatch!(
    RuntimeState,
    ExtBackgroundEffectSurfaceV1,
    BackgroundEffectSurfaceData
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_effect_is_invisible_until_commit_and_clears_once() {
        let mut state = BackgroundSurfaceState {
            pending_has_area: Some(true),
            ..BackgroundSurfaceState::default()
        };
        assert!(!state.current_has_area);
        state.commit();
        assert!(state.current_has_area);
        state.pending_has_area = Some(false);
        assert!(state.current_has_area);
        state.commit();
        assert!(!state.current_has_area);
        assert_eq!(state.pending_has_area, None);
    }
}

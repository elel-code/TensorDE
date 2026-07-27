//! Tensor-owned `ext-background-effect-v1` wire and double-buffered state.

use smithay::wayland::compositor::{RectangleKind, get_region_attributes};
use wayland_protocols::ext::background_effect::v1::server::{
    ext_background_effect_manager_v1::{self, ExtBackgroundEffectManagerV1},
    ext_background_effect_surface_v1::{self, ExtBackgroundEffectSurfaceV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId},
    protocol::{wl_region::WlRegion, wl_surface::WlSurface},
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

use super::surface_metadata::{AttachResult, install_metadata_hook};

pub(crate) struct BackgroundEffectProtocol {
    _global: GlobalId,
}

impl BackgroundEffectProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        let global = display.create_global::<RuntimeState, ExtBackgroundEffectManagerV1, _>(
            1,
            BackgroundEffectGlobalData,
        );
        Self { _global: global }
    }
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
                    .surface_metadata
                    .has_background(&surface)
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
                match state
                    .protocol_globals
                    .surface_metadata
                    .attach_background(&surface, &resource)
                {
                    AttachResult::AlreadyExists => manager.post_error(
                        ext_background_effect_manager_v1::Error::BackgroundEffectExists,
                        "the surface already has a background-effect object",
                    ),
                    AttachResult::Attached { install_hook } => {
                        install_metadata_hook(install_hook, &surface);
                    }
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
                    .surface_metadata
                    .set_pending_background(&surface, has_area);
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
            .surface_metadata
            .detach_background(&surface, resource);
    }
}

fn region_has_area(region: &WlRegion) -> bool {
    let attributes = get_region_attributes(region);
    attributes.rects.iter().any(|(kind, rect)| {
        matches!(kind, RectangleKind::Add) && rect.size.w > 0 && rect.size.h > 0
    })
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

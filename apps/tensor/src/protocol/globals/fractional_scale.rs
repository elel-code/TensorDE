//! Tensor-owned `wp_fractional_scale_v1` wire state.

use std::{cell::RefCell, collections::HashMap};

use tensor_util::OutputScale;
use wayland_protocols::wp::fractional_scale::v1::server::{
    wp_fractional_scale_manager_v1::{self, WpFractionalScaleManagerV1},
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct FractionalScaleProtocol {
    _global: GlobalId,
    surfaces: RefCell<HashMap<ObjectId, SurfaceScaleState>>,
}

impl FractionalScaleProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        let global = display.create_global::<RuntimeState, WpFractionalScaleManagerV1, _>(
            1,
            FractionalScaleGlobalData,
        );
        Self {
            _global: global,
            surfaces: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn set_preferred_scale(&self, surface: &WlSurface, scale: OutputScale) {
        let mut surfaces = self.surfaces.borrow_mut();
        let state = surfaces.entry(surface.id()).or_default();
        state.set_preferred_scale(scale.units());
    }

    pub(crate) fn remove_surface(&self, surface: &WlSurface) {
        self.surfaces.borrow_mut().remove(&surface.id());
    }

    fn has_resource(&self, surface: &WlSurface) -> bool {
        self.surfaces
            .borrow()
            .get(&surface.id())
            .is_some_and(SurfaceScaleState::has_resource)
    }

    fn attach(&self, surface: &WlSurface, resource: &WpFractionalScaleV1) {
        let mut surfaces = self.surfaces.borrow_mut();
        let state = surfaces.entry(surface.id()).or_default();
        state.resource = Some(ScaleResource {
            id: resource.id(),
            weak: resource.downgrade(),
        });
        state.publish_preferred_scale();
    }

    fn detach(&self, surface: &WlSurface, resource: &WpFractionalScaleV1) {
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
        }
    }
}

#[derive(Debug, Default)]
struct SurfaceScaleState {
    resource: Option<ScaleResource>,
    preferred_units: Option<u32>,
}

impl SurfaceScaleState {
    fn has_resource(&self) -> bool {
        self.resource
            .as_ref()
            .is_some_and(|resource| resource.weak.upgrade().is_ok())
    }

    fn set_preferred_scale(&mut self, units: u32) {
        if self.preferred_units == Some(units) {
            return;
        }
        self.preferred_units = Some(units);
        self.publish_preferred_scale();
    }

    fn publish_preferred_scale(&self) {
        let Some(units) = self.preferred_units else {
            return;
        };
        let Some(resource) = self
            .resource
            .as_ref()
            .and_then(|resource| resource.weak.upgrade().ok())
        else {
            return;
        };
        resource.preferred_scale(units);
    }
}

#[derive(Debug)]
struct ScaleResource {
    id: ObjectId,
    weak: Weak<WpFractionalScaleV1>,
}

#[derive(Debug)]
pub(in crate::protocol) struct FractionalScaleGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct FractionalScaleManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct FractionalScaleData {
    surface: Weak<WlSurface>,
}

impl GlobalDispatchDelegate<WpFractionalScaleManagerV1, RuntimeState>
    for FractionalScaleGlobalData
{
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpFractionalScaleManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, FractionalScaleManagerData);
    }
}

impl DispatchDelegate<WpFractionalScaleManagerV1, RuntimeState> for FractionalScaleManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &WpFractionalScaleManagerV1,
        request: wp_fractional_scale_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_fractional_scale_manager_v1::Request::Destroy => {}
            wp_fractional_scale_manager_v1::Request::GetFractionalScale { id, surface } => {
                if state
                    .protocol_globals
                    .fractional_scale
                    .has_resource(&surface)
                {
                    manager.post_error(
                        wp_fractional_scale_manager_v1::Error::FractionalScaleExists,
                        "the surface already has a fractional-scale object",
                    );
                    return;
                }
                let resource = data_init.init(
                    id,
                    FractionalScaleData {
                        surface: surface.downgrade(),
                    },
                );
                state
                    .protocol_globals
                    .fractional_scale
                    .attach(&surface, &resource);
                state.update_surface_scale(&surface);
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpFractionalScaleV1, RuntimeState> for FractionalScaleData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &WpFractionalScaleV1,
        request: wp_fractional_scale_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_fractional_scale_v1::Request::Destroy => self.detach(state, resource),
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &WpFractionalScaleV1,
    ) {
        self.detach(state, resource);
    }
}

impl FractionalScaleData {
    fn detach(&self, state: &RuntimeState, resource: &WpFractionalScaleV1) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        state
            .protocol_globals
            .fractional_scale
            .detach(&surface, resource);
    }
}

delegate_global_dispatch!(
    RuntimeState,
    WpFractionalScaleManagerV1,
    FractionalScaleGlobalData
);
delegate_dispatch!(
    RuntimeState,
    WpFractionalScaleManagerV1,
    FractionalScaleManagerData
);
delegate_dispatch!(RuntimeState, WpFractionalScaleV1, FractionalScaleData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_scale_uses_tensor_fixed_units_without_float_round_trip() {
        let mut state = SurfaceScaleState::default();
        let scale = OutputScale::from_units(157).unwrap();
        state.set_preferred_scale(scale.units());
        assert_eq!(state.preferred_units, Some(157));
    }
}

use smithay::utils::{Logical, Point};
use wayland_protocols::wp::pointer_warp::v1::server::wp_pointer_warp_v1::{self, WpPointerWarpV1};
use wayland_server::{Client, DataInit, DisplayHandle, New};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::{RuntimeState, surface_contains_point},
};

#[derive(Debug)]
pub(super) struct PointerWarpGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct PointerWarpData;

impl GlobalDispatchDelegate<WpPointerWarpV1, RuntimeState> for PointerWarpGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpPointerWarpV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, PointerWarpData);
    }
}

impl DispatchDelegate<WpPointerWarpV1, RuntimeState> for PointerWarpData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _resource: &WpPointerWarpV1,
        request: wp_pointer_warp_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_pointer_warp_v1::Request::WarpPointer {
                surface,
                pointer: _,
                x,
                y,
                serial,
            } => state.handle_pointer_warp(client, surface, x, y, serial),
            wp_pointer_warp_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl RuntimeState {
    fn handle_pointer_warp(
        &mut self,
        client: &Client,
        surface: wayland_server::protocol::wl_surface::WlSurface,
        client_x: f64,
        client_y: f64,
        serial: u32,
    ) {
        let Some(local) = logical_position(client_x, client_y, self.client_scale(client)) else {
            return;
        };
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        if self.protocol_globals.seat.pointer_enter_serial() != Some(serial.into())
            || pointer
                .current_focus()
                .as_ref()
                .is_none_or(|focused| focused.surface() != &surface)
            || !surface_contains_point(&surface, (local.x, local.y))
        {
            return;
        }

        let origin = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(&surface))
            .and_then(|window| self.space.element_geometry(window))
            .map(|geometry| geometry.loc.to_f64());
        #[cfg(feature = "tty")]
        let origin = origin.or_else(|| {
            self.layer_surface_origin(&surface)
                .map(|point| point.to_f64())
        });
        let Some(origin) = origin else {
            return;
        };
        let target = origin + local;
        #[cfg(feature = "tty")]
        if let Some(bounds) = self.pointer_coordinate_space() {
            pointer.set_location(crate::protocol::input::constrain_pointer_location(
                target, bounds,
            ));
        } else {
            pointer.set_location(target);
        }
        #[cfg(not(feature = "tty"))]
        pointer.set_location(target);
        #[cfg(feature = "tty")]
        self.request_redraw_at(pointer.current_location());
    }
}

fn logical_position(x: f64, y: f64, client_scale: f64) -> Option<Point<f64, Logical>> {
    if !x.is_finite() || !y.is_finite() || !client_scale.is_finite() || client_scale <= 0.0 {
        return None;
    }
    Some(Point::from((x / client_scale, y / client_scale)))
}

delegate_global_dispatch!(RuntimeState, WpPointerWarpV1, PointerWarpGlobalData);
delegate_dispatch!(RuntimeState, WpPointerWarpV1, PointerWarpData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_coordinates_are_scaled_once_and_reject_non_finite_values() {
        assert_eq!(
            logical_position(30.0, 45.0, 1.5),
            Some(Point::from((20.0, 30.0)))
        );
        assert!(logical_position(f64::NAN, 0.0, 1.0).is_none());
        assert!(logical_position(0.0, 0.0, 0.0).is_none());
    }
}

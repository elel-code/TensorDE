use tensor_event::{AxisDirection, AxisSource, PointerAxisEvent};
use wayland_server::{
    Resource,
    backend::ClientId,
    protocol::{
        wl_pointer::{self, WlPointer},
        wl_surface::WlSurface,
    },
};

use super::SeatProtocol;
use crate::protocol::serial::Serial;

#[derive(Debug)]
pub(super) struct PointerResource {
    resource: WlPointer,
    v120: [i32; 2],
}

impl PointerResource {
    pub(super) fn reset_v120(&mut self) {
        self.v120 = [0; 2];
    }
}

impl SeatProtocol {
    pub(crate) fn pointer_enter(
        &mut self,
        surface: &WlSurface,
        serial: Serial,
        location: (f64, f64),
        client_scale: f64,
    ) {
        let Some(client) = surface.client() else {
            return;
        };
        let client_id = client.id();
        self.pointer_focus = Some(client_id.clone());
        self.pointer_focus_surface = Some(surface.id());
        self.pointer_enter_serial = Some(serial);
        let location = client_point(location, client_scale);
        if let Some(pointers) = self.pointers.get(&client_id) {
            for pointer in pointers {
                pointer
                    .resource
                    .enter(serial.into(), surface, location.0, location.1);
            }
        }
    }

    pub(crate) fn pointer_leave(&mut self, surface: &WlSurface, serial: Serial) {
        let focus = self.pointer_focus.take();
        self.pointer_focus_surface = None;
        self.pointer_enter_serial = None;
        let Some(client) = focus else {
            return;
        };
        if !surface.is_alive() {
            return;
        }
        if let Some(pointers) = self.pointers.get_mut(&client) {
            for pointer in pointers {
                pointer.resource.leave(serial.into(), surface);
                if pointer.resource.version() >= 5 {
                    pointer.resource.frame();
                }
                pointer.reset_v120();
            }
        }
    }

    pub(crate) fn pointer_motion(&self, time: u32, location: (f64, f64), client_scale: f64) {
        let Some(client) = self.pointer_focus.as_ref() else {
            return;
        };
        let location = client_point(location, client_scale);
        if let Some(pointers) = self.pointers.get(client) {
            for pointer in pointers {
                pointer.resource.motion(time, location.0, location.1);
            }
        }
    }

    pub(crate) fn pointer_button(&self, serial: Serial, time: u32, button: u32, pressed: bool) {
        let Some(client) = self.pointer_focus.as_ref() else {
            return;
        };
        let state = if pressed {
            wl_pointer::ButtonState::Pressed
        } else {
            wl_pointer::ButtonState::Released
        };
        if let Some(pointers) = self.pointers.get(client) {
            for pointer in pointers {
                pointer.resource.button(serial.into(), time, button, state);
            }
        }
    }

    pub(crate) fn pointer_axis(&mut self, event: PointerAxisEvent, client_scale: f64) {
        let Some(client) = self.pointer_focus.as_ref() else {
            return;
        };
        let Some(pointers) = self.pointers.get_mut(client) else {
            return;
        };
        for pointer in pointers {
            send_axis(pointer, event, client_scale);
        }
    }

    pub(crate) fn pointer_frame(&self) {
        let Some(client) = self.pointer_focus.as_ref() else {
            return;
        };
        if let Some(pointers) = self.pointers.get(client) {
            for pointer in pointers {
                if pointer.resource.version() >= 5 {
                    pointer.resource.frame();
                }
            }
        }
    }

    pub(super) fn insert_pointer(&mut self, client: ClientId, pointer: WlPointer) {
        self.pointers
            .entry(client)
            .or_default()
            .push(PointerResource {
                resource: pointer,
                v120: [0; 2],
            });
    }

    pub(super) fn remove_pointer(&mut self, client: &ClientId, pointer: &WlPointer) {
        let mut remove_client = false;
        if let Some(pointers) = self.pointers.get_mut(client) {
            if let Some(index) = pointers
                .iter()
                .position(|candidate| candidate.resource.id() == pointer.id())
            {
                pointers.swap_remove(index);
            }
            remove_client = pointers.is_empty();
        }
        if remove_client {
            self.pointers.remove(client);
        }
    }
}

fn client_point(point: (f64, f64), scale: f64) -> (f64, f64) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    (point.0 * scale, point.1 * scale)
}

fn send_axis(pointer: &mut PointerResource, event: PointerAxisEvent, client_scale: f64) {
    let resource = &pointer.resource;
    if resource.version() >= 5 {
        let source = match event.source {
            AxisSource::Wheel => wl_pointer::AxisSource::Wheel,
            AxisSource::Finger => wl_pointer::AxisSource::Finger,
            AxisSource::Continuous | AxisSource::Unknown => wl_pointer::AxisSource::Continuous,
            AxisSource::WheelTilt if resource.version() >= 6 => wl_pointer::AxisSource::WheelTilt,
            AxisSource::WheelTilt => wl_pointer::AxisSource::Wheel,
        };
        resource.axis_source(source);
        if let Some(v120) = event.horizontal_v120() {
            send_v120(
                resource,
                wl_pointer::Axis::HorizontalScroll,
                v120,
                &mut pointer.v120[0],
            );
        }
        if let Some(v120) = event.vertical_v120() {
            send_v120(
                resource,
                wl_pointer::Axis::VerticalScroll,
                v120,
                &mut pointer.v120[1],
            );
        }
        if event.horizontal_stopped() {
            resource.axis_stop(event.time_msec(), wl_pointer::Axis::HorizontalScroll);
            pointer.v120[0] = 0;
        }
        if event.vertical_stopped() {
            resource.axis_stop(event.time_msec(), wl_pointer::Axis::VerticalScroll);
            pointer.v120[1] = 0;
        }
    }
    let scale = if client_scale.is_finite() && client_scale > 0.0 {
        client_scale
    } else {
        1.0
    };
    for (axis, value, direction) in [
        (
            wl_pointer::Axis::HorizontalScroll,
            event.horizontal().unwrap_or_default(),
            event.horizontal_direction,
        ),
        (
            wl_pointer::Axis::VerticalScroll,
            event.vertical().unwrap_or_default(),
            event.vertical_direction,
        ),
    ] {
        if value == 0.0 {
            continue;
        }
        if resource.version() >= 9 {
            resource.axis_relative_direction(
                axis,
                match direction {
                    AxisDirection::Identical => wl_pointer::AxisRelativeDirection::Identical,
                    AxisDirection::Inverted => wl_pointer::AxisRelativeDirection::Inverted,
                },
            );
        }
        resource.axis(event.time_msec(), axis, value * scale);
    }
}

fn send_v120(resource: &WlPointer, axis: wl_pointer::Axis, value: i32, accumulated: &mut i32) {
    if resource.version() >= 8 {
        if value != 0 {
            resource.axis_value120(axis, value);
        }
        return;
    }
    *accumulated = accumulated.saturating_add(value);
    if accumulated.abs() >= 120 {
        resource.axis_discrete(axis, *accumulated / 120);
        *accumulated %= 120;
    }
}

pub(super) fn logical_hotspot(value: i32, scale: f64) -> i32 {
    if !scale.is_finite() || scale <= 0.0 {
        return value;
    }
    let value = f64::from(value) / scale;
    if !value.is_finite() {
        return 0;
    }
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

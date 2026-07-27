use smithay::{
    backend::input::{AxisRelativeDirection, AxisSource, ButtonState},
    input::pointer::{AxisFrame, ButtonEvent, MotionEvent},
    utils::Serial,
};
use wayland_server::{
    Resource,
    backend::ClientId,
    protocol::{
        wl_pointer::{self, WlPointer},
        wl_surface::WlSurface,
    },
};

use super::SeatProtocol;

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
        event: &MotionEvent,
        client_scale: f64,
    ) {
        let Some(client) = surface.client() else {
            return;
        };
        let client_id = client.id();
        self.pointer_focus = Some(client_id.clone());
        self.pointer_focus_surface = Some(surface.id());
        self.pointer_enter_serial = Some(event.serial);
        let location = client_point(event.location, client_scale);
        if let Some(pointers) = self.pointers.get(&client_id) {
            for pointer in pointers {
                pointer
                    .resource
                    .enter(event.serial.into(), surface, location.0, location.1);
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

    pub(crate) fn pointer_motion(&self, event: &MotionEvent, client_scale: f64) {
        let Some(client) = self.pointer_focus.as_ref() else {
            return;
        };
        let location = client_point(event.location, client_scale);
        if let Some(pointers) = self.pointers.get(client) {
            for pointer in pointers {
                pointer.resource.motion(event.time, location.0, location.1);
            }
        }
    }

    pub(crate) fn pointer_button(&self, event: &ButtonEvent) {
        let Some(client) = self.pointer_focus.as_ref() else {
            return;
        };
        let state = match event.state {
            ButtonState::Pressed => wl_pointer::ButtonState::Pressed,
            ButtonState::Released => wl_pointer::ButtonState::Released,
        };
        if let Some(pointers) = self.pointers.get(client) {
            for pointer in pointers {
                pointer
                    .resource
                    .button(event.serial.into(), event.time, event.button, state);
            }
        }
    }

    pub(crate) fn pointer_axis(&mut self, frame: AxisFrame, client_scale: f64) {
        let Some(client) = self.pointer_focus.as_ref() else {
            return;
        };
        let Some(pointers) = self.pointers.get_mut(client) else {
            return;
        };
        for pointer in pointers {
            send_axis(pointer, frame, client_scale);
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

fn client_point(
    point: smithay::utils::Point<f64, smithay::utils::Logical>,
    scale: f64,
) -> (f64, f64) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    (point.x * scale, point.y * scale)
}

fn send_axis(pointer: &mut PointerResource, frame: AxisFrame, client_scale: f64) {
    let resource = &pointer.resource;
    if resource.version() >= 5 {
        if let Some(source) = frame.source {
            let source = match source {
                AxisSource::Wheel => wl_pointer::AxisSource::Wheel,
                AxisSource::Finger => wl_pointer::AxisSource::Finger,
                AxisSource::Continuous => wl_pointer::AxisSource::Continuous,
                AxisSource::WheelTilt if resource.version() >= 6 => {
                    wl_pointer::AxisSource::WheelTilt
                }
                AxisSource::WheelTilt => wl_pointer::AxisSource::Wheel,
            };
            resource.axis_source(source);
        }
        if let Some(v120) = frame.v120 {
            send_v120(
                resource,
                wl_pointer::Axis::HorizontalScroll,
                v120.0,
                &mut pointer.v120[0],
            );
            send_v120(
                resource,
                wl_pointer::Axis::VerticalScroll,
                v120.1,
                &mut pointer.v120[1],
            );
        }
        if frame.stop.0 {
            resource.axis_stop(frame.time, wl_pointer::Axis::HorizontalScroll);
            pointer.v120[0] = 0;
        }
        if frame.stop.1 {
            resource.axis_stop(frame.time, wl_pointer::Axis::VerticalScroll);
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
            frame.axis.0,
            frame.relative_direction.0,
        ),
        (
            wl_pointer::Axis::VerticalScroll,
            frame.axis.1,
            frame.relative_direction.1,
        ),
    ] {
        if value == 0.0 {
            continue;
        }
        if resource.version() >= 9 {
            resource.axis_relative_direction(
                axis,
                match direction {
                    AxisRelativeDirection::Identical => {
                        wl_pointer::AxisRelativeDirection::Identical
                    }
                    AxisRelativeDirection::Inverted => wl_pointer::AxisRelativeDirection::Inverted,
                },
            );
        }
        resource.axis(frame.time, axis, value * scale);
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

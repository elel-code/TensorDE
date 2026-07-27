//! Libinput adapter for the Smithay `zwp_tablet_manager_v2` protocol state.

use input::{
    Device,
    event::{
        pointer::ButtonState as LibinputButtonState,
        tablet_tool::{
            ProximityState as LibinputProximityState,
            TabletToolAxisEvent as LibinputTabletAxisEvent,
            TabletToolButtonEvent as LibinputTabletButtonEvent,
            TabletToolEvent as LibinputTabletEvent, TabletToolEventTrait,
            TabletToolProximityEvent as LibinputTabletProximityEvent,
            TabletToolTipEvent as LibinputTabletTipEvent, TabletToolType as LibinputTabletToolType,
            TipState as LibinputTipState,
        },
    },
};
use smithay::{
    backend::input::{ButtonState, TabletToolCapabilities, TabletToolDescriptor, TabletToolType},
    input::tablet::{
        TabletDescriptor, TabletSeatTrait,
        tool::{
            AxisFrame, ButtonEvent, DownEvent, MotionEvent, ProximityInEvent, ProximityOutEvent,
            UpEvent,
        },
    },
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
};
use tensor_input::{DeviceChange, DeviceId};
use tracing::debug;

use super::RuntimeState;

impl RuntimeState {
    pub(super) fn process_tablet_device(
        &mut self,
        id: DeviceId,
        device: Device,
        change: DeviceChange,
    ) {
        match change {
            DeviceChange::Added => {
                let descriptor = tablet_descriptor(&device);
                if let Some(previous) = self.tablet_devices.remove(&id) {
                    self.seat.tablet_seat().remove_tablet(&previous);
                }
                self.seat
                    .tablet_seat()
                    .add_wp_tablet(&self.display_handle, &descriptor);
                self.tablet_devices.insert(id, descriptor);
                debug!(name = %device.name(), "tablet tool device added");
            }
            DeviceChange::Removed => {
                if let Some(descriptor) = self.tablet_devices.remove(&id) {
                    self.seat.tablet_seat().remove_tablet(&descriptor);
                } else {
                    let descriptor = tablet_descriptor(&device);
                    self.seat.tablet_seat().remove_tablet(&descriptor);
                }
                debug!(name = %device.name(), "tablet tool device removed");
            }
        }
    }

    pub(super) fn process_tablet_event(&mut self, device: DeviceId, event: LibinputTabletEvent) {
        match event {
            LibinputTabletEvent::Axis(event) => self.on_tablet_axis(device, event),
            LibinputTabletEvent::Proximity(event) => self.on_tablet_proximity(device, event),
            LibinputTabletEvent::Tip(event) => self.on_tablet_tip(event),
            LibinputTabletEvent::Button(event) => self.on_tablet_button(event),
            _ => {}
        }
    }

    fn on_tablet_axis(&mut self, device: DeviceId, event: LibinputTabletAxisEvent) {
        let Some(bounds) = self.pointer_coordinate_space() else {
            return;
        };
        let location = tablet_location(&event, bounds);
        let focus = self.pointer_focus_under(location);
        let tool = {
            let descriptor = tablet_tool_descriptor(&event);
            let tablet_seat = self.seat.tablet_seat();
            tablet_seat.get_tool(&descriptor).unwrap_or_else(|| {
                tablet_seat.add_wp_tool(self, &self.display_handle.clone(), &descriptor)
            })
        };
        let tablet = self
            .tablet_devices
            .get(&device)
            .and_then(|descriptor| self.seat.tablet_seat().get_tablet(descriptor));
        if tablet.is_some() {
            let time = tablet_time_msec(&event);
            let serial = SERIAL_COUNTER.next_serial();
            tool.axis(self, tablet_axis_frame(&event));
            tool.motion(
                self,
                focus,
                &MotionEvent {
                    location,
                    serial,
                    time,
                },
            );
            tool.frame(self, time);
        }
        self.request_redraw_at(location);
    }

    fn on_tablet_proximity(&mut self, device: DeviceId, event: LibinputTabletProximityEvent) {
        let tool = {
            let descriptor = tablet_tool_descriptor(&event);
            let tablet_seat = self.seat.tablet_seat();
            tablet_seat.get_tool(&descriptor).unwrap_or_else(|| {
                tablet_seat.add_wp_tool(self, &self.display_handle.clone(), &descriptor)
            })
        };
        let tablet = self
            .tablet_devices
            .get(&device)
            .and_then(|descriptor| self.seat.tablet_seat().get_tablet(descriptor));
        let Some(tablet) = tablet else {
            return;
        };
        match event.proximity_state() {
            LibinputProximityState::In => {
                let location = self
                    .pointer_coordinate_space()
                    .map(|bounds| tablet_location(&event, bounds))
                    .unwrap_or_default();
                let time = tablet_time_msec(&event);
                tool.proximity_in(
                    self,
                    self.pointer_focus_under(location),
                    tablet,
                    &ProximityInEvent {
                        location,
                        axis: Some(tablet_axis_frame(&event)),
                        serial: SERIAL_COUNTER.next_serial(),
                        time,
                    },
                );
                tool.frame(self, time);
                self.request_redraw_at(location);
            }
            LibinputProximityState::Out => {
                let time = tablet_time_msec(&event);
                tool.proximity_out(
                    self,
                    &ProximityOutEvent {
                        serial: SERIAL_COUNTER.next_serial(),
                        time,
                    },
                );
                tool.frame(self, time);
            }
        }
    }

    fn on_tablet_tip(&mut self, event: LibinputTabletTipEvent) {
        let descriptor = tablet_tool_descriptor(&event);
        let Some(tool) = self.seat.tablet_seat().get_tool(&descriptor) else {
            return;
        };
        match event.tip_state() {
            LibinputTipState::Down => {
                tool.down(
                    self,
                    &DownEvent {
                        serial: SERIAL_COUNTER.next_serial(),
                        time: tablet_time_msec(&event),
                    },
                );
            }
            LibinputTipState::Up => {
                tool.up(
                    self,
                    &UpEvent {
                        serial: SERIAL_COUNTER.next_serial(),
                        time: tablet_time_msec(&event),
                    },
                );
            }
        }
        tool.frame(self, tablet_time_msec(&event));
    }

    fn on_tablet_button(&mut self, event: LibinputTabletButtonEvent) {
        let descriptor = tablet_tool_descriptor(&event);
        let Some(tool) = self.seat.tablet_seat().get_tool(&descriptor) else {
            return;
        };
        let state = match event.button_state() {
            LibinputButtonState::Pressed => ButtonState::Pressed,
            LibinputButtonState::Released => ButtonState::Released,
        };
        let time = tablet_time_msec(&event);
        tool.button(
            self,
            &ButtonEvent {
                serial: SERIAL_COUNTER.next_serial(),
                button: event.button(),
                state,
                time,
            },
        );
        tool.frame(self, time);
    }
}

#[allow(unsafe_code)]
fn tablet_descriptor(device: &Device) -> TabletDescriptor {
    // The device comes from the same udev-backed libinput context, satisfying
    // input.rs's context requirement for this borrowed udev conversion.
    let syspath = unsafe { device.udev_device() }.map(|device| device.syspath().to_owned());
    TabletDescriptor {
        name: device.name().into_owned(),
        usb_id: Some((device.id_product(), device.id_vendor())),
        syspath,
    }
}

fn tablet_tool_descriptor(event: &impl TabletToolEventTrait) -> TabletToolDescriptor {
    let tool = event.tool();
    let tool_type = match tool.tool_type() {
        Some(LibinputTabletToolType::Pen) => TabletToolType::Pen,
        Some(LibinputTabletToolType::Eraser) => TabletToolType::Eraser,
        Some(LibinputTabletToolType::Brush) => TabletToolType::Brush,
        Some(LibinputTabletToolType::Pencil) => TabletToolType::Pencil,
        Some(LibinputTabletToolType::Airbrush) => TabletToolType::Airbrush,
        Some(LibinputTabletToolType::Mouse) => TabletToolType::Mouse,
        Some(LibinputTabletToolType::Lens) => TabletToolType::Lens,
        Some(LibinputTabletToolType::Totem) => TabletToolType::Totem,
        _ => TabletToolType::Unknown,
    };
    let mut capabilities = TabletToolCapabilities::empty();
    capabilities.set(TabletToolCapabilities::TILT, tool.has_tilt());
    capabilities.set(TabletToolCapabilities::PRESSURE, tool.has_pressure());
    capabilities.set(TabletToolCapabilities::DISTANCE, tool.has_distance());
    capabilities.set(TabletToolCapabilities::ROTATION, tool.has_rotation());
    capabilities.set(TabletToolCapabilities::SLIDER, tool.has_slider());
    capabilities.set(TabletToolCapabilities::WHEEL, tool.has_wheel());
    TabletToolDescriptor {
        tool_type,
        hardware_serial: tool.serial(),
        hardware_id_wacom: tool.tool_id(),
        capabilities,
    }
}

fn tablet_location(
    event: &impl TabletToolEventTrait,
    bounds: Rectangle<i32, Logical>,
) -> Point<f64, Logical> {
    let location = Point::from((
        event.x_transformed(bounds.size.w as u32),
        event.y_transformed(bounds.size.h as u32),
    )) + bounds.loc.to_f64();
    super::constrain_pointer_location(location, bounds)
}

fn tablet_axis_frame(event: &impl TabletToolEventTrait) -> AxisFrame {
    AxisFrame {
        pressure: event.pressure_has_changed().then(|| event.pressure()),
        distance: event.distance_has_changed().then(|| event.distance()),
        tilt: (event.tilt_x_has_changed() || event.tilt_y_has_changed())
            .then(|| (event.tilt_x(), event.tilt_y())),
        rotation: event.rotation_has_changed().then(|| event.rotation()),
        slider: event.slider_has_changed().then(|| event.slider_position()),
        wheel: event.wheel_has_changed().then(|| {
            (
                event.wheel_delta(),
                event.wheel_delta_discrete().round() as i32,
            )
        }),
    }
}

#[inline]
fn tablet_time_msec(event: &impl TabletToolEventTrait) -> u32 {
    (event.time_usec() / 1_000) as u32
}

use smithay::{
    backend::{
        input::{
            Device, DeviceCapability, Event as InputEventTrait, InputEvent, KeyboardKeyEvent,
            PointerButtonEvent, PointerMotionEvent,
        },
        libinput::LibinputInputBackend,
    },
    input::{
        keyboard::FilterResult,
        pointer::{ButtonEvent, MotionEvent},
    },
    utils::SERIAL_COUNTER,
};
use tracing::{debug, warn};

use super::state::{InputDeviceCapabilities, RuntimeState};

impl RuntimeState {
    pub(crate) fn process_input_event(&mut self, event: InputEvent<LibinputInputBackend>) {
        match event {
            InputEvent::DeviceAdded { device } => {
                let capabilities = InputDeviceCapabilities {
                    keyboard: Device::has_capability(&device, DeviceCapability::Keyboard),
                    pointer: Device::has_capability(&device, DeviceCapability::Pointer),
                    touch: Device::has_capability(&device, DeviceCapability::Touch),
                };
                self.input_devices.insert(device.id(), capabilities);
                self.reconcile_seat_capabilities();
            }
            InputEvent::DeviceRemoved { device } => {
                self.input_devices.remove(&device.id());
                self.reconcile_seat_capabilities();
            }
            InputEvent::Keyboard { event } => self.forward_keyboard(event),
            InputEvent::PointerMotion { event } => self.forward_pointer_motion(event),
            InputEvent::PointerButton { event } => self.forward_pointer_button(event),
            InputEvent::PointerAxis { event } => self.forward_pointer_axis(event),
            _ => {}
        }
    }

    fn reconcile_seat_capabilities(&mut self) {
        let keyboard_count = self
            .input_devices
            .values()
            .filter(|capabilities| capabilities.keyboard)
            .count();
        let pointer_count = self
            .input_devices
            .values()
            .filter(|capabilities| capabilities.pointer)
            .count();
        let touch_count = self
            .input_devices
            .values()
            .filter(|capabilities| capabilities.touch)
            .count();

        if keyboard_count > 0 && self.seat.get_keyboard().is_none() {
            if let Err(error) = self.seat.add_keyboard(Default::default(), 200, 25) {
                warn!(%error, "failed to publish keyboard capability");
            }
        } else if keyboard_count == 0 && self.seat.get_keyboard().is_some() {
            self.seat.remove_keyboard();
        }

        if pointer_count > 0 && self.seat.get_pointer().is_none() {
            self.seat.add_pointer();
        } else if pointer_count == 0 && self.seat.get_pointer().is_some() {
            self.seat.remove_pointer();
        }

        debug!(
            keyboard_count,
            pointer_count, touch_count, "libinput seat capabilities reconciled"
        );
    }

    fn forward_keyboard(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::KeyboardKeyEvent,
    ) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        keyboard.input::<(), _>(
            self,
            event.key_code(),
            event.state(),
            SERIAL_COUNTER.next_serial(),
            event.time_msec(),
            |_, _, _| FilterResult::Forward,
        );
    }

    fn forward_pointer_motion(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::PointerMotionEvent,
    ) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        pointer.motion(
            self,
            None,
            &MotionEvent {
                location: pointer.current_location() + event.delta(),
                serial: SERIAL_COUNTER.next_serial(),
                time: event.time_msec(),
            },
        );
        pointer.frame(self);
    }

    fn forward_pointer_button(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::PointerButtonEvent,
    ) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        pointer.button(
            self,
            &ButtonEvent {
                serial: SERIAL_COUNTER.next_serial(),
                time: event.time_msec(),
                button: event.button_code(),
                state: event.state(),
            },
        );
        pointer.frame(self);
    }

    fn forward_pointer_axis(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::PointerAxisEvent,
    ) {
        use smithay::backend::input::{Axis, PointerAxisEvent};
        use smithay::input::pointer::AxisFrame;

        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let mut frame = AxisFrame::new(event.time_msec())
            .source(event.source())
            .relative_direction(Axis::Horizontal, event.relative_direction(Axis::Horizontal))
            .relative_direction(Axis::Vertical, event.relative_direction(Axis::Vertical));
        for axis in [Axis::Horizontal, Axis::Vertical] {
            if let Some(amount) = event.amount(axis) {
                frame = frame.value(axis, amount);
            }
            if let Some(steps) = event.amount_v120(axis) {
                frame = frame.v120(axis, steps.round() as i32);
            }
        }
        pointer.axis(self, frame);
    }
}

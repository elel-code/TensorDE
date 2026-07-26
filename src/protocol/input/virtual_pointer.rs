//! Apply `zwlr_virtual_pointer_v1` events through the ordinary seat path.

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, ButtonState, Event as InputEventTrait, PointerAxisEvent,
        PointerButtonEvent, PointerMotionEvent,
    },
    input::pointer::{AxisFrame, ButtonEvent},
    utils::SERIAL_COUNTER,
};

use crate::protocol::extensions::virtual_pointer::{
    VirtualPointerAxisEvent, VirtualPointerButtonEvent, VirtualPointerInputBackend,
    VirtualPointerMotionAbsoluteEvent, VirtualPointerMotionEvent,
};

use super::RuntimeState;

impl RuntimeState {
    pub(crate) fn forward_virtual_pointer_motion(&mut self, event: VirtualPointerMotionEvent) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let current = pointer.current_location();
        let delta = (
            PointerMotionEvent::<VirtualPointerInputBackend>::delta_x(&event),
            PointerMotionEvent::<VirtualPointerInputBackend>::delta_y(&event),
        )
            .into();
        let Some(location) = self.relative_pointer_location(current, delta) else {
            return;
        };
        self.forward_pointer_location(
            location,
            InputEventTrait::<VirtualPointerInputBackend>::time(&event).saturating_mul(1_000),
        );
    }

    pub(crate) fn forward_virtual_pointer_motion_absolute(
        &mut self,
        event: VirtualPointerMotionAbsoluteEvent,
    ) {
        let Some(bounds) = self.pointer_coordinate_space() else {
            return;
        };
        let location = AbsolutePositionEvent::<VirtualPointerInputBackend>::position_transformed(
            &event,
            bounds.size,
        ) + bounds.loc.to_f64();
        let location = super::constrain_pointer_location(location, bounds);
        self.forward_pointer_location(
            location,
            InputEventTrait::<VirtualPointerInputBackend>::time(&event).saturating_mul(1_000),
        );
    }

    pub(crate) fn forward_virtual_pointer_button(&mut self, event: VirtualPointerButtonEvent) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        let state = PointerButtonEvent::<VirtualPointerInputBackend>::state(&event);
        if state == ButtonState::Pressed && !pointer.is_grabbed() {
            self.focus_window_at(pointer.current_location(), serial);
        }
        pointer.button(
            self,
            &ButtonEvent {
                serial,
                time: (InputEventTrait::<VirtualPointerInputBackend>::time(&event) / 1000) as u32,
                button: PointerButtonEvent::<VirtualPointerInputBackend>::button_code(&event),
                state,
            },
        );
        pointer.frame(self);
    }

    pub(crate) fn forward_virtual_pointer_axis(&mut self, event: VirtualPointerAxisEvent) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let time = (InputEventTrait::<VirtualPointerInputBackend>::time(&event) / 1000) as u32;
        let mut frame = AxisFrame::new(time).source(
            PointerAxisEvent::<VirtualPointerInputBackend>::source(&event),
        );
        for axis in [Axis::Horizontal, Axis::Vertical] {
            if let Some(amount) =
                PointerAxisEvent::<VirtualPointerInputBackend>::amount(&event, axis)
            {
                frame = frame.value(axis, amount);
            }
            if let Some(steps) =
                PointerAxisEvent::<VirtualPointerInputBackend>::amount_v120(&event, axis)
            {
                frame = frame.v120(axis, steps.round() as i32);
            }
        }
        pointer.axis(self, frame);
        pointer.frame(self);
    }
}

//! Tablet tool event path for `zwp_tablet_manager_v2`.

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Device, DeviceCapability, Event as InputEventTrait, InputEvent,
        ProximityState, TabletToolEvent, TabletToolTipState,
    },
    backend::libinput::LibinputInputBackend,
    utils::SERIAL_COUNTER,
    wayland::tablet_manager::{TabletDescriptor, TabletSeatTrait},
};
use tracing::debug;

use super::RuntimeState;

impl RuntimeState {
    pub(super) fn process_tablet_event(&mut self, event: InputEvent<LibinputInputBackend>) {
        match event {
            InputEvent::DeviceAdded { device } => {
                if Device::has_capability(&device, DeviceCapability::TabletTool) {
                    let desc = TabletDescriptor::from(&device);
                    self.seat
                        .tablet_seat()
                        .add_tablet::<Self>(&self.display_handle, &desc);
                    debug!(name = %device.name(), "tablet tool device added");
                }
            }
            InputEvent::DeviceRemoved { device } => {
                if Device::has_capability(&device, DeviceCapability::TabletTool) {
                    let desc = TabletDescriptor::from(&device);
                    self.seat.tablet_seat().remove_tablet(&desc);
                    debug!(name = %device.name(), "tablet tool device removed");
                }
            }
            InputEvent::TabletToolAxis { event } => self.on_tablet_axis(event),
            InputEvent::TabletToolProximity { event } => self.on_tablet_proximity(event),
            InputEvent::TabletToolTip { event } => self.on_tablet_tip(event),
            InputEvent::TabletToolButton { event } => self.on_tablet_button(event),
            _ => {}
        }
    }

    fn on_tablet_axis(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::TabletToolAxisEvent,
    ) {
        let Some(bounds) = self.pointer_coordinate_space() else {
            return;
        };
        let location = super::constrain_pointer_location(
            event.position_transformed(bounds.size) + bounds.loc.to_f64(),
            bounds,
        );
        let focus = self.pointer_focus_under(location);
        let tool = {
            let tablet_seat = self.seat.tablet_seat();
            tablet_seat.add_tool::<Self>(self, &self.display_handle.clone(), &event.tool())
        };
        let tablet = self
            .seat
            .tablet_seat()
            .get_tablet(&TabletDescriptor::from(&event.device()));
        if let Some(tablet) = tablet {
            tool.motion(
                location,
                focus,
                &tablet,
                SERIAL_COUNTER.next_serial(),
                event.time_msec(),
            );
        }
        self.request_redraw_at(location);
    }

    fn on_tablet_proximity(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::TabletToolProximityEvent,
    ) {
        use smithay::backend::input::TabletToolProximityEvent;

        let tool = {
            let tablet_seat = self.seat.tablet_seat();
            tablet_seat.add_tool::<Self>(self, &self.display_handle.clone(), &event.tool())
        };
        let tablet = self
            .seat
            .tablet_seat()
            .get_tablet(&TabletDescriptor::from(&event.device()));
        let Some(tablet) = tablet else {
            return;
        };
        match TabletToolProximityEvent::state(&event) {
            ProximityState::In => {
                let location = self
                    .pointer_coordinate_space()
                    .map(|bounds| {
                        super::constrain_pointer_location(
                            event.position_transformed(bounds.size) + bounds.loc.to_f64(),
                            bounds,
                        )
                    })
                    .unwrap_or_default();
                if let Some(focus) = self.pointer_focus_under(location) {
                    tool.proximity_in(
                        location,
                        focus,
                        &tablet,
                        SERIAL_COUNTER.next_serial(),
                        event.time_msec(),
                    );
                }
                self.request_redraw_at(location);
            }
            ProximityState::Out => {
                tool.proximity_out(event.time_msec());
            }
        }
    }

    fn on_tablet_tip(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::TabletToolTipEvent,
    ) {
        use smithay::backend::input::TabletToolTipEvent;

        let Some(tool) = self.seat.tablet_seat().get_tool(&event.tool()) else {
            return;
        };
        match TabletToolTipEvent::tip_state(&event) {
            TabletToolTipState::Down => {
                tool.tip_down(SERIAL_COUNTER.next_serial(), event.time_msec());
            }
            TabletToolTipState::Up => {
                tool.tip_up(event.time_msec());
            }
        }
    }

    fn on_tablet_button(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::TabletToolButtonEvent,
    ) {
        use smithay::backend::input::TabletToolButtonEvent;

        let Some(tool) = self.seat.tablet_seat().get_tool(&event.tool()) else {
            return;
        };
        tool.button(
            TabletToolButtonEvent::button(&event),
            TabletToolButtonEvent::button_state(&event),
            SERIAL_COUNTER.next_serial(),
            event.time_msec(),
        );
    }
}

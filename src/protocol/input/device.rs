use tensor_event::{DeviceChange, DeviceEvent};

use crate::protocol::state::RuntimeState;

impl RuntimeState {
    pub(super) fn process_input_device_change(&mut self, event: DeviceEvent) {
        let cursor_changed = if event.change == DeviceChange::Removed {
            let tablet = &self.protocol_globals.tablet;
            let cursor = &mut self.cursor;
            let mut changed = false;
            for tool in tablet.tool_ids_for_device(event.id) {
                changed = cursor.clear_tablet(tool) || changed;
            }
            changed
        } else {
            false
        };
        self.protocol_globals
            .tablet
            .device_changed(&self.display_handle, event);
        match event.change {
            DeviceChange::Added => {
                self.input_devices.insert(event.id, event.capabilities);
            }
            DeviceChange::Removed => {
                self.input_devices.remove(&event.id);
            }
        }
        self.reconcile_seat_capabilities();
        if cursor_changed {
            self.request_redraw_all();
        }
    }
}

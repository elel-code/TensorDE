use tensor_event::{DeviceChange, DeviceEvent};

use crate::protocol::state::RuntimeState;

impl RuntimeState {
    pub(super) fn process_input_device_change(&mut self, event: DeviceEvent) {
        let removed_cursors = (event.change == DeviceChange::Removed).then(|| {
            self.cursor
                .tablet_positions_for(self.protocol_globals.tablet.tool_ids_for_device(event.id))
        });
        if let Some(cursors) = &removed_cursors {
            for (tool, location) in cursors.iter() {
                self.queue_cursor_redraw_between(tool.get(), location, location);
            }
            for (tool, _) in cursors.iter() {
                assert!(self.cursor.clear_tablet(tool));
            }
        }
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
        if removed_cursors.is_some_and(|cursors| cursors.iter().next().is_some()) {
            self.refresh_cursor_surface_outputs();
            self.flush_queued_redraws();
        }
    }
}

use tensor_event::{TabletToolAxesEvent, TabletToolProximityEvent};
use tensor_util::LogicalPoint;
use wayland_server::Resource;

use super::constrain_pointer_location;
use crate::protocol::state::RuntimeState;

impl RuntimeState {
    pub(super) fn forward_tablet_proximity(&mut self, event: TabletToolProximityEvent) {
        let previous = self.cursor.tablet_location(event.id);
        let target = event
            .in_proximity
            .then(|| self.tablet_target(event.x, event.y))
            .flatten();
        let cursor_location = target.as_ref().map(|target| target.location);
        self.protocol_globals.tablet.tool_proximity(event, target);
        if cursor_location.is_none()
            && let Some(previous) = previous
        {
            self.queue_cursor_redraw_between(event.id.get(), previous, previous);
        }
        let changed = if let Some(location) = cursor_location {
            self.cursor.note_tablet_activity(event.id, location)
        } else {
            self.cursor.clear_tablet(event.id)
        };
        if changed {
            self.refresh_cursor_surface_outputs();
            match (previous, cursor_location) {
                (Some(previous), Some(current)) => {
                    self.request_cursor_redraw_between(event.id.get(), previous, current)
                }
                (Some(_), None) => self.flush_queued_redraws(),
                (None, Some(current)) => {
                    self.request_cursor_redraw_between(event.id.get(), current, current);
                }
                (None, None) => {}
            }
        }
    }

    pub(super) fn forward_tablet_axes(&mut self, event: TabletToolAxesEvent) {
        let Some((x, y)) = self.protocol_globals.tablet.normalized_after_axes(event) else {
            return;
        };
        let target = self.tablet_target(x, y);
        let cursor_location = target.as_ref().map(|target| target.location);
        let previous = self.cursor.tablet_location(event.id);
        self.protocol_globals.tablet.tool_axes(event, target);
        if let Some(location) = cursor_location
            && self.cursor.note_tablet_activity(event.id, location)
        {
            self.refresh_cursor_surface_outputs();
            self.request_cursor_redraw_between(
                event.id.get(),
                previous.unwrap_or(location),
                location,
            );
        }
    }

    fn tablet_target(
        &self,
        normalized_x: f32,
        normalized_y: f32,
    ) -> Option<crate::protocol::globals::tablet::tool::TabletTarget> {
        let bounds = self.pointer_coordinate_space()?;
        let location = LogicalPoint::from((
            f64::from(normalized_x) * f64::from(bounds.size.w),
            f64::from(normalized_y) * f64::from(bounds.size.h),
        )) + bounds.loc.to_f64();
        let location = constrain_pointer_location(location, bounds);
        let (surface, origin) = if self.session_is_locked() {
            self.session_lock_pointer_focus(location)?
        } else {
            self.pointer_focus_under(location)?
        };
        let scale = surface
            .client()
            .map(|client| self.client_scale(&client))
            .unwrap_or(1.0);
        Some(crate::protocol::globals::tablet::tool::TabletTarget {
            surface,
            origin,
            location,
            scale,
        })
    }
}

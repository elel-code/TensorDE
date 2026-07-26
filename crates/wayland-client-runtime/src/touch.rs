use std::collections::HashMap;
use std::sync::Mutex;

use smithay_client_toolkit::dispatch2::Dispatch2;
use wayland_client::protocol::{wl_seat, wl_touch};
use wayland_client::{Connection, QueueHandle};

use crate::SurfaceId;

/// Callback boundary between raw `wl_touch` framing and runtime event policy.
pub(crate) trait TouchHandler {
    fn touch_frame_event(&mut self, seat: &wl_seat::WlSeat, event: wl_touch::Event);
    fn touch_cancelled(&mut self, seat: &wl_seat::WlSeat);
}

/// Per-`wl_touch` dispatch data. Wayland batches touch updates into frames;
/// this object owns that batching and the last-up compatibility fallback.
#[derive(Debug)]
pub(crate) struct TouchData {
    seat: wl_seat::WlSeat,
    frame: Mutex<TouchFrame>,
}

impl TouchData {
    pub(crate) fn new(seat: wl_seat::WlSeat) -> Self {
        Self {
            seat,
            frame: Mutex::new(TouchFrame::default()),
        }
    }
}

impl<D> Dispatch2<wl_touch::WlTouch, D> for TouchData
where
    D: TouchHandler,
{
    fn event(
        &self,
        state: &mut D,
        _: &wl_touch::WlTouch,
        event: wl_touch::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        let (events, cancelled) = self
            .frame
            .lock()
            .expect("touch frame mutex poisoned")
            .push(event);

        for event in events {
            state.touch_frame_event(&self.seat, event);
        }
        if cancelled {
            state.touch_cancelled(&self.seat);
        }
    }
}

#[derive(Debug, Default)]
struct TouchFrame {
    events: Vec<wl_touch::Event>,
    active_points: Vec<i32>,
}

impl TouchFrame {
    fn push(&mut self, event: wl_touch::Event) -> (Vec<wl_touch::Event>, bool) {
        let mut save_event = false;
        let mut process_events = false;
        let mut cancelled = false;

        match &event {
            wl_touch::Event::Down { id, .. } => {
                save_event = true;
                self.activate(*id);
            }
            wl_touch::Event::Up { id, .. } => {
                save_event = true;
                // Some compositors omit the final frame after the last up.
                process_events = self.deactivate(*id);
            }
            wl_touch::Event::Motion { .. }
            | wl_touch::Event::Shape { .. }
            | wl_touch::Event::Orientation { .. } => save_event = true,
            wl_touch::Event::Frame => process_events = true,
            wl_touch::Event::Cancel => {
                self.events.clear();
                self.active_points.clear();
                cancelled = true;
            }
            _ => {}
        }

        if save_event {
            self.events.push(event);
        }
        let events = if process_events {
            std::mem::take(&mut self.events)
        } else {
            Vec::new()
        };
        (events, cancelled)
    }

    fn activate(&mut self, id: i32) {
        if let Err(index) = self.active_points.binary_search(&id) {
            self.active_points.insert(index, id);
        }
    }

    fn deactivate(&mut self, id: i32) -> bool {
        if let Ok(index) = self.active_points.binary_search(&id) {
            self.active_points.remove(index);
        }
        self.active_points.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveTouchPoint {
    surface: SurfaceId,
    serial: u32,
}

/// Seat-local active point tracking used for routing later frame events and
/// validating touch-down popup grab serials.
#[derive(Debug, Default)]
pub(crate) struct TouchPoints(HashMap<i32, ActiveTouchPoint>);

impl TouchPoints {
    pub(crate) fn insert(&mut self, id: i32, surface: SurfaceId, serial: u32) {
        self.0.insert(id, ActiveTouchPoint { surface, serial });
    }

    pub(crate) fn remove(&mut self, id: i32) -> Option<SurfaceId> {
        self.0.remove(&id).map(|point| point.surface)
    }

    pub(crate) fn surface(&self, id: i32) -> Option<SurfaceId> {
        self.0.get(&id).map(|point| point.surface)
    }

    pub(crate) fn contains_serial(&self, serial: u32) -> bool {
        self.0.values().any(|point| point.serial == serial)
    }

    pub(crate) fn remove_surface(&mut self, surface: SurfaceId) {
        self.0.retain(|_, point| point.surface != surface);
    }

    pub(crate) fn drain_surfaces(&mut self) -> Vec<SurfaceId> {
        let mut surfaces: Vec<_> = self.0.drain().map(|(_, point)| point.surface).collect();
        surfaces.sort_unstable();
        surfaces.dedup();
        surfaces
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_tracks_multiple_points_until_the_last_up() {
        let mut frame = TouchFrame::default();
        frame.activate(8);
        frame.activate(3);
        frame.activate(8);

        assert_eq!(frame.active_points, vec![3, 8]);
        assert!(!frame.deactivate(3));
        assert!(frame.deactivate(8));
    }

    #[test]
    fn point_drain_deduplicates_surfaces_and_clears_state() {
        let first = SurfaceId(4);
        let second = SurfaceId(9);
        let mut points = TouchPoints::default();
        points.insert(10, second, 100);
        points.insert(2, first, 20);
        points.insert(7, second, 70);

        assert_eq!(points.drain_surfaces(), vec![first, second]);
        assert!(points.0.is_empty());
    }

    #[test]
    fn point_removal_invalidates_its_serial() {
        let mut points = TouchPoints::default();
        points.insert(5, SurfaceId(2), 55);

        assert!(points.contains_serial(55));
        assert_eq!(points.remove(5), Some(SurfaceId(2)));
        assert!(!points.contains_serial(55));
    }
}

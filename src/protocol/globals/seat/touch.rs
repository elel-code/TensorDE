use smithay::input::touch::{
    DownEvent, FrameMarker, MotionEvent, OrientationEvent, ShapeEvent, UpEvent,
};
use wayland_server::{
    Resource,
    backend::ClientId,
    protocol::{wl_surface::WlSurface, wl_touch::WlTouch},
};

use super::{SeatProtocol, remove_resource};

impl SeatProtocol {
    pub(crate) fn touch_down(&self, surface: &WlSurface, event: &DownEvent, client_scale: f64) {
        let Some(client) = surface.client() else {
            return;
        };
        let location = client_point(event.location, client_scale);
        if let Some(touches) = self.touches.get(&client.id()) {
            for touch in touches {
                touch.down(
                    event.serial.into(),
                    event.time,
                    surface,
                    event.slot.into(),
                    location.0,
                    location.1,
                );
            }
        }
    }

    pub(crate) fn touch_up(&self, surface: &WlSurface, event: &UpEvent) {
        let Some(client) = surface.client() else {
            return;
        };
        if let Some(touches) = self.touches.get(&client.id()) {
            for touch in touches {
                touch.up(event.serial.into(), event.time, event.slot.into());
            }
        }
    }

    pub(crate) fn touch_motion(&self, surface: &WlSurface, event: &MotionEvent, client_scale: f64) {
        let Some(client) = surface.client() else {
            return;
        };
        let location = client_point(event.location, client_scale);
        if let Some(touches) = self.touches.get(&client.id()) {
            for touch in touches {
                touch.motion(event.time, event.slot.into(), location.0, location.1);
            }
        }
    }

    pub(crate) fn touch_frame(&mut self, surface: &WlSurface, marker: FrameMarker) {
        if let Some(client) = surface.client() {
            self.last_touch_frames.insert(client.id(), marker);
        }
        self.for_each_touch(surface, |touch| touch.frame());
    }

    pub(crate) fn touch_cancel(&mut self, surface: &WlSurface, marker: FrameMarker) {
        if let Some(client) = surface.client() {
            self.last_touch_frames.insert(client.id(), marker);
        }
        self.for_each_touch(surface, |touch| touch.cancel());
    }

    pub(crate) fn last_touch_frame(&self, surface: &WlSurface) -> Option<FrameMarker> {
        let client = surface.client()?;
        self.last_touch_frames.get(&client.id()).copied()
    }

    pub(crate) fn touch_shape(&self, surface: &WlSurface, event: &ShapeEvent) {
        self.for_each_touch(surface, |touch| {
            if touch.version() >= 6 {
                touch.shape(event.slot.into(), event.major, event.minor);
            }
        });
    }

    pub(crate) fn touch_orientation(&self, surface: &WlSurface, event: &OrientationEvent) {
        self.for_each_touch(surface, |touch| {
            if touch.version() >= 6 {
                touch.orientation(event.slot.into(), event.orientation);
            }
        });
    }

    fn for_each_touch(&self, surface: &WlSurface, mut apply: impl FnMut(&WlTouch)) {
        let Some(client) = surface.client() else {
            return;
        };
        if let Some(touches) = self.touches.get(&client.id()) {
            for touch in touches {
                apply(touch);
            }
        }
    }

    pub(super) fn insert_touch(&mut self, client: ClientId, touch: WlTouch) {
        self.touches.entry(client).or_default().push(touch);
    }

    pub(super) fn remove_touch(&mut self, client: &ClientId, touch: &WlTouch) {
        remove_resource(&mut self.touches, client, touch);
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

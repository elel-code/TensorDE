use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use smithay_client_toolkit::dispatch2::Dispatch2;
use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::{wl_pointer, wl_seat, wl_surface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_hold_v1::{
    Event as HoldProtocolEvent, ZwpPointerGestureHoldV1,
};
use wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_pinch_v1::{
    Event as PinchProtocolEvent, ZwpPointerGesturePinchV1,
};
use wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_swipe_v1::{
    Event as SwipeProtocolEvent, ZwpPointerGestureSwipeV1,
};
use wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gestures_v1::ZwpPointerGesturesV1;

use super::event::{PointerGestureEvent, PointerHoldEvent, PointerPinchEvent, PointerSwipeEvent};
use crate::{InputSerial, InputSerialSource, SurfaceId};

pub(crate) trait PointerGestureHandler {
    fn pointer_gesture_surface(&mut self, surface: &wl_surface::WlSurface) -> Option<SurfaceId>;

    fn pointer_gesture_event(&mut self, event: PointerGestureEvent);
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ManagerData;

impl<D> Dispatch2<ZwpPointerGesturesV1, D> for ManagerData {
    fn event(
        &self,
        _: &mut D,
        _: &ZwpPointerGesturesV1,
        _: <ZwpPointerGesturesV1 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        unreachable!("zwp_pointer_gestures_v1 has no events");
    }
}

#[derive(Debug)]
pub(crate) struct PointerGestureManager {
    proxy: ZwpPointerGesturesV1,
}

impl PointerGestureManager {
    pub(crate) fn bind<D>(
        globals: &GlobalList,
        queue_handle: &QueueHandle<D>,
    ) -> Result<Self, BindError>
    where
        D: Dispatch<ZwpPointerGesturesV1, ManagerData> + 'static,
    {
        let proxy = globals.bind(queue_handle, 1..=3, ManagerData)?;
        Ok(Self { proxy })
    }

    pub(crate) fn supports_hold(&self) -> bool {
        self.proxy.version() >= 3
    }

    pub(crate) fn create_seat_gestures<D>(
        &self,
        pointer: &wl_pointer::WlPointer,
        seat: &wl_seat::WlSeat,
        queue_handle: &QueueHandle<D>,
    ) -> SeatPointerGestures
    where
        D: Dispatch<ZwpPointerGestureSwipeV1, GestureData>
            + Dispatch<ZwpPointerGesturePinchV1, GestureData>
            + Dispatch<ZwpPointerGestureHoldV1, GestureData>
            + 'static,
    {
        let swipe_data = GestureData::new(seat.clone());
        let swipe = self
            .proxy
            .get_swipe_gesture(pointer, queue_handle, swipe_data.clone());
        let pinch_data = GestureData::new(seat.clone());
        let pinch = self
            .proxy
            .get_pinch_gesture(pointer, queue_handle, pinch_data.clone());
        let hold = self.supports_hold().then(|| {
            let data = GestureData::new(seat.clone());
            let proxy = self
                .proxy
                .get_hold_gesture(pointer, queue_handle, data.clone());
            (proxy, data)
        });
        SeatPointerGestures {
            swipe,
            swipe_data,
            pinch,
            pinch_data,
            hold,
        }
    }
}

impl Drop for PointerGestureManager {
    fn drop(&mut self) {
        if self.proxy.is_alive() && self.proxy.version() >= 2 {
            self.proxy.release();
        }
    }
}

/// Per-seat gesture objects tied to one `wl_pointer` capability lifetime.
#[derive(Debug)]
pub(crate) struct SeatPointerGestures {
    swipe: ZwpPointerGestureSwipeV1,
    swipe_data: GestureData,
    pinch: ZwpPointerGesturePinchV1,
    pinch_data: GestureData,
    hold: Option<(ZwpPointerGestureHoldV1, GestureData)>,
}

impl SeatPointerGestures {
    pub(crate) fn remove_surface(&self, surface: SurfaceId) {
        self.swipe_data.remove_surface(surface);
        self.pinch_data.remove_surface(surface);
        if let Some((_, data)) = self.hold.as_ref() {
            data.remove_surface(surface);
        }
    }
}

impl Drop for SeatPointerGestures {
    fn drop(&mut self) {
        if self.swipe.is_alive() {
            self.swipe.destroy();
        }
        if self.pinch.is_alive() {
            self.pinch.destroy();
        }
        if let Some((hold, _)) = self.hold.take()
            && hold.is_alive()
        {
            hold.destroy();
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GestureData {
    seat: wl_seat::WlSeat,
    active_surface: Arc<ActiveGestureSurface>,
}

impl GestureData {
    fn new(seat: wl_seat::WlSeat) -> Self {
        Self {
            seat,
            active_surface: Arc::new(ActiveGestureSurface::default()),
        }
    }

    fn begin<D>(&self, state: &mut D, surface: &wl_surface::WlSurface) -> Option<SurfaceId>
    where
        D: PointerGestureHandler,
    {
        let surface = state.pointer_gesture_surface(surface);
        self.active_surface.set(surface);
        surface
    }

    fn active_surface(&self) -> Option<SurfaceId> {
        self.active_surface.get()
    }

    fn take_surface(&self) -> Option<SurfaceId> {
        self.active_surface.take()
    }

    fn remove_surface(&self, surface: SurfaceId) {
        self.active_surface.remove(surface);
    }

    fn serial(&self, serial: u32, source: InputSerialSource) -> InputSerial {
        InputSerial::new(self.seat.clone(), serial, source)
    }
}

/// User data must be `Sync`, although protocol dispatch for this runtime is
/// single-threaded. A relaxed atomic keeps update routing lock-free.
#[derive(Debug, Default)]
struct ActiveGestureSurface(AtomicU64);

impl ActiveGestureSurface {
    fn set(&self, surface: Option<SurfaceId>) {
        self.0
            .store(surface.map_or(0, SurfaceId::get), Ordering::Relaxed);
    }

    fn get(&self) -> Option<SurfaceId> {
        match self.0.load(Ordering::Relaxed) {
            0 => None,
            id => Some(SurfaceId(id)),
        }
    }

    fn take(&self) -> Option<SurfaceId> {
        match self.0.swap(0, Ordering::Relaxed) {
            0 => None,
            id => Some(SurfaceId(id)),
        }
    }

    fn remove(&self, surface: SurfaceId) {
        let _ = self
            .0
            .compare_exchange(surface.get(), 0, Ordering::Relaxed, Ordering::Relaxed);
    }
}

impl<D> Dispatch2<ZwpPointerGestureSwipeV1, D> for GestureData
where
    D: PointerGestureHandler,
{
    fn event(
        &self,
        state: &mut D,
        _: &ZwpPointerGestureSwipeV1,
        event: SwipeProtocolEvent,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        let event = match event {
            SwipeProtocolEvent::Begin {
                serial,
                time,
                surface,
                fingers,
            } => {
                let Some(surface) = self.begin(state, &surface) else {
                    return;
                };
                PointerSwipeEvent::Begin {
                    surface,
                    serial: self.serial(serial, InputSerialSource::PointerGestureBegin),
                    time,
                    fingers,
                }
            }
            SwipeProtocolEvent::Update { time, dx, dy } => {
                let Some(surface) = self.active_surface() else {
                    return;
                };
                PointerSwipeEvent::Update {
                    surface,
                    time,
                    delta: (dx, dy),
                }
            }
            SwipeProtocolEvent::End {
                serial,
                time,
                cancelled,
            } => {
                let Some(surface) = self.take_surface() else {
                    return;
                };
                PointerSwipeEvent::End {
                    surface,
                    serial: self.serial(serial, InputSerialSource::PointerGestureEnd),
                    time,
                    cancelled: cancelled != 0,
                }
            }
            _ => return,
        };
        state.pointer_gesture_event(PointerGestureEvent::Swipe(event));
    }
}

impl<D> Dispatch2<ZwpPointerGesturePinchV1, D> for GestureData
where
    D: PointerGestureHandler,
{
    fn event(
        &self,
        state: &mut D,
        _: &ZwpPointerGesturePinchV1,
        event: PinchProtocolEvent,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        let event = match event {
            PinchProtocolEvent::Begin {
                serial,
                time,
                surface,
                fingers,
            } => {
                let Some(surface) = self.begin(state, &surface) else {
                    return;
                };
                PointerPinchEvent::Begin {
                    surface,
                    serial: self.serial(serial, InputSerialSource::PointerGestureBegin),
                    time,
                    fingers,
                }
            }
            PinchProtocolEvent::Update {
                time,
                dx,
                dy,
                scale,
                rotation,
            } => {
                let Some(surface) = self.active_surface() else {
                    return;
                };
                PointerPinchEvent::Update {
                    surface,
                    time,
                    delta: (dx, dy),
                    scale,
                    rotation_degrees_cw: rotation,
                }
            }
            PinchProtocolEvent::End {
                serial,
                time,
                cancelled,
            } => {
                let Some(surface) = self.take_surface() else {
                    return;
                };
                PointerPinchEvent::End {
                    surface,
                    serial: self.serial(serial, InputSerialSource::PointerGestureEnd),
                    time,
                    cancelled: cancelled != 0,
                }
            }
            _ => return,
        };
        state.pointer_gesture_event(PointerGestureEvent::Pinch(event));
    }
}

impl<D> Dispatch2<ZwpPointerGestureHoldV1, D> for GestureData
where
    D: PointerGestureHandler,
{
    fn event(
        &self,
        state: &mut D,
        _: &ZwpPointerGestureHoldV1,
        event: HoldProtocolEvent,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        let event = match event {
            HoldProtocolEvent::Begin {
                serial,
                time,
                surface,
                fingers,
            } => {
                let Some(surface) = self.begin(state, &surface) else {
                    return;
                };
                PointerHoldEvent::Begin {
                    surface,
                    serial: self.serial(serial, InputSerialSource::PointerGestureBegin),
                    time,
                    fingers,
                }
            }
            HoldProtocolEvent::End {
                serial,
                time,
                cancelled,
            } => {
                let Some(surface) = self.take_surface() else {
                    return;
                };
                PointerHoldEvent::End {
                    surface,
                    serial: self.serial(serial, InputSerialSource::PointerGestureEnd),
                    time,
                    cancelled: cancelled != 0,
                }
            }
            _ => return,
        };
        state.pointer_gesture_event(PointerGestureEvent::Hold(event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_surface_is_taken_exactly_once() {
        let active = ActiveGestureSurface::default();
        active.set(Some(SurfaceId(7)));

        assert_eq!(active.get(), Some(SurfaceId(7)));
        assert_eq!(active.take(), Some(SurfaceId(7)));
        assert_eq!(active.take(), None);
    }

    #[test]
    fn removing_a_surface_does_not_clear_an_unrelated_gesture() {
        let active = ActiveGestureSurface::default();
        active.set(Some(SurfaceId(9)));

        active.remove(SurfaceId(4));
        assert_eq!(active.get(), Some(SurfaceId(9)));

        active.remove(SurfaceId(9));
        assert_eq!(active.get(), None);
    }

    #[test]
    fn failed_begin_resolution_clears_previous_routing() {
        let active = ActiveGestureSurface::default();
        active.set(Some(SurfaceId(3)));
        active.set(None);

        assert_eq!(active.get(), None);
    }
}

//! Pointer gestures and relative pointer mapping.

use wayland_client::protocol::wl_seat::WlSeat;

use crate::event::Event;
use crate::native::shell::NativeShellEvent;

use crate::input::{InputSerial, InputSerialSource, SeatId};
use crate::surface::SurfaceId;
use crate::{
    PointerGestureEvent, PointerHoldEvent, PointerPinchEvent, PointerSwipeEvent,
    RelativePointerEvent,
};

use super::{NativeEventMapState, SurfaceIdMap};

#[allow(unused_variables)]
pub(crate) fn map(
    event: NativeShellEvent,
    surfaces: &mut SurfaceIdMap,
    seat: Option<&WlSeat>,
    map_state: &mut NativeEventMapState,
) -> Option<Event> {
    match event {
        NativeShellEvent::GestureSwipeBegin {
            surface,
            fingers,
            time,
            seat: event_seat,
        } => {
            map_state.gesture_surface = Some(surface);
            let seat = seat?;
            Some(Event::PointerGesture(PointerGestureEvent::Swipe(
                PointerSwipeEvent::Begin {
                    surface: surfaces.intern(surface),
                    serial: InputSerial::new(
                        seat.clone(),
                        map_state.last_serial,
                        InputSerialSource::PointerGestureBegin,
                    ),
                    time,
                    fingers,
                    seat: event_seat.map(SeatId::from_raw),
                },
            )))
        }
        NativeShellEvent::GestureSwipeUpdate {
            dx,
            dy,
            time,
            seat: event_seat,
        } => {
            let surface = map_state
                .gesture_surface
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            Some(Event::PointerGesture(PointerGestureEvent::Swipe(
                PointerSwipeEvent::Update {
                    surface,
                    time,
                    delta: (dx, dy),
                    seat: event_seat.map(SeatId::from_raw),
                },
            )))
        }
        NativeShellEvent::GestureSwipeEnd {
            cancelled,
            time,
            seat: event_seat,
        } => {
            let seat = seat?;
            let surface = map_state
                .gesture_surface
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            map_state.gesture_surface = None;
            Some(Event::PointerGesture(PointerGestureEvent::Swipe(
                PointerSwipeEvent::End {
                    surface,
                    serial: InputSerial::new(
                        seat.clone(),
                        map_state.last_serial,
                        InputSerialSource::PointerGestureEnd,
                    ),
                    time,
                    cancelled,
                    seat: event_seat.map(SeatId::from_raw),
                },
            )))
        }
        NativeShellEvent::GesturePinchBegin {
            surface,
            fingers,
            time,
            seat: event_seat,
        } => {
            map_state.gesture_surface = Some(surface);
            let seat = seat?;
            Some(Event::PointerGesture(PointerGestureEvent::Pinch(
                PointerPinchEvent::Begin {
                    surface: surfaces.intern(surface),
                    serial: InputSerial::new(
                        seat.clone(),
                        map_state.last_serial,
                        InputSerialSource::PointerGestureBegin,
                    ),
                    time,
                    fingers,
                    seat: event_seat.map(SeatId::from_raw),
                },
            )))
        }
        NativeShellEvent::GesturePinchUpdate {
            dx,
            dy,
            scale,
            rotation,
            time,
            seat: event_seat,
        } => {
            let surface = map_state
                .gesture_surface
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            Some(Event::PointerGesture(PointerGestureEvent::Pinch(
                PointerPinchEvent::Update {
                    surface,
                    time,
                    delta: (dx, dy),
                    scale,
                    rotation_degrees_cw: rotation,
                    seat: event_seat.map(SeatId::from_raw),
                },
            )))
        }
        NativeShellEvent::GesturePinchEnd {
            cancelled,
            time,
            seat: event_seat,
        } => {
            let seat = seat?;
            let surface = map_state
                .gesture_surface
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            map_state.gesture_surface = None;
            Some(Event::PointerGesture(PointerGestureEvent::Pinch(
                PointerPinchEvent::End {
                    surface,
                    serial: InputSerial::new(
                        seat.clone(),
                        map_state.last_serial,
                        InputSerialSource::PointerGestureEnd,
                    ),
                    time,
                    cancelled,
                    seat: event_seat.map(SeatId::from_raw),
                },
            )))
        }
        NativeShellEvent::GestureHoldBegin {
            surface,
            fingers,
            time,
            seat: event_seat,
        } => {
            map_state.gesture_surface = Some(surface);
            let seat = seat?;
            Some(Event::PointerGesture(PointerGestureEvent::Hold(
                PointerHoldEvent::Begin {
                    surface: surfaces.intern(surface),
                    serial: InputSerial::new(
                        seat.clone(),
                        map_state.last_serial,
                        InputSerialSource::PointerGestureBegin,
                    ),
                    time,
                    fingers,
                    seat: event_seat.map(SeatId::from_raw),
                },
            )))
        }
        NativeShellEvent::GestureHoldEnd {
            cancelled,
            time,
            seat: event_seat,
        } => {
            let seat = seat?;
            let surface = map_state
                .gesture_surface
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            map_state.gesture_surface = None;
            Some(Event::PointerGesture(PointerGestureEvent::Hold(
                PointerHoldEvent::End {
                    surface,
                    serial: InputSerial::new(
                        seat.clone(),
                        map_state.last_serial,
                        InputSerialSource::PointerGestureEnd,
                    ),
                    time,
                    cancelled,
                    seat: event_seat.map(SeatId::from_raw),
                },
            )))
        }
        NativeShellEvent::RelativePointer {
            utime,
            dx,
            dy,
            dx_unaccel,
            dy_unaccel,
            seat,
        } => {
            // Prefer the seat's pointer focus; fall back to last-wins map state.
            let focus = map_state.pointer_focus;
            let surface = focus
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            Some(Event::RelativePointer(RelativePointerEvent {
                surface,
                time_micros: utime,
                delta: (dx, dy),
                delta_unaccelerated: (dx_unaccel, dy_unaccel),
                seat: seat.map(SeatId::from_raw),
            }))
        }

        _ => unreachable!("event routed to wrong mapper"),
    }
}

//! pointer-gestures-v1 dispatch for the native shell.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_hold_v1, zwp_pointer_gesture_pinch_v1, zwp_pointer_gesture_swipe_v1,
    zwp_pointer_gestures_v1,
};

use super::types::{NativeShellEvent, NativeShellState};

fn seat_for_swipe(
    state: &NativeShellState,
    gesture: &zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1,
) -> Option<u32> {
    state
        .swipe_objects
        .get(&gesture.id().protocol_id())
        .copied()
}

fn seat_for_pinch(
    state: &NativeShellState,
    gesture: &zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1,
) -> Option<u32> {
    state
        .pinch_objects
        .get(&gesture.id().protocol_id())
        .copied()
}

fn seat_for_hold(
    state: &NativeShellState,
    gesture: &zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1,
) -> Option<u32> {
    state
        .hold_objects
        .get(&gesture.id().protocol_id())
        .copied()
}

impl Dispatch<zwp_pointer_gestures_v1::ZwpPointerGesturesV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &zwp_pointer_gestures_v1::ZwpPointerGesturesV1,
        _: zwp_pointer_gestures_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        gesture: &zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1,
        event: zwp_pointer_gesture_swipe_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let seat_global = seat_for_swipe(state, gesture);
        match event {
            zwp_pointer_gesture_swipe_v1::Event::Begin {
                serial,
                time,
                surface,
                fingers,
            } => {
                state.note_seat_serial(seat_global, serial);
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                state.gesture_surface = id;
                if let Some(surface) = id {
                    state.push(NativeShellEvent::GestureSwipeBegin {
                        surface,
                        fingers,
                        time,
                        seat: seat_global,
                    });
                }
            }
            zwp_pointer_gesture_swipe_v1::Event::Update { time, dx, dy } => {
                state.push(NativeShellEvent::GestureSwipeUpdate {
                    dx,
                    dy,
                    time,
                    seat: seat_global,
                });
            }
            zwp_pointer_gesture_swipe_v1::Event::End {
                serial,
                time,
                cancelled,
            } => {
                state.note_seat_serial(seat_global, serial);
                state.gesture_surface = None;
                state.push(NativeShellEvent::GestureSwipeEnd {
                    cancelled: cancelled != 0,
                    time,
                    seat: seat_global,
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        gesture: &zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let seat_global = seat_for_pinch(state, gesture);
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin {
                serial,
                time,
                surface,
                fingers,
            } => {
                state.note_seat_serial(seat_global, serial);
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                state.gesture_surface = id;
                if let Some(surface) = id {
                    state.push(NativeShellEvent::GesturePinchBegin {
                        surface,
                        fingers,
                        time,
                        seat: seat_global,
                    });
                }
            }
            zwp_pointer_gesture_pinch_v1::Event::Update {
                time,
                dx,
                dy,
                scale,
                rotation,
            } => {
                state.push(NativeShellEvent::GesturePinchUpdate {
                    dx,
                    dy,
                    scale,
                    rotation,
                    time,
                    seat: seat_global,
                });
            }
            zwp_pointer_gesture_pinch_v1::Event::End {
                serial,
                time,
                cancelled,
            } => {
                state.note_seat_serial(seat_global, serial);
                state.gesture_surface = None;
                state.push(NativeShellEvent::GesturePinchEnd {
                    cancelled: cancelled != 0,
                    time,
                    seat: seat_global,
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        gesture: &zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1,
        event: zwp_pointer_gesture_hold_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let seat_global = seat_for_hold(state, gesture);
        match event {
            zwp_pointer_gesture_hold_v1::Event::Begin {
                serial,
                time,
                surface,
                fingers,
            } => {
                state.note_seat_serial(seat_global, serial);
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                state.gesture_surface = id;
                if let Some(surface) = id {
                    state.push(NativeShellEvent::GestureHoldBegin {
                        surface,
                        fingers,
                        time,
                        seat: seat_global,
                    });
                }
            }
            zwp_pointer_gesture_hold_v1::Event::End {
                serial,
                time,
                cancelled,
            } => {
                state.note_seat_serial(seat_global, serial);
                state.gesture_surface = None;
                state.push(NativeShellEvent::GestureHoldEnd {
                    cancelled: cancelled != 0,
                    time,
                    seat: seat_global,
                });
            }
            _ => {}
        }
    }
}

//! pointer-gestures-v1 dispatch for the native shell.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_hold_v1, zwp_pointer_gesture_pinch_v1, zwp_pointer_gesture_swipe_v1,
    zwp_pointer_gestures_v1,
};

use super::types::{NativeShellEvent, NativeShellState};

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
        _: &zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1,
        event: zwp_pointer_gesture_swipe_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_pointer_gesture_swipe_v1::Event::Begin {
                serial,
                time,
                surface,
                fingers,
            } => {
                state.last_input_serial = Some(serial);
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
                    });
                }
            }
            zwp_pointer_gesture_swipe_v1::Event::Update { time, dx, dy } => {
                state.push(NativeShellEvent::GestureSwipeUpdate { dx, dy, time });
            }
            zwp_pointer_gesture_swipe_v1::Event::End {
                serial,
                time,
                cancelled,
            } => {
                state.last_input_serial = Some(serial);
                state.gesture_surface = None;
                state.push(NativeShellEvent::GestureSwipeEnd {
                    cancelled: cancelled != 0,
                    time,
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        _: &zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin {
                serial,
                time,
                surface,
                fingers,
            } => {
                state.last_input_serial = Some(serial);
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
                });
            }
            zwp_pointer_gesture_pinch_v1::Event::End {
                serial,
                time,
                cancelled,
            } => {
                state.last_input_serial = Some(serial);
                state.gesture_surface = None;
                state.push(NativeShellEvent::GesturePinchEnd {
                    cancelled: cancelled != 0,
                    time,
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        _: &zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1,
        event: zwp_pointer_gesture_hold_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_pointer_gesture_hold_v1::Event::Begin {
                serial,
                time,
                surface,
                fingers,
            } => {
                state.last_input_serial = Some(serial);
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
                    });
                }
            }
            zwp_pointer_gesture_hold_v1::Event::End {
                serial,
                time,
                cancelled,
            } => {
                state.last_input_serial = Some(serial);
                state.gesture_surface = None;
                state.push(NativeShellEvent::GestureHoldEnd {
                    cancelled: cancelled != 0,
                    time,
                });
            }
            _ => {}
        }
    }
}

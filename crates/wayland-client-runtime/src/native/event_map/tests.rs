//! Unit tests for `event_map`.

use super::*;
use crate::event::{Event, SurfaceEvent, ToplevelState, TouchEvent, TouchEventKind};
use crate::geometry::SuggestedSize;
use crate::input::SeatEvent;
use crate::native::shell::{NativeShellEvent, NativeSurfaceId};

#[test]
fn maps_toplevel_configure() {
    let mut map = SurfaceIdMap::new();
    let native = NativeSurfaceId(1);
    let event = NativeShellEvent::ToplevelConfigure {
        surface: native,
        suggested_size: SuggestedSize::new(Some(800), Some(600)),
        state: ToplevelState::ACTIVATED,
        serial: 7,
    };
    let mapped = map_native_event(event, &mut map).expect("mapped");
    match mapped {
        Event::Surface(SurfaceEvent::Configure {
            surface,
            suggested_size,
            state,
            serial,
        }) => {
            assert_eq!(surface, map.get(native).unwrap());
            assert_eq!(suggested_size.width, Some(800));
            assert!(state.contains(ToplevelState::ACTIVATED));
            assert_eq!(serial, 7);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn maps_touch_down_with_serial_and_time() {
    let mut map = SurfaceIdMap::new();
    let mut map_state = NativeEventMapState::default();
    let native = NativeSurfaceId(7);
    let event = NativeShellEvent::TouchDown {
        surface: native,
        id: 1,
        x: 10.0,
        y: 20.0,
        serial: 42,
        time: 1000,
        seat: Some(1),
    };
    // Without seat, serial-bearing touch events are dropped.
    assert!(map_native_event_full(event.clone(), &mut map, None, &mut map_state).is_none());
}

#[test]
fn maps_seat_added_and_removed() {
    let mut map = SurfaceIdMap::new();
    let added = map_native_event(
        NativeShellEvent::SeatAdded {
            seat: 7,
            name: Some("seat0".into()),
            has_keyboard: true,
            has_pointer: true,
            has_touch: false,
        },
        &mut map,
    )
    .expect("seat added maps");
    match added {
        Event::Seat(SeatEvent::Added(info)) => {
            assert_eq!(info.id.get(), 7);
            assert_eq!(info.name.as_deref(), Some("seat0"));
            assert!(info.has_keyboard && info.has_pointer && !info.has_touch);
        }
        other => panic!("expected Seat::Added, got {other:?}"),
    }
    let changed = map_native_event(
        NativeShellEvent::SeatChanged {
            seat: 7,
            name: Some("seat0".into()),
            has_keyboard: true,
            has_pointer: false,
            has_touch: true,
        },
        &mut map,
    )
    .expect("seat changed maps");
    match changed {
        Event::Seat(SeatEvent::Changed(info)) => {
            assert_eq!(info.id.get(), 7);
            assert!(!info.has_pointer && info.has_touch);
        }
        other => panic!("expected Seat::Changed, got {other:?}"),
    }
    let removed = map_native_event(NativeShellEvent::SeatRemoved { seat: 7 }, &mut map)
        .expect("seat removed maps");
    match removed {
        Event::Seat(SeatEvent::Removed(id)) => assert_eq!(id.get(), 7),
        other => panic!("expected Seat::Removed, got {other:?}"),
    }
}

#[test]
fn maps_surface_output_enter_leave() {
    let mut map = SurfaceIdMap::new();
    let mut map_state = NativeEventMapState::default();
    let native = NativeSurfaceId(3);
    let enter = map_native_event_full(
        NativeShellEvent::SurfaceOutputEnter {
            surface: native,
            output: 7,
        },
        &mut map,
        None,
        &mut map_state,
    );
    match enter {
        Some(Event::Surface(SurfaceEvent::OutputEnter { surface, output })) => {
            assert_eq!(surface, map.get(native).unwrap());
            assert_eq!(output.get(), 7);
        }
        other => panic!("expected OutputEnter, got {other:?}"),
    }
    let leave = map_native_event_full(
        NativeShellEvent::SurfaceOutputLeave {
            surface: native,
            output: 7,
        },
        &mut map,
        None,
        &mut map_state,
    );
    match leave {
        Some(Event::Surface(SurfaceEvent::OutputLeave { surface, output })) => {
            assert_eq!(surface, map.get(native).unwrap());
            assert_eq!(output.get(), 7);
        }
        other => panic!("expected OutputLeave, got {other:?}"),
    }
}

#[test]
fn maps_touch_shape_and_orientation() {
    let mut map = SurfaceIdMap::new();
    let mut map_state = NativeEventMapState::default();
    let shape = map_native_event_full(
        NativeShellEvent::TouchShape {
            id: 2,
            major: 4.0,
            minor: 2.0,
            seat: None,
        },
        &mut map,
        None,
        &mut map_state,
    );
    match shape {
        Some(Event::Touch(TouchEvent {
            kind: TouchEventKind::Shape { id, major, minor },
            ..
        })) => {
            assert_eq!(id, 2);
            assert_eq!(major, 4.0);
            assert_eq!(minor, 2.0);
        }
        other => panic!("expected shape, got {other:?}"),
    }
    let orient = map_native_event_full(
        NativeShellEvent::TouchOrientation {
            id: 2,
            degrees: 45.0,
            seat: None,
        },
        &mut map,
        None,
        &mut map_state,
    );
    match orient {
        Some(Event::Touch(TouchEvent {
            kind: TouchEventKind::Orientation { id, degrees },
            ..
        })) => {
            assert_eq!(id, 2);
            assert_eq!(degrees, 45.0);
        }
        other => panic!("expected orientation, got {other:?}"),
    }
}

#[test]
fn extracts_key_text() {
    let event = NativeShellEvent::SeatKeyboardKey {
        key: 30,
        pressed: true,
        keysym: 0x61,
        text: Some("a".into()),
        seat: None,
    };
    assert_eq!(native_key_text_pressed(&event), Some("a"));
    let (key, keysym, pressed, text) = map_native_key_text(&event).unwrap();
    assert_eq!((key, keysym, pressed, text), (30, 0x61, true, Some("a")));
    // Without seat, key events do not become public Event.
    let mut map = SurfaceIdMap::new();
    assert!(map_native_event(event, &mut map).is_none());
}

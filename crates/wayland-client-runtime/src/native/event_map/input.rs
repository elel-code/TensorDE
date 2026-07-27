//! Keyboard, pointer, and touch mapping.

use wayland_client::protocol::wl_seat::WlSeat;

use crate::event::Event;
use crate::native::shell::NativeShellEvent;

use crate::event::{KeyState, KeyboardEvent, PointerEvent, PointerEventKind, TouchEvent, TouchEventKind};
use crate::input::{InputSerial, InputSerialSource, SeatId};
use crate::surface::SurfaceId;

use super::helpers::modifiers_from_xkb_mask;

use super::{NativeEventMapState, SurfaceIdMap};

#[allow(unused_variables)]
pub(crate) fn map(
    event: NativeShellEvent,
    surfaces: &mut SurfaceIdMap,
    seat: Option<&WlSeat>,
    map_state: &mut NativeEventMapState,
) -> Option<Event> {
    match event {
        NativeShellEvent::SeatKeyboardEnter {
            surface,
            seat: event_seat,
        } => {
            map_state.keyboard_focus = surface;
            let seat = seat?;
            let surface = surface.map(|s| surfaces.intern(s))?;
            Some(Event::Keyboard(KeyboardEvent::Enter {
                surface,
                serial: InputSerial::new(
                    seat.clone(),
                    map_state.last_serial,
                    InputSerialSource::KeyboardEnter,
                ),
                pressed_raw_codes: Vec::new(),
                seat: event_seat.map(SeatId::from_raw),
            }))
        }
        NativeShellEvent::SeatKeyboardLeave {
            surface,
            seat: event_seat,
        } => {
            let surface = surface
                .or(map_state.keyboard_focus)
                .map(|s| surfaces.intern(s))?;
            map_state.keyboard_focus = None;
            Some(Event::Keyboard(KeyboardEvent::Leave {
                surface,
                seat: event_seat.map(SeatId::from_raw),
            }))
        }
        NativeShellEvent::SeatKeyboardKey {
            key,
            pressed,
            keysym,
            text,
            seat: event_seat,
        } => {
            let seat = seat?;
            let surface = map_state
                .keyboard_focus
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            Some(Event::Keyboard(KeyboardEvent::Key {
                surface,
                state: if pressed {
                    KeyState::Pressed
                } else {
                    KeyState::Released
                },
                time: 0,
                raw_code: key,
                keysym,
                text,
                serial: InputSerial::new(
                    seat.clone(),
                    map_state.last_serial,
                    InputSerialSource::KeyboardKey,
                ),
                seat: event_seat.map(SeatId::from_raw),
            }))
        }
        NativeShellEvent::PointerEnter {
            surface,
            x,
            y,
            seat: event_seat,
        } => {
            map_state.pointer_focus = Some(surface);
            map_state.pointer_pos = (x, y);
            let seat = seat?;
            Some(Event::Pointer(PointerEvent {
                surface: surfaces.intern(surface),
                position: (x, y),
                kind: PointerEventKind::Enter {
                    serial: InputSerial::new(
                        seat.clone(),
                        map_state.last_serial,
                        InputSerialSource::PointerEnter,
                    ),
                },
                seat: event_seat.map(SeatId::from_raw),
            }))
        }
        NativeShellEvent::PointerLeave {
            surface,
            seat: event_seat,
        } => {
            map_state.pointer_focus = None;
            Some(Event::Pointer(PointerEvent {
                surface: surfaces.intern(surface),
                position: map_state.pointer_pos,
                kind: PointerEventKind::Leave,
                seat: event_seat.map(SeatId::from_raw),
            }))
        }
        NativeShellEvent::PointerMotion {
            surface,
            x,
            y,
            seat: event_seat,
        } => {
            map_state.pointer_focus = Some(surface);
            map_state.pointer_pos = (x, y);
            Some(Event::Pointer(PointerEvent {
                surface: surfaces.intern(surface),
                position: (x, y),
                kind: PointerEventKind::Motion { time: 0 },
                seat: event_seat.map(SeatId::from_raw),
            }))
        }
        NativeShellEvent::PointerAxis {
            surface,
            horizontal,
            vertical,
            source,
            seat: event_seat,
        } => {
            let surface = surface
                .or(map_state.pointer_focus)
                .map(|s| surfaces.intern(s))?;
            Some(Event::Pointer(PointerEvent {
                surface,
                position: map_state.pointer_pos,
                kind: PointerEventKind::Axis {
                    time: 0,
                    horizontal,
                    vertical,
                    source,
                },
                seat: event_seat.map(SeatId::from_raw),
            }))
        }
        NativeShellEvent::SeatModifiers {
            mods_depressed,
            mods_latched,
            mods_locked,
            seat: event_seat,
            ..
        } => {
            let surface = map_state
                .keyboard_focus
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            let effective = mods_depressed | mods_latched | mods_locked;
            Some(Event::Keyboard(KeyboardEvent::Modifiers {
                surface,
                modifiers: modifiers_from_xkb_mask(effective),
                seat: event_seat.map(SeatId::from_raw),
            }))
        }
        NativeShellEvent::PointerButton {
            surface,
            button,
            pressed,
            seat: event_seat,
        } => {
            let seat = seat?;
            let surface = surface
                .or(map_state.pointer_focus)
                .map(|s| surfaces.intern(s))?;
            let source = if pressed {
                InputSerialSource::PointerPress
            } else {
                InputSerialSource::PointerRelease
            };
            Some(Event::Pointer(PointerEvent {
                surface,
                position: map_state.pointer_pos,
                kind: if pressed {
                    PointerEventKind::Press {
                        time: 0,
                        button,
                        serial: InputSerial::new(seat.clone(), map_state.last_serial, source),
                    }
                } else {
                    PointerEventKind::Release {
                        time: 0,
                        button,
                        serial: InputSerial::new(seat.clone(), map_state.last_serial, source),
                    }
                },
                seat: event_seat.map(SeatId::from_raw),
            }))
        }
        NativeShellEvent::TouchDown {
            surface,
            id,
            x,
            y,
            serial,
            time,
            seat: event_seat,
        } => {
            let seat = seat?;
            map_state.last_serial = serial;
            Some(Event::Touch(TouchEvent {
                surface: Some(surfaces.intern(surface)),
                kind: TouchEventKind::Down {
                    time,
                    id,
                    position: (x, y),
                    serial: InputSerial::new(
                        seat.clone(),
                        serial,
                        InputSerialSource::TouchDown,
                    ),
                },
                seat: event_seat.map(SeatId::from_raw),
            }))
        }
        NativeShellEvent::TouchUp {
            id,
            serial,
            time,
            seat: event_seat,
        } => {
            let seat = seat?;
            map_state.last_serial = serial;
            Some(Event::Touch(TouchEvent {
                surface: None,
                kind: TouchEventKind::Up {
                    time,
                    id,
                    serial: InputSerial::new(
                        seat.clone(),
                        serial,
                        InputSerialSource::TouchUp,
                    ),
                },
                seat: event_seat.map(SeatId::from_raw),
            }))
        }
        NativeShellEvent::TouchMotion {
            id,
            x,
            y,
            time,
            seat: event_seat,
        } => Some(Event::Touch(TouchEvent {
            surface: None,
            kind: TouchEventKind::Motion {
                time,
                id,
                position: (x, y),
            },
            seat: event_seat.map(SeatId::from_raw),
        })),
        NativeShellEvent::TouchShape {
            id,
            major,
            minor,
            seat: event_seat,
        } => Some(Event::Touch(TouchEvent {
            surface: None,
            kind: TouchEventKind::Shape { id, major, minor },
            seat: event_seat.map(SeatId::from_raw),
        })),
        NativeShellEvent::TouchOrientation {
            id,
            degrees,
            seat: event_seat,
        } => Some(Event::Touch(TouchEvent {
            surface: None,
            kind: TouchEventKind::Orientation { id, degrees },
            seat: event_seat.map(SeatId::from_raw),
        })),
        NativeShellEvent::TouchFrame { seat: event_seat } => {
            // Frame is protocol-level; no public event (already expanded).
            let _ = event_seat;
            None
        }
        NativeShellEvent::TouchCancel { seat: event_seat } => Some(Event::Touch(TouchEvent {
            surface: None,
            kind: TouchEventKind::Cancelled,
            seat: event_seat.map(SeatId::from_raw),
        })),

        _ => unreachable!("event routed to wrong mapper"),
    }
}

//! Map [`NativeShellEvent`] toward the public [`crate::Event`] model.
//!
//! Structural bridge for migrating Fika off SCTK `Runtime`. With a live seat
//! proxy and focus tracking, keyboard / pointer / gesture events map fully.

use std::collections::HashMap;

use wayland_client::protocol::wl_seat::WlSeat;

use crate::event::{
    Event, KeyState, KeyboardEvent, Modifiers, PointerEvent, PointerEventKind, SurfaceEvent,
    TouchEvent, TouchEventKind,
};
use crate::pointer_axis::PointerAxisValue;
use crate::geometry::{LogicalPosition, LogicalSize};
use crate::input::{InputSerial, InputSerialSource};
use crate::native::shell::{NativeShellEvent, NativeSurfaceId};
use crate::surface::SurfaceId;
use crate::dnd::{DndAction, DndActions, DndEvent, DndOfferId, DndSourceId};
use crate::geometry::LogicalPosition as GeoLogicalPosition;
use crate::pointer_constraints::{PointerConstraint, PointerConstraintEvent};
use crate::{
    LayerSurfaceEvent, PointerGestureEvent, PointerHoldEvent, PointerPinchEvent, PointerSwipeEvent,
    RelativePointerEvent,
};

/// Bidirectional id map for native ↔ public surface identifiers.
#[derive(Clone, Debug, Default)]
pub struct SurfaceIdMap {
    native_to_public: HashMap<NativeSurfaceId, SurfaceId>,
    next_public: u64,
}

impl SurfaceIdMap {
    pub fn new() -> Self {
        Self {
            native_to_public: HashMap::new(),
            next_public: 1,
        }
    }

    /// Allocate or reuse a public [`SurfaceId`] for a native surface.
    pub fn intern(&mut self, native: NativeSurfaceId) -> SurfaceId {
        *self.native_to_public.entry(native).or_insert_with(|| {
            let id = SurfaceId(self.next_public);
            self.next_public = self.next_public.saturating_add(1);
            id
        })
    }

    pub fn get(&self, native: NativeSurfaceId) -> Option<SurfaceId> {
        self.native_to_public.get(&native).copied()
    }

    pub fn remove(&mut self, native: NativeSurfaceId) -> Option<SurfaceId> {
        self.native_to_public.remove(&native)
    }
}

/// Mutable mapping context (seat + focus) for serial-bearing events.
#[derive(Clone, Debug, Default)]
pub struct NativeEventMapState {
    pub keyboard_focus: Option<NativeSurfaceId>,
    pub pointer_focus: Option<NativeSurfaceId>,
    pub pointer_pos: (f64, f64),
    pub gesture_surface: Option<NativeSurfaceId>,
    pub dnd_surface: Option<NativeSurfaceId>,
    /// Latest input serial from the native shell (updated by drain helpers).
    pub last_serial: u32,
}

/// Convert one native shell event into a public crate event when possible.
///
/// Without `seat`, only surface/layer lifecycle events map. With `seat`,
/// keyboard/pointer/touch/gesture paths fill real [`InputSerial`] values and
/// update `map_state` focus tracking.
pub fn map_native_event(
    event: NativeShellEvent,
    surfaces: &mut SurfaceIdMap,
) -> Option<Event> {
    map_native_event_full(event, surfaces, None, &mut NativeEventMapState::default())
}

/// Full mapping with seat + focus state (preferred for Fika merge).
pub fn map_native_event_full(
    event: NativeShellEvent,
    surfaces: &mut SurfaceIdMap,
    seat: Option<&WlSeat>,
    map_state: &mut NativeEventMapState,
) -> Option<Event> {
    match event {
        NativeShellEvent::ToplevelConfigure {
            surface,
            suggested_size,
            state,
            serial,
        } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::Configure {
                surface,
                suggested_size,
                state,
                serial,
            }))
        }
        NativeShellEvent::ToplevelClose { surface } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::CloseRequested { surface }))
        }
        NativeShellEvent::ScaleFactorChanged { surface, factor } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::ScaleFactorChanged {
                surface,
                factor,
            }))
        }
        NativeShellEvent::Frame { surface, time } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::Frame { surface, time }))
        }
        NativeShellEvent::PopupConfigure {
            surface,
            x,
            y,
            width,
            height,
            serial,
            reposition_token,
        } => {
            let surface = surfaces.intern(surface);
            let kind = match reposition_token {
                Some(token) => crate::event::PopupConfigureKind::Reposition { token },
                None => crate::event::PopupConfigureKind::Initial,
            };
            Some(Event::Surface(SurfaceEvent::PopupConfigure {
                surface,
                position: LogicalPosition::new(x, y),
                size: LogicalSize::new(width.max(0) as u32, height.max(0) as u32),
                serial,
                kind,
            }))
        }
        NativeShellEvent::PopupDone { surface } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::PopupDone { surface }))
        }
        NativeShellEvent::LayerConfigure {
            surface,
            suggested_size,
            serial,
        } => Some(Event::LayerSurface(LayerSurfaceEvent::Configure {
            surface: surfaces.intern(surface),
            suggested_size,
            serial,
        })),
        NativeShellEvent::LayerClosed { surface } => {
            Some(Event::LayerSurface(LayerSurfaceEvent::Closed {
                surface: surfaces.intern(surface),
            }))
        }
        NativeShellEvent::SeatKeyboardEnter { surface } => {
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
            }))
        }
        NativeShellEvent::SeatKeyboardLeave { surface } => {
            let surface = surface
                .or(map_state.keyboard_focus)
                .map(|s| surfaces.intern(s))?;
            map_state.keyboard_focus = None;
            Some(Event::Keyboard(KeyboardEvent::Leave { surface }))
        }
        NativeShellEvent::SeatKeyboardKey {
            key,
            pressed,
            keysym,
            text,
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
            }))
        }
        NativeShellEvent::PointerEnter { surface, x, y } => {
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
            }))
        }
        NativeShellEvent::PointerLeave { surface } => {
            map_state.pointer_focus = None;
            Some(Event::Pointer(PointerEvent {
                surface: surfaces.intern(surface),
                position: map_state.pointer_pos,
                kind: PointerEventKind::Leave,
            }))
        }
        NativeShellEvent::PointerMotion { surface, x, y } => {
            map_state.pointer_focus = Some(surface);
            map_state.pointer_pos = (x, y);
            Some(Event::Pointer(PointerEvent {
                surface: surfaces.intern(surface),
                position: (x, y),
                kind: PointerEventKind::Motion { time: 0 },
            }))
        }
        NativeShellEvent::PointerAxis {
            surface,
            horizontal,
            vertical,
            horizontal_value120,
            vertical_value120,
        } => {
            let surface = surface
                .or(map_state.pointer_focus)
                .map(|s| surfaces.intern(s))?;
            Some(Event::Pointer(PointerEvent {
                surface,
                position: map_state.pointer_pos,
                kind: PointerEventKind::Axis {
                    time: 0,
                    horizontal: PointerAxisValue {
                        continuous: horizontal,
                        value120: horizontal_value120,
                        ..PointerAxisValue::default()
                    },
                    vertical: PointerAxisValue {
                        continuous: vertical,
                        value120: vertical_value120,
                        ..PointerAxisValue::default()
                    },
                    source: None,
                },
            }))
        }
        NativeShellEvent::SeatModifiers {
            mods_depressed,
            mods_latched,
            mods_locked,
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
            }))
        }
        NativeShellEvent::PointerButton {
            surface,
            button,
            pressed,
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
            }))
        }
        NativeShellEvent::TouchDown {
            surface,
            id,
            x,
            y,
        } => {
            let seat = seat?;
            Some(Event::Touch(TouchEvent {
                surface: Some(surfaces.intern(surface)),
                kind: TouchEventKind::Down {
                    time: 0,
                    id,
                    position: (x, y),
                    serial: InputSerial::new(
                        seat.clone(),
                        map_state.last_serial,
                        InputSerialSource::TouchDown,
                    ),
                },
            }))
        }
        NativeShellEvent::TouchUp { id } => {
            let seat = seat?;
            Some(Event::Touch(TouchEvent {
                surface: None,
                kind: TouchEventKind::Up {
                    time: 0,
                    id,
                    serial: InputSerial::new(
                        seat.clone(),
                        map_state.last_serial,
                        InputSerialSource::TouchUp,
                    ),
                },
            }))
        }
        NativeShellEvent::TouchMotion { id, x, y } => Some(Event::Touch(TouchEvent {
            surface: None,
            kind: TouchEventKind::Motion {
                time: 0,
                id,
                position: (x, y),
            },
        })),
        NativeShellEvent::TouchCancel => Some(Event::Touch(TouchEvent {
            surface: None,
            kind: TouchEventKind::Cancelled,
        })),
        NativeShellEvent::GestureSwipeBegin {
            surface,
            fingers,
            time,
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
                },
            )))
        }
        NativeShellEvent::GestureSwipeUpdate { dx, dy, time } => {
            let surface = map_state
                .gesture_surface
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            Some(Event::PointerGesture(PointerGestureEvent::Swipe(
                PointerSwipeEvent::Update {
                    surface,
                    time,
                    delta: (dx, dy),
                },
            )))
        }
        NativeShellEvent::GestureSwipeEnd { cancelled, time } => {
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
                },
            )))
        }
        NativeShellEvent::GesturePinchBegin {
            surface,
            fingers,
            time,
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
                },
            )))
        }
        NativeShellEvent::GesturePinchUpdate {
            dx,
            dy,
            scale,
            rotation,
            time,
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
                },
            )))
        }
        NativeShellEvent::GesturePinchEnd { cancelled, time } => {
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
                },
            )))
        }
        NativeShellEvent::GestureHoldBegin {
            surface,
            fingers,
            time,
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
                },
            )))
        }
        NativeShellEvent::GestureHoldEnd { cancelled, time } => {
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
                },
            )))
        }
        NativeShellEvent::RelativePointer {
            utime,
            dx,
            dy,
            dx_unaccel,
            dy_unaccel,
        } => {
            let surface = map_state
                .pointer_focus
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            Some(Event::RelativePointer(RelativePointerEvent {
                surface,
                time_micros: utime,
                delta: (dx, dy),
                delta_unaccelerated: (dx_unaccel, dy_unaccel),
            }))
        }
        NativeShellEvent::TextInputEnter { surface } => {
            Some(Event::TextInput(crate::TextInputEvent::Entered {
                surface: surfaces.intern(surface),
            }))
        }
        NativeShellEvent::TextInputLeave { surface } => {
            Some(Event::TextInput(crate::TextInputEvent::Left {
                surface: surfaces.intern(surface),
            }))
        }
        NativeShellEvent::TextInputDone {
            surface,
            serial,
            commit,
            preedit,
            delete_before,
            delete_after,
        } => {
            let surface = surfaces.intern(surface);
            let delete_surrounding =
                if delete_before > 0 || delete_after > 0 {
                    Some(crate::TextInputDeleteSurrounding {
                        before_bytes: delete_before as usize,
                        after_bytes: delete_after as usize,
                    })
                } else {
                    None
                };
            let preedit = preedit.map(|text| crate::TextInputPreedit {
                text,
                cursor_range: None,
            });
            Some(Event::TextInput(crate::TextInputEvent::Done(
                crate::TextInputDone {
                    surface,
                    serial,
                    delete_surrounding,
                    commit,
                    preedit,
                },
            )))
        }
        NativeShellEvent::DndEnter {
            offer,
            surface,
            x,
            y,
            mimes,
        } => {
            map_state.dnd_surface = Some(surface);
            Some(Event::Dnd(DndEvent::Enter {
                offer: DndOfferId(offer),
                surface: surfaces.intern(surface),
                position: GeoLogicalPosition::new(x as i32, y as i32),
                mime_types: mimes,
                source_actions: DndActions::COPY | DndActions::MOVE,
            }))
        }
        NativeShellEvent::DndLeave { offer, surface } => {
            let surface = surface
                .or(map_state.dnd_surface)
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            map_state.dnd_surface = None;
            Some(Event::Dnd(DndEvent::Leave {
                offer: DndOfferId(offer),
                surface,
            }))
        }
        NativeShellEvent::DndMotion { offer, x, y } => {
            let surface = map_state
                .dnd_surface
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            Some(Event::Dnd(DndEvent::Motion {
                offer: DndOfferId(offer),
                surface,
                position: GeoLogicalPosition::new(x as i32, y as i32),
            }))
        }
        NativeShellEvent::DndDrop { offer } => {
            let surface = map_state
                .dnd_surface
                .map(|s| surfaces.intern(s))
                .unwrap_or(SurfaceId(0));
            Some(Event::Dnd(DndEvent::Drop {
                offer: DndOfferId(offer),
                surface,
                action: Some(DndAction::Copy),
            }))
        }
        NativeShellEvent::DndFinished { source, cancelled } => {
            if cancelled {
                Some(Event::Dnd(DndEvent::SourceCancelled {
                    source: DndSourceId(source),
                }))
            } else {
                Some(Event::Dnd(DndEvent::SourceFinished {
                    source: DndSourceId(source),
                    action: Some(DndAction::Copy),
                }))
            }
        }
        NativeShellEvent::PointerConstraint {
            surface,
            kind,
            active,
        } => {
            let constraint = match kind {
                1 => PointerConstraint::Confined,
                2 => PointerConstraint::Locked,
                _ => PointerConstraint::None,
            };
            Some(Event::PointerConstraint(PointerConstraintEvent {
                surface: surfaces.intern(surface),
                constraint,
                active,
            }))
        }
        // Still deferred: outputs, clipboard Selection events, activation.
        NativeShellEvent::TouchFrame
        | NativeShellEvent::OutputGeometry { .. }
        | NativeShellEvent::OutputMode { .. }
        | NativeShellEvent::OutputScale { .. }
        | NativeShellEvent::OutputDone { .. }
        | NativeShellEvent::SurfaceOutputEnter { .. }
        | NativeShellEvent::SurfaceOutputLeave { .. }
        | NativeShellEvent::Selection { .. }
        | NativeShellEvent::SelectionCancelled
        | NativeShellEvent::ActivationToken { .. } => None,
    }
}

/// Extract UTF-8 / keysym from a native key event without building a full
/// [`KeyboardEvent`].
pub fn map_native_key_text(
    event: &NativeShellEvent,
) -> Option<(u32, u32, bool, Option<&str>)> {
    match event {
        NativeShellEvent::SeatKeyboardKey {
            key,
            pressed,
            keysym,
            text,
        } => Some((*key, *keysym, *pressed, text.as_deref())),
        _ => None,
    }
}

/// Decode a Wayland/XKB modifier mask into public [`Modifiers`].
///
/// Bits follow the common libxkbcommon core mod indices (Shift/Caps/Ctrl/Alt/Num/Logo).
fn modifiers_from_xkb_mask(mask: u32) -> Modifiers {
    const SHIFT: u32 = 1 << 0;
    const CAPS: u32 = 1 << 1;
    const CTRL: u32 = 1 << 2;
    const ALT: u32 = 1 << 3;
    const NUM: u32 = 1 << 4;
    const LOGO: u32 = 1 << 6;
    Modifiers {
        shift: mask & SHIFT != 0,
        caps_lock: mask & CAPS != 0,
        ctrl: mask & CTRL != 0,
        alt: mask & ALT != 0,
        num_lock: mask & NUM != 0,
        logo: mask & LOGO != 0,
    }
}

/// Convenience: whether the event is a press that produced printable text.
pub fn native_key_text_pressed(event: &NativeShellEvent) -> Option<&str> {
    match event {
        NativeShellEvent::SeatKeyboardKey {
            pressed: true,
            text: Some(text),
            ..
        } if !text.is_empty() => Some(text.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::ToplevelState;
    use crate::SuggestedSize;

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
    fn extracts_key_text() {
        let event = NativeShellEvent::SeatKeyboardKey {
            key: 30,
            pressed: true,
            keysym: 0x61,
            text: Some("a".into()),
        };
        assert_eq!(native_key_text_pressed(&event), Some("a"));
        let (key, keysym, pressed, text) = map_native_key_text(&event).unwrap();
        assert_eq!((key, keysym, pressed, text), (30, 0x61, true, Some("a")));
        // Without seat, key events do not become public Event.
        let mut map = SurfaceIdMap::new();
        assert!(map_native_event(event, &mut map).is_none());
    }
}

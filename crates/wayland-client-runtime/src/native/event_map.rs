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
use crate::geometry::{LogicalPosition, LogicalSize};
use crate::input::{InputSerial, InputSerialSource};
use crate::native::shell::{NativeShellEvent, NativeSurfaceId};
use crate::surface::SurfaceId;
use crate::dnd::{DndAction, DndActions, DndEvent, DndOfferId, DndSourceId};
use crate::geometry::LogicalPosition as GeoLogicalPosition;
use crate::pointer_constraints::{PointerConstraint, PointerConstraintEvent};
use crate::output::{OutputEvent, OutputId, OutputInfo};
use crate::{
    LayerSurfaceEvent, PointerGestureEvent, PointerHoldEvent, PointerPinchEvent, PointerSwipeEvent,
    RelativePointerEvent, SeatEvent, SeatId, SeatInfo,
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
        NativeShellEvent::Presented {
            surface,
            tv_sec,
            tv_nsec,
            refresh_ns,
            seq,
            flags_bits,
            sync_output,
        } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::Presented {
                surface,
                tv_sec,
                tv_nsec,
                refresh_ns,
                seq,
                flags: flags_bits,
                sync_output: sync_output.map(OutputId::from_raw),
            }))
        }
        NativeShellEvent::PresentationDiscarded { surface } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::PresentationDiscarded {
                surface,
            }))
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
        NativeShellEvent::IdleNotify { id, idle } => {
            Some(Event::IdleNotify(crate::IdleNotifyEvent { id, idle }))
        }
        NativeShellEvent::ForeignExported { surface, handle } => {
            Some(Event::Foreign(crate::ForeignEvent::Exported {
                surface: surfaces.intern(surface),
                handle,
            }))
        }
        NativeShellEvent::ForeignImportedDestroyed { id } => {
            Some(Event::Foreign(crate::ForeignEvent::ImportedDestroyed { id }))
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
        NativeShellEvent::OutputDone { output } => {
            // Emit Updated with a minimal snapshot; full fields live in NativeShell::outputs().
            Some(Event::Output(OutputEvent::Updated(OutputInfo {
                id: OutputId::from_raw(output),
                name: None,
                description: None,
                make: String::new(),
                model: String::new(),
                logical_position: None,
                logical_size: None,
                scale_factor: 1,
                refresh_mhz: None,
            })))
        }
        NativeShellEvent::OutputRemoved { output } => {
            Some(Event::Output(OutputEvent::Removed(OutputId::from_raw(output))))
        }
        NativeShellEvent::SeatAdded {
            seat,
            name,
            has_keyboard,
            has_pointer,
            has_touch,
        } => Some(Event::Seat(SeatEvent::Added(SeatInfo {
            id: SeatId::from_raw(seat),
            name,
            has_keyboard,
            has_pointer,
            has_touch,
        }))),
        NativeShellEvent::SeatChanged {
            seat,
            name,
            has_keyboard,
            has_pointer,
            has_touch,
        } => Some(Event::Seat(SeatEvent::Changed(SeatInfo {
            id: SeatId::from_raw(seat),
            name,
            has_keyboard,
            has_pointer,
            has_touch,
        }))),
        NativeShellEvent::SeatRemoved { seat } => {
            Some(Event::Seat(SeatEvent::Removed(SeatId::from_raw(seat))))
        }
        NativeShellEvent::OutputGeometry {
            output,
            x,
            y,
            physical_width,
            physical_height,
            make,
            model,
        } => Some(Event::Output(OutputEvent::Updated(OutputInfo {
            id: OutputId::from_raw(output),
            name: None,
            description: None,
            make,
            model,
            logical_position: Some(LogicalPosition::new(x, y)),
            logical_size: (physical_width > 0 && physical_height > 0).then(|| {
                LogicalSize::new(physical_width as u32, physical_height as u32)
            }),
            scale_factor: 1,
            refresh_mhz: None,
        }))),
        NativeShellEvent::OutputScale { output, factor } => {
            Some(Event::Output(OutputEvent::Updated(OutputInfo {
                id: OutputId::from_raw(output),
                name: None,
                description: None,
                make: String::new(),
                model: String::new(),
                logical_position: None,
                logical_size: None,
                scale_factor: factor,
                refresh_mhz: None,
            })))
        }
        NativeShellEvent::SurfaceOutputEnter { surface, output } => {
            Some(Event::Surface(SurfaceEvent::OutputEnter {
                surface: surfaces.intern(surface),
                output: OutputId::from_raw(output),
            }))
        }
        NativeShellEvent::SurfaceOutputLeave { surface, output } => {
            Some(Event::Surface(SurfaceEvent::OutputLeave {
                surface: surfaces.intern(surface),
                output: OutputId::from_raw(output),
            }))
        }
        NativeShellEvent::DmabufFeedback { surface, feedback } => {
            Some(Event::Dmabuf(crate::dmabuf::DmabufEvent::Feedback {
                surface: surface.map(|s| surfaces.intern(s)),
                feedback,
            }))
        }
        NativeShellEvent::DmabufBufferCreated { id } => {
            Some(Event::Dmabuf(crate::dmabuf::DmabufEvent::BufferCreated { id }))
        }
        NativeShellEvent::DmabufBufferFailed => {
            Some(Event::Dmabuf(crate::dmabuf::DmabufEvent::BufferFailed))
        }
        NativeShellEvent::DmabufBufferReleased { id } => {
            Some(Event::Dmabuf(crate::dmabuf::DmabufEvent::BufferReleased { id }))
        }
        // ActivationToken is correlated in NativeRuntime::drain_events_into.
        // Selection / primary selection mime lists remain internal until apps
        // request a receive (same model as clipboard).
        // OutputMode is folded into OutputDone / NativeShell::outputs() snapshots.
        NativeShellEvent::OutputMode { .. }
        | NativeShellEvent::Selection { .. }
        | NativeShellEvent::SelectionCancelled
        | NativeShellEvent::PrimarySelection { .. }
        | NativeShellEvent::PrimarySelectionCancelled
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
            ..
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
#[path = "event_map_tests.rs"]
mod tests;

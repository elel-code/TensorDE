//! Top-level native → public event mapping.

use wayland_client::protocol::wl_seat::WlSeat;

use crate::event::Event;
use crate::native::shell::NativeShellEvent;

use super::{NativeEventMapState, SurfaceIdMap, gestures, input, surface, system, transfer};

fn category(event: &NativeShellEvent) -> &'static str {
    use NativeShellEvent::*;
    match event {
        ToplevelConfigure { .. }
        | ToplevelClose { .. }
        | ScaleFactorChanged { .. }
        | Frame { .. }
        | Presented { .. }
        | PresentationDiscarded { .. }
        | PopupConfigure { .. }
        | PopupDone { .. }
        | LayerConfigure { .. }
        | LayerClosed { .. }
        | SurfaceOutputEnter { .. }
        | SurfaceOutputLeave { .. } => "surface",
        SeatKeyboardEnter { .. }
        | SeatKeyboardLeave { .. }
        | SeatKeyboardKey { .. }
        | SeatModifiers { .. }
        | PointerEnter { .. }
        | PointerLeave { .. }
        | PointerMotion { .. }
        | PointerAxis { .. }
        | PointerButton { .. }
        | TouchDown { .. }
        | TouchUp { .. }
        | TouchMotion { .. }
        | TouchShape { .. }
        | TouchOrientation { .. }
        | TouchFrame { .. }
        | TouchCancel { .. } => "input",
        GestureSwipeBegin { .. }
        | GestureSwipeUpdate { .. }
        | GestureSwipeEnd { .. }
        | GesturePinchBegin { .. }
        | GesturePinchUpdate { .. }
        | GesturePinchEnd { .. }
        | GestureHoldBegin { .. }
        | GestureHoldEnd { .. }
        | RelativePointer { .. } => "gestures",
        TextInputEnter { .. }
        | TextInputLeave { .. }
        | TextInputDone { .. }
        | DndEnter { .. }
        | DndLeave { .. }
        | DndMotion { .. }
        | DndDrop { .. }
        | DndFinished { .. } => "transfer",
        PointerConstraint { .. }
        | OutputDone { .. }
        | OutputRemoved { .. }
        | OutputPowerMode { .. }
        | OutputPowerFailed { .. }
        | SeatAdded { .. }
        | SeatChanged { .. }
        | SeatRemoved { .. }
        | OutputGeometry { .. }
        | OutputScale { .. }
        | DmabufFeedback { .. }
        | DmabufBufferCreated { .. }
        | DmabufBufferFailed
        | DmabufBufferReleased { .. }
        | IdleNotify { .. }
        | ForeignExported { .. }
        | ForeignImportedDestroyed { .. } => "system",
        OutputMode { .. }
        | Selection { .. }
        | SelectionCancelled
        | PrimarySelection { .. }
        | PrimarySelectionCancelled
        | ActivationToken { .. } => "ignored",
    }
}

/// Convert one native shell event into a public crate event when possible.
pub fn map_native_event(event: NativeShellEvent, surfaces: &mut SurfaceIdMap) -> Option<Event> {
    map_native_event_full(event, surfaces, None, &mut NativeEventMapState::default())
}

/// Full mapping with seat + focus state (preferred for Fika merge).
pub fn map_native_event_full(
    event: NativeShellEvent,
    surfaces: &mut SurfaceIdMap,
    seat: Option<&WlSeat>,
    map_state: &mut NativeEventMapState,
) -> Option<Event> {
    match category(&event) {
        "surface" => surface::map(event, surfaces, seat, map_state),
        "input" => input::map(event, surfaces, seat, map_state),
        "gestures" => gestures::map(event, surfaces, seat, map_state),
        "transfer" => transfer::map(event, surfaces, seat, map_state),
        "system" => system::map(event, surfaces, seat, map_state),
        _ => None,
    }
}

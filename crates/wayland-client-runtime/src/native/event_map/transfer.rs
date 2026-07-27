//! Text-input and drag-and-drop mapping.

use wayland_client::protocol::wl_seat::WlSeat;

use crate::event::Event;
use crate::native::shell::NativeShellEvent;

use crate::dnd::{DndAction, DndActions, DndEvent, DndOfferId, DndSourceId};
use crate::geometry::LogicalPosition as GeoLogicalPosition;
use crate::surface::SurfaceId;

use super::{NativeEventMapState, SurfaceIdMap};

#[allow(unused_variables)]
pub(crate) fn map(
    event: NativeShellEvent,
    surfaces: &mut SurfaceIdMap,
    seat: Option<&WlSeat>,
    map_state: &mut NativeEventMapState,
) -> Option<Event> {
    match event {
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

        _ => unreachable!("event routed to wrong mapper"),
    }
}

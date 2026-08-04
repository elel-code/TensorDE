//! Seats, outputs, constraints, dmabuf, idle, and foreign mapping.

use wayland_client::protocol::wl_seat::WlSeat;

use crate::event::Event;
use crate::native::shell::NativeShellEvent;

use crate::geometry::{LogicalPosition, LogicalSize};
use crate::input::{SeatEvent, SeatId, SeatInfo};
use crate::output::{OutputEvent, OutputId, OutputInfo};
use crate::pointer_constraints::{PointerConstraint, PointerConstraintEvent};

use super::{NativeEventMapState, SurfaceIdMap};

#[allow(unused_variables)]
pub(crate) fn map(
    event: NativeShellEvent,
    surfaces: &mut SurfaceIdMap,
    seat: Option<&WlSeat>,
    map_state: &mut NativeEventMapState,
) -> Option<Event> {
    match event {
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
            Some(Event::Foreign(crate::ForeignEvent::ImportedDestroyed {
                id,
            }))
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
        NativeShellEvent::OutputRemoved { output } => Some(Event::Output(OutputEvent::Removed(
            OutputId::from_raw(output),
        ))),
        NativeShellEvent::OutputPowerMode { output, mode } => {
            Some(Event::OutputPower(crate::OutputPowerEvent::Mode {
                output: OutputId::from_raw(output),
                mode,
            }))
        }
        NativeShellEvent::OutputPowerFailed { output } => {
            Some(Event::OutputPower(crate::OutputPowerEvent::Failed {
                output: OutputId::from_raw(output),
            }))
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
            logical_size: (physical_width > 0 && physical_height > 0)
                .then(|| LogicalSize::new(physical_width as u32, physical_height as u32)),
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
        NativeShellEvent::DmabufFeedback { surface, feedback } => {
            Some(Event::Dmabuf(crate::dmabuf::DmabufEvent::Feedback {
                surface: surface.map(|s| surfaces.intern(s)),
                feedback,
            }))
        }
        NativeShellEvent::DmabufBufferCreated { id } => {
            Some(Event::Dmabuf(crate::dmabuf::DmabufEvent::BufferCreated {
                id,
            }))
        }
        NativeShellEvent::DmabufBufferFailed => {
            Some(Event::Dmabuf(crate::dmabuf::DmabufEvent::BufferFailed))
        }
        NativeShellEvent::DmabufBufferReleased { id } => {
            Some(Event::Dmabuf(crate::dmabuf::DmabufEvent::BufferReleased {
                id,
            }))
        }

        _ => unreachable!("event routed to wrong mapper"),
    }
}

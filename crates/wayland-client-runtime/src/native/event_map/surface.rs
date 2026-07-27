//! Surface, layer, popup, and presentation mapping.

use wayland_client::protocol::wl_seat::WlSeat;

use crate::event::Event;
use crate::native::shell::NativeShellEvent;

use crate::event::SurfaceEvent;
use crate::geometry::{LogicalPosition, LogicalSize};
use crate::output::OutputId;
use crate::LayerSurfaceEvent;

use super::{NativeEventMapState, SurfaceIdMap};

#[allow(unused_variables)]
pub(crate) fn map(
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

        _ => unreachable!("event routed to wrong mapper"),
    }
}

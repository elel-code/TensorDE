//! `wl_pointer` to typed scene-pointer conversion.

use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};

use crate::engine::scene::{
    ScenePointerEvent, ScenePointerEventKind, ScenePointerSource,
};

pub(super) fn scene_pointer_event(
    event: &PointerEvent,
    surface_id: u64,
    surface_size: [u32; 2],
    last_time_millis: &mut u32,
) -> ScenePointerEvent {
    let kind = match event.kind {
        PointerEventKind::Enter { serial } => ScenePointerEventKind::Enter { serial },
        PointerEventKind::Leave { serial } => ScenePointerEventKind::Leave { serial },
        PointerEventKind::Motion { time } => {
            *last_time_millis = time;
            ScenePointerEventKind::Motion
        }
        PointerEventKind::Press {
            time,
            button,
            serial,
        } => {
            *last_time_millis = time;
            ScenePointerEventKind::Button {
                button,
                pressed: true,
                serial,
            }
        }
        PointerEventKind::Release {
            time,
            button,
            serial,
        } => {
            *last_time_millis = time;
            ScenePointerEventKind::Button {
                button,
                pressed: false,
                serial,
            }
        }
        PointerEventKind::Axis {
            time,
            horizontal,
            vertical,
            ..
        } => {
            *last_time_millis = time;
            ScenePointerEventKind::Scroll {
                horizontal: horizontal.absolute,
                vertical: vertical.absolute,
            }
        }
    };
    ScenePointerEvent {
        source: ScenePointerSource::WaylandSurface,
        surface_id,
        time_millis: *last_time_millis,
        position: [event.position.0, event.position.1],
        surface_size,
        kind,
    }
}

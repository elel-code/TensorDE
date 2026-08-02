//! Shared native pointer events to typed scene-pointer conversion.

use wayland_client_runtime::{NativeShellEvent, NativeSurfaceId};

use crate::engine::scene::{ScenePointerEvent, ScenePointerEventKind, ScenePointerSource};

#[derive(Debug, Default)]
pub(super) struct PointerState {
    time_millis: u32,
    position: [f64; 2],
}

pub(super) fn scene_pointer_event(
    event: &NativeShellEvent,
    surface: NativeSurfaceId,
    surface_id: u64,
    surface_size: [u32; 2],
    state: &mut PointerState,
) -> Option<ScenePointerEvent> {
    let kind = match event {
        NativeShellEvent::PointerEnter {
            surface: event_surface,
            x,
            y,
            serial,
            ..
        } if *event_surface == surface => {
            state.position = [*x, *y];
            ScenePointerEventKind::Enter { serial: *serial }
        }
        NativeShellEvent::PointerLeave {
            surface: event_surface,
            serial,
            ..
        } if *event_surface == surface => ScenePointerEventKind::Leave { serial: *serial },
        NativeShellEvent::PointerMotion {
            surface: event_surface,
            x,
            y,
            time,
            ..
        } if *event_surface == surface => {
            state.time_millis = *time;
            state.position = [*x, *y];
            ScenePointerEventKind::Motion
        }
        NativeShellEvent::PointerButton {
            surface: event_surface,
            button,
            pressed,
            serial,
            time,
            ..
        } if event_surface.is_none() || *event_surface == Some(surface) => {
            state.time_millis = *time;
            ScenePointerEventKind::Button {
                button: *button,
                pressed: *pressed,
                serial: *serial,
            }
        }
        NativeShellEvent::PointerAxis {
            surface: event_surface,
            horizontal,
            vertical,
            time,
            ..
        } if event_surface.is_none() || *event_surface == Some(surface) => {
            state.time_millis = *time;
            ScenePointerEventKind::Scroll {
                horizontal: horizontal.continuous,
                vertical: vertical.continuous,
            }
        }
        _ => return None,
    };
    Some(ScenePointerEvent {
        source: ScenePointerSource::WaylandSurface,
        surface_id,
        time_millis: state.time_millis,
        position: state.position,
        surface_size,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_wire_facts_reach_scene_events_exactly() {
        let surface = NativeSurfaceId::from_raw(5);
        let mut state = PointerState::default();
        let motion = scene_pointer_event(
            &NativeShellEvent::PointerMotion {
                surface,
                x: 20.5,
                y: 10.25,
                time: 1_234,
                seat: Some(2),
            },
            surface,
            77,
            [1920, 1080],
            &mut state,
        )
        .expect("motion event");
        assert_eq!(motion.time_millis, 1_234);
        assert_eq!(motion.position, [20.5, 10.25]);

        let button = scene_pointer_event(
            &NativeShellEvent::PointerButton {
                surface: Some(surface),
                button: 0x110,
                pressed: true,
                serial: 0xaabb_ccdd,
                time: 1_250,
                seat: Some(2),
            },
            surface,
            77,
            [1920, 1080],
            &mut state,
        )
        .expect("button event");
        assert_eq!(button.time_millis, 1_250);
        assert!(matches!(
            button.kind,
            ScenePointerEventKind::Button {
                button: 0x110,
                pressed: true,
                serial: 0xaabb_ccdd,
            }
        ));
    }
}

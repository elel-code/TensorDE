use tensor_event::{
    DeviceEvent, TabletPadEvent, TabletPadRingEvent, TabletPadStripEvent, TabletToolAxesEvent,
    TabletToolButtonEvent, TabletToolId, TabletToolProximityEvent, TabletToolTipEvent,
};
use wayland_server::protocol::wl_surface::WlSurface;

use super::super::WaylandRuntime;
use crate::protocol::globals::tablet::tool::TabletTarget;

pub(super) fn inject_full_tablet_sequence(
    runtime: &mut WaylandRuntime,
    device: DeviceEvent,
    surface: WlSurface,
) {
    let target = || TabletTarget {
        surface: surface.clone(),
        origin: (0.0, 0.0).into(),
        location: (20.0, 30.0).into(),
        scale: 1.0,
    };
    runtime.state.protocol_globals.tablet.tool_proximity(
        TabletToolProximityEvent {
            id: TabletToolId::new(11),
            device: device.id,
            x: 0.25,
            y: 0.5,
            in_proximity: true,
            time_ns: 1_000_000,
        },
        Some(target()),
    );
    runtime.state.protocol_globals.tablet.tool_axes(
        TabletToolAxesEvent::new(
            TabletToolId::new(11),
            2_000_000,
            Some(0.3),
            Some(0.6),
            Some(0.5),
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        ),
        Some(target()),
    );
    runtime
        .state
        .protocol_globals
        .tablet
        .tool_tip(TabletToolTipEvent {
            id: TabletToolId::new(11),
            down: true,
            time_ns: 3_000_000,
        });
    runtime
        .state
        .protocol_globals
        .tablet
        .tool_button(TabletToolButtonEvent {
            id: TabletToolId::new(11),
            button: 0x14b,
            pressed: true,
            time_ns: 4_000_000,
        });
    for event in [
        TabletPadEvent::Button {
            device: device.id,
            button: 0,
            mode_group: 0,
            mode: 1,
            pressed: true,
            time_ns: 5_000_000,
        },
        TabletPadEvent::Ring(TabletPadRingEvent {
            device: device.id,
            index: 0,
            mode_group: 0,
            mode: 1,
            position: Some(45.0),
            finger: true,
            time_ns: 6_000_000,
        }),
        TabletPadEvent::Strip(TabletPadStripEvent {
            device: device.id,
            index: 0,
            mode_group: 0,
            mode: 1,
            position: Some(0.5),
            finger: true,
            time_ns: 7_000_000,
        }),
        TabletPadEvent::Dial {
            device: device.id,
            index: 0,
            mode_group: 0,
            mode: 1,
            delta_v120: 120,
            time_ns: 8_000_000,
        },
    ] {
        runtime
            .state
            .protocol_globals
            .tablet
            .pad_event(&runtime.state.display_handle, event);
    }
    runtime
        .state
        .protocol_globals
        .tablet
        .tool_button(TabletToolButtonEvent {
            id: TabletToolId::new(11),
            button: 0x14b,
            pressed: false,
            time_ns: 9_000_000,
        });
    runtime
        .state
        .protocol_globals
        .tablet
        .tool_tip(TabletToolTipEvent {
            id: TabletToolId::new(11),
            down: false,
            time_ns: 10_000_000,
        });
    runtime.state.protocol_globals.tablet.tool_proximity(
        TabletToolProximityEvent {
            id: TabletToolId::new(11),
            device: device.id,
            x: 0.3,
            y: 0.6,
            in_proximity: false,
            time_ns: 11_000_000,
        },
        None,
    );
}

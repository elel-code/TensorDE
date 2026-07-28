use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use tensor_event::{
    DeviceCapabilities, DeviceChange, DeviceEvent, DeviceGroupId, DeviceId, TabletPadDescriptor,
    TabletPadEvent, TabletPadGroupDescriptor, TabletToolCapabilities, TabletToolDescriptor,
    TabletToolId, TabletToolType,
};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::wp::tablet::zv2::client::{
    zwp_tablet_manager_v2, zwp_tablet_pad_dial_v2, zwp_tablet_pad_group_v2, zwp_tablet_pad_ring_v2,
    zwp_tablet_pad_strip_v2, zwp_tablet_pad_v2, zwp_tablet_seat_v2, zwp_tablet_tool_v2,
    zwp_tablet_v2,
};

use super::*;

mod events;
use events::inject_full_tablet_sequence;

#[derive(Debug, Eq, PartialEq)]
enum TabletStep {
    Bound,
    Snapshot { added: u8, done: u8, removed: u8 },
}

#[derive(Default)]
struct TabletClient {
    added: u8,
    done: u8,
    removed: u8,
    identity: Option<(u32, u32)>,
    bus_type: Option<u32>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for TabletClient {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(TabletClient: ignore wl_seat::WlSeat);
delegate_noop!(TabletClient: ignore zwp_tablet_manager_v2::ZwpTabletManagerV2);

impl Dispatch<zwp_tablet_seat_v2::ZwpTabletSeatV2, ()> for TabletClient {
    fn event(
        state: &mut Self,
        _: &zwp_tablet_seat_v2::ZwpTabletSeatV2,
        event: zwp_tablet_seat_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zwp_tablet_seat_v2::Event::TabletAdded { .. }) {
            state.added = state.added.saturating_add(1);
        }
    }

    wayland_client::event_created_child!(TabletClient, zwp_tablet_seat_v2::ZwpTabletSeatV2, [
        0 => (zwp_tablet_v2::ZwpTabletV2, ())
    ]);
}

impl Dispatch<zwp_tablet_v2::ZwpTabletV2, ()> for TabletClient {
    fn event(
        state: &mut Self,
        _: &zwp_tablet_v2::ZwpTabletV2,
        event: zwp_tablet_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_tablet_v2::Event::Id { vid, pid } => state.identity = Some((vid, pid)),
            zwp_tablet_v2::Event::Bustype { bustype } => {
                state.bus_type = bustype.into_result().ok().map(|value| value as u32)
            }
            zwp_tablet_v2::Event::Done => state.done = state.done.saturating_add(1),
            zwp_tablet_v2::Event::Removed => state.removed = state.removed.saturating_add(1),
            _ => {}
        }
    }
}

#[test]
fn tablet_discovery_is_grouped_removed_and_snapshotted_for_late_seats() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();

    let (steps_tx, steps_rx) = mpsc::sync_channel(1);
    let (advance_tx, advance_rx) = mpsc::sync_channel(0);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<TabletClient>(&connection).unwrap();
        let handle = queue.handle();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<zwp_tablet_manager_v2::ZwpTabletManagerV2, _, _>(&handle, 2..=2, ())
            .unwrap();
        let first = manager.get_tablet_seat(&seat, &handle, ());
        let mut state = TabletClient::default();
        queue.roundtrip(&mut state).unwrap();
        steps_tx.send(TabletStep::Bound).unwrap();

        advance_rx.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        assert_eq!(state.identity, Some((0x1234, 0x5678)));
        assert_eq!(state.bus_type, Some(3));
        send_snapshot(&steps_tx, &state);

        advance_rx.recv().unwrap();
        let second = manager.get_tablet_seat(&seat, &handle, ());
        queue.roundtrip(&mut state).unwrap();
        send_snapshot(&steps_tx, &state);

        advance_rx.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        send_snapshot(&steps_tx, &state);

        first.destroy();
        second.destroy();
        manager.destroy();
        seat.release();
    });

    assert_eq!(
        dispatch_until_step(&mut runtime, &steps_rx),
        TabletStep::Bound
    );
    let event = tablet_device(DeviceChange::Added);
    runtime
        .state
        .protocol_globals
        .tablet
        .device_changed(&runtime.state.display_handle, event);
    runtime.state.display_handle.flush_clients().unwrap();
    advance_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until_step(&mut runtime, &steps_rx),
        TabletStep::Snapshot {
            added: 1,
            done: 1,
            removed: 0,
        }
    );

    advance_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until_step(&mut runtime, &steps_rx),
        TabletStep::Snapshot {
            added: 2,
            done: 2,
            removed: 0,
        }
    );

    runtime.state.protocol_globals.tablet.device_changed(
        &runtime.state.display_handle,
        tablet_device(DeviceChange::Removed),
    );
    runtime.state.display_handle.flush_clients().unwrap();
    advance_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until_step(&mut runtime, &steps_rx),
        TabletStep::Snapshot {
            added: 2,
            done: 2,
            removed: 2,
        }
    );
    client.join().unwrap();
}

fn tablet_device(change: DeviceChange) -> DeviceEvent {
    DeviceEvent {
        id: DeviceId::new(7),
        group: DeviceGroupId::new(3),
        bus_type: 3,
        vendor_id: 0x1234,
        product_id: 0x5678,
        capabilities: DeviceCapabilities {
            tablet: true,
            ..DeviceCapabilities::empty()
        },
        change,
    }
}

fn send_snapshot(steps: &mpsc::SyncSender<TabletStep>, state: &TabletClient) {
    steps
        .send(TabletStep::Snapshot {
            added: state.added,
            done: state.done,
            removed: state.removed,
        })
        .unwrap();
}

fn dispatch_until_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<TabletStep>,
) -> TabletStep {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("tablet client did not complete before the dispatch limit");
}

#[derive(Default)]
struct TabletTreeClient {
    tablets: u8,
    tools: u8,
    tool_done: u8,
    tool_capabilities: u8,
    pads: u8,
    pad_done: u8,
    groups: u8,
    group_done: u8,
    rings: u8,
    strips: u8,
    dials: u8,
    proximity_in: u8,
    proximity_out: u8,
    motion: u8,
    pressure: u8,
    down: u8,
    up: u8,
    tool_buttons: u8,
    tool_frames: u8,
    pad_enter: u8,
    pad_leave: u8,
    pad_buttons: u8,
    mode_switches: u8,
    ring_frames: u8,
    strip_frames: u8,
    dial_frames: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TreeSnapshot {
    tablets: u8,
    tools: u8,
    tool_done: u8,
    tool_capabilities: u8,
    pads: u8,
    pad_done: u8,
    groups: u8,
    group_done: u8,
    rings: u8,
    strips: u8,
    dials: u8,
    proximity_in: u8,
    proximity_out: u8,
    motion: u8,
    pressure: u8,
    down: u8,
    up: u8,
    tool_buttons: u8,
    tool_frames: u8,
    pad_enter: u8,
    pad_leave: u8,
    pad_buttons: u8,
    mode_switches: u8,
    ring_frames: u8,
    strip_frames: u8,
    dial_frames: u8,
}

impl TabletTreeClient {
    fn snapshot(&self) -> TreeSnapshot {
        TreeSnapshot {
            tablets: self.tablets,
            tools: self.tools,
            tool_done: self.tool_done,
            tool_capabilities: self.tool_capabilities,
            pads: self.pads,
            pad_done: self.pad_done,
            groups: self.groups,
            group_done: self.group_done,
            rings: self.rings,
            strips: self.strips,
            dials: self.dials,
            proximity_in: self.proximity_in,
            proximity_out: self.proximity_out,
            motion: self.motion,
            pressure: self.pressure,
            down: self.down,
            up: self.up,
            tool_buttons: self.tool_buttons,
            tool_frames: self.tool_frames,
            pad_enter: self.pad_enter,
            pad_leave: self.pad_leave,
            pad_buttons: self.pad_buttons,
            mode_switches: self.mode_switches,
            ring_frames: self.ring_frames,
            strip_frames: self.strip_frames,
            dial_frames: self.dial_frames,
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for TabletTreeClient {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(TabletTreeClient: ignore wl_seat::WlSeat);
delegate_noop!(TabletTreeClient: ignore wl_compositor::WlCompositor);
delegate_noop!(TabletTreeClient: ignore wl_surface::WlSurface);
delegate_noop!(TabletTreeClient: ignore zwp_tablet_manager_v2::ZwpTabletManagerV2);
delegate_noop!(TabletTreeClient: ignore zwp_tablet_v2::ZwpTabletV2);

impl Dispatch<zwp_tablet_seat_v2::ZwpTabletSeatV2, ()> for TabletTreeClient {
    fn event(
        state: &mut Self,
        _: &zwp_tablet_seat_v2::ZwpTabletSeatV2,
        event: zwp_tablet_seat_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_tablet_seat_v2::Event::TabletAdded { .. } => {
                state.tablets = state.tablets.saturating_add(1)
            }
            zwp_tablet_seat_v2::Event::ToolAdded { .. } => {
                state.tools = state.tools.saturating_add(1)
            }
            zwp_tablet_seat_v2::Event::PadAdded { .. } => state.pads = state.pads.saturating_add(1),
            _ => {}
        }
    }

    wayland_client::event_created_child!(TabletTreeClient, zwp_tablet_seat_v2::ZwpTabletSeatV2, [
        0 => (zwp_tablet_v2::ZwpTabletV2, ()),
        1 => (zwp_tablet_tool_v2::ZwpTabletToolV2, ()),
        2 => (zwp_tablet_pad_v2::ZwpTabletPadV2, ())
    ]);
}

impl Dispatch<zwp_tablet_tool_v2::ZwpTabletToolV2, ()> for TabletTreeClient {
    fn event(
        state: &mut Self,
        _: &zwp_tablet_tool_v2::ZwpTabletToolV2,
        event: zwp_tablet_tool_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_tablet_tool_v2::Event::Capability { .. } => {
                state.tool_capabilities = state.tool_capabilities.saturating_add(1)
            }
            zwp_tablet_tool_v2::Event::Done => state.tool_done = state.tool_done.saturating_add(1),
            zwp_tablet_tool_v2::Event::ProximityIn { .. } => {
                state.proximity_in = state.proximity_in.saturating_add(1)
            }
            zwp_tablet_tool_v2::Event::ProximityOut => {
                state.proximity_out = state.proximity_out.saturating_add(1)
            }
            zwp_tablet_tool_v2::Event::Motion { .. } => {
                state.motion = state.motion.saturating_add(1)
            }
            zwp_tablet_tool_v2::Event::Pressure { .. } => {
                state.pressure = state.pressure.saturating_add(1)
            }
            zwp_tablet_tool_v2::Event::Down { .. } => state.down = state.down.saturating_add(1),
            zwp_tablet_tool_v2::Event::Up => state.up = state.up.saturating_add(1),
            zwp_tablet_tool_v2::Event::Button { .. } => {
                state.tool_buttons = state.tool_buttons.saturating_add(1)
            }
            zwp_tablet_tool_v2::Event::Frame { .. } => {
                state.tool_frames = state.tool_frames.saturating_add(1)
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_tablet_pad_v2::ZwpTabletPadV2, ()> for TabletTreeClient {
    fn event(
        state: &mut Self,
        _: &zwp_tablet_pad_v2::ZwpTabletPadV2,
        event: zwp_tablet_pad_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_tablet_pad_v2::Event::Group { .. } => state.groups = state.groups.saturating_add(1),
            zwp_tablet_pad_v2::Event::Done => state.pad_done = state.pad_done.saturating_add(1),
            zwp_tablet_pad_v2::Event::Enter { .. } => {
                state.pad_enter = state.pad_enter.saturating_add(1)
            }
            zwp_tablet_pad_v2::Event::Leave { .. } => {
                state.pad_leave = state.pad_leave.saturating_add(1)
            }
            zwp_tablet_pad_v2::Event::Button { .. } => {
                state.pad_buttons = state.pad_buttons.saturating_add(1)
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(TabletTreeClient, zwp_tablet_pad_v2::ZwpTabletPadV2, [
        0 => (zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2, ())
    ]);
}

impl Dispatch<zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2, ()> for TabletTreeClient {
    fn event(
        state: &mut Self,
        _: &zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2,
        event: zwp_tablet_pad_group_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_tablet_pad_group_v2::Event::Ring { .. } => {
                state.rings = state.rings.saturating_add(1)
            }
            zwp_tablet_pad_group_v2::Event::Strip { .. } => {
                state.strips = state.strips.saturating_add(1)
            }
            zwp_tablet_pad_group_v2::Event::Dial { .. } => {
                state.dials = state.dials.saturating_add(1)
            }
            zwp_tablet_pad_group_v2::Event::Done => {
                state.group_done = state.group_done.saturating_add(1)
            }
            zwp_tablet_pad_group_v2::Event::ModeSwitch { .. } => {
                state.mode_switches = state.mode_switches.saturating_add(1)
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(TabletTreeClient, zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2, [
        1 => (zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2, ()),
        2 => (zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2, ()),
        6 => (zwp_tablet_pad_dial_v2::ZwpTabletPadDialV2, ())
    ]);
}

impl Dispatch<zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2, ()> for TabletTreeClient {
    fn event(
        state: &mut Self,
        _: &zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2,
        event: zwp_tablet_pad_ring_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zwp_tablet_pad_ring_v2::Event::Frame { .. }) {
            state.ring_frames = state.ring_frames.saturating_add(1);
        }
    }
}

impl Dispatch<zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2, ()> for TabletTreeClient {
    fn event(
        state: &mut Self,
        _: &zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2,
        event: zwp_tablet_pad_strip_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zwp_tablet_pad_strip_v2::Event::Frame { .. }) {
            state.strip_frames = state.strip_frames.saturating_add(1);
        }
    }
}

impl Dispatch<zwp_tablet_pad_dial_v2::ZwpTabletPadDialV2, ()> for TabletTreeClient {
    fn event(
        state: &mut Self,
        _: &zwp_tablet_pad_dial_v2::ZwpTabletPadDialV2,
        event: zwp_tablet_pad_dial_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zwp_tablet_pad_dial_v2::Event::Frame { .. }) {
            state.dial_frames = state.dial_frames.saturating_add(1);
        }
    }
}

#[test]
fn tablet_tool_and_full_pad_topology_reach_the_wire() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (ready_tx, ready_rx) = mpsc::sync_channel(0);
    let (advance_tx, advance_rx) = mpsc::sync_channel(0);
    let (snapshot_tx, snapshot_rx) = mpsc::sync_channel(0);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<TabletTreeClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<zwp_tablet_manager_v2::ZwpTabletManagerV2, _, _>(&handle, 2..=2, ())
            .unwrap();
        let tablet_seat = manager.get_tablet_seat(&seat, &handle, ());
        let surface = compositor.create_surface(&handle, ());
        let mut state = TabletTreeClient::default();
        queue.roundtrip(&mut state).unwrap();
        ready_tx.send(surface.id().protocol_id()).unwrap();
        advance_rx.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        snapshot_tx.send(state.snapshot()).unwrap();
        advance_rx.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        snapshot_tx.send(state.snapshot()).unwrap();
        surface.destroy();
        tablet_seat.destroy();
        manager.destroy();
        seat.release();
    });

    let surface_protocol_id = dispatch_until_signal(&mut runtime, &ready_rx);
    let device = tablet_device(DeviceChange::Added);
    runtime
        .state
        .protocol_globals
        .tablet
        .device_changed(&runtime.state.display_handle, device);
    runtime.state.protocol_globals.tablet.add_tool(
        &runtime.state.display_handle,
        TabletToolDescriptor {
            id: TabletToolId::new(11),
            device: device.id,
            hardware_serial: 12,
            hardware_id: 13,
            tool_type: TabletToolType::Pen,
            capabilities: TabletToolCapabilities::from_bits(
                TabletToolCapabilities::PRESSURE | TabletToolCapabilities::TILT,
            ),
        },
    );
    runtime.state.protocol_globals.tablet.pad_event(
        &runtime.state.display_handle,
        TabletPadEvent::Added(TabletPadDescriptor {
            device: device.id,
            buttons: 4,
            rings: 1,
            strips: 1,
            dials: 1,
            groups: 1,
        }),
    );
    runtime.state.protocol_globals.tablet.pad_event(
        &runtime.state.display_handle,
        TabletPadEvent::Group(TabletPadGroupDescriptor {
            device: device.id,
            index: 0,
            modes: 2,
            current_mode: 0,
            buttons: 0b1111,
            rings: 1,
            strips: 1,
            dials: 1,
            final_group: true,
        }),
    );
    runtime.state.display_handle.flush_clients().unwrap();
    advance_tx.send(()).unwrap();
    let snapshot = dispatch_until_tree_snapshot(&mut runtime, &snapshot_rx);
    assert_eq!(
        snapshot,
        TreeSnapshot {
            tablets: 1,
            tools: 1,
            tool_done: 1,
            tool_capabilities: 2,
            pads: 1,
            pad_done: 1,
            groups: 1,
            group_done: 1,
            rings: 1,
            strips: 1,
            dials: 1,
            proximity_in: 0,
            proximity_out: 0,
            motion: 0,
            pressure: 0,
            down: 0,
            up: 0,
            tool_buttons: 0,
            tool_frames: 0,
            pad_enter: 0,
            pad_leave: 0,
            pad_buttons: 0,
            mode_switches: 0,
            ring_frames: 0,
            strip_frames: 0,
            dial_frames: 0,
        }
    );

    let server_client = runtime
        .state
        .protocol_globals
        .tablet
        .client_for_tool(&runtime.state.display_handle, TabletToolId::new(11))
        .unwrap();
    let surface = server_client
        .object_from_protocol_id::<wayland_server::protocol::wl_surface::WlSurface>(
            &runtime.state.display_handle,
            surface_protocol_id,
        )
        .unwrap();
    inject_full_tablet_sequence(&mut runtime, device, surface);
    runtime.state.display_handle.flush_clients().unwrap();
    advance_tx.send(()).unwrap();
    let snapshot = dispatch_until_tree_snapshot(&mut runtime, &snapshot_rx);
    assert_eq!(
        snapshot,
        TreeSnapshot {
            tablets: 1,
            tools: 1,
            tool_done: 1,
            tool_capabilities: 2,
            pads: 1,
            pad_done: 1,
            groups: 1,
            group_done: 1,
            rings: 1,
            strips: 1,
            dials: 1,
            proximity_in: 1,
            proximity_out: 1,
            motion: 2,
            pressure: 1,
            down: 1,
            up: 1,
            tool_buttons: 2,
            tool_frames: 6,
            pad_enter: 1,
            pad_leave: 1,
            pad_buttons: 1,
            mode_switches: 2,
            ring_frames: 1,
            strip_frames: 1,
            dial_frames: 1,
        }
    );
    client.join().unwrap();
}

fn dispatch_until_signal(runtime: &mut WaylandRuntime, signal: &mpsc::Receiver<u32>) -> u32 {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(value) = signal.try_recv() {
            return value;
        }
    }
    panic!("tablet client did not bind before the dispatch limit");
}

fn dispatch_until_tree_snapshot(
    runtime: &mut WaylandRuntime,
    snapshots: &mpsc::Receiver<TreeSnapshot>,
) -> TreeSnapshot {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(snapshot) = snapshots.try_recv() {
            return snapshot;
        }
    }
    panic!("tablet tree snapshot did not arrive before the dispatch limit");
}

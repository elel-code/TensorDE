use std::{
    collections::HashSet, os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration,
};

use super::{super::*, tablet_device};
use crate::protocol::{
    cursor::CursorImage,
    globals::{
        compositor::{get_role, give_role, with_states},
        seat::CursorSurfaceState,
        tablet::tool::TabletTarget,
    },
};
use tensor_event::{
    TabletToolCapabilities, TabletToolDescriptor, TabletToolId, TabletToolProximityEvent,
    TabletToolType,
};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_compositor, wl_output, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::wp::{
    single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
    tablet::zv2::client::{
        zwp_tablet_manager_v2, zwp_tablet_seat_v2, zwp_tablet_tool_v2, zwp_tablet_v2,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CursorStep {
    Ready {
        cursor: u32,
        other: u32,
        conflict: u32,
    },
    Proximity(u32),
    InitialCursor {
        enters: u8,
        leaves: u8,
    },
    CurrentCursorUpdate,
    StaleCursor,
    Detached {
        enters: u8,
        leaves: u8,
    },
    Restored {
        enters: u8,
        leaves: u8,
    },
    CursorDestroyed,
    RoleRejected,
}

#[derive(Clone, Copy, Debug)]
enum CursorAction {
    InitialCursor,
    CurrentCursorUpdate,
    StaleCursor,
    Detach,
    Restore,
    DestroyCursor,
    AwaitProximity,
    Conflict,
}

#[derive(Default)]
struct TabletCursorClient {
    tool: Option<zwp_tablet_tool_v2::ZwpTabletToolV2>,
    proximity_serial: Option<u32>,
    cursor_surface: Option<wayland_client::backend::ObjectId>,
    cursor_outputs: HashSet<wayland_client::backend::ObjectId>,
    cursor_enters: u8,
    cursor_leaves: u8,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for TabletCursorClient {
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

impl Dispatch<zwp_tablet_seat_v2::ZwpTabletSeatV2, ()> for TabletCursorClient {
    fn event(
        state: &mut Self,
        _: &zwp_tablet_seat_v2::ZwpTabletSeatV2,
        event: zwp_tablet_seat_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_tablet_seat_v2::Event::ToolAdded { id } = event {
            state.tool = Some(id);
        }
    }

    wayland_client::event_created_child!(TabletCursorClient, zwp_tablet_seat_v2::ZwpTabletSeatV2, [
        0 => (zwp_tablet_v2::ZwpTabletV2, ()),
        1 => (zwp_tablet_tool_v2::ZwpTabletToolV2, ())
    ]);
}

impl Dispatch<zwp_tablet_tool_v2::ZwpTabletToolV2, ()> for TabletCursorClient {
    fn event(
        state: &mut Self,
        _: &zwp_tablet_tool_v2::ZwpTabletToolV2,
        event: zwp_tablet_tool_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_tablet_tool_v2::Event::ProximityIn { serial, .. } = event {
            state.proximity_serial = Some(serial);
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for TabletCursorClient {
    fn event(
        state: &mut Self,
        surface: &wl_surface::WlSurface,
        event: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if state.cursor_surface.as_ref() != Some(&surface.id()) {
            return;
        }
        match event {
            wl_surface::Event::Enter { output } => {
                state.cursor_outputs.insert(output.id());
                state.cursor_enters = state.cursor_enters.saturating_add(1);
            }
            wl_surface::Event::Leave { output } => {
                state.cursor_outputs.remove(&output.id());
                state.cursor_leaves = state.cursor_leaves.saturating_add(1);
            }
            _ => {}
        }
    }
}

delegate_noop!(TabletCursorClient: ignore wl_buffer::WlBuffer);
delegate_noop!(TabletCursorClient: ignore wl_compositor::WlCompositor);
delegate_noop!(TabletCursorClient: ignore wl_output::WlOutput);
delegate_noop!(TabletCursorClient: ignore wl_seat::WlSeat);
delegate_noop!(TabletCursorClient: ignore wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1);
delegate_noop!(TabletCursorClient: ignore zwp_tablet_manager_v2::ZwpTabletManagerV2);
delegate_noop!(TabletCursorClient: ignore zwp_tablet_v2::ZwpTabletV2);

#[test]
fn tablet_cursor_wire_enforces_serial_role_and_surface_lifetime() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(0);
    let (action_tx, action_rx) = mpsc::sync_channel(0);
    let client = spawn_tablet_cursor_client(socket_path, step_tx, action_rx);

    let CursorStep::Ready {
        cursor,
        other,
        conflict,
    } = dispatch_until_cursor_step(&mut runtime, &step_rx)
    else {
        panic!("tablet cursor client did not publish its surfaces");
    };
    let device = tablet_device(tensor_event::DeviceChange::Added);
    runtime
        .state
        .protocol_globals
        .tablet
        .device_changed(&runtime.state.display_handle, device);
    let tool_id = TabletToolId::new(41);
    runtime.state.protocol_globals.tablet.add_tool(
        &runtime.state.display_handle,
        TabletToolDescriptor {
            id: tool_id,
            device: device.id,
            hardware_serial: 42,
            hardware_id: 43,
            tool_type: TabletToolType::Pen,
            capabilities: TabletToolCapabilities::from_bits(0),
        },
    );
    let server_client = runtime
        .state
        .protocol_globals
        .tablet
        .client_for_tool(&runtime.state.display_handle, tool_id)
        .expect("client has a live tablet tool");
    let cursor_surface = server_surface(&runtime, &server_client, cursor);
    let other_surface = server_surface(&runtime, &server_client, other);
    let conflict_surface = server_surface(&runtime, &server_client, conflict);
    give_role(&conflict_surface, "tablet_cursor_wire_conflict").unwrap();

    let location = tensor_util::LogicalPoint::from((20.0, 30.0));
    runtime.state.cursor.note_tablet_activity(tool_id, location);
    runtime.state.protocol_globals.tablet.tool_proximity(
        TabletToolProximityEvent {
            id: tool_id,
            device: device.id,
            x: 0.25,
            y: 0.5,
            in_proximity: true,
            time_ns: 1_000_000,
        },
        Some(TabletTarget {
            surface: cursor_surface.clone(),
            origin: (0.0, 0.0).into(),
            location,
            scale: 1.0,
        }),
    );
    runtime.state.display_handle.flush_clients().unwrap();
    let CursorStep::Proximity(_) = dispatch_until_cursor_step(&mut runtime, &step_rx) else {
        panic!("tablet cursor client did not receive proximity");
    };

    action_tx.send(CursorAction::InitialCursor).unwrap();
    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::InitialCursor {
            enters: 1,
            leaves: 0,
        }
    );
    assert_eq!(get_role(&cursor_surface), Some("zwp_tablet_tool_v2_cursor"));
    assert_eq!(
        cursor_hotspot(&cursor_surface),
        tensor_util::Point::new(5, 7)
    );
    assert!(
        runtime
            .state
            .cursor
            .tablet_uses_surface(tool_id, &cursor_surface)
    );

    action_tx.send(CursorAction::CurrentCursorUpdate).unwrap();
    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::CurrentCursorUpdate
    );
    assert_eq!(
        cursor_hotspot(&cursor_surface),
        tensor_util::Point::new(9, 11)
    );

    action_tx.send(CursorAction::Detach).unwrap();
    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::Detached {
            enters: 1,
            leaves: 1,
        }
    );
    assert!(
        runtime
            .state
            .cursor
            .tablet_image_matches(tool_id, &CursorImage::Hidden)
    );

    action_tx.send(CursorAction::Restore).unwrap();
    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::Restored {
            enters: 2,
            leaves: 1,
        }
    );
    assert!(
        runtime
            .state
            .cursor
            .tablet_uses_surface(tool_id, &cursor_surface)
    );

    action_tx.send(CursorAction::StaleCursor).unwrap();
    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::StaleCursor
    );
    assert_eq!(get_role(&other_surface), None);
    assert!(
        runtime
            .state
            .cursor
            .tablet_uses_surface(tool_id, &cursor_surface)
    );

    action_tx.send(CursorAction::DestroyCursor).unwrap();
    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::CursorDestroyed
    );
    assert!(
        runtime
            .state
            .cursor
            .tablet_image_matches(tool_id, &CursorImage::default_named())
    );

    runtime.state.cursor.note_tablet_activity(tool_id, location);
    runtime.state.protocol_globals.tablet.tool_proximity(
        TabletToolProximityEvent {
            id: tool_id,
            device: device.id,
            x: 0.25,
            y: 0.5,
            in_proximity: true,
            time_ns: 2_000_000,
        },
        Some(TabletTarget {
            surface: other_surface,
            origin: (0.0, 0.0).into(),
            location,
            scale: 1.0,
        }),
    );
    runtime.state.display_handle.flush_clients().unwrap();
    action_tx.send(CursorAction::AwaitProximity).unwrap();
    assert!(matches!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::Proximity(_)
    ));
    action_tx.send(CursorAction::Conflict).unwrap();
    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::RoleRejected
    );
    assert_eq!(
        get_role(&conflict_surface),
        Some("tablet_cursor_wire_conflict")
    );
    client.join().unwrap();
}

fn spawn_tablet_cursor_client(
    socket_path: PathBuf,
    steps: mpsc::SyncSender<CursorStep>,
    actions: mpsc::Receiver<CursorAction>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<TabletCursorClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let output = globals
            .bind::<wl_output::WlOutput, _, _>(&handle, 1..=4, ())
            .unwrap();
        let manager = globals
            .bind::<zwp_tablet_manager_v2::ZwpTabletManagerV2, _, _>(&handle, 2..=2, ())
            .unwrap();
        let single_pixel = globals
            .bind::<wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let tablet_seat = manager.get_tablet_seat(&seat, &handle, ());
        let cursor = compositor.create_surface(&handle, ());
        let cursor_buffer = single_pixel.create_u32_rgba_buffer(
            u32::MAX,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            &handle,
            (),
        );
        cursor.attach(Some(&cursor_buffer), 0, 0);
        cursor.damage_buffer(0, 0, 1, 1);
        cursor.commit();
        let other = compositor.create_surface(&handle, ());
        let conflict = compositor.create_surface(&handle, ());
        let mut state = TabletCursorClient {
            cursor_surface: Some(cursor.id()),
            ..TabletCursorClient::default()
        };
        queue.roundtrip(&mut state).unwrap();
        steps
            .send(CursorStep::Ready {
                cursor: cursor.id().protocol_id(),
                other: other.id().protocol_id(),
                conflict: conflict.id().protocol_id(),
            })
            .unwrap();
        while state.proximity_serial.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let mut serial = state.proximity_serial.unwrap();
        steps.send(CursorStep::Proximity(serial)).unwrap();
        for action in actions {
            let tool = state.tool.as_ref().expect("tool accompanies proximity");
            match action {
                CursorAction::InitialCursor => tool.set_cursor(serial, Some(&cursor), 5, 7),
                CursorAction::CurrentCursorUpdate => {
                    tool.set_cursor(stale_serial(serial), Some(&cursor), 9, 11)
                }
                CursorAction::StaleCursor => {
                    tool.set_cursor(stale_serial(serial), Some(&other), 13, 15)
                }
                CursorAction::Detach => tool.set_cursor(serial, None, 0, 0),
                CursorAction::Restore => tool.set_cursor(serial, Some(&cursor), 9, 11),
                CursorAction::DestroyCursor => cursor.destroy(),
                CursorAction::AwaitProximity => {
                    let previous = serial;
                    while state.proximity_serial == Some(previous) {
                        queue.blocking_dispatch(&mut state).unwrap();
                    }
                    serial = state.proximity_serial.expect("new proximity has a serial");
                    steps.send(CursorStep::Proximity(serial)).unwrap();
                    continue;
                }
                CursorAction::Conflict => {
                    tool.set_cursor(serial, Some(&conflict), 17, 19);
                    let rejected = queue.roundtrip(&mut state).is_err();
                    assert!(rejected, "a conflicting wl_surface role must be fatal");
                    steps.send(CursorStep::RoleRejected).unwrap();
                    return;
                }
            }
            queue.roundtrip(&mut state).unwrap();
            let step = match action {
                CursorAction::InitialCursor => CursorStep::InitialCursor {
                    enters: state.cursor_enters,
                    leaves: state.cursor_leaves,
                },
                CursorAction::CurrentCursorUpdate => CursorStep::CurrentCursorUpdate,
                CursorAction::StaleCursor => CursorStep::StaleCursor,
                CursorAction::Detach => CursorStep::Detached {
                    enters: state.cursor_enters,
                    leaves: state.cursor_leaves,
                },
                CursorAction::Restore => CursorStep::Restored {
                    enters: state.cursor_enters,
                    leaves: state.cursor_leaves,
                },
                CursorAction::DestroyCursor => CursorStep::CursorDestroyed,
                CursorAction::AwaitProximity | CursorAction::Conflict => unreachable!(),
            };
            steps.send(step).unwrap();
        }
        drop((
            cursor_buffer,
            single_pixel,
            tablet_seat,
            manager,
            seat,
            output,
            other,
            conflict,
        ));
    })
}

fn server_surface(
    runtime: &WaylandRuntime,
    client: &wayland_server::Client,
    protocol_id: u32,
) -> wayland_server::protocol::wl_surface::WlSurface {
    client
        .object_from_protocol_id(&runtime.state.display_handle, protocol_id)
        .expect("client surface is live")
}

fn cursor_hotspot(surface: &wayland_server::protocol::wl_surface::WlSurface) -> tensor_util::Point {
    with_states(surface, |states| {
        states
            .data_map
            .get::<std::sync::Mutex<CursorSurfaceState>>()
            .expect("cursor surface has cursor state")
            .lock()
            .unwrap()
            .hotspot
    })
}

const fn stale_serial(serial: u32) -> u32 {
    serial.wrapping_add(1)
}

fn dispatch_until_cursor_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<CursorStep>,
) -> CursorStep {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("tablet cursor wire step did not arrive before the dispatch limit");
}

use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_buffer, wl_compositor, wl_data_device, wl_data_device_manager, wl_data_source,
        wl_pointer, wl_registry, wl_seat, wl_surface,
    },
};
use wayland_protocols::{
    wp::{
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
        viewporter::client::{wp_viewport, wp_viewporter},
    },
    xdg::{
        dialog::v1::client::{xdg_dialog_v1, xdg_wm_dialog_v1},
        shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
        toplevel_drag::v1::client::{xdg_toplevel_drag_manager_v1, xdg_toplevel_drag_v1},
    },
};

use crate::ecs::{ViewId, ViewPlacement};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DragStep {
    Ready,
    Active { enters: u32 },
    DialogUpdated { enters: u32 },
    Moved { enters: u32 },
    Ended { cancelled: bool, dropped: bool },
}

#[derive(Default)]
struct DragClient {
    configured: bool,
    pointer: Option<wl_pointer::WlPointer>,
    button_serial: Option<u32>,
    dnd_enters: u32,
    source_cancelled: bool,
    drop_performed: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for DragClient {
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

impl Dispatch<wl_seat::WlSeat, ()> for DragClient {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
            && capabilities.contains(wl_seat::Capability::Pointer)
            && state.pointer.is_none()
        {
            state.pointer = Some(seat.get_pointer(queue, ()));
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for DragClient {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Button {
            serial,
            state: WEnum::Value(wl_pointer::ButtonState::Pressed),
            ..
        } = event
        {
            state.button_serial = Some(serial);
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for DragClient {
    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_data_device::Event::Enter { .. }) {
            state.dnd_enters = state.dnd_enters.saturating_add(1);
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for DragClient {
    fn event(
        state: &mut Self,
        _: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Cancelled => state.source_cancelled = true,
            wl_data_source::Event::DndDropPerformed => state.drop_performed = true,
            _ => {}
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for DragClient {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for DragClient {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.configured = true;
        }
    }
}

delegate_noop!(DragClient: ignore wl_buffer::WlBuffer);
delegate_noop!(DragClient: ignore wl_compositor::WlCompositor);
delegate_noop!(DragClient: ignore wl_data_device_manager::WlDataDeviceManager);
delegate_noop!(DragClient: ignore wl_surface::WlSurface);
delegate_noop!(DragClient: ignore wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1);
delegate_noop!(DragClient: ignore wp_viewport::WpViewport);
delegate_noop!(DragClient: ignore wp_viewporter::WpViewporter);
delegate_noop!(DragClient: ignore xdg_dialog_v1::XdgDialogV1);
delegate_noop!(DragClient: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(DragClient: ignore xdg_toplevel_drag_manager_v1::XdgToplevelDragManagerV1);
delegate_noop!(DragClient: ignore xdg_toplevel_drag_v1::XdgToplevelDragV1);
delegate_noop!(DragClient: ignore xdg_wm_dialog_v1::XdgWmDialogV1);

#[test]
fn toplevel_drag_moves_one_floating_view_and_survives_dialog_updates() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    runtime.state.input_devices.insert(
        tensor_event::DeviceId::new(1),
        crate::protocol::state::InputDeviceCapabilities {
            pointer: true,
            ..Default::default()
        },
    );
    runtime.state.reconcile_seat_capabilities();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(0);
    let (advance_tx, advance_rx) = mpsc::sync_channel(0);
    let client = spawn_drag_client(socket_path, step_tx, advance_rx);

    assert_eq!(dispatch_drag_step(&mut runtime, &step_rx), DragStep::Ready);
    let initial = runtime
        .state
        .world
        .geometry(ViewId::new(1))
        .expect("mapped toplevel has retained geometry");
    let pointer = (f64::from(initial.x + 20), f64::from(initial.y + 18)).into();
    runtime.state.input_seat.set_pointer_location(pointer);
    runtime
        .state
        .forward_pointer_motion(tensor_event::RelativeMotionEvent {
            delta_x: 0.0,
            delta_y: 0.0,
            unaccelerated_x: 0.0,
            unaccelerated_y: 0.0,
            time_ns: 1_000_000,
        });
    runtime
        .state
        .forward_pointer_button(tensor_event::PointerButtonEvent {
            button: 0x110,
            pressed: true,
            time_ns: 2_000_000,
        });
    runtime.state.flush_wayland_clients();

    assert_eq!(
        dispatch_drag_step(&mut runtime, &step_rx),
        DragStep::Active { enters: 0 }
    );
    assert!(runtime.state.protocol_globals.selection.dnd_active());
    assert!(matches!(
        runtime.state.world.view_placement(ViewId::new(1)),
        Some(ViewPlacement::Floating { geometry })
            if geometry.x == initial.x + 12
                && geometry.y == initial.y + 12
                && geometry.width == initial.width
                && geometry.height == initial.height
    ));

    advance_tx.send(()).unwrap();
    assert_eq!(
        dispatch_drag_step(&mut runtime, &step_rx),
        DragStep::DialogUpdated { enters: 0 }
    );
    assert!(matches!(
        runtime.state.world.view_placement(ViewId::new(1)),
        Some(ViewPlacement::Floating { .. })
    ));

    runtime
        .state
        .forward_pointer_motion(tensor_event::RelativeMotionEvent {
            delta_x: 40.0,
            delta_y: 24.0,
            unaccelerated_x: 40.0,
            unaccelerated_y: 24.0,
            time_ns: 3_000_000,
        });
    runtime.state.flush_wayland_clients();
    advance_tx.send(()).unwrap();
    assert_eq!(
        dispatch_drag_step(&mut runtime, &step_rx),
        DragStep::Moved { enters: 0 }
    );
    assert!(matches!(
        runtime.state.world.view_placement(ViewId::new(1)),
        Some(ViewPlacement::Floating { geometry })
            if geometry.x == initial.x + 52 && geometry.y == initial.y + 36
    ));

    runtime
        .state
        .forward_pointer_button(tensor_event::PointerButtonEvent {
            button: 0x110,
            pressed: false,
            time_ns: 4_000_000,
        });
    runtime.state.flush_wayland_clients();
    assert_eq!(
        dispatch_drag_step(&mut runtime, &step_rx),
        DragStep::Ended {
            cancelled: true,
            dropped: false,
        }
    );
    assert!(!runtime.state.protocol_globals.selection.dnd_active());
    assert!(matches!(
        runtime.state.world.view_placement(ViewId::new(1)),
        Some(ViewPlacement::Floating { geometry })
            if geometry.x == initial.x + 52 && geometry.y == initial.y + 36
    ));
    advance_tx.send(()).unwrap();
    client.join().unwrap();
}

fn spawn_drag_client(
    socket_path: PathBuf,
    steps: mpsc::SyncSender<DragStep>,
    advances: mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<DragClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let data_manager = globals
            .bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(&handle, 1..=3, ())
            .unwrap();
        let drag_manager = globals
            .bind::<xdg_toplevel_drag_manager_v1::XdgToplevelDragManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let dialogs = globals
            .bind::<xdg_wm_dialog_v1::XdgWmDialogV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let viewporter = globals
            .bind::<wp_viewporter::WpViewporter, _, _>(&handle, 1..=1, ())
            .unwrap();
        let single_pixel = globals
            .bind::<wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let data_device = data_manager.get_data_device(&seat, &handle, ());
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        let viewport = viewporter.get_viewport(&surface, &handle, ());
        viewport.set_destination(96, 64);
        let buffer = single_pixel.create_u32_rgba_buffer(
            u32::MAX,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            &handle,
            (),
        );
        surface.commit();

        let mut state = DragClient::default();
        while !state.configured || state.pointer.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        queue.roundtrip(&mut state).unwrap();
        steps.send(DragStep::Ready).unwrap();
        while state.button_serial.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        let source = data_manager.create_data_source(&handle, ());
        let drag = drag_manager.get_xdg_toplevel_drag(&source, &handle, ());
        drag.attach(&toplevel, 8, 6);
        data_device.start_drag(Some(&source), &surface, None, state.button_serial.unwrap());
        queue.roundtrip(&mut state).unwrap();
        steps
            .send(DragStep::Active {
                enters: state.dnd_enters,
            })
            .unwrap();

        advances.recv().unwrap();
        let dialog = dialogs.get_xdg_dialog(&toplevel, &handle, ());
        dialog.set_modal();
        queue.roundtrip(&mut state).unwrap();
        steps
            .send(DragStep::DialogUpdated {
                enters: state.dnd_enters,
            })
            .unwrap();

        advances.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        steps
            .send(DragStep::Moved {
                enters: state.dnd_enters,
            })
            .unwrap();

        while !state.source_cancelled && !state.drop_performed {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        steps
            .send(DragStep::Ended {
                cancelled: state.source_cancelled,
                dropped: state.drop_performed,
            })
            .unwrap();
        advances.recv().unwrap();

        drag.destroy();
        dialog.destroy();
        source.destroy();
        buffer.destroy();
        viewport.destroy();
        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
        data_device.release();
        seat.release();
    })
}

fn dispatch_drag_step(runtime: &mut WaylandRuntime, steps: &mpsc::Receiver<DragStep>) -> DragStep {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("xdg-toplevel-drag client did not complete before the dispatch limit");
}

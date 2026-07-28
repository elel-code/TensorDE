use std::{
    collections::HashSet,
    os::{fd::AsFd, unix::net::UnixStream},
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

use cursor_icon::CursorIcon;
use rustix::fs::{MemfdFlags, ftruncate, memfd_create};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
    backend::ObjectId,
    delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_buffer, wl_compositor, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool,
        wl_surface,
    },
};
use wayland_protocols::{
    wp::{
        cursor_shape::v1::client::{wp_cursor_shape_device_v1, wp_cursor_shape_manager_v1},
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
        viewporter::client::{wp_viewport, wp_viewporter},
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;
use crate::protocol::cursor::CursorImage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CursorStep {
    Ready,
    InvalidShapeSent,
    ValidShapeSent,
    ClientSurfaceSent,
    OffsetSent,
    SurfaceRetired,
    SurfaceRestored,
    StraddledOutputs,
    CrossedOutput,
    CurrentSurfaceUpdated,
    SurfaceDestroyed,
    Destroyed,
}

#[derive(Default)]
struct CursorClient {
    configured: bool,
    pointer: Option<wl_pointer::WlPointer>,
    enter_serial: Option<u32>,
    cursor_surface: Option<ObjectId>,
    cursor_outputs: HashSet<ObjectId>,
    cursor_enter_count: u32,
    cursor_leave_count: u32,
    cursor_scale: Option<i32>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for CursorClient {
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

impl Dispatch<wl_seat::WlSeat, ()> for CursorClient {
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

impl Dispatch<wl_pointer::WlPointer, ()> for CursorClient {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Enter { serial, .. } = event {
            state.enter_serial = Some(serial);
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for CursorClient {
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
                state.cursor_enter_count = state.cursor_enter_count.saturating_add(1);
            }
            wl_surface::Event::Leave { output } => {
                state.cursor_outputs.remove(&output.id());
                state.cursor_leave_count = state.cursor_leave_count.saturating_add(1);
            }
            wl_surface::Event::PreferredBufferScale { factor } => {
                state.cursor_scale = Some(factor);
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for CursorClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for CursorClient {
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

delegate_noop!(CursorClient: ignore wl_buffer::WlBuffer);
delegate_noop!(CursorClient: ignore wl_compositor::WlCompositor);
delegate_noop!(CursorClient: ignore wl_output::WlOutput);
delegate_noop!(CursorClient: ignore wl_shm::WlShm);
delegate_noop!(CursorClient: ignore wl_shm_pool::WlShmPool);
delegate_noop!(CursorClient: ignore wp_cursor_shape_device_v1::WpCursorShapeDeviceV1);
delegate_noop!(CursorClient: ignore wp_cursor_shape_manager_v1::WpCursorShapeManagerV1);
delegate_noop!(CursorClient: ignore wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1);
delegate_noop!(CursorClient: ignore wp_viewport::WpViewport);
delegate_noop!(CursorClient: ignore wp_viewporter::WpViewporter);
delegate_noop!(CursorClient: ignore xdg_toplevel::XdgToplevel);

#[test]
fn cursor_shape_v1_rejects_v2_shapes() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
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
    let (result_tx, result_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<CursorClient>(&connection).unwrap();
        let handle = queue.handle();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let mut state = CursorClient::default();
        while state.pointer.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let device = manager.get_pointer(state.pointer.as_ref().unwrap(), &handle, ());
        device.set_shape(0, wp_cursor_shape_device_v1::Shape::DndAsk);
        result_tx
            .send(queue.roundtrip(&mut state).is_err())
            .unwrap();
        seat.release();
    });

    let rejected = dispatch_until_cursor_result(&mut runtime, &result_rx);
    assert!(rejected, "a version-1 device must reject version-2 shapes");
    client.join().unwrap();
}

#[test]
fn cursor_shape_requires_the_active_enter_serial_and_focused_client() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    install_second_cursor_test_output(&mut runtime);
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
    let client = spawn_cursor_client(socket_path, step_tx, advance_rx);

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::Ready
    );
    let pointer_location: tensor_util::LogicalPoint<f64> = runtime
        .state
        .space
        .elements()
        .next()
        .and_then(|window| runtime.state.space.element_geometry(window))
        .map(|geometry| {
            (
                f64::from(geometry.loc.x) + 10.0,
                f64::from(geometry.loc.y) + 10.0,
            )
                .into()
        })
        .expect("cursor-shape test surface is mapped");
    runtime
        .state
        .input_seat
        .set_pointer_location(pointer_location);
    runtime
        .state
        .forward_pointer_motion(tensor_event::RelativeMotionEvent {
            delta_x: 1.0,
            delta_y: 1.0,
            unaccelerated_x: 1.0,
            unaccelerated_y: 1.0,
            time_ns: 1_000_000,
        });
    runtime.state.flush_wayland_clients();

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::InvalidShapeSent
    );
    assert_eq!(
        runtime.state.cursor.image(),
        &CursorImage::Named(CursorIcon::Default)
    );
    advance_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::ValidShapeSent
    );
    assert_eq!(
        runtime.state.cursor.image(),
        &CursorImage::Named(CursorIcon::Pointer)
    );
    advance_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::ClientSurfaceSent
    );
    assert_eq!(cursor_hotspot(&runtime), tensor_util::Point::new(5, 7));
    assert_eq!(runtime.state.test_cursor_surface_output_count(), 1);
    let raster = runtime.state.test_cursor_raster().unwrap();
    assert_eq!(raster.size, tensor_util::Size::new(16, 16));
    assert_eq!(raster.hotspot, tensor_util::Point::new(5, 7));
    advance_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::OffsetSent
    );
    assert_eq!(cursor_hotspot(&runtime), tensor_util::Point::new(3, 4));
    advance_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::SurfaceRetired
    );
    assert_eq!(
        runtime.state.cursor.image(),
        &CursorImage::Named(CursorIcon::Pointer)
    );
    advance_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::SurfaceRestored
    );
    assert_eq!(runtime.state.test_cursor_surface_output_count(), 1);
    runtime
        .state
        .input_seat
        .set_pointer_location((799.0, pointer_location.y).into());
    runtime.state.refresh_cursor_surface_outputs();
    runtime.state.flush_wayland_clients();
    advance_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::StraddledOutputs
    );
    assert_eq!(runtime.state.test_cursor_surface_output_count(), 2);
    runtime
        .state
        .input_seat
        .set_pointer_location((810.0, pointer_location.y).into());
    runtime.state.refresh_cursor_surface_outputs();
    runtime.state.flush_wayland_clients();
    advance_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::CrossedOutput
    );
    assert_eq!(runtime.state.test_cursor_surface_output_count(), 1);
    advance_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::CurrentSurfaceUpdated
    );
    assert_eq!(cursor_hotspot(&runtime), tensor_util::Point::new(9, 11));
    advance_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::SurfaceDestroyed
    );
    assert_eq!(
        runtime.state.cursor.image(),
        &CursorImage::Named(CursorIcon::Default)
    );
    assert_eq!(runtime.state.test_cursor_surface_output_count(), 0);
    advance_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_cursor_step(&mut runtime, &step_rx),
        CursorStep::Destroyed
    );
    client.join().unwrap();
}

fn spawn_cursor_client(
    socket_path: PathBuf,
    steps: mpsc::SyncSender<CursorStep>,
    advances: mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<CursorClient>(&connection).unwrap();
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
        let _outputs = globals
            .contents()
            .clone_list()
            .into_iter()
            .filter(|global| global.interface == wl_output::WlOutput::interface().name)
            .map(|global| {
                globals.registry().bind::<wl_output::WlOutput, _, _>(
                    global.name,
                    global.version.min(4),
                    &handle,
                    (),
                )
            })
            .collect::<Vec<_>>();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
            .unwrap();
        let cursor_manager = globals
            .bind::<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1, _, _>(&handle, 1..=2, ())
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
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        surface.commit();

        let mut state = CursorClient::default();
        while !state.configured || state.pointer.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let viewport = viewporter.get_viewport(&surface, &handle, ());
        viewport.set_destination(64, 32);
        let buffer = single_pixel.create_u32_rgba_buffer(
            u32::MAX,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            &handle,
            (),
        );
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, 1, 1);
        surface.commit();
        let pointer = state.pointer.clone().unwrap();
        let cursor_device = cursor_manager.get_pointer(&pointer, &handle, ());
        queue.roundtrip(&mut state).unwrap();
        steps.send(CursorStep::Ready).unwrap();

        while state.enter_serial.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let serial = state.enter_serial.unwrap();
        cursor_device.set_shape(
            serial.wrapping_add(1),
            wp_cursor_shape_device_v1::Shape::Wait,
        );
        queue.roundtrip(&mut state).unwrap();
        steps.send(CursorStep::InvalidShapeSent).unwrap();
        advances.recv().unwrap();

        cursor_device.set_shape(serial, wp_cursor_shape_device_v1::Shape::Pointer);
        queue.roundtrip(&mut state).unwrap();
        steps.send(CursorStep::ValidShapeSent).unwrap();
        advances.recv().unwrap();

        let cursor_surface = compositor.create_surface(&handle, ());
        state.cursor_surface = Some(cursor_surface.id());
        let cursor_viewport = viewporter.get_viewport(&cursor_surface, &handle, ());
        cursor_viewport.set_destination(16, 16);
        let cursor_buffer = create_cursor_buffer(&shm, &handle);
        pointer.set_cursor(serial, Some(&cursor_surface), 5, 7);
        cursor_surface.attach(Some(&cursor_buffer), 0, 0);
        cursor_surface.damage_buffer(0, 0, 1, 1);
        cursor_surface.commit();
        queue.roundtrip(&mut state).unwrap();
        steps.send(CursorStep::ClientSurfaceSent).unwrap();
        advances.recv().unwrap();

        cursor_surface.offset(2, 3);
        cursor_surface.commit();
        queue.roundtrip(&mut state).unwrap();
        steps.send(CursorStep::OffsetSent).unwrap();
        advances.recv().unwrap();

        assert_eq!(state.cursor_outputs.len(), 1);
        assert_eq!(state.cursor_scale, Some(2));
        cursor_device.set_shape(serial, wp_cursor_shape_device_v1::Shape::Pointer);
        queue.roundtrip(&mut state).unwrap();
        assert!(state.cursor_outputs.is_empty());
        assert_eq!(state.cursor_leave_count, 1);
        steps.send(CursorStep::SurfaceRetired).unwrap();
        advances.recv().unwrap();

        pointer.set_cursor(serial, Some(&cursor_surface), 3, 4);
        queue.roundtrip(&mut state).unwrap();
        assert_eq!(state.cursor_outputs.len(), 1);
        assert_eq!(state.cursor_enter_count, 2);
        steps.send(CursorStep::SurfaceRestored).unwrap();
        advances.recv().unwrap();

        queue.roundtrip(&mut state).unwrap();
        assert_eq!(state.cursor_outputs.len(), 2);
        assert_eq!(state.cursor_scale, Some(3));
        steps.send(CursorStep::StraddledOutputs).unwrap();
        advances.recv().unwrap();

        queue.roundtrip(&mut state).unwrap();
        assert_eq!(state.cursor_outputs.len(), 1);
        assert_eq!(state.cursor_scale, Some(3));
        steps.send(CursorStep::CrossedOutput).unwrap();
        advances.recv().unwrap();

        pointer.set_cursor(serial.wrapping_add(17), Some(&cursor_surface), 9, 11);
        queue.roundtrip(&mut state).unwrap();
        steps.send(CursorStep::CurrentSurfaceUpdated).unwrap();
        advances.recv().unwrap();

        cursor_device.destroy();
        cursor_buffer.destroy();
        cursor_viewport.destroy();
        cursor_surface.destroy();
        queue.roundtrip(&mut state).unwrap();
        steps.send(CursorStep::SurfaceDestroyed).unwrap();
        advances.recv().unwrap();

        buffer.destroy();
        viewport.destroy();
        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
        cursor_manager.destroy();
        single_pixel.destroy();
        viewporter.destroy();
        seat.release();
        queue.roundtrip(&mut state).unwrap();
        steps.send(CursorStep::Destroyed).unwrap();
    })
}

fn install_second_cursor_test_output(runtime: &mut WaylandRuntime) {
    let mode = tensor_host::PhysicalMode::new(500, 800, 60_000);
    let output = crate::protocol::globals::output::Output::new(
        tensor_host::ConnectorId::new(2, 2),
        "cursor-test-output-2".to_owned(),
        (300, 340),
        tensor_host::SubpixelLayout::Unknown,
        vec![mode],
        mode,
        mode,
        tensor_util::OutputScale::from_f64(2.5).unwrap(),
    );
    let _global = output.create_global(&runtime.state.display_handle);
    runtime.state.space.map_output(&output, (800, 0));
}

fn cursor_hotspot(runtime: &WaylandRuntime) -> tensor_util::Point {
    let CursorImage::Surface(surface) = runtime.state.cursor.image() else {
        panic!("client cursor surface must be active");
    };
    crate::protocol::globals::compositor::with_states(surface, |states| {
        states
            .data_map
            .get::<std::sync::Mutex<crate::protocol::globals::seat::CursorSurfaceState>>()
            .unwrap()
            .lock()
            .unwrap()
            .hotspot
    })
}

fn create_cursor_buffer(
    shm: &wl_shm::WlShm,
    handle: &QueueHandle<CursorClient>,
) -> wl_buffer::WlBuffer {
    const WIDTH: i32 = 8;
    const HEIGHT: i32 = 8;
    const STRIDE: i32 = WIDTH * 4;
    const LEN: i32 = STRIDE * HEIGHT;
    let fd = memfd_create("tensor-cursor-test", MemfdFlags::CLOEXEC).unwrap();
    ftruncate(&fd, u64::try_from(LEN).unwrap()).unwrap();
    let pool = shm.create_pool(fd.as_fd(), LEN, handle, ());
    let buffer = pool.create_buffer(
        0,
        WIDTH,
        HEIGHT,
        STRIDE,
        wl_shm::Format::Argb8888,
        handle,
        (),
    );
    pool.destroy();
    buffer
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
    panic!("cursor-shape client did not complete before the dispatch limit");
}

fn dispatch_until_cursor_result(
    runtime: &mut WaylandRuntime,
    result: &mpsc::Receiver<bool>,
) -> bool {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = result.try_recv() {
            return result;
        }
    }
    panic!("cursor-shape client did not report a protocol result");
}

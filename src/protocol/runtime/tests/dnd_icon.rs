use std::{
    collections::HashSet,
    os::{fd::AsFd, unix::net::UnixStream},
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

use rustix::fs::{MemfdFlags, ftruncate, memfd_create};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
    backend::ObjectId,
    delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_data_device, wl_data_device_manager, wl_output,
        wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::{
    wp::{
        fractional_scale::v1::client::{wp_fractional_scale_manager_v1, wp_fractional_scale_v1},
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
        viewporter::client::{wp_viewport, wp_viewporter},
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DndStep {
    Ready,
    Committed {
        outputs: usize,
        integer_scale: i32,
        fractional_scale: u32,
        frame_done: bool,
    },
    Straddled {
        outputs: usize,
        integer_scale: i32,
        fractional_scale: u32,
    },
    Crossed {
        outputs: usize,
        leaves: u32,
    },
    Retired {
        outputs: usize,
        leaves: u32,
    },
}

#[derive(Default)]
struct DndClient {
    configured: bool,
    pointer: Option<wl_pointer::WlPointer>,
    button_serial: Option<u32>,
    icon: Option<ObjectId>,
    icon_outputs: HashSet<ObjectId>,
    icon_leaves: u32,
    integer_scale: Option<i32>,
    fractional_scale: Option<u32>,
    frame_done: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for DndClient {
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

impl Dispatch<wl_seat::WlSeat, ()> for DndClient {
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

impl Dispatch<wl_pointer::WlPointer, ()> for DndClient {
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

impl Dispatch<wl_surface::WlSurface, ()> for DndClient {
    fn event(
        state: &mut Self,
        surface: &wl_surface::WlSurface,
        event: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if state.icon.as_ref() != Some(&surface.id()) {
            return;
        }
        match event {
            wl_surface::Event::Enter { output } => {
                state.icon_outputs.insert(output.id());
            }
            wl_surface::Event::Leave { output } => {
                state.icon_outputs.remove(&output.id());
                state.icon_leaves = state.icon_leaves.saturating_add(1);
            }
            wl_surface::Event::PreferredBufferScale { factor } => {
                state.integer_scale = Some(factor);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for DndClient {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_callback::Event::Done { .. }) {
            state.frame_done = true;
        }
    }
}

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for DndClient {
    fn event(
        state: &mut Self,
        _: &wp_fractional_scale_v1::WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            state.fractional_scale = Some(scale);
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for DndClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for DndClient {
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

impl Dispatch<wl_data_device::WlDataDevice, ()> for DndClient {
    fn event(
        _: &mut Self,
        _: &wl_data_device::WlDataDevice,
        _: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(DndClient: ignore wl_buffer::WlBuffer);
delegate_noop!(DndClient: ignore wl_compositor::WlCompositor);
delegate_noop!(DndClient: ignore wl_data_device_manager::WlDataDeviceManager);
delegate_noop!(DndClient: ignore wl_output::WlOutput);
delegate_noop!(DndClient: ignore wl_shm::WlShm);
delegate_noop!(DndClient: ignore wl_shm_pool::WlShmPool);
delegate_noop!(DndClient: ignore wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);
delegate_noop!(DndClient: ignore wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1);
delegate_noop!(DndClient: ignore wp_viewport::WpViewport);
delegate_noop!(DndClient: ignore wp_viewporter::WpViewporter);
delegate_noop!(DndClient: ignore xdg_toplevel::XdgToplevel);

#[test]
fn drag_icon_tracks_committed_offset_outputs_and_lifecycle() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    install_second_output(&mut runtime);
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
    let client = spawn_dnd_client(socket_path, step_tx, advance_rx);

    assert_eq!(dispatch_step(&mut runtime, &step_rx), DndStep::Ready);
    let location: tensor_util::LogicalPoint<f64> = runtime
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
        .expect("DnD origin surface is mapped");
    runtime.state.input_seat.set_pointer_location(location);
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
        dispatch_step(&mut runtime, &step_rx),
        DndStep::Committed {
            outputs: 1,
            integer_scale: 2,
            fractional_scale: 150,
            frame_done: false,
        }
    );
    assert!(runtime.state.protocol_globals.selection.dnd_active());
    assert_eq!(runtime.state.dnd_icon.outputs.len(), 1);
    assert_eq!(
        runtime.state.dnd_icon.logical_bounds(location, (20, 10)),
        Some((
            location.x - 5.0,
            location.y + 3.0,
            location.x + 15.0,
            location.y + 13.0
        ))
    );

    runtime
        .state
        .input_seat
        .set_pointer_location((799.0, location.y).into());
    runtime.state.refresh_dnd_icon_outputs();
    runtime.state.flush_wayland_clients();
    advance_tx.send(()).unwrap();
    assert_eq!(
        dispatch_step(&mut runtime, &step_rx),
        DndStep::Straddled {
            outputs: 2,
            integer_scale: 3,
            fractional_scale: 300,
        }
    );
    assert_eq!(runtime.state.dnd_icon.outputs.len(), 2);

    runtime
        .state
        .input_seat
        .set_pointer_location((820.0, location.y).into());
    runtime.state.refresh_dnd_icon_outputs();
    runtime.state.flush_wayland_clients();
    advance_tx.send(()).unwrap();
    assert_eq!(
        dispatch_step(&mut runtime, &step_rx),
        DndStep::Crossed {
            outputs: 1,
            leaves: 1,
        }
    );

    runtime
        .state
        .forward_pointer_button(tensor_event::PointerButtonEvent {
            button: 0x110,
            pressed: false,
            time_ns: 3_000_000,
        });
    runtime.state.flush_wayland_clients();
    advance_tx.send(()).unwrap();
    assert_eq!(
        dispatch_step(&mut runtime, &step_rx),
        DndStep::Retired {
            outputs: 0,
            leaves: 2,
        }
    );
    assert!(!runtime.state.protocol_globals.selection.dnd_active());
    assert!(runtime.state.dnd_icon.surface().is_none());
    client.join().unwrap();
}

fn spawn_dnd_client(
    socket_path: PathBuf,
    steps: mpsc::SyncSender<DndStep>,
    advances: mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<DndClient>(&connection).unwrap();
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
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
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
        let fractional_manager = globals
            .bind::<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
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
        let data_device = data_manager.get_data_device(&seat, &handle, ());
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        let viewport = viewporter.get_viewport(&surface, &handle, ());
        viewport.set_destination(64, 32);
        let root_buffer = single_pixel.create_u32_rgba_buffer(
            u32::MAX,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            &handle,
            (),
        );
        surface.commit();

        let mut state = DndClient::default();
        while !state.configured || state.pointer.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        surface.attach(Some(&root_buffer), 0, 0);
        surface.commit();
        queue.roundtrip(&mut state).unwrap();
        steps.send(DndStep::Ready).unwrap();
        while state.button_serial.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        let icon = compositor.create_surface(&handle, ());
        state.icon = Some(icon.id());
        let icon_viewport = viewporter.get_viewport(&icon, &handle, ());
        icon_viewport.set_destination(20, 10);
        let _fractional = fractional_manager.get_fractional_scale(&icon, &handle, ());
        let icon_buffer = create_icon_buffer(&shm, &handle);
        let _frame = icon.frame(&handle, ());
        data_device.start_drag(None, &surface, Some(&icon), state.button_serial.unwrap());
        icon.offset(-5, 3);
        icon.attach(Some(&icon_buffer), 0, 0);
        icon.damage_buffer(0, 0, 8, 8);
        icon.commit();
        queue.roundtrip(&mut state).unwrap();
        steps
            .send(DndStep::Committed {
                outputs: state.icon_outputs.len(),
                integer_scale: state.integer_scale.unwrap(),
                fractional_scale: state.fractional_scale.unwrap(),
                frame_done: state.frame_done,
            })
            .unwrap();

        advances.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        steps
            .send(DndStep::Straddled {
                outputs: state.icon_outputs.len(),
                integer_scale: state.integer_scale.unwrap(),
                fractional_scale: state.fractional_scale.unwrap(),
            })
            .unwrap();

        advances.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        steps
            .send(DndStep::Crossed {
                outputs: state.icon_outputs.len(),
                leaves: state.icon_leaves,
            })
            .unwrap();

        advances.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        steps
            .send(DndStep::Retired {
                outputs: state.icon_outputs.len(),
                leaves: state.icon_leaves,
            })
            .unwrap();

        icon_buffer.destroy();
        icon_viewport.destroy();
        icon.destroy();
        root_buffer.destroy();
        viewport.destroy();
        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
        data_device.release();
        seat.release();
    })
}

fn create_icon_buffer(shm: &wl_shm::WlShm, handle: &QueueHandle<DndClient>) -> wl_buffer::WlBuffer {
    const WIDTH: i32 = 8;
    const HEIGHT: i32 = 8;
    const STRIDE: i32 = WIDTH * 4;
    const LEN: i32 = STRIDE * HEIGHT;
    let fd = memfd_create("tensor-dnd-icon-test", MemfdFlags::CLOEXEC).unwrap();
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

fn install_second_output(runtime: &mut WaylandRuntime) {
    let mode = tensor_host::PhysicalMode::new(500, 800, 60_000);
    let output = crate::protocol::globals::output::Output::new(
        tensor_host::ConnectorId::new(2, 2),
        "dnd-test-output-2".to_owned(),
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

fn dispatch_step(runtime: &mut WaylandRuntime, steps: &mpsc::Receiver<DndStep>) -> DndStep {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("DnD icon client did not complete before the dispatch limit");
}

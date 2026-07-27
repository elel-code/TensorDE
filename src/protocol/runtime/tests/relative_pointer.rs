use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::{
    wp::{
        relative_pointer::zv1::client::{zwp_relative_pointer_manager_v1, zwp_relative_pointer_v1},
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
        viewporter::client::{wp_viewport, wp_viewporter},
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
struct RelativeEvent {
    time_hi: u32,
    time_lo: u32,
    dx: f64,
    dy: f64,
    dx_unaccelerated: f64,
    dy_unaccelerated: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RelativeStep {
    Registered,
    Motion(RelativeEvent),
    Destroyed,
}

#[derive(Default)]
struct RelativeClient {
    configured: bool,
    pointer: Option<wl_pointer::WlPointer>,
    motion: Option<RelativeEvent>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for RelativeClient {
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

impl Dispatch<wl_seat::WlSeat, ()> for RelativeClient {
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

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for RelativeClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for RelativeClient {
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

impl Dispatch<zwp_relative_pointer_v1::ZwpRelativePointerV1, ()> for RelativeClient {
    fn event(
        state: &mut Self,
        _: &zwp_relative_pointer_v1::ZwpRelativePointerV1,
        event: zwp_relative_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_relative_pointer_v1::Event::RelativeMotion {
            utime_hi,
            utime_lo,
            dx,
            dy,
            dx_unaccel,
            dy_unaccel,
        } = event
        {
            state.motion = Some(RelativeEvent {
                time_hi: utime_hi,
                time_lo: utime_lo,
                dx,
                dy,
                dx_unaccelerated: dx_unaccel,
                dy_unaccelerated: dy_unaccel,
            });
        }
    }
}

delegate_noop!(RelativeClient: ignore wl_compositor::WlCompositor);
delegate_noop!(RelativeClient: ignore wl_buffer::WlBuffer);
delegate_noop!(RelativeClient: ignore wl_surface::WlSurface);
delegate_noop!(RelativeClient: ignore wl_pointer::WlPointer);
delegate_noop!(RelativeClient: ignore wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1);
delegate_noop!(RelativeClient: ignore wp_viewport::WpViewport);
delegate_noop!(RelativeClient: ignore wp_viewporter::WpViewporter);
delegate_noop!(RelativeClient: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(RelativeClient: ignore zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1);

#[test]
fn relative_pointer_sends_unclipped_motion_and_removes_destroyed_resource() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    runtime.state.input_devices.insert(
        tensor_input::DeviceId::new(1),
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
    let (release_tx, release_rx) = mpsc::sync_channel(0);
    let client = spawn_relative_client(socket_path, step_tx, release_rx);

    assert_eq!(
        dispatch_until_relative_step(&mut runtime, &step_rx),
        RelativeStep::Registered
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .relative_pointer
            .pointer_count(),
        1
    );
    let pointer_location: smithay::utils::Point<f64, smithay::utils::Logical> = runtime
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
        .expect("relative-pointer test surface is mapped");
    runtime
        .state
        .input_seat
        .set_pointer_location(pointer_location);

    let time_usec = (1_u64 << 32) | 17;
    runtime
        .state
        .forward_pointer_motion(tensor_input::RelativeMotionEvent {
            delta_x: 3.5,
            delta_y: -2.25,
            unaccelerated_x: 7.0,
            unaccelerated_y: -4.0,
            time_ns: time_usec * 1_000,
        });
    runtime.state.flush_wayland_clients();
    assert_eq!(
        dispatch_until_relative_step(&mut runtime, &step_rx),
        RelativeStep::Motion(RelativeEvent {
            time_hi: 1,
            time_lo: 17,
            dx: 3.5,
            dy: -2.25,
            dx_unaccelerated: 7.0,
            dy_unaccelerated: -4.0,
        })
    );

    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until_relative_step(&mut runtime, &step_rx),
        RelativeStep::Destroyed
    );
    client.join().unwrap();
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .relative_pointer
            .pointer_count(),
        0
    );
}

fn spawn_relative_client(
    socket_path: PathBuf,
    steps: mpsc::SyncSender<RelativeStep>,
    releases: mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<RelativeClient>(&connection).unwrap();
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
        let manager = globals
            .bind::<zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
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

        let mut state = RelativeClient::default();
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
        let pointer = state.pointer.as_ref().unwrap();
        let relative = manager.get_relative_pointer(pointer, &handle, ());
        queue.roundtrip(&mut state).unwrap();
        steps.send(RelativeStep::Registered).unwrap();

        while state.motion.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        steps
            .send(RelativeStep::Motion(state.motion.take().unwrap()))
            .unwrap();
        releases.recv().unwrap();

        relative.destroy();
        buffer.destroy();
        viewport.destroy();
        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
        manager.destroy();
        single_pixel.destroy();
        viewporter.destroy();
        seat.release();
        queue.roundtrip(&mut state).unwrap();
        steps.send(RelativeStep::Destroyed).unwrap();
    })
}

fn dispatch_until_relative_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<RelativeStep>,
) -> RelativeStep {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("relative-pointer client did not complete before the dispatch limit");
}

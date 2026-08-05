use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_compositor, wl_pointer, wl_region, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::{
    wp::{
        pointer_constraints::zv1::client::{
            zwp_confined_pointer_v1, zwp_locked_pointer_v1, zwp_pointer_constraints_v1,
            zwp_pointer_constraints_v1::Lifetime,
        },
        relative_pointer::zv1::client::{zwp_relative_pointer_manager_v1, zwp_relative_pointer_v1},
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
        viewporter::client::{wp_viewport, wp_viewporter},
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum ConstraintStep {
    SurfaceReady,
    Locked,
    LockMotion {
        relative_dx: f64,
        pointer_motions: usize,
    },
    LockDestroyed,
    Confined,
    ConfinedMotion {
        x: f64,
        y: f64,
    },
    Unconfined,
    Destroyed,
}

#[derive(Default)]
struct ConstraintClient {
    configured: bool,
    pointer: Option<wl_pointer::WlPointer>,
    pointer_motions: usize,
    last_pointer_motion: Option<(f64, f64)>,
    relative_dx: Option<f64>,
    locked: bool,
    confined: bool,
    unconfined: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ConstraintClient {
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

impl Dispatch<wl_seat::WlSeat, ()> for ConstraintClient {
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

impl Dispatch<wl_pointer::WlPointer, ()> for ConstraintClient {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Motion {
            surface_x,
            surface_y,
            ..
        } = event
        {
            state.pointer_motions += 1;
            state.last_pointer_motion = Some((surface_x, surface_y));
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for ConstraintClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for ConstraintClient {
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

impl Dispatch<zwp_relative_pointer_v1::ZwpRelativePointerV1, ()> for ConstraintClient {
    fn event(
        state: &mut Self,
        _: &zwp_relative_pointer_v1::ZwpRelativePointerV1,
        event: zwp_relative_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_relative_pointer_v1::Event::RelativeMotion { dx, .. } = event {
            state.relative_dx = Some(dx);
        }
    }
}

impl Dispatch<zwp_locked_pointer_v1::ZwpLockedPointerV1, ()> for ConstraintClient {
    fn event(
        state: &mut Self,
        _: &zwp_locked_pointer_v1::ZwpLockedPointerV1,
        event: zwp_locked_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_locked_pointer_v1::Event::Locked = event {
            state.locked = true;
        }
    }
}

impl Dispatch<zwp_confined_pointer_v1::ZwpConfinedPointerV1, ()> for ConstraintClient {
    fn event(
        state: &mut Self,
        _: &zwp_confined_pointer_v1::ZwpConfinedPointerV1,
        event: zwp_confined_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_confined_pointer_v1::Event::Confined => state.confined = true,
            zwp_confined_pointer_v1::Event::Unconfined => state.unconfined = true,
            _ => {}
        }
    }
}

delegate_noop!(ConstraintClient: ignore wl_buffer::WlBuffer);
delegate_noop!(ConstraintClient: ignore wl_compositor::WlCompositor);
delegate_noop!(ConstraintClient: ignore wl_region::WlRegion);
delegate_noop!(ConstraintClient: ignore wl_surface::WlSurface);
delegate_noop!(ConstraintClient: ignore zwp_pointer_constraints_v1::ZwpPointerConstraintsV1);
delegate_noop!(ConstraintClient: ignore zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1);
delegate_noop!(ConstraintClient: ignore wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1);
delegate_noop!(ConstraintClient: ignore wp_viewport::WpViewport);
delegate_noop!(ConstraintClient: ignore wp_viewporter::WpViewporter);
delegate_noop!(ConstraintClient: ignore xdg_toplevel::XdgToplevel);

#[test]
fn pointer_constraints_lock_confine_commit_and_release_without_motion_path_copies() {
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
    let (start_tx, start_rx) = mpsc::sync_channel(0);
    let client = spawn_constraint_client(socket_path, step_tx, start_rx);

    assert_eq!(
        dispatch_until_constraint_step(&mut runtime, &step_rx),
        ConstraintStep::SurfaceReady
    );
    let initial: tensor_util::LogicalPoint<f64> = runtime
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
        .expect("pointer-constraints test surface is mapped");
    runtime.state.input_seat.set_pointer_location(initial);
    runtime
        .state
        .forward_pointer_motion(relative_motion(1.0, 1.0, 1));
    let focused = runtime
        .state
        .pointer_focus_under(runtime.state.input_seat.pointer_location().unwrap())
        .expect("test surface has pointer focus");
    let surface_origin = focused.1;
    start_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_constraint_step(&mut runtime, &step_rx),
        ConstraintStep::Locked
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .pointer_constraints
            .constraint_count(),
        1
    );
    let locked_location = runtime.state.input_seat.pointer_location().unwrap();
    runtime
        .state
        .forward_pointer_motion(relative_motion(5.0, 2.0, 2));
    runtime.state.flush_wayland_clients();
    assert_eq!(
        dispatch_until_constraint_step(&mut runtime, &step_rx),
        ConstraintStep::LockMotion {
            relative_dx: 5.0,
            pointer_motions: 0,
        }
    );
    assert_eq!(
        runtime.state.input_seat.pointer_location().unwrap(),
        locked_location
    );

    assert_eq!(
        dispatch_until_constraint_step(&mut runtime, &step_rx),
        ConstraintStep::LockDestroyed
    );
    let hinted = runtime.state.input_seat.pointer_location().unwrap();
    assert_eq!(hinted.x, surface_origin.x + 20.0);
    assert_eq!(hinted.y, surface_origin.y + 15.0);

    assert_eq!(
        dispatch_until_constraint_step(&mut runtime, &step_rx),
        ConstraintStep::Confined
    );
    runtime
        .state
        .forward_pointer_motion(relative_motion(50.0, 0.0, 3));
    runtime.state.flush_wayland_clients();
    let ConstraintStep::ConfinedMotion { x, y } =
        dispatch_until_constraint_step(&mut runtime, &step_rx)
    else {
        panic!("expected confined pointer motion");
    };
    assert!((29.0..30.0).contains(&x));
    assert_eq!(y, 15.0);
    let confined_location = runtime.state.input_seat.pointer_location().unwrap();
    assert!((29.0..30.0).contains(&(confined_location.x - surface_origin.x)));

    assert_eq!(
        dispatch_until_constraint_step(&mut runtime, &step_rx),
        ConstraintStep::Unconfined
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .pointer_constraints
            .constraint_count(),
        0
    );
    assert_eq!(
        dispatch_until_constraint_step(&mut runtime, &step_rx),
        ConstraintStep::Destroyed
    );
    client.join().unwrap();
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .pointer_constraints
            .constraint_count(),
        0
    );
}

fn relative_motion(
    delta_x: f64,
    delta_y: f64,
    time_msec: u64,
) -> tensor_event::RelativeMotionEvent {
    tensor_event::RelativeMotionEvent {
        delta_x,
        delta_y,
        unaccelerated_x: delta_x,
        unaccelerated_y: delta_y,
        time_ns: time_msec * 1_000_000,
    }
}

fn spawn_constraint_client(
    socket_path: PathBuf,
    steps: mpsc::SyncSender<ConstraintStep>,
    start: mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<ConstraintClient>(&connection).unwrap();
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
        let constraints = globals
            .bind::<zwp_pointer_constraints_v1::ZwpPointerConstraintsV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let relative_manager = globals
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

        let mut state = ConstraintClient::default();
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
        queue.roundtrip(&mut state).unwrap();
        steps.send(ConstraintStep::SurfaceReady).unwrap();
        start.recv().unwrap();

        let pointer = state.pointer.clone().unwrap();
        let relative = relative_manager.get_relative_pointer(&pointer, &handle, ());
        let locked =
            constraints.lock_pointer(&surface, &pointer, None, Lifetime::Persistent, &handle, ());
        while !state.locked {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        state.pointer_motions = 0;
        state.last_pointer_motion = None;
        steps.send(ConstraintStep::Locked).unwrap();

        while state.relative_dx.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        steps
            .send(ConstraintStep::LockMotion {
                relative_dx: state.relative_dx.take().unwrap(),
                pointer_motions: state.pointer_motions,
            })
            .unwrap();
        locked.set_cursor_position_hint(20.0, 15.0);
        surface.commit();
        locked.destroy();
        queue.roundtrip(&mut state).unwrap();
        steps.send(ConstraintStep::LockDestroyed).unwrap();

        let region = compositor.create_region(&handle, ());
        region.add(0, 0, 30, 30);
        let confined = constraints.confine_pointer(
            &surface,
            &pointer,
            Some(&region),
            Lifetime::Oneshot,
            &handle,
            (),
        );
        region.destroy();
        while !state.confined {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        state.last_pointer_motion = None;
        steps.send(ConstraintStep::Confined).unwrap();

        while state.last_pointer_motion.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let (x, y) = state.last_pointer_motion.take().unwrap();
        steps.send(ConstraintStep::ConfinedMotion { x, y }).unwrap();

        let smaller = compositor.create_region(&handle, ());
        smaller.add(0, 0, 10, 10);
        confined.set_region(Some(&smaller));
        surface.commit();
        smaller.destroy();
        while !state.unconfined {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        steps.send(ConstraintStep::Unconfined).unwrap();

        confined.destroy();
        relative.destroy();
        buffer.destroy();
        viewport.destroy();
        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
        relative_manager.destroy();
        constraints.destroy();
        single_pixel.destroy();
        viewporter.destroy();
        seat.release();
        queue.roundtrip(&mut state).unwrap();
        steps.send(ConstraintStep::Destroyed).unwrap();
    })
}

fn dispatch_until_constraint_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<ConstraintStep>,
) -> ConstraintStep {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("pointer-constraints client did not complete before the dispatch limit");
}

use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use tensor_protocol::SurfacePresentationHint;
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::{
    wp::{
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
        tearing_control::v1::client::{wp_tearing_control_manager_v1, wp_tearing_control_v1},
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TearingStep {
    AsyncCommitted,
    DestroyCommitted,
}

#[derive(Default)]
struct TearingClient {
    configured: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for TearingClient {
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

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for TearingClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for TearingClient {
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

delegate_noop!(TearingClient: ignore wl_buffer::WlBuffer);
delegate_noop!(TearingClient: ignore wl_compositor::WlCompositor);
delegate_noop!(TearingClient: ignore wl_surface::WlSurface);
delegate_noop!(TearingClient: ignore wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1);
delegate_noop!(TearingClient: ignore wp_tearing_control_manager_v1::WpTearingControlManagerV1);
delegate_noop!(TearingClient: ignore wp_tearing_control_v1::WpTearingControlV1);
delegate_noop!(TearingClient: ignore xdg_toplevel::XdgToplevel);

#[test]
fn tearing_hint_wire_is_double_buffered_and_destroy_resets_on_commit() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<TearingClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let manager = globals
            .bind::<wp_tearing_control_manager_v1::WpTearingControlManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
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
        let mut state = TearingClient::default();
        while !state.configured {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let pixel = single_pixel.create_u32_rgba_buffer(
            u32::MAX,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            &handle,
            (),
        );
        surface.attach(Some(&pixel), 0, 0);
        surface.damage_buffer(0, 0, 1, 1);
        surface.commit();
        queue.roundtrip(&mut state).unwrap();

        let tearing = manager.get_tearing_control(&surface, &handle, ());
        tearing.set_presentation_hint(wp_tearing_control_v1::PresentationHint::Async);
        surface.commit();
        queue.roundtrip(&mut state).unwrap();
        step_tx.send(TearingStep::AsyncCommitted).unwrap();
        release_rx.recv().unwrap();

        tearing.destroy();
        surface.commit();
        queue.roundtrip(&mut state).unwrap();
        step_tx.send(TearingStep::DestroyCommitted).unwrap();
        release_rx.recv().unwrap();

        pixel.destroy();
        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
        single_pixel.destroy();
        manager.destroy();
    });

    assert_eq!(
        dispatch_until_tearing_step(&mut runtime, &step_rx),
        TearingStep::AsyncCommitted
    );
    assert_tearing_state(&runtime, SurfacePresentationHint::Async);
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_tearing_step(&mut runtime, &step_rx),
        TearingStep::DestroyCommitted
    );
    assert_tearing_state(&runtime, SurfacePresentationHint::Vsync);
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

#[test]
fn tearing_manager_rejects_duplicate_surface_objects_on_the_wire() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<TearingClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let manager = globals
            .bind::<wp_tearing_control_manager_v1::WpTearingControlManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let _first = manager.get_tearing_control(&surface, &handle, ());
        let _duplicate = manager.get_tearing_control(&surface, &handle, ());

        assert!(queue.roundtrip(&mut TearingClient::default()).is_err());
        let error = connection
            .protocol_error()
            .expect("expected tearing-control protocol error");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    let result = dispatch_until_tearing_result(&mut runtime, &result_rx);
    assert_eq!(
        result,
        (
            "wp_tearing_control_manager_v1".to_owned(),
            wp_tearing_control_manager_v1::Error::TearingControlExists as u32,
        )
    );
    client.join().unwrap();
}

fn assert_tearing_state(runtime: &WaylandRuntime, expected: SurfacePresentationHint) {
    let root = runtime
        .state
        .space
        .elements()
        .next()
        .and_then(|window| window.wl_surface())
        .map(std::borrow::Cow::into_owned)
        .expect("tearing test toplevel is mapped");
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .tearing_control
            .committed_hint(&root),
        expected
    );
    let view_id = runtime.state.view_for_surface(&root).unwrap();
    assert_eq!(
        runtime.state.world.presentation_hint(view_id),
        Some(expected)
    );
}

fn dispatch_until_tearing_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<TearingStep>,
) -> TearingStep {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("tearing-control client did not complete before the dispatch limit");
}

fn dispatch_until_tearing_result<T>(
    runtime: &mut WaylandRuntime,
    results: &mpsc::Receiver<T>,
) -> T {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("tearing-control protocol error did not arrive before the dispatch limit");
}

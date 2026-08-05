use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use tensor_protocol::{ColorAlphaMode, ColorRepresentation};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::wp::color_representation::v1::client::{
    wp_color_representation_manager_v1, wp_color_representation_surface_v1,
};

use super::*;

struct ColorClient;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ColorClient {
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

delegate_noop!(ColorClient: ignore wl_compositor::WlCompositor);
delegate_noop!(ColorClient: ignore wl_surface::WlSurface);
delegate_noop!(ColorClient: ignore wp_color_representation_manager_v1::WpColorRepresentationManagerV1);
delegate_noop!(ColorClient: ignore wp_color_representation_surface_v1::WpColorRepresentationSurfaceV1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    StraightCommitted,
    DestroyCommitted,
}

#[test]
fn color_representation_wire_is_double_buffered_and_destroy_unsets_on_commit() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<ColorClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let manager = globals
            .bind::<wp_color_representation_manager_v1::WpColorRepresentationManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let representation = manager.get_surface(&surface, &handle, ());
        representation.set_alpha_mode(wp_color_representation_surface_v1::AlphaMode::Straight);
        surface.commit();
        queue.roundtrip(&mut ColorClient).unwrap();
        step_tx.send(Step::StraightCommitted).unwrap();
        release_rx.recv().unwrap();

        representation.destroy();
        surface.commit();
        queue.roundtrip(&mut ColorClient).unwrap();
        step_tx.send(Step::DestroyCommitted).unwrap();
        release_rx.recv().unwrap();

        surface.destroy();
        manager.destroy();
    });

    dispatch_until_step(&mut runtime, &step_rx, Step::StraightCommitted);
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .color_representation
            .first_committed()
            .alpha_mode,
        ColorAlphaMode::Straight
    );
    release_tx.send(()).unwrap();

    dispatch_until_step(&mut runtime, &step_rx, Step::DestroyCommitted);
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .color_representation
            .first_committed(),
        ColorRepresentation::default()
    );
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

#[test]
fn color_representation_manager_rejects_duplicate_surface_objects() {
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
        let (globals, mut queue) = registry_queue_init::<ColorClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let manager = globals
            .bind::<wp_color_representation_manager_v1::WpColorRepresentationManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let _first = manager.get_surface(&surface, &handle, ());
        let _duplicate = manager.get_surface(&surface, &handle, ());
        assert!(queue.roundtrip(&mut ColorClient).is_err());
        let error = connection
            .protocol_error()
            .expect("expected protocol error");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    let result = dispatch_until_result(&mut runtime, &result_rx);
    assert_eq!(
        result,
        (
            "wp_color_representation_manager_v1".to_owned(),
            wp_color_representation_manager_v1::Error::SurfaceExists as u32,
        )
    );
    client.join().unwrap();
}

fn dispatch_until_step(runtime: &mut WaylandRuntime, steps: &mpsc::Receiver<Step>, expected: Step) {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            assert_eq!(step, expected);
            return;
        }
    }
    panic!("color-representation client did not complete before the dispatch limit");
}

fn dispatch_until_result<T>(runtime: &mut WaylandRuntime, results: &mpsc::Receiver<T>) -> T {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("color-representation protocol error did not arrive before the dispatch limit");
}

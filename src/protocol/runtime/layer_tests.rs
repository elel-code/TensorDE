use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use super::*;

#[derive(Debug, Eq, PartialEq)]
enum LayerClientEvent {
    Configured { width: u32, height: u32 },
    Destroyed,
}

#[derive(Default)]
struct LayerClient {
    configured: Option<(u32, u32)>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for LayerClient {
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

delegate_noop!(LayerClient: ignore wl_compositor::WlCompositor);
delegate_noop!(LayerClient: ignore wl_surface::WlSurface);
delegate_noop!(LayerClient: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for LayerClient {
    fn event(
        state: &mut Self,
        surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            surface.ack_configure(serial);
            state.configured = Some((width, height));
        }
    }
}

#[test]
fn layer_surface_lifecycle_uses_tensor_map_and_exclusive_zone() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    super::tests::install_test_output(&mut runtime);
    let output = runtime.state.space.outputs().next().unwrap().clone();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();

    let (event_tx, event_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<LayerClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let layer_shell = globals
            .bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(&handle, 1..=5, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Top,
            "tensor-layer-map-test".to_owned(),
            &handle,
            (),
        );
        layer_surface.set_size(0, 32);
        layer_surface.set_anchor(
            zwlr_layer_surface_v1::Anchor::Top
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        );
        layer_surface.set_exclusive_zone(32);
        surface.commit();

        let mut state = LayerClient::default();
        while state.configured.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let (width, height) = state.configured.unwrap();
        event_tx
            .send(LayerClientEvent::Configured { width, height })
            .unwrap();
        release_rx.recv().unwrap();

        layer_surface.destroy();
        surface.destroy();
        connection.roundtrip().unwrap();
        event_tx.send(LayerClientEvent::Destroyed).unwrap();
    });

    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        LayerClientEvent::Configured {
            width: 800,
            height: 32,
        }
    );
    let (layer_count, zone) = runtime.state.layer_test_snapshot(&output).unwrap();
    assert_eq!(layer_count, 1);
    assert_eq!(zone.loc, (0, 32).into());
    assert_eq!(zone.size, (800, 608).into());

    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        LayerClientEvent::Destroyed
    );
    let (layer_count, zone) = runtime.state.layer_test_snapshot(&output).unwrap();
    assert_eq!(layer_count, 0);
    assert_eq!(zone.loc, (0, 0).into());
    assert_eq!(zone.size, (800, 640).into());
    client.join().unwrap();
}

fn dispatch_until(
    runtime: &mut WaylandRuntime,
    events: &mpsc::Receiver<LayerClientEvent>,
) -> LayerClientEvent {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(event) = events.try_recv() {
            return event;
        }
    }
    panic!("Wayland layer client did not complete before the dispatch limit");
}

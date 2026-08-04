use std::{
    os::{fd::AsFd, unix::net::UnixStream},
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

use rustix::fs::{MemfdFlags, ftruncate, memfd_create};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface},
};
use wayland_protocols::xdg::shell::client::{xdg_popup, xdg_positioner, xdg_surface, xdg_wm_base};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use super::*;

#[derive(Debug, Eq, PartialEq)]
enum LayerClientEvent {
    Configured { width: u32, height: u32 },
    PopupAttached,
    Closed,
    Destroyed,
}

#[derive(Default)]
struct LayerClient {
    configured: Option<(u32, u32)>,
    configures: Vec<u32>,
    closed: bool,
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
delegate_noop!(LayerClient: ignore wl_buffer::WlBuffer);
delegate_noop!(LayerClient: ignore wl_shm::WlShm);
delegate_noop!(LayerClient: ignore wl_shm_pool::WlShmPool);
delegate_noop!(LayerClient: ignore xdg_positioner::XdgPositioner);
delegate_noop!(LayerClient: ignore xdg_popup::XdgPopup);
delegate_noop!(LayerClient: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for LayerClient {
    fn event(
        _state: &mut Self,
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

impl Dispatch<xdg_surface::XdgSurface, ()> for LayerClient {
    fn event(
        _state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for LayerClient {
    fn event(
        state: &mut Self,
        _surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                state.configures.push(serial);
                state.configured = Some((width, height));
            }
            zwlr_layer_surface_v1::Event::Closed => state.closed = true,
            _ => unreachable!(),
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
    assert_eq!(
        runtime.state.protocol_globals.layer_shell.surface_count(),
        1
    );
    assert_eq!(zone.loc, (0, 32).into());
    assert_eq!(zone.size, (800, 608).into());

    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        LayerClientEvent::Destroyed
    );
    let (layer_count, zone) = runtime.state.layer_test_snapshot(&output).unwrap();
    assert_eq!(layer_count, 0);
    assert_eq!(
        runtime.state.protocol_globals.layer_shell.surface_count(),
        0
    );
    assert_eq!(zone.loc, (0, 0).into());
    assert_eq!(zone.size, (800, 640).into());
    client.join().unwrap();
}

#[test]
fn layer_surface_becomes_the_parent_of_an_uncommitted_xdg_popup() {
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
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let layer_shell = globals
            .bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(&handle, 1..=5, ())
            .unwrap();

        let root = compositor.create_surface(&handle, ());
        let layer_surface = layer_shell.get_layer_surface(
            &root,
            None,
            zwlr_layer_shell_v1::Layer::Top,
            "tensor-layer-popup-test".to_owned(),
            &handle,
            (),
        );
        layer_surface.set_size(64, 32);
        layer_surface
            .set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
        root.commit();

        let popup_surface = compositor.create_surface(&handle, ());
        let popup_xdg_surface = wm_base.get_xdg_surface(&popup_surface, &handle, ());
        let positioner = wm_base.create_positioner(&handle, ());
        positioner.set_size(16, 16);
        positioner.set_anchor_rect(0, 0, 1, 1);
        let popup = popup_xdg_surface.get_popup(None, &positioner, &handle, ());
        layer_surface.get_popup(&popup);
        queue.roundtrip(&mut LayerClient::default()).unwrap();
        event_tx.send(LayerClientEvent::PopupAttached).unwrap();
        release_rx.recv().unwrap();

        popup.destroy();
        popup_xdg_surface.destroy();
        popup_surface.destroy();
        positioner.destroy();
        layer_surface.destroy();
        root.destroy();
        wm_base.destroy();
        connection.roundtrip().unwrap();
        event_tx.send(LayerClientEvent::Destroyed).unwrap();
    });

    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        LayerClientEvent::PopupAttached
    );
    assert_eq!(runtime.state.layer_test_popup_count(&output), 1);
    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        LayerClientEvent::Destroyed
    );
    assert_eq!(runtime.state.layer_test_popup_count(&output), 0);
    client.join().unwrap();
}

#[derive(Clone, Copy, Debug)]
enum LayerViolation {
    BufferBeforeAck,
    ZeroWidthWithoutOppositeAnchors,
    ExclusiveEdgeOutsideAnchor,
    InvalidNegativeExclusiveZone,
}

#[test]
fn layer_surface_reports_tensor_owned_wire_errors() {
    let cases = [
        (
            LayerViolation::BufferBeforeAck,
            u32::from(zwlr_layer_surface_v1::Error::InvalidSurfaceState),
        ),
        (
            LayerViolation::ZeroWidthWithoutOppositeAnchors,
            u32::from(zwlr_layer_surface_v1::Error::InvalidSize),
        ),
        (
            LayerViolation::ExclusiveEdgeOutsideAnchor,
            u32::from(zwlr_layer_surface_v1::Error::InvalidExclusiveEdge),
        ),
        (
            LayerViolation::InvalidNegativeExclusiveZone,
            u32::from(zwlr_layer_surface_v1::Error::InvalidSurfaceState),
        ),
    ];

    for (violation, expected_code) in cases {
        let (interface, code) = layer_protocol_error(violation);
        assert_eq!(interface, "zwlr_layer_surface_v1", "{violation:?}");
        assert_eq!(code, expected_code, "{violation:?}");
    }
}

fn layer_protocol_error(violation: LayerViolation) -> (String, u32) {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    super::tests::install_test_output(&mut runtime);
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(1);

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
            format!("tensor-layer-error-{violation:?}"),
            &handle,
            (),
        );

        match violation {
            LayerViolation::BufferBeforeAck => {
                let shm = globals
                    .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
                    .unwrap();
                let buffer = create_shm_buffer(&shm, &handle, 32, 32);
                layer_surface.set_size(32, 32);
                layer_surface.set_anchor(
                    zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left,
                );
                surface.commit();
                surface.attach(Some(&buffer), 0, 0);
                surface.commit();
            }
            LayerViolation::ZeroWidthWithoutOppositeAnchors => {
                layer_surface.set_size(0, 32);
                layer_surface.set_anchor(zwlr_layer_surface_v1::Anchor::Top);
                surface.commit();
            }
            LayerViolation::ExclusiveEdgeOutsideAnchor => {
                layer_surface.set_size(32, 32);
                layer_surface.set_anchor(zwlr_layer_surface_v1::Anchor::Top);
                layer_surface.set_exclusive_edge(zwlr_layer_surface_v1::Anchor::Left);
                surface.commit();
            }
            LayerViolation::InvalidNegativeExclusiveZone => {
                layer_surface.set_exclusive_zone(-2);
            }
        }

        assert!(queue.roundtrip(&mut LayerClient::default()).is_err());
        let error = connection
            .protocol_error()
            .expect("expected layer-shell protocol error");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    let result = dispatch_until_protocol_error(&mut runtime, &result_rx);
    client.join().unwrap();
    result
}

#[test]
fn stale_configure_after_remap_cannot_authorize_a_buffer() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    super::tests::install_test_output(&mut runtime);
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(1);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<LayerClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
            .unwrap();
        let layer_shell = globals
            .bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(&handle, 1..=5, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Top,
            "tensor-layer-remap-test".to_owned(),
            &handle,
            (),
        );
        let buffer = create_shm_buffer(&shm, &handle, 48, 32);
        let anchor = zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left;

        layer_surface.set_size(32, 32);
        layer_surface.set_anchor(anchor);
        surface.commit();
        let mut state = LayerClient::default();
        dispatch_until_configures(&mut queue, &mut state, 1);
        layer_surface.ack_configure(state.configures[0]);
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        queue.roundtrip(&mut state).unwrap();

        layer_surface.set_size(48, 32);
        surface.commit();
        dispatch_until_configures(&mut queue, &mut state, 2);
        let stale = state.configures[1];

        surface.attach(None, 0, 0);
        surface.commit();
        queue.roundtrip(&mut state).unwrap();
        assert_eq!(
            state.configures.len(),
            2,
            "unmap commit must not configure remap"
        );

        layer_surface.set_size(48, 32);
        layer_surface.set_anchor(anchor);
        surface.commit();
        dispatch_until_configures(&mut queue, &mut state, 3);

        layer_surface.ack_configure(stale);
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        assert!(queue.roundtrip(&mut state).is_err());
        let error = connection
            .protocol_error()
            .expect("stale configure must not authorize the remapped buffer");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    let (interface, code) = dispatch_until_protocol_error(&mut runtime, &result_rx);
    assert_eq!(interface, "zwlr_layer_surface_v1");
    assert_eq!(
        code,
        u32::from(zwlr_layer_surface_v1::Error::InvalidSurfaceState)
    );
    client.join().unwrap();
}

#[test]
fn output_removal_closes_layer_surface_and_drops_it_after_destroy() {
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
            "tensor-layer-close-test".to_owned(),
            &handle,
            (),
        );
        layer_surface.set_size(32, 32);
        layer_surface
            .set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
        surface.commit();

        let mut state = LayerClient::default();
        dispatch_until_configures(&mut queue, &mut state, 1);
        event_tx
            .send(LayerClientEvent::Configured {
                width: state.configured.unwrap().0,
                height: state.configured.unwrap().1,
            })
            .unwrap();
        release_rx.recv().unwrap();
        while !state.closed {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx.send(LayerClientEvent::Closed).unwrap();

        layer_surface.destroy();
        surface.destroy();
        connection.roundtrip().unwrap();
        event_tx.send(LayerClientEvent::Destroyed).unwrap();
    });

    assert!(matches!(
        dispatch_until(&mut runtime, &event_rx),
        LayerClientEvent::Configured { .. }
    ));
    assert_eq!(
        runtime.state.protocol_globals.layer_shell.surface_count(),
        1
    );
    runtime.state.remove_layer_output(&output);
    runtime.state.display_handle.flush_clients().unwrap();
    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        LayerClientEvent::Closed
    );
    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        LayerClientEvent::Destroyed
    );
    assert_eq!(
        runtime.state.protocol_globals.layer_shell.surface_count(),
        0
    );
    client.join().unwrap();
}

fn create_shm_buffer(
    shm: &wl_shm::WlShm,
    handle: &QueueHandle<LayerClient>,
    width: i32,
    height: i32,
) -> wl_buffer::WlBuffer {
    let size = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .expect("test buffer size fits i32");
    let fd = memfd_create("tensor-layer-shell-test", MemfdFlags::CLOEXEC).unwrap();
    ftruncate(&fd, u64::try_from(size).unwrap()).unwrap();
    let pool = shm.create_pool(fd.as_fd(), size, handle, ());
    let buffer = pool.create_buffer(
        0,
        width,
        height,
        width * 4,
        wl_shm::Format::Argb8888,
        handle,
        (),
    );
    pool.destroy();
    buffer
}

fn dispatch_until_configures(
    queue: &mut wayland_client::EventQueue<LayerClient>,
    state: &mut LayerClient,
    count: usize,
) {
    while state.configures.len() < count {
        queue.blocking_dispatch(state).unwrap();
    }
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

fn dispatch_until_protocol_error(
    runtime: &mut WaylandRuntime,
    results: &mpsc::Receiver<(String, u32)>,
) -> (String, u32) {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("Wayland layer protocol-error client did not complete before the dispatch limit");
}

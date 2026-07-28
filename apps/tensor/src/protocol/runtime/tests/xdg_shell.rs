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
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_v1,
};
use wayland_protocols::xdg::shell::client::{
    xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};

use super::*;

#[derive(Default)]
struct XdgClient {
    configures: Vec<u32>,
    preferred_scales: Vec<u32>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for XdgClient {
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

delegate_noop!(XdgClient: ignore wl_compositor::WlCompositor);
delegate_noop!(XdgClient: ignore wl_surface::WlSurface);
delegate_noop!(XdgClient: ignore wl_buffer::WlBuffer);
delegate_noop!(XdgClient: ignore wl_shm::WlShm);
delegate_noop!(XdgClient: ignore wl_shm_pool::WlShmPool);
delegate_noop!(XdgClient: ignore xdg_positioner::XdgPositioner);
delegate_noop!(XdgClient: ignore xdg_popup::XdgPopup);
delegate_noop!(XdgClient: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(XdgClient: ignore wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for XdgClient {
    fn event(
        state: &mut Self,
        _: &wp_fractional_scale_v1::WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            state.preferred_scales.push(scale);
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for XdgClient {
    fn event(
        _: &mut Self,
        base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for XdgClient {
    fn event(
        state: &mut Self,
        _: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            state.configures.push(serial);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum XdgViolation {
    BufferBeforeAck,
    InvalidSerial,
    DefunctRole,
    IncompletePositioner,
}

#[test]
fn xdg_shell_reports_tensor_owned_wire_errors() {
    assert_eq!(
        xdg_protocol_error(XdgViolation::BufferBeforeAck),
        (
            "xdg_surface".to_owned(),
            u32::from(xdg_surface::Error::UnconfiguredBuffer),
        )
    );
    assert_eq!(
        xdg_protocol_error(XdgViolation::InvalidSerial),
        (
            "xdg_surface".to_owned(),
            u32::from(xdg_surface::Error::InvalidSerial),
        )
    );
    assert_eq!(
        xdg_protocol_error(XdgViolation::DefunctRole),
        (
            "xdg_surface".to_owned(),
            u32::from(xdg_surface::Error::DefunctRoleObject),
        )
    );
    assert_eq!(
        xdg_protocol_error(XdgViolation::IncompletePositioner),
        (
            "xdg_wm_base".to_owned(),
            u32::from(xdg_wm_base::Error::InvalidPositioner),
        )
    );
}

fn xdg_protocol_error(violation: XdgViolation) -> (String, u32) {
    let mut runtime = test_runtime();
    let socket_path = runtime_socket(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<XdgClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());

        match violation {
            XdgViolation::BufferBeforeAck => {
                let shm = globals
                    .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
                    .unwrap();
                let buffer = create_shm_buffer(&shm, &handle, 32, 32);
                surface.commit();
                let mut state = XdgClient::default();
                dispatch_until_configures(&mut queue, &mut state, 1);
                surface.attach(Some(&buffer), 0, 0);
                surface.commit();
            }
            XdgViolation::InvalidSerial => {
                surface.commit();
                let mut state = XdgClient::default();
                dispatch_until_configures(&mut queue, &mut state, 1);
                xdg_surface.ack_configure(state.configures[0].wrapping_add(1));
            }
            XdgViolation::DefunctRole => xdg_surface.destroy(),
            XdgViolation::IncompletePositioner => {
                let child = compositor.create_surface(&handle, ());
                let child_xdg = base.get_xdg_surface(&child, &handle, ());
                let positioner = base.create_positioner(&handle, ());
                let _popup = child_xdg.get_popup(Some(&xdg_surface), &positioner, &handle, ());
            }
        }

        assert!(queue.roundtrip(&mut XdgClient::default()).is_err());
        let error = connection
            .protocol_error()
            .expect("expected XDG protocol error");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
        drop(toplevel);
    });

    let result = dispatch_until_error(&mut runtime, &result_rx);
    client.join().unwrap();
    result
}

#[test]
fn stale_configure_cannot_authorize_an_xdg_toplevel_remap() {
    let mut runtime = test_runtime();
    install_test_output(&mut runtime);
    let socket_path = runtime_socket(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<XdgClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
            .unwrap();
        let base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        let buffer = create_shm_buffer(&shm, &handle, 48, 32);
        let mut state = XdgClient::default();

        surface.commit();
        dispatch_until_configures(&mut queue, &mut state, 1);
        xdg_surface.ack_configure(state.configures[0]);
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        queue.roundtrip(&mut state).unwrap();

        toplevel.set_maximized();
        dispatch_until_configures(&mut queue, &mut state, 2);
        let stale = state.configures[1];

        surface.attach(None, 0, 0);
        surface.commit();
        queue.roundtrip(&mut state).unwrap();
        assert_eq!(
            state.configures.len(),
            2,
            "detach must not configure a remap"
        );

        surface.commit();
        dispatch_until_configures(&mut queue, &mut state, 3);
        xdg_surface.ack_configure(stale);
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();

        assert!(queue.roundtrip(&mut state).is_err());
        let error = connection
            .protocol_error()
            .expect("stale XDG configure must reject the remapped buffer");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    let (interface, code) = dispatch_until_error(&mut runtime, &result_rx);
    assert_eq!(interface, "xdg_surface");
    assert_eq!(code, u32::from(xdg_surface::Error::UnconfiguredBuffer));
    client.join().unwrap();
}

#[test]
fn xdg_popup_inherits_fractional_scale_after_role_attachment() {
    let mut runtime = test_runtime();
    install_test_output(&mut runtime);
    let socket_path = runtime_socket(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<XdgClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
            .unwrap();
        let base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let fractional = globals
            .bind::<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();

        let root = compositor.create_surface(&handle, ());
        let root_xdg = base.get_xdg_surface(&root, &handle, ());
        let _toplevel = root_xdg.get_toplevel(&handle, ());
        let buffer = create_shm_buffer(&shm, &handle, 64, 48);
        let mut state = XdgClient::default();
        root.commit();
        dispatch_until_configures(&mut queue, &mut state, 1);
        root_xdg.ack_configure(state.configures[0]);
        root.attach(Some(&buffer), 0, 0);
        root.commit();
        queue.roundtrip(&mut state).unwrap();

        let popup_surface = compositor.create_surface(&handle, ());
        let _popup_scale = fractional.get_fractional_scale(&popup_surface, &handle, ());
        let popup_xdg = base.get_xdg_surface(&popup_surface, &handle, ());
        let positioner = base.create_positioner(&handle, ());
        positioner.set_size(32, 24);
        positioner.set_anchor_rect(0, 0, 1, 1);
        let _popup = popup_xdg.get_popup(Some(&root_xdg), &positioner, &handle, ());
        queue.roundtrip(&mut state).unwrap();
        result_tx.send(state.preferred_scales).unwrap();
    });

    let scales = dispatch_until_result(&mut runtime, &result_rx);
    assert_eq!(scales.last(), Some(&150));
    client.join().unwrap();
}

fn test_runtime() -> WaylandRuntime {
    WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap()
}

fn runtime_socket(runtime: &WaylandRuntime) -> PathBuf {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    PathBuf::from(runtime_dir).join(runtime.socket_name())
}

fn create_shm_buffer(
    shm: &wl_shm::WlShm,
    handle: &QueueHandle<XdgClient>,
    width: i32,
    height: i32,
) -> wl_buffer::WlBuffer {
    let size = width * height * 4;
    let fd = memfd_create("tensor-xdg-shell-test", MemfdFlags::CLOEXEC).unwrap();
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
    queue: &mut wayland_client::EventQueue<XdgClient>,
    state: &mut XdgClient,
    count: usize,
) {
    while state.configures.len() < count {
        queue.blocking_dispatch(state).unwrap();
    }
}

fn dispatch_until_error(
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
    panic!("XDG protocol error did not arrive before the dispatch limit");
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
    panic!("XDG client result did not arrive before the dispatch limit");
}

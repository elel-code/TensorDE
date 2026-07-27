use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::xdg::{
    decoration::zv1::client::{zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1},
    shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;

#[derive(Clone, Copy, Debug)]
enum DecorationViolation {
    Duplicate,
    Orphaned,
}

struct DecorationClient;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for DecorationClient {
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

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for DecorationClient {
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

delegate_noop!(DecorationClient: ignore wl_compositor::WlCompositor);
delegate_noop!(DecorationClient: ignore wl_surface::WlSurface);
delegate_noop!(DecorationClient: ignore xdg_surface::XdgSurface);
delegate_noop!(DecorationClient: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(DecorationClient: ignore zxdg_decoration_manager_v1::ZxdgDecorationManagerV1);
delegate_noop!(DecorationClient: ignore zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1);

#[test]
fn decoration_rejects_duplicate_and_orphaned_objects_on_the_wire() {
    use zxdg_toplevel_decoration_v1::Error;

    assert_eq!(
        decoration_protocol_error(DecorationViolation::Duplicate),
        (
            "zxdg_decoration_manager_v1".to_owned(),
            Error::AlreadyConstructed as u32,
        )
    );
    assert_eq!(
        decoration_protocol_error(DecorationViolation::Orphaned),
        (
            "zxdg_toplevel_decoration_v1".to_owned(),
            Error::Orphaned as u32,
        )
    );
}

fn decoration_protocol_error(violation: DecorationViolation) -> (String, u32) {
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
        let (globals, mut queue) = registry_queue_init::<DecorationClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let decorations = globals
            .bind::<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        let _decoration = decorations.get_toplevel_decoration(&toplevel, &handle, ());
        match violation {
            DecorationViolation::Duplicate => {
                let _duplicate = decorations.get_toplevel_decoration(&toplevel, &handle, ());
            }
            DecorationViolation::Orphaned => toplevel.destroy(),
        }

        assert!(queue.roundtrip(&mut DecorationClient).is_err());
        let error = connection
            .protocol_error()
            .expect("expected protocol error");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    let result = dispatch_until_decoration_error(&mut runtime, &result_rx);
    client.join().unwrap();
    result
}

fn dispatch_until_decoration_error(
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
    panic!("decoration protocol error did not arrive before the dispatch limit");
}

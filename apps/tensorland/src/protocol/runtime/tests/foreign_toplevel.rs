use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::{
    ext::foreign_toplevel_list::v1::client::{
        ext_foreign_toplevel_handle_v1, ext_foreign_toplevel_list_v1,
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;

#[derive(Debug, Eq, PartialEq)]
enum ForeignEvent {
    Initial {
        identifier: String,
        title: String,
        app_id: String,
    },
    Updated {
        identifier: String,
        title: String,
        app_id: String,
    },
    ClosedAndFinished,
}

#[derive(Default)]
struct ForeignClient {
    identifier: Option<String>,
    title: Option<String>,
    app_id: Option<String>,
    done: u32,
    closed: bool,
    finished: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ForeignClient {
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

impl Dispatch<ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1, ()> for ForeignClient {
    fn event(
        state: &mut Self,
        _: &ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, ext_foreign_toplevel_list_v1::Event::Finished) {
            state.finished = true;
        }
    }

    wayland_client::event_created_child!(ForeignClient, ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1, ()> for ForeignClient {
    fn event(
        state: &mut Self,
        _: &ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_foreign_toplevel_handle_v1::Event::Identifier { identifier } => {
                state.identifier = Some(identifier);
            }
            ext_foreign_toplevel_handle_v1::Event::Title { title } => {
                state.title = Some(title);
            }
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                state.app_id = Some(app_id);
            }
            ext_foreign_toplevel_handle_v1::Event::Done => {
                state.done = state.done.saturating_add(1);
            }
            ext_foreign_toplevel_handle_v1::Event::Closed => state.closed = true,
            _ => unreachable!(),
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for ForeignClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for ForeignClient {
    fn event(
        _: &mut Self,
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

delegate_noop!(ForeignClient: ignore wl_compositor::WlCompositor);
delegate_noop!(ForeignClient: ignore wl_surface::WlSurface);
delegate_noop!(ForeignClient: ignore xdg_toplevel::XdgToplevel);

#[test]
fn foreign_toplevel_wire_updates_and_closes_one_stable_handle() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (event_tx, event_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<ForeignClient>(&connection).unwrap();
        let handle = queue.handle();
        let list = globals
            .bind::<ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        toplevel.set_title("first".into());
        toplevel.set_app_id("com.tensor.first".into());
        surface.commit();

        let mut state = ForeignClient::default();
        while state.title.as_deref() != Some("first")
            || state.app_id.as_deref() != Some("com.tensor.first")
        {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let identifier = state.identifier.clone().unwrap();
        event_tx
            .send(ForeignEvent::Initial {
                identifier: identifier.clone(),
                title: state.title.clone().unwrap(),
                app_id: state.app_id.clone().unwrap(),
            })
            .unwrap();
        release_rx.recv().unwrap();

        let previous_done = state.done;
        toplevel.set_title("second".into());
        while state.done == previous_done || state.title.as_deref() != Some("second") {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx
            .send(ForeignEvent::Updated {
                identifier,
                title: state.title.clone().unwrap(),
                app_id: state.app_id.clone().unwrap(),
            })
            .unwrap();
        release_rx.recv().unwrap();

        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
        while !state.closed {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        list.stop();
        while !state.finished {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx.send(ForeignEvent::ClosedAndFinished).unwrap();
        list.destroy();
    });

    let initial = dispatch_until_result(&mut runtime, &event_rx);
    let identifier = match initial {
        ForeignEvent::Initial {
            identifier,
            title,
            app_id,
        } => {
            assert_eq!(title, "first");
            assert_eq!(app_id, "com.tensor.first");
            assert!(!identifier.is_empty() && identifier.len() <= 32);
            identifier
        }
        event => panic!("expected initial foreign-toplevel state, got {event:?}"),
    };
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_result(&mut runtime, &event_rx),
        ForeignEvent::Updated {
            identifier,
            title: "second".into(),
            app_id: "com.tensor.first".into(),
        }
    );
    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until_result(&mut runtime, &event_rx),
        ForeignEvent::ClosedAndFinished
    );
    client.join().unwrap();
}

fn dispatch_until_result<T>(runtime: &mut WaylandRuntime, results: &mpsc::Receiver<T>) -> T {
    for _ in 0..400 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        runtime.state.on_loop_idle();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("foreign-toplevel client did not complete before the dispatch limit");
}

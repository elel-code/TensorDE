use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::xdg::{
    dialog::v1::client::{xdg_dialog_v1, xdg_wm_dialog_v1},
    shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use crate::{
    ecs::{ViewId, ViewPlacement},
    protocol::{serial::next_serial, state::DEFAULT_WORKSPACE},
};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DialogStep {
    Modal,
    NonModal,
    Destroyed,
    Recreated,
    ParentDestroyed,
}

#[derive(Default)]
struct DialogClient;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for DialogClient {
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

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for DialogClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for DialogClient {
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

delegate_noop!(DialogClient: ignore wl_compositor::WlCompositor);
delegate_noop!(DialogClient: ignore wl_surface::WlSurface);
delegate_noop!(DialogClient: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(DialogClient: ignore xdg_wm_dialog_v1::XdgWmDialogV1);
delegate_noop!(DialogClient: ignore xdg_dialog_v1::XdgDialogV1);

#[test]
fn dialog_attachment_modal_focus_and_destroy_are_executed() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let socket_path = runtime_socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<DialogClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let dialogs = globals
            .bind::<xdg_wm_dialog_v1::XdgWmDialogV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let parent_surface = compositor.create_surface(&handle, ());
        let parent_xdg = wm_base.get_xdg_surface(&parent_surface, &handle, ());
        let parent = parent_xdg.get_toplevel(&handle, ());
        parent_surface.commit();
        let child_surface = compositor.create_surface(&handle, ());
        let child_xdg = wm_base.get_xdg_surface(&child_surface, &handle, ());
        let child = child_xdg.get_toplevel(&handle, ());
        child.set_parent(Some(&parent));
        let dialog = dialogs.get_xdg_dialog(&child, &handle, ());
        dialog.set_modal();
        child_surface.commit();
        queue.roundtrip(&mut DialogClient).unwrap();
        step_tx.send(DialogStep::Modal).unwrap();
        release_rx.recv().unwrap();

        dialog.unset_modal();
        queue.roundtrip(&mut DialogClient).unwrap();
        step_tx.send(DialogStep::NonModal).unwrap();
        release_rx.recv().unwrap();

        dialog.destroy();
        queue.roundtrip(&mut DialogClient).unwrap();
        step_tx.send(DialogStep::Destroyed).unwrap();
        release_rx.recv().unwrap();

        let replacement = dialogs.get_xdg_dialog(&child, &handle, ());
        replacement.set_modal();
        queue.roundtrip(&mut DialogClient).unwrap();
        step_tx.send(DialogStep::Recreated).unwrap();
        release_rx.recv().unwrap();

        parent.destroy();
        parent_xdg.destroy();
        parent_surface.destroy();
        queue.roundtrip(&mut DialogClient).unwrap();
        step_tx.send(DialogStep::ParentDestroyed).unwrap();
        release_rx.recv().unwrap();
    });

    assert_eq!(
        dispatch_until_dialog_step(&mut runtime, &step_rx),
        DialogStep::Modal
    );
    assert_eq!(
        runtime.state.world.view_placement(ViewId::new(2)),
        Some(ViewPlacement::Attached {
            owner: ViewId::new(1),
            preferred_size: tensor_util::Size::new(480, 320),
        })
    );
    let parent_window = runtime
        .state
        .mapped_window_for_view(ViewId::new(1))
        .unwrap();
    runtime
        .state
        .focus_mapped_window(parent_window.clone(), next_serial());
    assert_eq!(
        runtime.state.world.focused_view(DEFAULT_WORKSPACE),
        Some(ViewId::new(2)),
        "a modal child redirects attempts to focus its parent"
    );
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_dialog_step(&mut runtime, &step_rx),
        DialogStep::NonModal
    );
    runtime
        .state
        .focus_mapped_window(parent_window, next_serial());
    assert_eq!(
        runtime.state.world.focused_view(DEFAULT_WORKSPACE),
        Some(ViewId::new(1))
    );
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_dialog_step(&mut runtime, &step_rx),
        DialogStep::Destroyed
    );
    assert_eq!(
        runtime.state.world.view_placement(ViewId::new(2)),
        Some(ViewPlacement::Tiled)
    );
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_dialog_step(&mut runtime, &step_rx),
        DialogStep::Recreated
    );
    assert!(matches!(
        runtime.state.world.view_placement(ViewId::new(2)),
        Some(ViewPlacement::Attached {
            owner,
            preferred_size: _
        }) if owner == ViewId::new(1)
    ));
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_dialog_step(&mut runtime, &step_rx),
        DialogStep::ParentDestroyed
    );
    assert_eq!(runtime.state.world.view_placement(ViewId::new(1)), None);
    assert_eq!(
        runtime.state.world.view_placement(ViewId::new(2)),
        Some(ViewPlacement::Tiled),
        "parent teardown makes the surviving dialog independent"
    );
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

#[test]
fn dialog_manager_rejects_duplicate_objects_on_the_wire() {
    use xdg_wm_dialog_v1::Error;

    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let socket_path = runtime_socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<DialogClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let dialogs = globals
            .bind::<xdg_wm_dialog_v1::XdgWmDialogV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        let _dialog = dialogs.get_xdg_dialog(&toplevel, &handle, ());
        let _duplicate = dialogs.get_xdg_dialog(&toplevel, &handle, ());

        assert!(queue.roundtrip(&mut DialogClient).is_err());
        let error = connection
            .protocol_error()
            .expect("expected protocol error");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    assert_eq!(
        dispatch_until_dialog_result(&mut runtime, &result_rx),
        ("xdg_wm_dialog_v1".to_owned(), Error::AlreadyUsed as u32)
    );
    client.join().unwrap();
}

fn runtime_socket_path(runtime: &WaylandRuntime) -> PathBuf {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    PathBuf::from(runtime_dir).join(runtime.socket_name())
}

fn dispatch_until_dialog_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<DialogStep>,
) -> DialogStep {
    dispatch_until_dialog_result(runtime, steps)
}

fn dispatch_until_dialog_result<T>(runtime: &mut WaylandRuntime, results: &mpsc::Receiver<T>) -> T {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("xdg-dialog client did not complete before the dispatch limit");
}

use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::xdg::{
    foreign::zv2::client::{
        zxdg_exported_v2, zxdg_exporter_v2, zxdg_imported_v2, zxdg_importer_v2,
    },
    shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};
use wayland_server::Resource;

use super::*;

#[derive(Debug, Eq, PartialEq)]
enum ForeignStep {
    Handle {
        handle: String,
        parent: u32,
        first_child: u32,
        second_child: u32,
    },
    ParentsSet,
    FirstImportDestroyed,
    ExportRevoked,
    InvalidHandleRejected,
    SurfaceParentSet,
    ExportSurfaceDestroyed,
    Cleaned,
}

#[derive(Default)]
struct ForeignClient {
    configure_count: usize,
    handle: Option<String>,
    destroyed_imports: Vec<u8>,
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
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.configure_count += 1;
        }
    }
}

impl Dispatch<zxdg_exported_v2::ZxdgExportedV2, ()> for ForeignClient {
    fn event(
        state: &mut Self,
        _: &zxdg_exported_v2::ZxdgExportedV2,
        event: zxdg_exported_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_exported_v2::Event::Handle { handle } = event {
            state.handle = Some(handle);
        }
    }
}

impl Dispatch<zxdg_imported_v2::ZxdgImportedV2, u8> for ForeignClient {
    fn event(
        state: &mut Self,
        _: &zxdg_imported_v2::ZxdgImportedV2,
        event: zxdg_imported_v2::Event,
        tag: &u8,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zxdg_imported_v2::Event::Destroyed) {
            state.destroyed_imports.push(*tag);
        }
    }
}

delegate_noop!(ForeignClient: ignore wl_compositor::WlCompositor);
delegate_noop!(ForeignClient: ignore wl_surface::WlSurface);
delegate_noop!(ForeignClient: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(ForeignClient: ignore zxdg_exporter_v2::ZxdgExporterV2);
delegate_noop!(ForeignClient: ignore zxdg_importer_v2::ZxdgImporterV2);

#[test]
fn xdg_foreign_tracks_independent_relationships_and_revokes_exactly() {
    let mut runtime = foreign_runtime();
    let socket_path = runtime_socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(0);
    let (continue_tx, continue_rx) = mpsc::sync_channel(0);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<ForeignClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let exporter = globals
            .bind::<zxdg_exporter_v2::ZxdgExporterV2, _, _>(&handle, 1..=1, ())
            .unwrap();
        let importer = globals
            .bind::<zxdg_importer_v2::ZxdgImporterV2, _, _>(&handle, 1..=1, ())
            .unwrap();
        let mut state = ForeignClient::default();

        let (parent, parent_xdg, parent_toplevel) = create_toplevel(&compositor, &wm_base, &handle);
        let (first_child, first_xdg, first_toplevel) =
            create_toplevel(&compositor, &wm_base, &handle);
        let (second_child, second_xdg, second_toplevel) =
            create_toplevel(&compositor, &wm_base, &handle);
        while state.configure_count < 3 {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        let exported = exporter.export_toplevel(&parent, &handle, ());
        while state.handle.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let foreign_handle = state.handle.take().unwrap();
        step_tx
            .send(ForeignStep::Handle {
                handle: foreign_handle.clone(),
                parent: parent.id().protocol_id(),
                first_child: first_child.id().protocol_id(),
                second_child: second_child.id().protocol_id(),
            })
            .unwrap();
        continue_rx.recv().unwrap();

        let first_import = importer.import_toplevel(foreign_handle.clone(), &handle, 1);
        let second_import = importer.import_toplevel(foreign_handle, &handle, 2);
        first_import.set_parent_of(&first_child);
        second_import.set_parent_of(&second_child);
        connection.roundtrip().unwrap();
        step_tx.send(ForeignStep::ParentsSet).unwrap();
        continue_rx.recv().unwrap();

        first_import.destroy();
        connection.roundtrip().unwrap();
        step_tx.send(ForeignStep::FirstImportDestroyed).unwrap();
        continue_rx.recv().unwrap();

        exported.destroy();
        while !state.destroyed_imports.contains(&2) {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        step_tx.send(ForeignStep::ExportRevoked).unwrap();
        continue_rx.recv().unwrap();

        let invalid = importer.import_toplevel("not-a-live-handle".to_owned(), &handle, 3);
        while !state.destroyed_imports.contains(&3) {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        invalid.destroy();
        second_import.destroy();
        step_tx.send(ForeignStep::InvalidHandleRejected).unwrap();
        continue_rx.recv().unwrap();

        let surface_export = exporter.export_toplevel(&parent, &handle, ());
        while state.handle.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let surface_handle = state.handle.take().unwrap();
        let surface_import = importer.import_toplevel(surface_handle, &handle, 4);
        surface_import.set_parent_of(&first_child);
        connection.roundtrip().unwrap();
        step_tx.send(ForeignStep::SurfaceParentSet).unwrap();
        continue_rx.recv().unwrap();

        parent_toplevel.destroy();
        parent_xdg.destroy();
        parent.destroy();
        while !state.destroyed_imports.contains(&4) {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        step_tx.send(ForeignStep::ExportSurfaceDestroyed).unwrap();
        continue_rx.recv().unwrap();

        surface_import.destroy();
        surface_export.destroy();
        importer.destroy();
        exporter.destroy();
        second_toplevel.destroy();
        second_xdg.destroy();
        second_child.destroy();
        first_toplevel.destroy();
        first_xdg.destroy();
        first_child.destroy();
        connection.roundtrip().unwrap();
        step_tx.send(ForeignStep::Cleaned).unwrap();
    });

    let (parent, first_child, second_child) = match dispatch_foreign_step(&mut runtime, &step_rx) {
        ForeignStep::Handle {
            handle,
            parent,
            first_child,
            second_child,
        } => {
            assert_eq!(handle.len(), 64);
            assert!(handle.bytes().all(|byte| byte.is_ascii_hexdigit()));
            (parent, first_child, second_child)
        }
        step => panic!("expected xdg-foreign handle, got {step:?}"),
    };
    assert_eq!(
        runtime.state.protocol_globals.xdg_foreign.counts(),
        (1, 0, 0)
    );
    continue_tx.send(()).unwrap();

    assert_eq!(
        dispatch_foreign_step(&mut runtime, &step_rx),
        ForeignStep::ParentsSet
    );
    assert_eq!(parent_id(&runtime, first_child), Some(parent));
    assert_eq!(parent_id(&runtime, second_child), Some(parent));
    assert_eq!(
        runtime.state.protocol_globals.xdg_foreign.counts(),
        (1, 2, 2)
    );
    continue_tx.send(()).unwrap();

    assert_eq!(
        dispatch_foreign_step(&mut runtime, &step_rx),
        ForeignStep::FirstImportDestroyed
    );
    assert_eq!(parent_id(&runtime, first_child), None);
    assert_eq!(parent_id(&runtime, second_child), Some(parent));
    assert_eq!(
        runtime.state.protocol_globals.xdg_foreign.counts(),
        (1, 1, 1)
    );
    continue_tx.send(()).unwrap();

    assert_eq!(
        dispatch_foreign_step(&mut runtime, &step_rx),
        ForeignStep::ExportRevoked
    );
    assert_eq!(parent_id(&runtime, second_child), None);
    assert_eq!(
        runtime.state.protocol_globals.xdg_foreign.counts(),
        (0, 0, 0)
    );
    continue_tx.send(()).unwrap();

    assert_eq!(
        dispatch_foreign_step(&mut runtime, &step_rx),
        ForeignStep::InvalidHandleRejected
    );
    assert_eq!(
        runtime.state.protocol_globals.xdg_foreign.counts(),
        (0, 0, 0)
    );
    continue_tx.send(()).unwrap();

    assert_eq!(
        dispatch_foreign_step(&mut runtime, &step_rx),
        ForeignStep::SurfaceParentSet
    );
    assert_eq!(parent_id(&runtime, first_child), Some(parent));
    assert_eq!(
        runtime.state.protocol_globals.xdg_foreign.counts(),
        (1, 1, 1)
    );
    continue_tx.send(()).unwrap();

    assert_eq!(
        dispatch_foreign_step(&mut runtime, &step_rx),
        ForeignStep::ExportSurfaceDestroyed
    );
    assert_eq!(parent_id(&runtime, first_child), None);
    assert_eq!(
        runtime.state.protocol_globals.xdg_foreign.counts(),
        (0, 0, 0)
    );
    continue_tx.send(()).unwrap();
    assert_eq!(
        dispatch_foreign_step(&mut runtime, &step_rx),
        ForeignStep::Cleaned
    );
    client.join().unwrap();
}

#[test]
fn xdg_foreign_rejects_exporting_a_non_toplevel_surface() {
    let mut runtime = foreign_runtime();
    let socket_path = runtime_socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, _queue) = registry_queue_init::<ForeignClient>(&connection).unwrap();
        let handle = _queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let exporter = globals
            .bind::<zxdg_exporter_v2::ZxdgExporterV2, _, _>(&handle, 1..=1, ())
            .unwrap();
        let plain_surface = compositor.create_surface(&handle, ());
        let _exported = exporter.export_toplevel(&plain_surface, &handle, ());
        result_tx.send(connection.roundtrip().is_err()).unwrap();
    });

    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(rejected) = result_rx.try_recv() {
            assert!(rejected);
            assert_eq!(
                runtime.state.protocol_globals.xdg_foreign.counts(),
                (0, 0, 0)
            );
            client.join().unwrap();
            return;
        }
    }
    panic!("invalid xdg-foreign export was not rejected before the dispatch limit");
}

fn foreign_runtime() -> WaylandRuntime {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    runtime
}

fn runtime_socket_path(runtime: &WaylandRuntime) -> PathBuf {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    PathBuf::from(runtime_dir).join(runtime.socket_name())
}

fn create_toplevel(
    compositor: &wl_compositor::WlCompositor,
    wm_base: &xdg_wm_base::XdgWmBase,
    handle: &QueueHandle<ForeignClient>,
) -> (
    wl_surface::WlSurface,
    xdg_surface::XdgSurface,
    xdg_toplevel::XdgToplevel,
) {
    let surface = compositor.create_surface(handle, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, handle, ());
    let toplevel = xdg_surface.get_toplevel(handle, ());
    surface.commit();
    (surface, xdg_surface, toplevel)
}

fn parent_id(runtime: &WaylandRuntime, child_id: u32) -> Option<u32> {
    let child = runtime.state.space.elements().find_map(|window| {
        let surface = window.wl_surface()?.into_owned();
        (surface.id().protocol_id() == child_id).then_some(surface)
    })?;
    runtime
        .state
        .protocol_globals
        .xdg_shell
        .toplevel_for_surface(&child)?
        .parent_surface()
        .map(|parent| parent.id().protocol_id())
}

fn dispatch_foreign_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<ForeignStep>,
) -> ForeignStep {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("xdg-foreign client did not complete before the dispatch limit");
}

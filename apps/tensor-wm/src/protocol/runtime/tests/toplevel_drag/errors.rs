use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{Connection, Proxy, globals::registry_queue_init};
use wayland_protocols::xdg::{
    shell::client::xdg_wm_base, toplevel_drag::v1::client::xdg_toplevel_drag_manager_v1,
};

use super::*;

#[derive(Clone, Copy, Debug)]
enum DragViolation {
    DuplicateSource,
    DuplicateAttach,
    SelectionMisuse,
    DestroyBeforeEnd,
}

#[test]
fn duplicate_source_is_a_manager_protocol_error() {
    assert_drag_protocol_error(DragViolation::DuplicateSource, 0);
}

#[test]
fn duplicate_live_attachment_is_a_drag_protocol_error() {
    assert_drag_protocol_error(DragViolation::DuplicateAttach, 0);
}

#[test]
fn toplevel_drag_source_cannot_become_a_selection() {
    assert_drag_protocol_error(DragViolation::SelectionMisuse, 0);
}

#[test]
fn drag_object_cannot_be_destroyed_before_drag_end() {
    assert_drag_protocol_error(DragViolation::DestroyBeforeEnd, 1);
}

fn assert_drag_protocol_error(violation: DragViolation, expected_code: u32) {
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
        let (globals, mut queue) = registry_queue_init::<DragClient>(&connection).unwrap();
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
        let data_manager = globals
            .bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(&handle, 1..=3, ())
            .unwrap();
        let drag_manager = globals
            .bind::<xdg_toplevel_drag_manager_v1::XdgToplevelDragManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let data_device = data_manager.get_data_device(&seat, &handle, ());
        let source = data_manager.create_data_source(&handle, ());
        let drag = drag_manager.get_xdg_toplevel_drag(&source, &handle, ());
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        let expected_object_id = match violation {
            DragViolation::DuplicateSource | DragViolation::SelectionMisuse => {
                drag_manager.id().protocol_id()
            }
            DragViolation::DuplicateAttach | DragViolation::DestroyBeforeEnd => {
                drag.id().protocol_id()
            }
        };
        match violation {
            DragViolation::DuplicateSource => {
                let _duplicate = drag_manager.get_xdg_toplevel_drag(&source, &handle, ());
            }
            DragViolation::DuplicateAttach => {
                drag.attach(&toplevel, 1, 2);
                drag.attach(&toplevel, 3, 4);
            }
            DragViolation::SelectionMisuse => data_device.set_selection(Some(&source), 0),
            DragViolation::DestroyBeforeEnd => drag.destroy(),
        }
        assert!(queue.roundtrip(&mut DragClient::default()).is_err());
        let error = connection
            .protocol_error()
            .expect("xdg-toplevel-drag violation has a protocol error");
        result_tx
            .send((error.object_id, expected_object_id, error.code))
            .unwrap();
        drop((toplevel, xdg_surface, surface, data_device));
    });

    let (object_id, expected_object_id, code) = dispatch_until_error(&mut runtime, &result_rx);
    if !matches!(violation, DragViolation::DestroyBeforeEnd) {
        assert_eq!(object_id, expected_object_id);
    }
    assert_eq!(code, expected_code);
    client.join().unwrap();
}

fn dispatch_until_error(
    runtime: &mut WaylandRuntime,
    results: &mpsc::Receiver<(u32, u32, u32)>,
) -> (u32, u32, u32) {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("xdg-toplevel-drag protocol error did not arrive before the dispatch limit");
}

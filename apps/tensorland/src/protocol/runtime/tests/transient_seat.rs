use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry, wl_seat},
};
use wayland_protocols::ext::transient_seat::v1::client::{
    ext_transient_seat_manager_v1, ext_transient_seat_v1,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};

use super::*;

#[derive(Debug, Eq, PartialEq)]
enum TransientEvent {
    Ready(u32),
    Denied,
}

#[derive(Default)]
struct TransientClient {
    event: Option<TransientEvent>,
    ready_count: usize,
    denied_count: usize,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for TransientClient {
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

impl Dispatch<ext_transient_seat_v1::ExtTransientSeatV1, ()> for TransientClient {
    fn event(
        state: &mut Self,
        _: &ext_transient_seat_v1::ExtTransientSeatV1,
        event: ext_transient_seat_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.event = Some(match event {
            ext_transient_seat_v1::Event::Ready { global_name } => {
                state.ready_count += 1;
                TransientEvent::Ready(global_name)
            }
            ext_transient_seat_v1::Event::Denied => {
                state.denied_count += 1;
                TransientEvent::Denied
            }
            _ => return,
        });
    }
}

delegate_noop!(TransientClient: ignore wl_seat::WlSeat);
delegate_noop!(TransientClient: ignore ext_transient_seat_manager_v1::ExtTransientSeatManagerV1);
delegate_noop!(TransientClient: ignore zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1);
delegate_noop!(TransientClient: ignore zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1);
delegate_noop!(TransientClient: ignore zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1);
delegate_noop!(TransientClient: ignore zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1);

#[test]
fn transient_seat_ready_names_a_creator_scoped_global_and_destroy_removes_it() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<TransientClient>(&connection).unwrap();
        let handle = queue.handle();
        let manager = globals
            .bind::<ext_transient_seat_manager_v1::ExtTransientSeatManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let seat = manager.create(&handle, ());
        let mut state = TransientClient::default();
        while state.event.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let event = state.event.take().unwrap();
        let TransientEvent::Ready(global_name) = event else {
            panic!("unrestricted client was denied a transient seat");
        };
        let advertised = globals.contents().with_list(|list| {
            list.iter().any(|global| {
                global.name == global_name && global.interface == wl_seat::WlSeat::interface().name
            })
        });
        step_tx.send((global_name, advertised)).unwrap();
        release_rx.recv().unwrap();

        let transient_wl_seat = globals.registry().bind(global_name, 9, &handle, ());
        let pointer_manager = globals
            .bind::<zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1, _, _>(
                &handle,
                1..=2,
                (),
            )
            .unwrap();
        let pointer = pointer_manager.create_virtual_pointer(Some(&transient_wl_seat), &handle, ());
        pointer.motion(7, 2.0, -1.0);
        queue.roundtrip(&mut state).unwrap();
        step_tx.send((global_name, true)).unwrap();
        release_rx.recv().unwrap();

        pointer.destroy();
        queue.roundtrip(&mut state).unwrap();
        step_tx.send((global_name, true)).unwrap();
        release_rx.recv().unwrap();

        let keyboard_manager = globals
            .bind::<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let keyboard = keyboard_manager.create_virtual_keyboard(&transient_wl_seat, &handle, ());
        queue.roundtrip(&mut state).unwrap();
        step_tx.send((global_name, true)).unwrap();
        release_rx.recv().unwrap();

        keyboard.destroy();
        queue.roundtrip(&mut state).unwrap();
        step_tx.send((global_name, true)).unwrap();
        release_rx.recv().unwrap();

        seat.destroy();
        queue.roundtrip(&mut state).unwrap();
        let removed = globals
            .contents()
            .with_list(|list| list.iter().all(|global| global.name != global_name));
        step_tx.send((global_name, removed)).unwrap();
        release_rx.recv().unwrap();
        manager.destroy();
    });

    let (global_name, advertised) = dispatch_until_transient_step(&mut runtime, &step_rx);
    assert!(global_name > 0);
    assert!(advertised);
    assert_eq!(
        runtime.state.protocol_globals.transient_seat.live_count(),
        1
    );
    release_tx.send(()).unwrap();

    let _ = dispatch_until_transient_step(&mut runtime, &step_rx);
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .transient_seat
            .pointer_snapshot(1),
        Some((1, 1))
    );
    assert_eq!(runtime.state.input_seat.pointer_location(), None);
    release_tx.send(()).unwrap();

    let _ = dispatch_until_transient_step(&mut runtime, &step_rx);
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .transient_seat
            .pointer_snapshot(1),
        Some((0, 1))
    );
    release_tx.send(()).unwrap();

    let _ = dispatch_until_transient_step(&mut runtime, &step_rx);
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .transient_seat
            .keyboard_snapshot(1),
        Some((1, 0))
    );
    assert!(!runtime.state.protocol_globals.virtual_keyboard.is_active());
    release_tx.send(()).unwrap();

    let _ = dispatch_until_transient_step(&mut runtime, &step_rx);
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .transient_seat
            .keyboard_snapshot(1),
        Some((0, 0))
    );
    release_tx.send(()).unwrap();

    let (removed_name, removed) = dispatch_until_transient_step(&mut runtime, &step_rx);
    assert_eq!(removed_name, global_name);
    assert!(removed);
    assert_eq!(
        runtime.state.protocol_globals.transient_seat.live_count(),
        0
    );
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

#[test]
fn transient_seat_capacity_denies_the_seventeenth_handle() {
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
        let (globals, mut queue) = registry_queue_init::<TransientClient>(&connection).unwrap();
        let handle = queue.handle();
        let manager = globals
            .bind::<ext_transient_seat_manager_v1::ExtTransientSeatManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let seats = (0..=16)
            .map(|_| manager.create(&handle, ()))
            .collect::<Vec<_>>();
        let mut state = TransientClient::default();
        while state.ready_count + state.denied_count < seats.len() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        result_tx
            .send((state.ready_count, state.denied_count))
            .unwrap();
        // Drop the connection without protocol destructors. Server-side
        // resource destruction must remove every creator-owned seat.
        drop(seats);
        drop(manager);
    });

    let counts = dispatch_until_transient_step(&mut runtime, &result_rx);
    assert_eq!(counts, (16, 1));
    assert_eq!(
        runtime.state.protocol_globals.transient_seat.live_count(),
        16
    );
    client.join().unwrap();
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if runtime.state.protocol_globals.transient_seat.live_count() == 0 {
            return;
        }
    }
    panic!("transient seats survived creator teardown");
}

fn dispatch_until_transient_step<T>(runtime: &mut WaylandRuntime, steps: &mpsc::Receiver<T>) -> T {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("transient-seat client did not complete before the dispatch limit");
}

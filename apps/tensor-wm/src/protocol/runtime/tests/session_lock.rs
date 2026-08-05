use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_output, wl_registry, wl_surface},
};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1,
};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LockEvent {
    Locked,
    Finished,
}

#[derive(Default)]
struct LockClient {
    lock_events: Vec<LockEvent>,
    configure: Option<(u32, u32, u32)>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for LockClient {
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

impl Dispatch<ext_session_lock_v1::ExtSessionLockV1, ()> for LockClient {
    fn event(
        state: &mut Self,
        _: &ext_session_lock_v1::ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_session_lock_v1::Event::Locked => state.lock_events.push(LockEvent::Locked),
            ext_session_lock_v1::Event::Finished => state.lock_events.push(LockEvent::Finished),
            _ => unreachable!(),
        }
    }
}

impl Dispatch<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, ()> for LockClient {
    fn event(
        state: &mut Self,
        _: &ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            state.configure = Some((serial, width, height));
        }
    }
}

delegate_noop!(LockClient: ignore wl_compositor::WlCompositor);
delegate_noop!(LockClient: ignore wl_output::WlOutput);
delegate_noop!(LockClient: ignore wl_surface::WlSurface);
delegate_noop!(LockClient: ignore ext_session_lock_manager_v1::ExtSessionLockManagerV1);

#[test]
fn session_lock_confirms_without_outputs_and_rejects_a_second_controller() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let socket_path = runtime_socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(0);
    let (continue_tx, continue_rx) = mpsc::sync_channel(0);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<LockClient>(&connection).unwrap();
        let handle = queue.handle();
        let manager = globals
            .bind::<ext_session_lock_manager_v1::ExtSessionLockManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let first = manager.lock(&handle, ());
        let second = manager.lock(&handle, ());
        let mut state = LockClient::default();
        while state.lock_events.len() < 2 {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        step_tx.send(state.lock_events.clone()).unwrap();
        continue_rx.recv().unwrap();
        first.unlock_and_destroy();
        second.destroy();
        connection.roundtrip().unwrap();
        step_tx.send(Vec::new()).unwrap();
    });

    assert_eq!(
        dispatch_lock_step(&mut runtime, &step_rx),
        vec![LockEvent::Locked, LockEvent::Finished]
    );
    assert!(runtime.state.session_is_locked());
    continue_tx.send(()).unwrap();
    assert!(dispatch_lock_step(&mut runtime, &step_rx).is_empty());
    assert!(!runtime.state.session_is_locked());
    assert_eq!(runtime.state.protocol_globals.session_lock.counts(), (0, 0));
    client.join().unwrap();
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SurfaceStep {
    Configured(u32, u32),
    Locked,
    Unlocked,
    Cancelled,
    Cleaned,
}

#[test]
fn session_lock_with_an_output_waits_for_a_present_completion_and_cancels_exactly() {
    let mut runtime = lock_runtime_with_output();
    let socket_path = runtime_socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(0);
    let (continue_tx, continue_rx) = mpsc::sync_channel(0);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<LockClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let output = globals
            .bind::<wl_output::WlOutput, _, _>(&handle, 1..=4, ())
            .unwrap();
        let manager = globals
            .bind::<ext_session_lock_manager_v1::ExtSessionLockManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let lock = manager.lock(&handle, ());
        let surface = compositor.create_surface(&handle, ());
        let lock_surface = lock.get_lock_surface(&surface, &output, &handle, ());
        let mut state = LockClient::default();
        while state.configure.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let (serial, width, height) = state.configure.take().unwrap();
        lock_surface.ack_configure(serial);
        connection.flush().unwrap();
        step_tx
            .send(SurfaceStep::Configured(width, height))
            .unwrap();
        continue_rx.recv().unwrap();
        lock.destroy();
        connection.roundtrip().unwrap();
        step_tx.send(SurfaceStep::Cancelled).unwrap();
        continue_rx.recv().unwrap();
        lock_surface.destroy();
        surface.destroy();
        manager.destroy();
        connection.roundtrip().unwrap();
        step_tx.send(SurfaceStep::Cleaned).unwrap();
    });

    assert_eq!(
        dispatch_surface_step(&mut runtime, &step_rx),
        SurfaceStep::Configured(800, 640)
    );
    assert!(runtime.state.session_is_locked());
    assert_eq!(runtime.state.protocol_globals.session_lock.counts(), (1, 1));
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .session_lock
            .active_output_count(),
        1
    );
    continue_tx.send(()).unwrap();
    assert_eq!(
        dispatch_surface_step(&mut runtime, &step_rx),
        SurfaceStep::Cancelled
    );
    assert!(!runtime.state.session_is_locked());
    assert_eq!(runtime.state.protocol_globals.session_lock.counts(), (1, 1));
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .session_lock
            .active_output_count(),
        0
    );
    continue_tx.send(()).unwrap();
    assert_eq!(
        dispatch_surface_step(&mut runtime, &step_rx),
        SurfaceStep::Cleaned
    );
    assert_eq!(runtime.state.protocol_globals.session_lock.counts(), (0, 0));
    client.join().unwrap();
}

#[test]
fn session_lock_confirms_only_the_submitted_protected_frame_completion() {
    let mut runtime = lock_runtime_with_output();
    let socket_path = runtime_socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(0);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<LockClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let output = globals
            .bind::<wl_output::WlOutput, _, _>(&handle, 1..=4, ())
            .unwrap();
        let manager = globals
            .bind::<ext_session_lock_manager_v1::ExtSessionLockManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let lock = manager.lock(&handle, ());
        let surface = compositor.create_surface(&handle, ());
        let lock_surface = lock.get_lock_surface(&surface, &output, &handle, ());
        let mut state = LockClient::default();
        while state.configure.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        step_tx.send(SurfaceStep::Configured(0, 0)).unwrap();
        while !state.lock_events.contains(&LockEvent::Locked) {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        step_tx.send(SurfaceStep::Locked).unwrap();
        lock.unlock_and_destroy();
        lock_surface.destroy();
        surface.destroy();
        manager.destroy();
        connection.roundtrip().unwrap();
        step_tx.send(SurfaceStep::Unlocked).unwrap();
    });

    assert_eq!(
        dispatch_surface_step(&mut runtime, &step_rx),
        SurfaceStep::Configured(0, 0)
    );
    let output = runtime.state.space.outputs().next().unwrap().id();
    assert!(
        runtime
            .state
            .protocol_globals
            .session_lock
            .frame_completed(output, 41)
            .is_none()
    );
    runtime
        .state
        .protocol_globals
        .session_lock
        .frame_submitted(output, 42);
    assert!(
        runtime
            .state
            .protocol_globals
            .session_lock
            .frame_completed(output, 41)
            .is_none()
    );
    let lock = runtime
        .state
        .protocol_globals
        .session_lock
        .frame_completed(output, 42)
        .expect("the exact protected frame completion confirms the lock");
    lock.locked();
    runtime.state.flush_wayland_clients();
    assert_eq!(
        dispatch_surface_step(&mut runtime, &step_rx),
        SurfaceStep::Locked
    );
    assert!(runtime.state.session_is_locked());
    assert!(
        runtime
            .state
            .protocol_globals
            .session_lock
            .surface_for_output(output)
            .is_some()
    );
    runtime.state.session_lock_output_removed(output);
    runtime
        .state
        .protocol_globals
        .session_lock
        .output_added(output);
    assert!(
        runtime
            .state
            .protocol_globals
            .session_lock
            .surface_for_output(output)
            .is_none(),
        "a surface from a retired output instance must not migrate to a reconnect"
    );
    assert_eq!(
        dispatch_surface_step(&mut runtime, &step_rx),
        SurfaceStep::Unlocked
    );
    assert!(!runtime.state.session_is_locked());
    client.join().unwrap();
}

#[test]
fn session_lock_rejects_commit_before_the_first_ack() {
    let mut runtime = lock_runtime_with_output();
    let socket_path = runtime_socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, queue) = registry_queue_init::<LockClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let output = globals
            .bind::<wl_output::WlOutput, _, _>(&handle, 1..=4, ())
            .unwrap();
        let manager = globals
            .bind::<ext_session_lock_manager_v1::ExtSessionLockManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let lock = manager.lock(&handle, ());
        let surface = compositor.create_surface(&handle, ());
        let _lock_surface = lock.get_lock_surface(&surface, &output, &handle, ());
        surface.commit();
        result_tx.send(connection.roundtrip().is_err()).unwrap();
    });

    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(rejected) = result_rx.try_recv() {
            assert!(rejected);
            assert!(!runtime.state.session_is_locked());
            client.join().unwrap();
            return;
        }
    }
    panic!("pre-ack lock-surface commit was not rejected before the dispatch limit");
}

#[derive(Clone, Copy, Debug)]
enum InvalidLockRequest {
    DuplicateOutput,
    InvalidSerial,
    NullBuffer,
    UnlockBeforeConfirmation,
}

#[test]
fn session_lock_rejects_duplicate_output_surface() {
    assert_invalid_lock_request(InvalidLockRequest::DuplicateOutput);
}

#[test]
fn session_lock_rejects_unknown_configure_serial() {
    assert_invalid_lock_request(InvalidLockRequest::InvalidSerial);
}

#[test]
fn session_lock_rejects_an_explicit_null_buffer() {
    assert_invalid_lock_request(InvalidLockRequest::NullBuffer);
}

#[test]
fn session_lock_rejects_unlock_before_protected_frame_completion() {
    assert_invalid_lock_request(InvalidLockRequest::UnlockBeforeConfirmation);
}

fn assert_invalid_lock_request(request: InvalidLockRequest) {
    let mut runtime = lock_runtime_with_output();
    let socket_path = runtime_socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<LockClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let output = globals
            .bind::<wl_output::WlOutput, _, _>(&handle, 1..=4, ())
            .unwrap();
        let manager = globals
            .bind::<ext_session_lock_manager_v1::ExtSessionLockManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let lock = manager.lock(&handle, ());
        let surface = compositor.create_surface(&handle, ());
        let lock_surface = lock.get_lock_surface(&surface, &output, &handle, ());
        match request {
            InvalidLockRequest::DuplicateOutput => {
                let second = compositor.create_surface(&handle, ());
                let _second_lock_surface = lock.get_lock_surface(&second, &output, &handle, ());
            }
            InvalidLockRequest::UnlockBeforeConfirmation => lock.unlock_and_destroy(),
            InvalidLockRequest::InvalidSerial | InvalidLockRequest::NullBuffer => {
                let mut state = LockClient::default();
                while state.configure.is_none() {
                    queue.blocking_dispatch(&mut state).unwrap();
                }
                let (serial, _, _) = state.configure.unwrap();
                if matches!(request, InvalidLockRequest::InvalidSerial) {
                    lock_surface.ack_configure(serial.wrapping_add(1));
                } else {
                    lock_surface.ack_configure(serial);
                    surface.attach(None, 0, 0);
                    surface.commit();
                }
            }
        }
        result_tx.send(connection.roundtrip().is_err()).unwrap();
    });

    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(rejected) = result_rx.try_recv() {
            assert!(rejected, "{request:?} was not rejected");
            assert!(!runtime.state.session_is_locked());
            client.join().unwrap();
            return;
        }
    }
    panic!("{request:?} was not rejected before the dispatch limit");
}

fn lock_runtime_with_output() -> WaylandRuntime {
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

fn dispatch_lock_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<Vec<LockEvent>>,
) -> Vec<LockEvent> {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("session-lock client did not complete before the dispatch limit");
}

fn dispatch_surface_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<SurfaceStep>,
) -> SurfaceStep {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("session-lock surface did not complete before the dispatch limit");
}

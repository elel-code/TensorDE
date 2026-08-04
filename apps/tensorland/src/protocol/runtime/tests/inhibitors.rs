use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::wp::{
    idle_inhibit::zv1::client::{zwp_idle_inhibit_manager_v1, zwp_idle_inhibitor_v1},
    keyboard_shortcuts_inhibit::zv1::client::{
        zwp_keyboard_shortcuts_inhibit_manager_v1, zwp_keyboard_shortcuts_inhibitor_v1,
    },
};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InhibitorStep {
    Active,
    OneIdleRemoved,
    Cleared,
}

#[derive(Default)]
struct InhibitorClient {
    shortcut_active_events: u8,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for InhibitorClient {
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

impl Dispatch<zwp_keyboard_shortcuts_inhibitor_v1::ZwpKeyboardShortcutsInhibitorV1, ()>
    for InhibitorClient
{
    fn event(
        state: &mut Self,
        _: &zwp_keyboard_shortcuts_inhibitor_v1::ZwpKeyboardShortcutsInhibitorV1,
        event: zwp_keyboard_shortcuts_inhibitor_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zwp_keyboard_shortcuts_inhibitor_v1::Event::Active) {
            state.shortcut_active_events = state.shortcut_active_events.saturating_add(1);
        }
    }
}

delegate_noop!(InhibitorClient: ignore wl_compositor::WlCompositor);
delegate_noop!(InhibitorClient: ignore wl_surface::WlSurface);
delegate_noop!(InhibitorClient: ignore wl_seat::WlSeat);
delegate_noop!(InhibitorClient: ignore zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1);
delegate_noop!(InhibitorClient: ignore zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1);
delegate_noop!(InhibitorClient: ignore zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1);

#[test]
fn inhibitors_keep_exact_lifetimes_and_publish_shortcut_activation() {
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
        let (globals, mut queue) = registry_queue_init::<InhibitorClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let idle = globals
            .bind::<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let shortcuts = globals
            .bind::<
                zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
                _,
                _,
            >(&handle, 1..=1, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let idle_a = idle.create_inhibitor(&surface, &handle, ());
        let idle_b = idle.create_inhibitor(&surface, &handle, ());
        let shortcut = shortcuts.inhibit_shortcuts(&surface, &seat, &handle, ());
        let mut state = InhibitorClient::default();
        queue.roundtrip(&mut state).unwrap();
        assert_eq!(state.shortcut_active_events, 1);
        step_tx.send(InhibitorStep::Active).unwrap();
        release_rx.recv().unwrap();

        idle_a.destroy();
        queue.roundtrip(&mut state).unwrap();
        step_tx.send(InhibitorStep::OneIdleRemoved).unwrap();
        release_rx.recv().unwrap();

        shortcut.destroy();
        idle_b.destroy();
        queue.roundtrip(&mut state).unwrap();
        step_tx.send(InhibitorStep::Cleared).unwrap();
        release_rx.recv().unwrap();

        shortcuts.destroy();
        idle.destroy();
        seat.release();
        surface.destroy();
    });

    assert_eq!(
        dispatch_until_inhibitor_step(&mut runtime, &step_rx),
        InhibitorStep::Active
    );
    assert_eq!(runtime.state.idle_inhibitor_count(), 2);
    assert_eq!(runtime.state.shortcut_inhibitor_count(), 1);
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_inhibitor_step(&mut runtime, &step_rx),
        InhibitorStep::OneIdleRemoved
    );
    assert_eq!(runtime.state.idle_inhibitor_count(), 1);
    assert_eq!(runtime.state.shortcut_inhibitor_count(), 1);
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_inhibitor_step(&mut runtime, &step_rx),
        InhibitorStep::Cleared
    );
    assert_eq!(runtime.state.idle_inhibitor_count(), 0);
    assert_eq!(runtime.state.shortcut_inhibitor_count(), 0);
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

#[test]
fn duplicate_shortcut_inhibitor_is_a_manager_protocol_error() {
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
        let (globals, mut queue) = registry_queue_init::<InhibitorClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let shortcuts = globals
            .bind::<
                zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
                _,
                _,
            >(&handle, 1..=1, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let _first = shortcuts.inhibit_shortcuts(&surface, &seat, &handle, ());
        let _duplicate = shortcuts.inhibit_shortcuts(&surface, &seat, &handle, ());

        assert!(queue.roundtrip(&mut InhibitorClient::default()).is_err());
        let error = connection
            .protocol_error()
            .expect("expected protocol error");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    let result = dispatch_until_inhibitor_error(&mut runtime, &result_rx);
    client.join().unwrap();
    assert_eq!(
        result,
        (
            "zwp_keyboard_shortcuts_inhibit_manager_v1".to_owned(),
            zwp_keyboard_shortcuts_inhibit_manager_v1::Error::AlreadyInhibited as u32,
        )
    );
}

fn dispatch_until_inhibitor_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<InhibitorStep>,
) -> InhibitorStep {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("inhibitor client did not complete before the dispatch limit");
}

fn dispatch_until_inhibitor_error(
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
    panic!("inhibitor protocol error did not arrive before the dispatch limit");
}

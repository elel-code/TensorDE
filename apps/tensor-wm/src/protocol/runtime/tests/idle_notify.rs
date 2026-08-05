use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::{
    ext::idle_notify::v1::client::{ext_idle_notification_v1, ext_idle_notifier_v1},
    wp::idle_inhibit::zv1::client::{zwp_idle_inhibit_manager_v1, zwp_idle_inhibitor_v1},
};

use super::*;

const IDLE_TIMEOUT_MS: u32 = 80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NotificationClass {
    Inhibitable,
    InputOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NotificationEvent {
    Idled(NotificationClass),
    Resumed(NotificationClass),
}

#[derive(Debug, Eq, PartialEq)]
enum IdleStep {
    Registered,
    Events(Vec<NotificationEvent>),
    Inhibited,
    Uninhibited,
    Done,
}

#[derive(Default)]
struct IdleClient {
    events: Vec<NotificationEvent>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for IdleClient {
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

impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, NotificationClass> for IdleClient {
    fn event(
        state: &mut Self,
        _: &ext_idle_notification_v1::ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        class: &NotificationClass,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_idle_notification_v1::Event::Idled => {
                state.events.push(NotificationEvent::Idled(*class));
            }
            ext_idle_notification_v1::Event::Resumed => {
                state.events.push(NotificationEvent::Resumed(*class));
            }
            _ => unreachable!(),
        }
    }
}

delegate_noop!(IdleClient: ignore wl_compositor::WlCompositor);
delegate_noop!(IdleClient: ignore wl_surface::WlSurface);
delegate_noop!(IdleClient: ignore wl_seat::WlSeat);
delegate_noop!(IdleClient: ignore ext_idle_notifier_v1::ExtIdleNotifierV1);
delegate_noop!(IdleClient: ignore zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1);
delegate_noop!(IdleClient: ignore zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1);

#[test]
fn idle_notify_waits_for_cqe_and_keeps_input_only_notifications_running() {
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
    let client = spawn_idle_client(socket_path, step_tx, release_rx);

    assert_eq!(
        dispatch_until_idle_step(&mut runtime, &step_rx),
        IdleStep::Registered
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .idle_notify
            .notification_count(),
        2
    );
    assert_eq!(runtime.state.protocol_globals.idle_notify.idle_count(), 0);

    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(runtime.state.protocol_globals.idle_notify.idle_count(), 0);
    std::thread::sleep(Duration::from_millis(90));
    assert_eq!(
        runtime.state.protocol_globals.idle_notify.idle_count(),
        0,
        "timer expiry alone must not run protocol policy before its CQE is consumed"
    );
    assert!(runtime.state.complete_idle_timer());
    assert_eq!(runtime.state.protocol_globals.idle_notify.idle_count(), 2);
    runtime.state.flush_wayland_clients();
    release_tx.send(()).unwrap();
    assert_events(
        dispatch_until_idle_step(&mut runtime, &step_rx),
        &[
            NotificationEvent::Idled(NotificationClass::Inhibitable),
            NotificationEvent::Idled(NotificationClass::InputOnly),
        ],
    );

    runtime.state.notify_idle_activity();
    assert_eq!(runtime.state.protocol_globals.idle_notify.idle_count(), 0);
    runtime.state.flush_wayland_clients();
    release_tx.send(()).unwrap();
    assert_events(
        dispatch_until_idle_step(&mut runtime, &step_rx),
        &[
            NotificationEvent::Resumed(NotificationClass::Inhibitable),
            NotificationEvent::Resumed(NotificationClass::InputOnly),
        ],
    );

    let first_deadline = runtime
        .state
        .protocol_globals
        .idle_notify
        .armed_deadline()
        .expect("activity must arm an idle deadline");
    std::thread::sleep(Duration::from_millis(60));
    runtime.state.notify_idle_activity();
    assert_eq!(
        runtime.state.protocol_globals.idle_notify.armed_deadline(),
        Some(first_deadline),
        "steady input must not resubmit a later timer while one completion is pending"
    );
    std::thread::sleep(Duration::from_millis(30));
    assert!(runtime.state.complete_idle_timer());
    assert_eq!(runtime.state.protocol_globals.idle_notify.idle_count(), 0);
    assert!(
        runtime
            .state
            .protocol_globals
            .idle_notify
            .armed_deadline()
            .is_some_and(|deadline| deadline > first_deadline),
        "the early CQE must explicitly rearm the shifted deadline"
    );

    assert_eq!(
        dispatch_until_idle_step(&mut runtime, &step_rx),
        IdleStep::Inhibited
    );
    assert_eq!(runtime.state.idle_inhibitor_count(), 1);
    std::thread::sleep(Duration::from_millis(110));
    assert!(runtime.state.complete_idle_timer());
    assert_eq!(runtime.state.protocol_globals.idle_notify.idle_count(), 1);
    runtime.state.flush_wayland_clients();
    release_tx.send(()).unwrap();
    assert_events(
        dispatch_until_idle_step(&mut runtime, &step_rx),
        &[NotificationEvent::Idled(NotificationClass::InputOnly)],
    );

    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until_idle_step(&mut runtime, &step_rx),
        IdleStep::Uninhibited
    );
    assert_eq!(runtime.state.idle_inhibitor_count(), 0);
    std::thread::sleep(Duration::from_millis(110));
    assert!(runtime.state.complete_idle_timer());
    assert_eq!(runtime.state.protocol_globals.idle_notify.idle_count(), 2);
    runtime.state.flush_wayland_clients();
    release_tx.send(()).unwrap();
    assert_events(
        dispatch_until_idle_step(&mut runtime, &step_rx),
        &[NotificationEvent::Idled(NotificationClass::Inhibitable)],
    );

    runtime.state.notify_idle_activity();
    runtime.state.flush_wayland_clients();
    release_tx.send(()).unwrap();
    assert_events(
        dispatch_until_idle_step(&mut runtime, &step_rx),
        &[
            NotificationEvent::Resumed(NotificationClass::Inhibitable),
            NotificationEvent::Resumed(NotificationClass::InputOnly),
        ],
    );
    assert_eq!(
        dispatch_until_idle_step(&mut runtime, &step_rx),
        IdleStep::Done
    );
    client.join().unwrap();
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .idle_notify
            .notification_count(),
        0
    );
}

fn spawn_idle_client(
    socket_path: PathBuf,
    steps: mpsc::SyncSender<IdleStep>,
    releases: mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<IdleClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let notifier = globals
            .bind::<ext_idle_notifier_v1::ExtIdleNotifierV1, _, _>(&handle, 2..=2, ())
            .unwrap();
        let inhibit_manager = globals
            .bind::<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let inhibitable = notifier.get_idle_notification(
            IDLE_TIMEOUT_MS,
            &seat,
            &handle,
            NotificationClass::Inhibitable,
        );
        let input_only = notifier.get_input_idle_notification(
            IDLE_TIMEOUT_MS,
            &seat,
            &handle,
            NotificationClass::InputOnly,
        );
        let mut state = IdleClient::default();
        queue.roundtrip(&mut state).unwrap();
        steps.send(IdleStep::Registered).unwrap();

        releases.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        send_events(&steps, &mut state);

        releases.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        send_events(&steps, &mut state);

        let inhibitor = inhibit_manager.create_inhibitor(&surface, &handle, ());
        queue.roundtrip(&mut state).unwrap();
        steps.send(IdleStep::Inhibited).unwrap();

        releases.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        send_events(&steps, &mut state);

        releases.recv().unwrap();
        inhibitor.destroy();
        queue.roundtrip(&mut state).unwrap();
        steps.send(IdleStep::Uninhibited).unwrap();

        releases.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        send_events(&steps, &mut state);

        releases.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        send_events(&steps, &mut state);

        inhibitable.destroy();
        input_only.destroy();
        surface.destroy();
        notifier.destroy();
        inhibit_manager.destroy();
        seat.release();
        queue.roundtrip(&mut state).unwrap();
        steps.send(IdleStep::Done).unwrap();
    })
}

fn send_events(steps: &mpsc::SyncSender<IdleStep>, state: &mut IdleClient) {
    steps
        .send(IdleStep::Events(std::mem::take(&mut state.events)))
        .unwrap();
}

fn assert_events(step: IdleStep, expected: &[NotificationEvent]) {
    let IdleStep::Events(events) = step else {
        panic!("expected idle notification events, got {step:?}");
    };
    assert_eq!(events.len(), expected.len());
    for event in expected {
        assert!(events.contains(event), "missing {event:?} in {events:?}");
    }
}

fn dispatch_until_idle_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<IdleStep>,
) -> IdleStep {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("idle-notify client did not complete before the dispatch limit");
}

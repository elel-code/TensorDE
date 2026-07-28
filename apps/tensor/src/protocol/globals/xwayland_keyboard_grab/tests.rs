use std::{os::unix::net::UnixStream, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1, ext_session_lock_v1,
};
use wayland_protocols::xwayland::keyboard_grab::zv1::client::{
    zwp_xwayland_keyboard_grab_manager_v1, zwp_xwayland_keyboard_grab_v1,
};
use wayland_server::{Display, protocol::wl_surface::WlSurface};

use crate::{
    layout::{LayoutEngine, LayoutKind},
    protocol::{serial::next_serial, state::WaylandClientState},
    scene::SceneAppearance,
};

use super::*;

#[derive(Debug, Default)]
struct GrabClient {
    focus_events: Vec<(bool, u32)>,
    focused_surface: Option<u32>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for GrabClient {
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

delegate_noop!(GrabClient: ignore wl_compositor::WlCompositor);
delegate_noop!(GrabClient: ignore wl_surface::WlSurface);
delegate_noop!(GrabClient: ignore wl_seat::WlSeat);
delegate_noop!(GrabClient: ignore zwp_xwayland_keyboard_grab_manager_v1::ZwpXwaylandKeyboardGrabManagerV1);
delegate_noop!(GrabClient: ignore zwp_xwayland_keyboard_grab_v1::ZwpXwaylandKeyboardGrabV1);
delegate_noop!(GrabClient: ignore ext_session_lock_manager_v1::ExtSessionLockManagerV1);
delegate_noop!(GrabClient: ignore ext_session_lock_v1::ExtSessionLockV1);

impl Dispatch<wl_keyboard::WlKeyboard, ()> for GrabClient {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { surface, .. } => {
                let surface = surface.id().protocol_id();
                state.focused_surface = Some(surface);
                state.focus_events.push((true, surface));
            }
            wl_keyboard::Event::Leave { surface, .. } => {
                let observed = surface.id().protocol_id();
                let surface = if observed == 0 {
                    state.focused_surface.unwrap_or(observed)
                } else {
                    observed
                };
                state.focused_surface = None;
                state.focus_events.push((false, surface));
            }
            _ => {}
        }
    }
}

enum ClientCommand {
    ObserveFocus,
    StartGrab,
    StopGrab,
    LockSession,
    UnlockSession,
    DestroyGrabbedSurface,
}

#[derive(Debug, Eq, PartialEq)]
enum ClientEvent {
    Ready { logical: u32, grabbed: u32 },
    Focus(Vec<(bool, u32)>),
}

#[test]
fn global_is_xwayland_only_and_grab_switches_only_wire_focus() {
    let display = Display::<RuntimeState>::new().unwrap();
    let handle = display.handle();
    let (xwayland_server, xwayland_socket) = UnixStream::pair().unwrap();
    let xwayland_client = handle
        .clone()
        .insert_client(xwayland_server, XWaylandClientData::for_test())
        .unwrap();
    let (normal_server, normal_socket) = UnixStream::pair().unwrap();
    handle
        .clone()
        .insert_client(
            normal_server,
            std::sync::Arc::new(WaylandClientState::default()),
        )
        .unwrap();
    let mut state = RuntimeState::with_appearance(
        display,
        LayoutEngine::new(LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    );

    let (normal_tx, normal_rx) = mpsc::channel();
    let normal = std::thread::spawn(move || {
        let connection = Connection::from_socket(normal_socket).unwrap();
        let (globals, _queue) = registry_queue_init::<GrabClient>(&connection).unwrap();
        normal_tx
            .send(
                globals
                    .bind::<
                        zwp_xwayland_keyboard_grab_manager_v1::ZwpXwaylandKeyboardGrabManagerV1,
                        _,
                        _,
                    >(&_queue.handle(), 1..=1, ())
                    .is_err(),
            )
            .unwrap();
    });

    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    let xwayland = std::thread::spawn(move || {
        let connection = Connection::from_socket(xwayland_socket).unwrap();
        let (globals, mut queue) = registry_queue_init::<GrabClient>(&connection).unwrap();
        let queue_handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&queue_handle, 1..=6, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&queue_handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<zwp_xwayland_keyboard_grab_manager_v1::ZwpXwaylandKeyboardGrabManagerV1, _, _>(
                &queue_handle,
                1..=1,
                (),
            )
            .unwrap();
        let lock_manager = globals
            .bind::<ext_session_lock_manager_v1::ExtSessionLockManagerV1, _, _>(
                &queue_handle,
                1..=1,
                (),
            )
            .unwrap();
        let logical = compositor.create_surface(&queue_handle, ());
        let grabbed = compositor.create_surface(&queue_handle, ());
        let _keyboard = seat.get_keyboard(&queue_handle, ());
        let mut client_state = GrabClient::default();
        queue.roundtrip(&mut client_state).unwrap();
        event_tx
            .send(ClientEvent::Ready {
                logical: logical.id().protocol_id(),
                grabbed: grabbed.id().protocol_id(),
            })
            .unwrap();

        assert!(matches!(
            command_rx.recv().unwrap(),
            ClientCommand::ObserveFocus
        ));
        queue.roundtrip(&mut client_state).unwrap();
        event_tx
            .send(ClientEvent::Focus(std::mem::take(
                &mut client_state.focus_events,
            )))
            .unwrap();

        assert!(matches!(
            command_rx.recv().unwrap(),
            ClientCommand::StartGrab
        ));
        let grab = manager.grab_keyboard(&grabbed, &seat, &queue_handle, ());
        queue.roundtrip(&mut client_state).unwrap();
        event_tx
            .send(ClientEvent::Focus(std::mem::take(
                &mut client_state.focus_events,
            )))
            .unwrap();

        assert!(matches!(
            command_rx.recv().unwrap(),
            ClientCommand::LockSession
        ));
        let lock = lock_manager.lock(&queue_handle, ());
        queue.roundtrip(&mut client_state).unwrap();
        event_tx
            .send(ClientEvent::Focus(std::mem::take(
                &mut client_state.focus_events,
            )))
            .unwrap();

        assert!(matches!(
            command_rx.recv().unwrap(),
            ClientCommand::UnlockSession
        ));
        lock.unlock_and_destroy();
        queue.roundtrip(&mut client_state).unwrap();
        event_tx
            .send(ClientEvent::Focus(std::mem::take(
                &mut client_state.focus_events,
            )))
            .unwrap();

        assert!(matches!(
            command_rx.recv().unwrap(),
            ClientCommand::StopGrab
        ));
        grab.destroy();
        queue.roundtrip(&mut client_state).unwrap();
        event_tx
            .send(ClientEvent::Focus(client_state.focus_events))
            .unwrap();

        assert!(matches!(
            command_rx.recv().unwrap(),
            ClientCommand::StartGrab
        ));
        let _grab = manager.grab_keyboard(&grabbed, &seat, &queue_handle, ());
        client_state.focus_events = Vec::new();
        queue.roundtrip(&mut client_state).unwrap();
        event_tx
            .send(ClientEvent::Focus(std::mem::take(
                &mut client_state.focus_events,
            )))
            .unwrap();

        assert!(matches!(
            command_rx.recv().unwrap(),
            ClientCommand::DestroyGrabbedSurface
        ));
        grabbed.destroy();
        queue.roundtrip(&mut client_state).unwrap();
        event_tx
            .send(ClientEvent::Focus(client_state.focus_events))
            .unwrap();
    });

    assert!(dispatch_until(&mut state, &normal_rx));
    normal.join().unwrap();
    let ClientEvent::Ready { logical, grabbed } = dispatch_until(&mut state, &event_rx) else {
        panic!("XWayland client did not publish its surfaces");
    };
    let logical_surface = xwayland_client
        .object_from_protocol_id::<WlSurface>(&state.display_handle, logical)
        .unwrap();
    let keymap = state.input_seat.enable_keyboard().unwrap().to_owned();
    state
        .protocol_globals
        .seat
        .set_keyboard_enabled(true, Some(&keymap))
        .unwrap();
    state.set_keyboard_focus(Some(logical_surface.clone()), next_serial());

    command_tx.send(ClientCommand::ObserveFocus).unwrap();
    assert_eq!(
        dispatch_until(&mut state, &event_rx),
        ClientEvent::Focus(vec![(true, logical)])
    );
    command_tx.send(ClientCommand::StartGrab).unwrap();
    assert_eq!(
        dispatch_until(&mut state, &event_rx),
        ClientEvent::Focus(vec![(false, logical), (true, grabbed)])
    );
    assert_eq!(state.input_seat.keyboard_focus(), Some(&logical_surface));

    command_tx.send(ClientCommand::LockSession).unwrap();
    assert_eq!(
        dispatch_until(&mut state, &event_rx),
        ClientEvent::Focus(vec![(false, grabbed)])
    );
    assert!(state.session_is_locked());
    command_tx.send(ClientCommand::UnlockSession).unwrap();
    assert_eq!(
        dispatch_until(&mut state, &event_rx),
        ClientEvent::Focus(vec![(true, grabbed)])
    );
    assert!(!state.session_is_locked());
    state.set_keyboard_focus(Some(logical_surface.clone()), next_serial());

    command_tx.send(ClientCommand::StopGrab).unwrap();
    assert_eq!(
        dispatch_until(&mut state, &event_rx),
        ClientEvent::Focus(vec![(false, grabbed), (true, logical)])
    );

    command_tx.send(ClientCommand::StartGrab).unwrap();
    assert_eq!(
        dispatch_until(&mut state, &event_rx),
        ClientEvent::Focus(vec![(false, logical), (true, grabbed)])
    );
    command_tx
        .send(ClientCommand::DestroyGrabbedSurface)
        .unwrap();
    assert_eq!(
        dispatch_until(&mut state, &event_rx),
        ClientEvent::Focus(vec![(false, grabbed), (true, logical)])
    );
    xwayland.join().unwrap();
}

#[test]
fn grab_capacity_is_fixed_and_a_destroyed_slot_is_reusable_in_the_same_batch() {
    let display = Display::<RuntimeState>::new().unwrap();
    let handle = display.handle();
    let (server, socket) = UnixStream::pair().unwrap();
    handle
        .clone()
        .insert_client(server, XWaylandClientData::for_test())
        .unwrap();
    let mut state = RuntimeState::with_appearance(
        display,
        LayoutEngine::new(LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    );

    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    let client = std::thread::spawn(move || {
        let connection = Connection::from_socket(socket).unwrap();
        let (globals, mut queue) = registry_queue_init::<GrabClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<zwp_xwayland_keyboard_grab_manager_v1::ZwpXwaylandKeyboardGrabManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let mut grabs = (0..MAX_XWAYLAND_KEYBOARD_GRABS)
            .map(|_| manager.grab_keyboard(&surface, &seat, &handle, ()))
            .collect::<Vec<_>>();
        let mut client_state = GrabClient::default();
        queue.roundtrip(&mut client_state).unwrap();
        event_tx.send(false).unwrap();
        command_rx.recv().unwrap();

        grabs.swap_remove(0).destroy();
        grabs.push(manager.grab_keyboard(&surface, &seat, &handle, ()));
        queue.roundtrip(&mut client_state).unwrap();
        event_tx.send(false).unwrap();
        command_rx.recv().unwrap();

        let _overflow = manager.grab_keyboard(&surface, &seat, &handle, ());
        event_tx
            .send(queue.roundtrip(&mut client_state).is_err())
            .unwrap();
    });

    assert!(!dispatch_until(&mut state, &event_rx));
    assert_eq!(
        state.protocol_globals.xwayland_keyboard_grab.grabs.len(),
        MAX_XWAYLAND_KEYBOARD_GRABS
    );
    command_tx.send(()).unwrap();
    assert!(!dispatch_until(&mut state, &event_rx));
    assert_eq!(
        state.protocol_globals.xwayland_keyboard_grab.grabs.len(),
        MAX_XWAYLAND_KEYBOARD_GRABS
    );
    command_tx.send(()).unwrap();
    assert!(dispatch_until(&mut state, &event_rx));
    client.join().unwrap();
}

fn dispatch_until<T>(state: &mut RuntimeState, receiver: &mpsc::Receiver<T>) -> T {
    for _ in 0..300 {
        state.dispatch_wayland_clients().unwrap();
        state.display_handle.flush_clients().unwrap();
        if let Ok(event) = receiver.try_recv() {
            return event;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("XWayland keyboard-grab client did not complete before the dispatch limit");
}

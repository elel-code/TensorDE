use std::{
    os::{fd::AsFd, unix::net::UnixStream},
    sync::mpsc,
    time::Duration,
};

use rustix::fs::{MemfdFlags, memfd_create};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1,
};
use wayland_server::{Display, protocol::wl_surface::WlSurface};

use crate::{
    layout::{LayoutEngine, LayoutKind},
    protocol::{serial::next_serial, state::WaylandClientState},
    scene::SceneAppearance,
};

use super::*;

#[derive(Debug, Default)]
struct KeyboardClient {
    focus: Vec<bool>,
    keys: Vec<(u32, bool)>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for KeyboardClient {
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

delegate_noop!(KeyboardClient: ignore wl_compositor::WlCompositor);
delegate_noop!(KeyboardClient: ignore wl_surface::WlSurface);
delegate_noop!(KeyboardClient: ignore wl_seat::WlSeat);
delegate_noop!(KeyboardClient: ignore zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1);
delegate_noop!(KeyboardClient: ignore zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1);

impl Dispatch<wl_keyboard::WlKeyboard, ()> for KeyboardClient {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { .. } => state.focus.push(true),
            wl_keyboard::Event::Leave { .. } => state.focus.push(false),
            wl_keyboard::Event::Key {
                key,
                state: key_state,
                ..
            } => state.keys.push((
                key,
                matches!(key_state, WEnum::Value(wl_keyboard::KeyState::Pressed)),
            )),
            _ => {}
        }
    }
}

enum Command {
    Observe,
    Inject,
    Destroy,
}

#[derive(Debug, Eq, PartialEq)]
enum Event {
    Ready(u32),
    Wire {
        focus: Vec<bool>,
        keys: Vec<(u32, bool)>,
    },
}

#[test]
fn virtual_keyboard_injects_with_a_bounded_keymap_and_releases_on_destroy() {
    let display = Display::<RuntimeState>::new().unwrap();
    let handle = display.handle();
    let (server, socket) = UnixStream::pair().unwrap();
    let server_client = handle
        .clone()
        .insert_client(server, std::sync::Arc::new(WaylandClientState::default()))
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
        let (globals, mut queue) = registry_queue_init::<KeyboardClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let _keyboard = seat.get_keyboard(&handle, ());
        let virtual_keyboard = manager.create_virtual_keyboard(&seat, &handle, ());
        let mut client_state = KeyboardClient::default();
        queue.roundtrip(&mut client_state).unwrap();
        event_tx
            .send(Event::Ready(surface.id().protocol_id()))
            .unwrap();

        assert!(matches!(command_rx.recv().unwrap(), Command::Observe));
        queue.roundtrip(&mut client_state).unwrap();
        send_wire(&event_tx, &mut client_state);

        assert!(matches!(command_rx.recv().unwrap(), Command::Inject));
        let keymap = default_keymap();
        let fd = memfd_create("tensor-virtual-keymap-test", MemfdFlags::CLOEXEC).unwrap();
        write_all(&fd, keymap.as_bytes());
        virtual_keyboard.keymap(
            wl_keyboard::KeymapFormat::XkbV1 as u32,
            fd.as_fd(),
            keymap.len() as u32,
        );
        virtual_keyboard.modifiers(0, 0, 0, 0);
        virtual_keyboard.key(7, 30, wl_keyboard::KeyState::Pressed as u32);
        queue.roundtrip(&mut client_state).unwrap();
        send_wire(&event_tx, &mut client_state);

        assert!(matches!(command_rx.recv().unwrap(), Command::Destroy));
        virtual_keyboard.destroy();
        queue.roundtrip(&mut client_state).unwrap();
        send_wire(&event_tx, &mut client_state);
    });

    let Event::Ready(surface_id) = dispatch_until(&mut state, &event_rx) else {
        panic!("virtual-keyboard client did not publish its surface");
    };
    let surface = server_client
        .object_from_protocol_id::<WlSurface>(&state.display_handle, surface_id)
        .unwrap();
    assert!(state.input_seat.keyboard_enabled());
    state.set_keyboard_focus(Some(surface), next_serial());

    command_tx.send(Command::Observe).unwrap();
    assert_eq!(
        dispatch_until(&mut state, &event_rx),
        Event::Wire {
            focus: vec![true],
            keys: vec![],
        }
    );
    command_tx.send(Command::Inject).unwrap();
    assert_eq!(
        dispatch_until(&mut state, &event_rx),
        Event::Wire {
            focus: vec![],
            keys: vec![(30, true)],
        }
    );
    command_tx.send(Command::Destroy).unwrap();
    assert_eq!(
        dispatch_until(&mut state, &event_rx),
        Event::Wire {
            focus: vec![false],
            keys: vec![(30, false)],
        }
    );
    assert!(!state.input_seat.keyboard_enabled());
    client.join().unwrap();
}

#[test]
fn key_before_keymap_is_a_fatal_protocol_error() {
    let display = Display::<RuntimeState>::new().unwrap();
    let handle = display.handle();
    let (server, socket) = UnixStream::pair().unwrap();
    handle
        .clone()
        .insert_client(server, std::sync::Arc::new(WaylandClientState::default()))
        .unwrap();
    let mut state = RuntimeState::with_appearance(
        display,
        LayoutEngine::new(LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    );
    let (event_tx, event_rx) = mpsc::channel();

    let client = std::thread::spawn(move || {
        let connection = Connection::from_socket(socket).unwrap();
        let (globals, mut queue) = registry_queue_init::<KeyboardClient>(&connection).unwrap();
        let handle = queue.handle();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let virtual_keyboard = manager.create_virtual_keyboard(&seat, &handle, ());
        virtual_keyboard.key(1, 30, wl_keyboard::KeyState::Pressed as u32);
        event_tx
            .send(queue.roundtrip(&mut KeyboardClient::default()).is_err())
            .unwrap();
    });

    assert!(dispatch_until(&mut state, &event_rx));
    client.join().unwrap();
    state.dispatch_wayland_clients().unwrap();
    assert!(!state.protocol_globals.virtual_keyboard.is_active());
    assert!(!state.input_seat.keyboard_enabled());
}

fn default_keymap() -> String {
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    xkb::Keymap::new_from_names(&context, "", "", "", "", None, xkb::KEYMAP_COMPILE_NO_FLAGS)
        .unwrap()
        .get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1)
}

fn write_all(fd: &impl AsFd, mut bytes: &[u8]) {
    let mut offset = 0;
    while !bytes.is_empty() {
        let written = rustix::io::pwrite(fd, bytes, offset).unwrap();
        assert_ne!(written, 0);
        bytes = &bytes[written..];
        offset += written as u64;
    }
}

fn send_wire(sender: &mpsc::Sender<Event>, state: &mut KeyboardClient) {
    sender
        .send(Event::Wire {
            focus: std::mem::take(&mut state.focus),
            keys: std::mem::take(&mut state.keys),
        })
        .unwrap();
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
    panic!("virtual-keyboard client did not complete before the dispatch limit");
}

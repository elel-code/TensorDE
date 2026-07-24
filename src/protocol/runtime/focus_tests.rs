use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use smithay::output::{Mode as OutputMode, Output, PhysicalProperties, Scale, Subpixel};
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use super::*;

#[derive(Debug, Eq, PartialEq)]
enum ClientEvent {
    ConfiguredActivated,
    KeyboardEntered,
}

#[derive(Default)]
struct FocusClient {
    configured: bool,
    activated: bool,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keyboard_entered: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for FocusClient {
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

delegate_noop!(FocusClient: ignore wl_compositor::WlCompositor);
delegate_noop!(FocusClient: ignore wl_surface::WlSurface);

impl Dispatch<wl_seat::WlSeat, ()> for FocusClient {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
            && capabilities.contains(wl_seat::Capability::Keyboard)
            && state.keyboard.is_none()
        {
            state.keyboard = Some(seat.get_keyboard(queue, ()));
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for FocusClient {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_keyboard::Event::Enter { .. }) {
            state.keyboard_entered = true;
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for FocusClient {
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

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for FocusClient {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Configure { states, .. } = event {
            state.activated = states.contains(&(xdg_toplevel::State::Activated as u8));
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for FocusClient {
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
            state.configured = true;
        }
    }
}

#[test]
fn focused_toplevel_is_activated_and_receives_late_keyboard_enter() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        crate::scene::SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    runtime.prepare(false).unwrap();

    let (event_tx, event_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<FocusClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let _seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let _toplevel = xdg_surface.get_toplevel(&handle, ());
        surface.commit();

        let mut state = FocusClient::default();
        while !(state.configured && state.activated) {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx.send(ClientEvent::ConfiguredActivated).unwrap();
        while !state.keyboard_entered {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx.send(ClientEvent::KeyboardEntered).unwrap();
        release_rx.recv().unwrap();
    });

    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        ClientEvent::ConfiguredActivated
    );
    assert!(
        runtime
            .state
            .world
            .focused_view(super::super::state::DEFAULT_WORKSPACE)
            .is_some()
    );
    runtime.state.input_devices.insert(
        "test-late-keyboard".to_owned(),
        super::super::state::InputDeviceCapabilities {
            keyboard: true,
            ..Default::default()
        },
    );
    runtime.state.reconcile_seat_capabilities();
    runtime.state.flush_wayland_clients();

    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        ClientEvent::KeyboardEntered
    );
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

fn install_test_output(runtime: &mut WaylandRuntime) {
    let mode = OutputMode {
        size: (1000, 800).into(),
        refresh: 60_000,
    };
    let output = Output::new(
        "focus-test-output".to_owned(),
        PhysicalProperties {
            size: (600, 340).into(),
            subpixel: Subpixel::Unknown,
            make: "Tensor".to_owned(),
            model: "Nested".to_owned(),
            serial_number: "focus-test".to_owned(),
        },
    );
    output.add_mode(mode);
    output.set_preferred(mode);
    output.change_current_state(
        Some(mode),
        None,
        Some(Scale::Fractional(1.25)),
        Some((0, 0).into()),
    );
    runtime.state.space.map_output(&output, (0, 0));
}

fn dispatch_until(
    runtime: &mut WaylandRuntime,
    events: &mpsc::Receiver<ClientEvent>,
) -> ClientEvent {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(event) = events.try_recv() {
            return event;
        }
    }
    panic!("Wayland focus client did not complete before the dispatch limit");
}

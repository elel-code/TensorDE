use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use tensor_host::{ConnectorId, PhysicalMode, SubpixelLayout};
use tensor_util::OutputScale;
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use crate::protocol::globals::output::Output;

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
    let _socket_completions = runtime.prepare_for_test(false).unwrap();

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
        tensor_input::DeviceId::new(1),
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

#[derive(Debug, Eq, PartialEq)]
enum LifecycleEvent {
    FirstActivated,
    SecondActivated,
    FirstRestored,
}

#[derive(Default)]
struct LifecycleClient {
    configured: [bool; 2],
    activated: [bool; 2],
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keyboard_enters: u8,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for LifecycleClient {
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

delegate_noop!(LifecycleClient: ignore wl_compositor::WlCompositor);
delegate_noop!(LifecycleClient: ignore wl_surface::WlSurface);

impl Dispatch<wl_seat::WlSeat, ()> for LifecycleClient {
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

impl Dispatch<wl_keyboard::WlKeyboard, ()> for LifecycleClient {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_keyboard::Event::Enter { .. }) {
            state.keyboard_enters = state.keyboard_enters.saturating_add(1);
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for LifecycleClient {
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

impl Dispatch<xdg_toplevel::XdgToplevel, usize> for LifecycleClient {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Configure { states, .. } = event {
            state.activated[*index] = states.contains(&(xdg_toplevel::State::Activated as u8));
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, usize> for LifecycleClient {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.configured[*index] = true;
        }
    }
}

#[test]
fn close_time_focus_transfers_activation_keyboard_and_scene_selection() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        crate::scene::SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();

    let (event_tx, event_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<LifecycleClient>(&connection).unwrap();
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
        let (_first_surface, _first_xdg_surface, _first_toplevel) =
            lifecycle_toplevel(&compositor, &wm_base, &handle, 0);

        let mut state = LifecycleClient::default();
        while !(state.configured[0] && state.activated[0]) {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx.send(LifecycleEvent::FirstActivated).unwrap();

        while state.keyboard_enters < 1 {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let (second_surface, second_xdg_surface, second_toplevel) =
            lifecycle_toplevel(&compositor, &wm_base, &handle, 1);
        while !(state.configured[1]
            && state.activated[1]
            && !state.activated[0]
            && state.keyboard_enters >= 2)
        {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx.send(LifecycleEvent::SecondActivated).unwrap();

        second_toplevel.destroy();
        second_xdg_surface.destroy();
        second_surface.destroy();
        while !(state.activated[0] && state.keyboard_enters >= 3) {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx.send(LifecycleEvent::FirstRestored).unwrap();
        release_rx.recv().unwrap();
    });

    assert_eq!(
        dispatch_lifecycle_until(&mut runtime, &event_rx),
        LifecycleEvent::FirstActivated
    );
    runtime.state.input_devices.insert(
        tensor_input::DeviceId::new(1),
        super::super::state::InputDeviceCapabilities {
            keyboard: true,
            ..Default::default()
        },
    );
    runtime.state.reconcile_seat_capabilities();
    runtime.state.flush_wayland_clients();

    assert_eq!(
        dispatch_lifecycle_until(&mut runtime, &event_rx),
        LifecycleEvent::SecondActivated
    );
    assert_eq!(
        dispatch_lifecycle_until(&mut runtime, &event_rx),
        LifecycleEvent::FirstRestored
    );
    assert_eq!(
        runtime
            .state
            .world
            .focused_view(super::super::state::DEFAULT_WORKSPACE),
        Some(crate::ecs::ViewId::new(1)),
        "the surviving first toplevel regains the ECS focus marker"
    );
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

fn lifecycle_toplevel(
    compositor: &wl_compositor::WlCompositor,
    wm_base: &xdg_wm_base::XdgWmBase,
    handle: &QueueHandle<LifecycleClient>,
    index: usize,
) -> (
    wl_surface::WlSurface,
    xdg_surface::XdgSurface,
    xdg_toplevel::XdgToplevel,
) {
    let surface = compositor.create_surface(handle, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, handle, index);
    let toplevel = xdg_surface.get_toplevel(handle, index);
    surface.commit();
    (surface, xdg_surface, toplevel)
}

fn install_test_output(runtime: &mut WaylandRuntime) {
    let mode = PhysicalMode::new(1000, 800, 60_000);
    let output = Output::new(
        ConnectorId::new(1, 1),
        "focus-test-output".to_owned(),
        (600, 340),
        SubpixelLayout::Unknown,
        vec![mode],
        mode,
        mode,
        OutputScale::from_f64(1.25).unwrap(),
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

fn dispatch_lifecycle_until(
    runtime: &mut WaylandRuntime,
    events: &mpsc::Receiver<LifecycleEvent>,
) -> LifecycleEvent {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(event) = events.try_recv() {
            return event;
        }
    }
    panic!("Wayland focus lifecycle client did not complete before the dispatch limit");
}

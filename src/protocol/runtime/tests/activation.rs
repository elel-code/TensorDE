use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::xdg::{
    activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1},
    shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;

#[derive(Debug, Eq, PartialEq)]
enum ActivationStep {
    Surfaces { first: u32, second: u32 },
    ExternalActivated,
    Token(String),
    Activated,
    Reused,
    InvalidSerialToken(String),
    InvalidSerialRejected,
    Destroyed,
}

enum ActivationCommand {
    ActivateExternal(String),
    Continue,
}

#[derive(Default)]
struct ActivationClient {
    configure_count: usize,
    token: Option<String>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ActivationClient {
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

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for ActivationClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for ActivationClient {
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

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, ()> for ActivationClient {
    fn event(
        state: &mut Self,
        _: &xdg_activation_token_v1::XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            state.token = Some(token);
        }
    }
}

delegate_noop!(ActivationClient: ignore wl_compositor::WlCompositor);
delegate_noop!(ActivationClient: ignore wl_seat::WlSeat);
delegate_noop!(ActivationClient: ignore wl_surface::WlSurface);
delegate_noop!(ActivationClient: ignore xdg_activation_v1::XdgActivationV1);
delegate_noop!(ActivationClient: ignore xdg_toplevel::XdgToplevel);

#[test]
fn activation_tokens_are_authorized_one_shot_and_release_builders() {
    let mut runtime = activation_runtime();
    let socket_path = runtime_socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(0);
    let (command_tx, command_rx) = mpsc::sync_channel(0);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<ActivationClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let activation = globals
            .bind::<xdg_activation_v1::XdgActivationV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let mut state = ActivationClient::default();

        let (first, first_xdg, first_toplevel) = create_toplevel(&compositor, &wm_base, &handle);
        while state.configure_count < 1 {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let (second, second_xdg, second_toplevel) = create_toplevel(&compositor, &wm_base, &handle);
        while state.configure_count < 2 {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        step_tx
            .send(ActivationStep::Surfaces {
                first: first.id().protocol_id(),
                second: second.id().protocol_id(),
            })
            .unwrap();
        let ActivationCommand::ActivateExternal(external_token) = command_rx.recv().unwrap() else {
            panic!("expected external activation command");
        };
        activation.activate(external_token, &first);
        connection.roundtrip().unwrap();
        step_tx.send(ActivationStep::ExternalActivated).unwrap();
        assert!(matches!(
            command_rx.recv().unwrap(),
            ActivationCommand::Continue
        ));

        let request = activation.get_activation_token(&handle, ());
        request.set_app_id("org.tensor.activation-test".to_owned());
        request.set_surface(&first);
        request.commit();
        while state.token.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let token = state.token.take().unwrap();
        step_tx.send(ActivationStep::Token(token.clone())).unwrap();
        activation.activate(token.clone(), &second);
        connection.roundtrip().unwrap();
        step_tx.send(ActivationStep::Activated).unwrap();
        assert!(matches!(
            command_rx.recv().unwrap(),
            ActivationCommand::Continue
        ));

        activation.activate(token, &first);
        connection.roundtrip().unwrap();
        step_tx.send(ActivationStep::Reused).unwrap();
        assert!(matches!(
            command_rx.recv().unwrap(),
            ActivationCommand::Continue
        ));

        let invalid_serial = activation.get_activation_token(&handle, ());
        invalid_serial.set_surface(&second);
        invalid_serial.set_serial(u32::MAX, &seat);
        invalid_serial.commit();
        while state.token.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let invalid_token = state.token.take().unwrap();
        step_tx
            .send(ActivationStep::InvalidSerialToken(invalid_token.clone()))
            .unwrap();
        activation.activate(invalid_token, &first);
        connection.roundtrip().unwrap();
        step_tx.send(ActivationStep::InvalidSerialRejected).unwrap();
        assert!(matches!(
            command_rx.recv().unwrap(),
            ActivationCommand::Continue
        ));

        invalid_serial.destroy();
        request.destroy();
        second_toplevel.destroy();
        second_xdg.destroy();
        second.destroy();
        first_toplevel.destroy();
        first_xdg.destroy();
        first.destroy();
        activation.destroy();
        connection.roundtrip().unwrap();
        step_tx.send(ActivationStep::Destroyed).unwrap();
    });

    let (first, second) = match dispatch_activation_step(&mut runtime, &step_rx) {
        ActivationStep::Surfaces { first, second } => (first, second),
        step => panic!("expected mapped surfaces, got {step:?}"),
    };
    assert_eq!(focused_surface_id(&mut runtime), Some(second));
    let external_token = runtime.state.issue_spawn_activation_token().unwrap();
    assert_eq!(runtime.state.protocol_globals.activation.token_count(), 1);
    command_tx
        .send(ActivationCommand::ActivateExternal(external_token))
        .unwrap();
    assert_eq!(
        dispatch_activation_step(&mut runtime, &step_rx),
        ActivationStep::ExternalActivated
    );
    assert_eq!(focused_surface_id(&mut runtime), Some(first));
    assert_eq!(runtime.state.protocol_globals.activation.token_count(), 0);
    command_tx.send(ActivationCommand::Continue).unwrap();

    let token = match dispatch_activation_step(&mut runtime, &step_rx) {
        ActivationStep::Token(token) => token,
        step => panic!("expected activation token, got {step:?}"),
    };
    assert_eq!(token.len(), 64);
    assert_eq!(runtime.state.protocol_globals.activation.token_count(), 1);
    assert_eq!(runtime.state.protocol_globals.activation.builder_count(), 1);
    assert_eq!(
        dispatch_activation_step(&mut runtime, &step_rx),
        ActivationStep::Activated
    );
    assert_eq!(focused_surface_id(&mut runtime), Some(second));
    assert_eq!(runtime.state.protocol_globals.activation.token_count(), 0);
    command_tx.send(ActivationCommand::Continue).unwrap();

    assert_eq!(
        dispatch_activation_step(&mut runtime, &step_rx),
        ActivationStep::Reused
    );
    assert_eq!(focused_surface_id(&mut runtime), Some(second));
    command_tx.send(ActivationCommand::Continue).unwrap();

    let invalid_token = match dispatch_activation_step(&mut runtime, &step_rx) {
        ActivationStep::InvalidSerialToken(token) => token,
        step => panic!("expected invalid-serial token, got {step:?}"),
    };
    assert_eq!(invalid_token.len(), 64);
    assert_eq!(runtime.state.protocol_globals.activation.token_count(), 0);
    assert_eq!(
        dispatch_activation_step(&mut runtime, &step_rx),
        ActivationStep::InvalidSerialRejected
    );
    assert_eq!(focused_surface_id(&mut runtime), Some(second));
    command_tx.send(ActivationCommand::Continue).unwrap();
    assert_eq!(
        dispatch_activation_step(&mut runtime, &step_rx),
        ActivationStep::Destroyed
    );
    for _ in 0..16 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(2), &mut runtime.state)
            .unwrap();
        if runtime.state.protocol_globals.activation.builder_count() == 0 {
            break;
        }
    }
    assert_eq!(runtime.state.protocol_globals.activation.builder_count(), 0);
    client.join().unwrap();
}

#[test]
fn committed_activation_token_rejects_all_further_requests() {
    let mut runtime = activation_runtime();
    let socket_path = runtime_socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<ActivationClient>(&connection).unwrap();
        let handle = queue.handle();
        let activation = globals
            .bind::<xdg_activation_v1::XdgActivationV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let request = activation.get_activation_token(&handle, ());
        request.commit();
        let mut state = ActivationClient::default();
        while state.token.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        request.set_app_id("reuse-must-fail".to_owned());
        result_tx.send(connection.roundtrip().is_err()).unwrap();
    });

    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(rejected) = result_rx.try_recv() {
            assert!(rejected);
            client.join().unwrap();
            return;
        }
    }
    panic!("activation token reuse was not rejected before the dispatch limit");
}

fn activation_runtime() -> WaylandRuntime {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    runtime.state.input_devices.insert(
        tensor_input::DeviceId::new(1),
        crate::protocol::state::InputDeviceCapabilities {
            keyboard: true,
            ..Default::default()
        },
    );
    runtime.state.reconcile_seat_capabilities();
    runtime
}

fn runtime_socket_path(runtime: &WaylandRuntime) -> PathBuf {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    PathBuf::from(runtime_dir).join(runtime.socket_name())
}

fn create_toplevel(
    compositor: &wl_compositor::WlCompositor,
    wm_base: &xdg_wm_base::XdgWmBase,
    handle: &QueueHandle<ActivationClient>,
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

fn focused_surface_id(runtime: &mut WaylandRuntime) -> Option<u32> {
    let focused = runtime
        .state
        .world
        .focused_view(crate::protocol::state::DEFAULT_WORKSPACE)?;
    runtime.state.space.elements().find_map(|window| {
        let surface = window.wl_surface()?.into_owned();
        (runtime.state.view_for_surface(&surface) == Some(focused))
            .then(|| surface.id().protocol_id())
    })
}

fn dispatch_activation_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<ActivationStep>,
) -> ActivationStep {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("xdg-activation client did not complete before the dispatch limit");
}

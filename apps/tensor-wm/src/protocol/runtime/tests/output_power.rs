use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use tensor_host::{ConnectorId, PhysicalMode, SubpixelLayout};
use tensor_util::OutputScale;
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_output, wl_registry},
};
use wayland_protocols_wlr::output_power_management::v1::client::{
    zwlr_output_power_manager_v1, zwlr_output_power_v1,
};

use crate::protocol::globals::output::Output;

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PowerEvent {
    Mode(bool),
    Failed,
}

#[derive(Default)]
struct OutputPowerClient {
    events: Vec<PowerEvent>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for OutputPowerClient {
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

impl Dispatch<zwlr_output_power_v1::ZwlrOutputPowerV1, ()> for OutputPowerClient {
    fn event(
        state: &mut Self,
        _: &zwlr_output_power_v1::ZwlrOutputPowerV1,
        event: zwlr_output_power_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_output_power_v1::Event::Mode {
                mode: WEnum::Value(zwlr_output_power_v1::Mode::Off),
            } => state.events.push(PowerEvent::Mode(false)),
            zwlr_output_power_v1::Event::Mode {
                mode: WEnum::Value(zwlr_output_power_v1::Mode::On),
            } => state.events.push(PowerEvent::Mode(true)),
            zwlr_output_power_v1::Event::Failed => state.events.push(PowerEvent::Failed),
            _ => {}
        }
    }
}

delegate_noop!(OutputPowerClient: ignore wl_output::WlOutput);
delegate_noop!(OutputPowerClient: ignore zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1);

#[test]
fn output_power_binds_but_fails_explicitly_without_a_native_kms_output() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let mode = PhysicalMode::new(1920, 1080, 60_000);
    let output = Output::new(
        ConnectorId::new(31, 41),
        "headless-output".to_owned(),
        (600, 340),
        SubpixelLayout::HorizontalRgb,
        vec![mode],
        mode,
        mode,
        OutputScale::ONE,
    );
    let _global = output.create_global(&runtime.state.display_handle);

    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (events_tx, events_rx) = mpsc::channel();

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<OutputPowerClient>(&connection).unwrap();
        let handle = queue.handle();
        let output = globals
            .bind::<wl_output::WlOutput, _, _>(&handle, 1..=4, ())
            .unwrap();
        let manager = globals
            .bind::<zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let control = manager.get_output_power(&output, &handle, ());
        let mut state = OutputPowerClient::default();
        queue.roundtrip(&mut state).unwrap();
        control.destroy();
        manager.destroy();
        queue.roundtrip(&mut state).unwrap();
        events_tx.send(state.events).unwrap();
    });

    let events = receive_power_events(&mut runtime, &events_rx);
    client.join().unwrap();
    assert_eq!(events, [PowerEvent::Failed]);
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .output_power()
            .control_count(),
        0
    );
}

fn receive_power_events(
    runtime: &mut WaylandRuntime,
    receiver: &mpsc::Receiver<Vec<PowerEvent>>,
) -> Vec<PowerEvent> {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(events) = receiver.try_recv() {
            return events;
        }
    }
    panic!("timed out waiting for output-power protocol events");
}

use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use tensor_host::{ConnectorId, PhysicalMode, SubpixelLayout};
use tensor_util::OutputScale;
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_output, wl_registry},
};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};

use crate::protocol::globals::output::Output;

use super::*;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct OutputEvents {
    name: Option<String>,
    description: Option<String>,
    physical_size: Option<(i32, i32)>,
    current_mode: Option<(i32, i32, i32)>,
    scale: Option<i32>,
    logical_position: Option<(i32, i32)>,
    logical_size: Option<(i32, i32)>,
    wl_done: u32,
    xdg_done: u32,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for OutputEvents {
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

impl Dispatch<wl_output::WlOutput, ()> for OutputEvents {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Geometry {
                physical_width,
                physical_height,
                ..
            } => state.physical_size = Some((physical_width, physical_height)),
            wl_output::Event::Mode {
                flags: WEnum::Value(flags),
                width,
                height,
                refresh,
            } if flags.contains(wl_output::Mode::Current) => {
                state.current_mode = Some((width, height, refresh))
            }
            wl_output::Event::Done => state.wl_done += 1,
            wl_output::Event::Scale { factor } => state.scale = Some(factor),
            wl_output::Event::Name { name } => state.name = Some(name),
            wl_output::Event::Description { description } => {
                state.description = Some(description);
            }
            _ => {}
        }
    }
}

delegate_noop!(OutputEvents: ignore zxdg_output_manager_v1::ZxdgOutputManagerV1);

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, ()> for OutputEvents {
    fn event(
        state: &mut Self,
        _: &zxdg_output_v1::ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                state.logical_position = Some((x, y));
            }
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                state.logical_size = Some((width, height));
            }
            zxdg_output_v1::Event::Done => state.xdg_done += 1,
            zxdg_output_v1::Event::Name { name } => state.name = Some(name),
            zxdg_output_v1::Event::Description { description } => {
                state.description = Some(description);
            }
            _ => {}
        }
    }
}

#[test]
fn wl_and_xdg_output_publish_coherent_initial_and_changed_state() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let initial_mode = PhysicalMode::new(1000, 800, 60_000);
    let output = Output::new(
        ConnectorId::new(7, 11),
        "test-output".to_owned(),
        (600, 340),
        SubpixelLayout::HorizontalRgb,
        vec![initial_mode],
        initial_mode,
        initial_mode,
        OutputScale::from_f64(1.25).unwrap(),
    );
    let _global = output.create_global(&runtime.state.display_handle);
    runtime.state.space.map_output(&output, (0, 0));

    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (events_tx, events_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<OutputEvents>(&connection).unwrap();
        let handle = queue.handle();
        let wl_output = globals
            .bind::<wl_output::WlOutput, _, _>(&handle, 1..=4, ())
            .unwrap();
        let manager = globals
            .bind::<zxdg_output_manager_v1::ZxdgOutputManagerV1, _, _>(&handle, 1..=3, ())
            .unwrap();
        let _xdg_output = manager.get_xdg_output(&wl_output, &handle, ());
        let mut state = OutputEvents::default();
        queue.roundtrip(&mut state).unwrap();
        events_tx.send(state.clone()).unwrap();
        continue_rx.recv().unwrap();
        queue.roundtrip(&mut state).unwrap();
        events_tx.send(state).unwrap();
    });

    let initial = receive_output_events(&mut runtime, &events_rx);
    assert_eq!(initial.name.as_deref(), Some("test-output"));
    assert_eq!(initial.description.as_deref(), Some("test-output"));
    assert_eq!(initial.physical_size, Some((600, 340)));
    assert_eq!(initial.current_mode, Some((1000, 800, 60_000)));
    assert_eq!(initial.scale, Some(2));
    assert_eq!(initial.logical_position, Some((0, 0)));
    assert_eq!(initial.logical_size, Some((800, 640)));
    assert!(initial.wl_done >= 2);
    assert_eq!(
        initial.xdg_done, 0,
        "xdg-output v3 synchronizes via wl_output.done"
    );

    let changed_mode = PhysicalMode::new(1280, 720, 144_000);
    output.reconfigure(
        (610, 350),
        SubpixelLayout::VerticalBgr,
        vec![changed_mode],
        changed_mode,
        changed_mode,
        OutputScale::from_f64(2.5).unwrap(),
    );
    output.set_location((50, -20));
    continue_tx.send(()).unwrap();

    let changed = receive_output_events(&mut runtime, &events_rx);
    assert_eq!(changed.physical_size, Some((610, 350)));
    assert_eq!(changed.current_mode, Some((1280, 720, 144_000)));
    assert_eq!(changed.scale, Some(3));
    assert_eq!(changed.logical_position, Some((50, -20)));
    assert_eq!(changed.logical_size, Some((512, 288)));
    assert!(changed.wl_done >= initial.wl_done + 2);
    assert_eq!(changed.xdg_done, 0);
    client.join().unwrap();
}

fn receive_output_events(
    runtime: &mut WaylandRuntime,
    receiver: &mpsc::Receiver<OutputEvents>,
) -> OutputEvents {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(events) = receiver.try_recv() {
            return events;
        }
    }
    panic!("timed out waiting for output protocol events");
}

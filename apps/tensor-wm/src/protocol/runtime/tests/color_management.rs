use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use tensor_protocol::{ColorPrimaries, RenderIntent, TransferFunction};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_output, wl_registry, wl_surface},
};
use wayland_protocols::wp::color_management::v1::client::{
    wp_color_management_output_v1, wp_color_management_surface_feedback_v1,
    wp_color_management_surface_v1, wp_color_manager_v1, wp_image_description_creator_params_v1,
    wp_image_description_info_v1, wp_image_description_v1,
};

use crate::protocol::globals::output::Output;

use super::*;

#[derive(Default)]
struct ColorManagementClient {
    ready_identity: Option<u64>,
    info_primaries: Option<(i32, i32)>,
    info_luminances: Option<(u32, u32, u32)>,
    info_done: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ColorManagementClient {
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

impl Dispatch<wp_image_description_v1::WpImageDescriptionV1, ()> for ColorManagementClient {
    fn event(
        state: &mut Self,
        _: &wp_image_description_v1::WpImageDescriptionV1,
        event: wp_image_description_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_image_description_v1::Event::Ready2 {
            identity_hi,
            identity_lo,
        } = event
        {
            state.ready_identity = Some((u64::from(identity_hi) << 32) | u64::from(identity_lo));
        }
    }
}

impl Dispatch<wp_image_description_info_v1::WpImageDescriptionInfoV1, ()>
    for ColorManagementClient
{
    fn event(
        state: &mut Self,
        _: &wp_image_description_info_v1::WpImageDescriptionInfoV1,
        event: wp_image_description_info_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wp_image_description_info_v1::Event::Primaries { r_x, r_y, .. } => {
                state.info_primaries = Some((r_x, r_y));
            }
            wp_image_description_info_v1::Event::Luminances {
                min_lum,
                max_lum,
                reference_lum,
            } => state.info_luminances = Some((min_lum, max_lum, reference_lum)),
            wp_image_description_info_v1::Event::Done => state.info_done = true,
            _ => {}
        }
    }
}

delegate_noop!(ColorManagementClient: ignore wl_compositor::WlCompositor);
delegate_noop!(ColorManagementClient: ignore wl_output::WlOutput);
delegate_noop!(ColorManagementClient: ignore wl_surface::WlSurface);
delegate_noop!(ColorManagementClient: ignore wp_color_manager_v1::WpColorManagerV1);
delegate_noop!(ColorManagementClient: ignore wp_color_management_output_v1::WpColorManagementOutputV1);
delegate_noop!(ColorManagementClient: ignore wp_color_management_surface_v1::WpColorManagementSurfaceV1);
delegate_noop!(ColorManagementClient: ignore wp_color_management_surface_feedback_v1::WpColorManagementSurfaceFeedbackV1);
delegate_noop!(ColorManagementClient: ignore wp_image_description_creator_params_v1::WpImageDescriptionCreatorParamsV1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    DescriptionCommitted,
    DescriptionUnset,
}

#[test]
fn parametric_description_is_ready_copy_attached_and_double_buffered() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) =
            registry_queue_init::<ColorManagementClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let manager = globals
            .bind::<wp_color_manager_v1::WpColorManagerV1, _, _>(&handle, 3..=3, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let color_surface = manager.get_surface(&surface, &handle, ());
        let creator = manager.create_parametric_creator(&handle, ());
        creator.set_tf_named(wp_color_manager_v1::TransferFunction::St2084Pq);
        creator.set_primaries_named(wp_color_manager_v1::Primaries::Bt2020);
        let image = creator.create(&handle, ());

        let mut client_state = ColorManagementClient::default();
        queue.roundtrip(&mut client_state).unwrap();
        assert!(
            client_state
                .ready_identity
                .is_some_and(|identity| identity != 0)
        );

        color_surface.set_image_description(&image, wp_color_manager_v1::RenderIntent::Perceptual);
        image.destroy();
        surface.commit();
        queue.roundtrip(&mut client_state).unwrap();
        step_tx.send(Step::DescriptionCommitted).unwrap();
        release_rx.recv().unwrap();

        color_surface.unset_image_description();
        surface.commit();
        queue.roundtrip(&mut client_state).unwrap();
        step_tx.send(Step::DescriptionUnset).unwrap();
        release_rx.recv().unwrap();

        color_surface.destroy();
        surface.destroy();
        manager.destroy();
    });

    dispatch_until_step(&mut runtime, &step_rx, Step::DescriptionCommitted);
    let (description, intent) = runtime
        .state
        .protocol_globals
        .color_management
        .first_committed_description()
        .expect("the committed surface has a description");
    assert_eq!(description.primaries, ColorPrimaries::Bt2020);
    assert_eq!(description.transfer_function, TransferFunction::St2084Pq);
    assert_eq!(description.luminances.max_luminance, 10_000);
    assert_eq!(intent, RenderIntent::Perceptual);
    release_tx.send(()).unwrap();

    dispatch_until_step(&mut runtime, &step_rx, Step::DescriptionUnset);
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .color_management
            .first_committed_description(),
        None
    );
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

#[test]
fn parametric_creator_rejects_an_incomplete_set() {
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
        let (globals, mut queue) =
            registry_queue_init::<ColorManagementClient>(&connection).unwrap();
        let handle = queue.handle();
        let manager = globals
            .bind::<wp_color_manager_v1::WpColorManagerV1, _, _>(&handle, 3..=3, ())
            .unwrap();
        let creator = manager.create_parametric_creator(&handle, ());
        creator.set_tf_named(wp_color_manager_v1::TransferFunction::Gamma22);
        let _image = creator.create(&handle, ());
        let mut client_state = ColorManagementClient::default();
        assert!(queue.roundtrip(&mut client_state).is_err());
        let error = connection
            .protocol_error()
            .expect("expected protocol error");
        result_tx.send(error.code).unwrap();
    });

    let result = dispatch_until_result(&mut runtime, &result_rx);
    assert_eq!(
        result,
        wp_image_description_creator_params_v1::Error::IncompleteSet as u32
    );
    client.join().unwrap();
}

#[test]
fn preferred_sdr_description_exposes_complete_parametric_information() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) =
            registry_queue_init::<ColorManagementClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let manager = globals
            .bind::<wp_color_manager_v1::WpColorManagerV1, _, _>(&handle, 3..=3, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let feedback = manager.get_surface_feedback(&surface, &handle, ());
        let image = feedback.get_preferred(&handle, ());
        let mut client_state = ColorManagementClient::default();
        queue.roundtrip(&mut client_state).unwrap();
        image.get_information(&handle, ());
        queue.roundtrip(&mut client_state).unwrap();
        result_tx
            .send((
                client_state.ready_identity,
                client_state.info_primaries,
                client_state.info_luminances,
                client_state.info_done,
            ))
            .unwrap();
        release_rx.recv().unwrap();
        image.destroy();
        feedback.destroy();
        surface.destroy();
        manager.destroy();
    });

    let result = dispatch_until_result(&mut runtime, &result_rx);
    assert_eq!(result.0, Some(1));
    assert_eq!(result.1, Some((640_000, 330_000)));
    assert_eq!(result.2, Some((2_000, 80, 80)));
    assert!(result.3);
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

#[test]
fn creator_description_rejects_information_queries() {
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
        let (globals, mut queue) =
            registry_queue_init::<ColorManagementClient>(&connection).unwrap();
        let handle = queue.handle();
        let manager = globals
            .bind::<wp_color_manager_v1::WpColorManagerV1, _, _>(&handle, 3..=3, ())
            .unwrap();
        let creator = manager.create_parametric_creator(&handle, ());
        creator.set_tf_power(22_000);
        creator.set_primaries_named(wp_color_manager_v1::Primaries::Srgb);
        let image = creator.create(&handle, ());
        let mut client_state = ColorManagementClient::default();
        queue.roundtrip(&mut client_state).unwrap();
        image.get_information(&handle, ());
        assert!(queue.roundtrip(&mut client_state).is_err());
        let error = connection
            .protocol_error()
            .expect("expected protocol error");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    let result = dispatch_until_result(&mut runtime, &result_rx);
    assert_eq!(
        result,
        (
            "wp_image_description_v1".to_owned(),
            wp_image_description_v1::Error::NoInformation as u32,
        )
    );
    client.join().unwrap();
}

#[test]
fn live_output_exposes_the_fixed_sdr_description() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let mode = PhysicalMode::new(1920, 1080, 60_000);
    let output = Output::new(
        ConnectorId::new(17, 23),
        "color-output".to_owned(),
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
    let (result_tx, result_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) =
            registry_queue_init::<ColorManagementClient>(&connection).unwrap();
        let handle = queue.handle();
        let wl_output = globals
            .bind::<wl_output::WlOutput, _, _>(&handle, 1..=4, ())
            .unwrap();
        let manager = globals
            .bind::<wp_color_manager_v1::WpColorManagerV1, _, _>(&handle, 3..=3, ())
            .unwrap();
        let color_output = manager.get_output(&wl_output, &handle, ());
        let image = color_output.get_image_description(&handle, ());
        let mut client_state = ColorManagementClient::default();
        queue.roundtrip(&mut client_state).unwrap();
        result_tx.send(client_state.ready_identity).unwrap();
        release_rx.recv().unwrap();
        image.destroy();
        color_output.destroy();
        manager.destroy();
        wl_output.release();
    });

    assert_eq!(dispatch_until_result(&mut runtime, &result_rx), Some(1));
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

fn dispatch_until_step(runtime: &mut WaylandRuntime, steps: &mpsc::Receiver<Step>, expected: Step) {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            assert_eq!(step, expected);
            return;
        }
    }
    panic!("color-management client did not complete before the dispatch limit");
}

fn dispatch_until_result<T>(runtime: &mut WaylandRuntime, results: &mpsc::Receiver<T>) -> T {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("color-management protocol error did not arrive before the dispatch limit");
}

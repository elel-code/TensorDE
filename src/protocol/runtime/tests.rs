use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use smithay::{
    desktop::utils::OutputPresentationFeedback,
    output::{Mode as OutputMode, Output, PhysicalProperties, Scale, Subpixel},
    utils::{ClockSource, Monotonic},
    wayland::seat::WaylandFocus,
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_subcompositor, wl_subsurface, wl_surface},
};
use wayland_protocols::{
    wp::{
        fractional_scale::v1::client::wp_fractional_scale_manager_v1,
        fractional_scale::v1::client::wp_fractional_scale_v1,
        pointer_gestures::zv1::client::zwp_pointer_gestures_v1,
        presentation_time::client::{wp_presentation, wp_presentation_feedback},
        primary_selection::zv1::client::zwp_primary_selection_device_manager_v1,
        relative_pointer::zv1::client::zwp_relative_pointer_manager_v1,
        viewporter::client::wp_viewporter,
    },
    xdg::{
        decoration::zv1::client::zxdg_decoration_manager_v1,
        decoration::zv1::client::zxdg_toplevel_decoration_v1,
        shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
    },
};

use super::*;

#[derive(Debug, Eq, PartialEq)]
enum ClientEvent {
    Configured {
        size: (i32, i32),
        preferred_scale: u32,
        client_side_decoration: bool,
    },
    SubsurfaceDeferred,
    SubsurfaceCommitted,
    SubsurfaceDestroyed,
    PresentationClock(u32),
    PresentationDiscarded,
    Destroyed,
}

struct TestClient {
    configured: bool,
    configured_size: Option<(i32, i32)>,
    preferred_scale: Option<u32>,
    client_side_decoration: bool,
    presentation_clock_id: Option<u32>,
    presentation_discarded: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for TestClient {
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

delegate_noop!(TestClient: ignore wl_compositor::WlCompositor);
delegate_noop!(TestClient: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(TestClient: ignore wl_subsurface::WlSubsurface);
delegate_noop!(TestClient: ignore wl_surface::WlSurface);
delegate_noop!(TestClient: ignore wp_viewporter::WpViewporter);
delegate_noop!(TestClient: ignore wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);
delegate_noop!(TestClient: ignore zxdg_decoration_manager_v1::ZxdgDecorationManagerV1);
delegate_noop!(TestClient: ignore zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1);
delegate_noop!(TestClient: ignore zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1);
delegate_noop!(TestClient: ignore zwp_pointer_gestures_v1::ZwpPointerGesturesV1);
impl Dispatch<wp_presentation::WpPresentation, ()> for TestClient {
    fn event(
        state: &mut Self,
        _: &wp_presentation::WpPresentation,
        event: wp_presentation::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event {
            state.presentation_clock_id = Some(clk_id);
        }
    }
}

impl Dispatch<wp_presentation_feedback::WpPresentationFeedback, ()> for TestClient {
    fn event(
        state: &mut Self,
        _: &wp_presentation_feedback::WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wp_presentation_feedback::Event::Discarded) {
            state.presentation_discarded = true;
        }
    }
}

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for TestClient {
    fn event(
        state: &mut Self,
        _: &wp_fractional_scale_v1::WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            state.preferred_scale = Some(scale);
        }
    }
}

impl Dispatch<zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1, ()> for TestClient {
    fn event(
        state: &mut Self,
        _: &zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1,
        event: zxdg_toplevel_decoration_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_toplevel_decoration_v1::Event::Configure { mode } = event {
            state.client_side_decoration = matches!(
                mode,
                wayland_client::WEnum::Value(zxdg_toplevel_decoration_v1::Mode::ClientSide)
            );
        }
    }
}
impl Dispatch<xdg_toplevel::XdgToplevel, ()> for TestClient {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Configure { width, height, .. } = event {
            state.configured_size = Some((width, height));
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for TestClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for TestClient {
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
fn presentation_global_uses_monotonic_clock_and_discards_destroyed_surface() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();

    let (event_tx, event_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<TestClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let presentation = globals
            .bind::<wp_presentation::WpPresentation, _, _>(&handle, 1..=2, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let _feedback = presentation.feedback(&surface, &handle, ());
        surface.commit();

        let mut state = TestClient {
            configured: false,
            configured_size: None,
            preferred_scale: None,
            client_side_decoration: false,
            presentation_clock_id: None,
            presentation_discarded: false,
        };
        while state.presentation_clock_id.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx
            .send(ClientEvent::PresentationClock(
                state.presentation_clock_id.unwrap(),
            ))
            .unwrap();
        release_rx.recv().unwrap();

        surface.destroy();
        while !state.presentation_discarded {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx.send(ClientEvent::PresentationDiscarded).unwrap();
    });

    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        ClientEvent::PresentationClock(Monotonic::ID as u32)
    );
    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        ClientEvent::PresentationDiscarded
    );
    client.join().unwrap();
}

#[test]
fn xdg_toplevel_lifecycle_is_owned_by_runtime_state() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
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
        let (globals, mut queue) = registry_queue_init::<TestClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let subcompositor = globals
            .bind::<wl_subcompositor::WlSubcompositor, _, _>(&handle, 1..=1, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let _viewporter = globals
            .bind::<wp_viewporter::WpViewporter, _, _>(&handle, 1..=1, ())
            .unwrap();
        let fractional_scale_manager = globals
            .bind::<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let decoration_manager = globals
            .bind::<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let _primary_selection = globals
            .bind::<
                zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1,
                _,
                _,
            >(&handle, 1..=1, ())
            .unwrap();
        let _relative_pointer = globals
            .bind::<zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let _pointer_gestures = globals
            .bind::<zwp_pointer_gestures_v1::ZwpPointerGesturesV1, _, _>(&handle, 1..=3, ())
            .unwrap();
        let presentation = globals
            .bind::<wp_presentation::WpPresentation, _, _>(&handle, 1..=2, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let _feedback = presentation.feedback(&surface, &handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        let _fractional_scale =
            fractional_scale_manager.get_fractional_scale(&surface, &handle, ());
        let _decoration = decoration_manager.get_toplevel_decoration(&toplevel, &handle, ());
        toplevel.set_min_size(320, 200);
        toplevel.set_max_size(640, 480);
        surface.commit();

        let mut state = TestClient {
            configured: false,
            configured_size: None,
            preferred_scale: None,
            client_side_decoration: false,
            presentation_clock_id: None,
            presentation_discarded: false,
        };
        while !state.configured {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx
            .send(ClientEvent::Configured {
                size: state.configured_size.unwrap(),
                preferred_scale: state.preferred_scale.unwrap(),
                client_side_decoration: state.client_side_decoration,
            })
            .unwrap();
        while !state.presentation_discarded {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        event_tx.send(ClientEvent::PresentationDiscarded).unwrap();

        let child = compositor.create_surface(&handle, ());
        let subsurface = subcompositor.get_subsurface(&child, &surface, &handle, ());
        subsurface.set_position(12, 9);
        child.commit();
        connection.roundtrip().unwrap();
        event_tx.send(ClientEvent::SubsurfaceDeferred).unwrap();
        release_rx.recv().unwrap();

        surface.commit();
        connection.roundtrip().unwrap();
        event_tx.send(ClientEvent::SubsurfaceCommitted).unwrap();
        release_rx.recv().unwrap();

        subsurface.destroy();
        child.destroy();
        connection.roundtrip().unwrap();
        event_tx.send(ClientEvent::SubsurfaceDestroyed).unwrap();
        release_rx.recv().unwrap();

        _decoration.destroy();
        _fractional_scale.destroy();
        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
        connection.roundtrip().unwrap();
        event_tx.send(ClientEvent::Destroyed).unwrap();
    });

    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        ClientEvent::Configured {
            size: (388, 480),
            preferred_scale: 150,
            client_side_decoration: true,
        }
    );
    assert_eq!(runtime.state.view_count(), 1);
    let window = runtime.state.space.elements().next().unwrap().clone();
    let output = runtime.state.space.outputs().next().unwrap().clone();
    let mut feedback = OutputPresentationFeedback::new(&output);
    window.take_presentation_feedback(
        &mut feedback,
        |_, _| Some(output.clone()),
        |_, _| {
            wayland_protocols::wp::presentation_time::server::wp_presentation_feedback::Kind::empty(
            )
        },
    );
    drop(feedback);
    runtime.state.display_handle.flush_clients().unwrap();
    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        ClientEvent::PresentationDiscarded
    );
    assert_eq!(
        runtime.state.space.element_location(&window),
        Some((8, 80).into())
    );
    assert_eq!(
        runtime
            .state
            .world
            .layout_snapshot(super::super::state::DEFAULT_WORKSPACE)
            .unwrap()
            .placements[0]
            .geometry,
        crate::layout::Rect::new(8, 80, 388, 480)
    );
    let view_id = runtime.state.view_for_surface(
        runtime
            .state
            .space
            .elements()
            .next()
            .unwrap()
            .wl_surface()
            .as_deref()
            .unwrap(),
    );
    assert_eq!(
        view_id.and_then(|view_id| runtime.state.world.view_layout(view_id)),
        Some(crate::ecs::ViewLayout {
            constraints: crate::layout::SizeConstraints::new(
                tensor_util::Size::new(320, 200),
                Some(640),
                Some(480),
            ),
            primary_size: None,
        })
    );

    #[cfg(feature = "tty")]
    {
        let focused_view = view_id.expect("mapped test window has an ECS view");
        assert!(runtime.state.world.is_focused(focused_view));
        assert!(runtime.state.seat.get_keyboard().is_none());
        runtime.state.input_devices.insert(
            tensor_input::DeviceId::new(1),
            super::super::state::InputDeviceCapabilities {
                keyboard: true,
                ..Default::default()
            },
        );
        runtime.state.reconcile_seat_capabilities();
        let keyboard = runtime
            .state
            .seat
            .get_keyboard()
            .expect("test keyboard capability is published");
        assert!(keyboard.current_focus().is_some());
    }

    #[cfg(feature = "tty")]
    let root = runtime
        .state
        .space
        .elements()
        .next()
        .unwrap()
        .wl_surface()
        .unwrap()
        .into_owned();
    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        ClientEvent::SubsurfaceDeferred
    );
    #[cfg(feature = "tty")]
    assert_eq!(runtime.state.surface_tree_member_count(&root), 1);
    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        ClientEvent::SubsurfaceCommitted
    );
    #[cfg(feature = "tty")]
    assert_eq!(runtime.state.surface_tree_member_count(&root), 2);
    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        ClientEvent::SubsurfaceDestroyed
    );
    #[cfg(feature = "tty")]
    assert_eq!(runtime.state.surface_tree_member_count(&root), 1);
    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until(&mut runtime, &event_rx),
        ClientEvent::Destroyed
    );
    for _ in 0..16 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(2), &mut runtime.state)
            .unwrap();
        if runtime.state.view_count() == 0 {
            break;
        }
    }
    assert_eq!(runtime.state.view_count(), 0);
    #[cfg(feature = "tty")]
    assert!(
        runtime
            .state
            .seat
            .get_keyboard()
            .is_some_and(|keyboard| keyboard.current_focus().is_none())
    );
    client.join().unwrap();
}

fn install_test_output(runtime: &mut WaylandRuntime) {
    let mode = OutputMode {
        size: (1000, 800).into(),
        refresh: 60_000,
    };
    let output = Output::new(
        "test-output".to_owned(),
        PhysicalProperties {
            size: (600, 340).into(),
            subpixel: Subpixel::Unknown,
            make: "Tensor".to_owned(),
            model: "Nested".to_owned(),
            serial_number: "test".to_owned(),
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
    panic!("Wayland client did not complete before the dispatch limit");
}

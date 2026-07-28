use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use tensor_input::PointerGestureEvent;
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::{
    wp::{
        pointer_gestures::zv1::client::{
            zwp_pointer_gesture_hold_v1, zwp_pointer_gesture_pinch_v1,
            zwp_pointer_gesture_swipe_v1, zwp_pointer_gestures_v1,
        },
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
        viewporter::client::{wp_viewport, wp_viewporter},
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum GestureWireEvent {
    SwipeBegin {
        time: u32,
        fingers: u32,
    },
    SwipeUpdate {
        time: u32,
        dx: f64,
        dy: f64,
    },
    SwipeEnd {
        time: u32,
        cancelled: bool,
    },
    PinchBegin {
        time: u32,
        fingers: u32,
    },
    PinchUpdate {
        time: u32,
        dx: f64,
        dy: f64,
        scale: f64,
        rotation: f64,
    },
    PinchEnd {
        time: u32,
        cancelled: bool,
    },
    HoldBegin {
        time: u32,
        fingers: u32,
    },
    HoldEnd {
        time: u32,
        cancelled: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum GestureStep {
    Ready,
    Event(GestureWireEvent),
    Destroyed,
}

#[derive(Default)]
struct GestureClient {
    configured: bool,
    pointer: Option<wl_pointer::WlPointer>,
    events: Vec<GestureWireEvent>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for GestureClient {
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

impl Dispatch<wl_seat::WlSeat, ()> for GestureClient {
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
            && capabilities.contains(wl_seat::Capability::Pointer)
            && state.pointer.is_none()
        {
            state.pointer = Some(seat.get_pointer(queue, ()));
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for GestureClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for GestureClient {
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

impl Dispatch<zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1, ()> for GestureClient {
    fn event(
        state: &mut Self,
        _: &zwp_pointer_gesture_swipe_v1::ZwpPointerGestureSwipeV1,
        event: zwp_pointer_gesture_swipe_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let event = match event {
            zwp_pointer_gesture_swipe_v1::Event::Begin { time, fingers, .. } => {
                GestureWireEvent::SwipeBegin { time, fingers }
            }
            zwp_pointer_gesture_swipe_v1::Event::Update { time, dx, dy } => {
                GestureWireEvent::SwipeUpdate { time, dx, dy }
            }
            zwp_pointer_gesture_swipe_v1::Event::End {
                time, cancelled, ..
            } => GestureWireEvent::SwipeEnd {
                time,
                cancelled: cancelled != 0,
            },
            _ => return,
        };
        state.events.push(event);
    }
}

impl Dispatch<zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1, ()> for GestureClient {
    fn event(
        state: &mut Self,
        _: &zwp_pointer_gesture_pinch_v1::ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let event = match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin { time, fingers, .. } => {
                GestureWireEvent::PinchBegin { time, fingers }
            }
            zwp_pointer_gesture_pinch_v1::Event::Update {
                time,
                dx,
                dy,
                scale,
                rotation,
            } => GestureWireEvent::PinchUpdate {
                time,
                dx,
                dy,
                scale,
                rotation,
            },
            zwp_pointer_gesture_pinch_v1::Event::End {
                time, cancelled, ..
            } => GestureWireEvent::PinchEnd {
                time,
                cancelled: cancelled != 0,
            },
            _ => return,
        };
        state.events.push(event);
    }
}

impl Dispatch<zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1, ()> for GestureClient {
    fn event(
        state: &mut Self,
        _: &zwp_pointer_gesture_hold_v1::ZwpPointerGestureHoldV1,
        event: zwp_pointer_gesture_hold_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let event = match event {
            zwp_pointer_gesture_hold_v1::Event::Begin { time, fingers, .. } => {
                GestureWireEvent::HoldBegin { time, fingers }
            }
            zwp_pointer_gesture_hold_v1::Event::End {
                time, cancelled, ..
            } => GestureWireEvent::HoldEnd {
                time,
                cancelled: cancelled != 0,
            },
            _ => return,
        };
        state.events.push(event);
    }
}

delegate_noop!(GestureClient: ignore wl_buffer::WlBuffer);
delegate_noop!(GestureClient: ignore wl_compositor::WlCompositor);
delegate_noop!(GestureClient: ignore wl_pointer::WlPointer);
delegate_noop!(GestureClient: ignore wl_surface::WlSurface);
delegate_noop!(GestureClient: ignore zwp_pointer_gestures_v1::ZwpPointerGesturesV1);
delegate_noop!(GestureClient: ignore wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1);
delegate_noop!(GestureClient: ignore wp_viewport::WpViewport);
delegate_noop!(GestureClient: ignore wp_viewporter::WpViewporter);
delegate_noop!(GestureClient: ignore xdg_toplevel::XdgToplevel);

#[test]
fn pointer_gestures_send_complete_value_sequences_and_release_resources() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    runtime.state.input_devices.insert(
        tensor_input::DeviceId::new(1),
        crate::protocol::state::InputDeviceCapabilities {
            pointer: true,
            ..Default::default()
        },
    );
    runtime.state.reconcile_seat_capabilities();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);
    let client = spawn_gesture_client(socket_path, step_tx, release_rx);

    assert_eq!(
        dispatch_until_gesture_step(&mut runtime, &step_rx),
        GestureStep::Ready
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .pointer_gestures
            .resource_count(),
        3
    );

    let pointer_location: tensor_util::LogicalPoint<f64> = runtime
        .state
        .space
        .elements()
        .next()
        .and_then(|window| runtime.state.space.element_geometry(window))
        .map(|geometry| {
            (
                f64::from(geometry.loc.x) + 10.0,
                f64::from(geometry.loc.y) + 10.0,
            )
                .into()
        })
        .expect("pointer-gestures test surface is mapped");
    runtime
        .state
        .input_seat
        .set_pointer_location(pointer_location);
    runtime
        .state
        .forward_pointer_motion(tensor_input::RelativeMotionEvent {
            delta_x: 1.0,
            delta_y: 1.0,
            unaccelerated_x: 1.0,
            unaccelerated_y: 1.0,
            time_ns: 500_000,
        });

    for event in gesture_input_sequence() {
        runtime.state.forward_pointer_gesture(event);
    }
    runtime
        .state
        .forward_pointer_gesture(PointerGestureEvent::SwipeBegin {
            fingers: 3,
            time_ns: 9_000_000,
        });
    runtime
        .state
        .forward_pointer_motion(tensor_input::RelativeMotionEvent {
            delta_x: 100.0,
            delta_y: 0.0,
            unaccelerated_x: 100.0,
            unaccelerated_y: 0.0,
            time_ns: 10_000_000,
        });
    runtime.state.flush_wayland_clients();
    for expected in gesture_wire_sequence() {
        assert_eq!(
            dispatch_until_gesture_step(&mut runtime, &step_rx),
            GestureStep::Event(expected)
        );
    }
    assert_eq!(
        dispatch_until_gesture_step(&mut runtime, &step_rx),
        GestureStep::Event(GestureWireEvent::SwipeBegin {
            time: 9,
            fingers: 3,
        })
    );
    assert_eq!(
        dispatch_until_gesture_step(&mut runtime, &step_rx),
        GestureStep::Event(GestureWireEvent::SwipeEnd {
            time: 10,
            cancelled: true,
        })
    );

    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until_gesture_step(&mut runtime, &step_rx),
        GestureStep::Destroyed
    );
    client.join().unwrap();
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .pointer_gestures
            .resource_count(),
        0
    );
}

fn gesture_input_sequence() -> [PointerGestureEvent; 8] {
    [
        PointerGestureEvent::SwipeBegin {
            fingers: 3,
            time_ns: 1_000_000,
        },
        PointerGestureEvent::SwipeUpdate {
            delta_x: 1.5,
            delta_y: -2.0,
            time_ns: 2_000_000,
        },
        PointerGestureEvent::SwipeEnd {
            cancelled: false,
            time_ns: 3_000_000,
        },
        PointerGestureEvent::PinchBegin {
            fingers: 2,
            time_ns: 4_000_000,
        },
        PointerGestureEvent::PinchUpdate {
            delta_x: -3.0,
            delta_y: 4.5,
            scale: 1.25,
            rotation: -30.0,
            time_ns: 5_000_000,
        },
        PointerGestureEvent::PinchEnd {
            cancelled: true,
            time_ns: 6_000_000,
        },
        PointerGestureEvent::HoldBegin {
            fingers: 4,
            time_ns: 7_000_000,
        },
        PointerGestureEvent::HoldEnd {
            cancelled: false,
            time_ns: 8_000_000,
        },
    ]
}

fn gesture_wire_sequence() -> [GestureWireEvent; 8] {
    [
        GestureWireEvent::SwipeBegin {
            time: 1,
            fingers: 3,
        },
        GestureWireEvent::SwipeUpdate {
            time: 2,
            dx: 1.5,
            dy: -2.0,
        },
        GestureWireEvent::SwipeEnd {
            time: 3,
            cancelled: false,
        },
        GestureWireEvent::PinchBegin {
            time: 4,
            fingers: 2,
        },
        GestureWireEvent::PinchUpdate {
            time: 5,
            dx: -3.0,
            dy: 4.5,
            scale: 1.25,
            rotation: -30.0,
        },
        GestureWireEvent::PinchEnd {
            time: 6,
            cancelled: true,
        },
        GestureWireEvent::HoldBegin {
            time: 7,
            fingers: 4,
        },
        GestureWireEvent::HoldEnd {
            time: 8,
            cancelled: false,
        },
    ]
}

fn spawn_gesture_client(
    socket_path: PathBuf,
    steps: mpsc::SyncSender<GestureStep>,
    releases: mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<GestureClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let gesture_manager = globals
            .bind::<zwp_pointer_gestures_v1::ZwpPointerGesturesV1, _, _>(&handle, 1..=3, ())
            .unwrap();
        let viewporter = globals
            .bind::<wp_viewporter::WpViewporter, _, _>(&handle, 1..=1, ())
            .unwrap();
        let single_pixel = globals
            .bind::<wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        surface.commit();

        let mut state = GestureClient::default();
        while !state.configured || state.pointer.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let viewport = viewporter.get_viewport(&surface, &handle, ());
        viewport.set_destination(64, 32);
        let buffer = single_pixel.create_u32_rgba_buffer(
            u32::MAX,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            &handle,
            (),
        );
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, 1, 1);
        surface.commit();
        let pointer = state.pointer.as_ref().unwrap();
        let swipe = gesture_manager.get_swipe_gesture(pointer, &handle, ());
        let pinch = gesture_manager.get_pinch_gesture(pointer, &handle, ());
        let hold = gesture_manager.get_hold_gesture(pointer, &handle, ());
        queue.roundtrip(&mut state).unwrap();
        steps.send(GestureStep::Ready).unwrap();

        while state.events.len() < 10 {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        for event in state.events.drain(..) {
            steps.send(GestureStep::Event(event)).unwrap();
        }
        releases.recv().unwrap();

        swipe.destroy();
        pinch.destroy();
        hold.destroy();
        buffer.destroy();
        viewport.destroy();
        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
        gesture_manager.release();
        single_pixel.destroy();
        viewporter.destroy();
        seat.release();
        queue.roundtrip(&mut state).unwrap();
        steps.send(GestureStep::Destroyed).unwrap();
    })
}

fn dispatch_until_gesture_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<GestureStep>,
) -> GestureStep {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("pointer-gestures client did not complete before the dispatch limit");
}

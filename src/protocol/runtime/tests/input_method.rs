use std::{
    os::{fd::AsFd, unix::net::UnixStream},
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

use rustix::fs::{MemfdFlags, ftruncate, memfd_create};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_shm, wl_shm_pool,
        wl_surface,
    },
};
use wayland_protocols::{
    wp::{
        fractional_scale::v1::client::{wp_fractional_scale_manager_v1, wp_fractional_scale_v1},
        text_input::zv3::client::{zwp_text_input_manager_v3, zwp_text_input_v3},
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_keyboard_grab_v2, zwp_input_method_manager_v2, zwp_input_method_v2,
    zwp_input_popup_surface_v2,
};

use super::*;

mod errors;

#[derive(Debug, Eq, PartialEq)]
enum ResultEvent {
    AppReady,
    InputMethodIdleReady,
    InputMethodActive {
        serial: u32,
        rectangle: (i32, i32, i32, i32),
        preferred_scale: u32,
    },
    KeyboardRouted {
        key: u32,
        press_time: u32,
        release_time: u32,
    },
    AppRejectedStaleEdit {
        serial: u32,
        commit: String,
        preedit: Option<String>,
        delete: Option<(u32, u32)>,
    },
    AppEdited {
        serial: u32,
        commit: String,
        preedit: String,
        delete: (u32, u32),
    },
    InputMethodInactive,
}

enum AppCommand {
    Enable,
    Disable,
    Shutdown,
}

enum InputMethodCommand {
    CommitCurrent,
}

#[derive(Default)]
struct AppClient {
    configure: Option<u32>,
    entered: bool,
    done: Option<u32>,
    commit: Option<String>,
    preedit: Option<String>,
    delete: Option<(u32, u32)>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for AppClient {
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

delegate_noop!(AppClient: ignore wl_compositor::WlCompositor);
delegate_noop!(AppClient: ignore wl_surface::WlSurface);
delegate_noop!(AppClient: ignore wl_buffer::WlBuffer);
delegate_noop!(AppClient: ignore wl_shm::WlShm);
delegate_noop!(AppClient: ignore wl_shm_pool::WlShmPool);
delegate_noop!(AppClient: ignore wl_seat::WlSeat);
delegate_noop!(AppClient: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(AppClient: ignore zwp_text_input_manager_v3::ZwpTextInputManagerV3);

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for AppClient {
    fn event(
        _: &mut Self,
        base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for AppClient {
    fn event(
        state: &mut Self,
        _: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            state.configure = Some(serial);
        }
    }
}

impl Dispatch<zwp_text_input_v3::ZwpTextInputV3, ()> for AppClient {
    fn event(
        state: &mut Self,
        _: &zwp_text_input_v3::ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::Enter { .. } => state.entered = true,
            zwp_text_input_v3::Event::CommitString { text } => state.commit = text,
            zwp_text_input_v3::Event::PreeditString { text, .. } => state.preedit = text,
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => state.delete = Some((before_length, after_length)),
            zwp_text_input_v3::Event::Done { serial } => state.done = Some(serial),
            _ => {}
        }
    }
}

#[derive(Debug, Default)]
struct InputMethodClient {
    active: bool,
    inactive: bool,
    done_count: u32,
    rectangle: Option<(i32, i32, i32, i32)>,
    preferred_scale: Option<u32>,
    unavailable: bool,
    keymap: bool,
    repeat: Option<(i32, i32)>,
    modifiers: bool,
    keys: Vec<(u32, u32, wayland_client::WEnum<wl_keyboard::KeyState>)>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for InputMethodClient {
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

delegate_noop!(InputMethodClient: ignore wl_compositor::WlCompositor);
delegate_noop!(InputMethodClient: ignore wl_surface::WlSurface);
delegate_noop!(InputMethodClient: ignore wl_buffer::WlBuffer);
delegate_noop!(InputMethodClient: ignore wl_shm::WlShm);
delegate_noop!(InputMethodClient: ignore wl_shm_pool::WlShmPool);
delegate_noop!(InputMethodClient: ignore wl_seat::WlSeat);
delegate_noop!(InputMethodClient: ignore zwp_input_method_manager_v2::ZwpInputMethodManagerV2);
delegate_noop!(InputMethodClient: ignore wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);

impl Dispatch<zwp_input_method_v2::ZwpInputMethodV2, ()> for InputMethodClient {
    fn event(
        state: &mut Self,
        _: &zwp_input_method_v2::ZwpInputMethodV2,
        event: zwp_input_method_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_v2::Event::Activate => state.active = true,
            zwp_input_method_v2::Event::Deactivate => state.inactive = true,
            zwp_input_method_v2::Event::Done => {
                state.done_count = state.done_count.wrapping_add(1);
            }
            zwp_input_method_v2::Event::Unavailable => state.unavailable = true,
            _ => {}
        }
    }
}

impl Dispatch<zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2, ()> for InputMethodClient {
    fn event(
        state: &mut Self,
        _: &zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2,
        event: zwp_input_popup_surface_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_input_popup_surface_v2::Event::TextInputRectangle {
            x,
            y,
            width,
            height,
        } = event
        {
            state.rectangle = Some((x, y, width, height));
        }
    }
}

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for InputMethodClient {
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

impl Dispatch<zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2, ()>
    for InputMethodClient
{
    fn event(
        state: &mut Self,
        _: &zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2,
        event: zwp_input_method_keyboard_grab_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_keyboard_grab_v2::Event::Keymap { size, .. } => {
                state.keymap = size > 0;
            }
            zwp_input_method_keyboard_grab_v2::Event::Key {
                time,
                key,
                state: key_state,
                ..
            } => state.keys.push((time, key, key_state)),
            zwp_input_method_keyboard_grab_v2::Event::Modifiers { .. } => {
                state.modifiers = true;
            }
            zwp_input_method_keyboard_grab_v2::Event::RepeatInfo { rate, delay } => {
                state.repeat = Some((rate, delay));
            }
            _ => {}
        }
    }
}

#[test]
fn text_input_and_input_method_drive_a_scaled_scene_popup() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    let socket_path = runtime_socket(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();

    let (result_tx, result_rx) = mpsc::channel();
    let (app_command_tx, app_command_rx) = mpsc::channel();
    let (input_method_command_tx, input_method_command_rx) = mpsc::channel();
    let app = spawn_application(socket_path.clone(), result_tx.clone(), app_command_rx);

    assert_eq!(
        dispatch_until_result(&mut runtime, &result_rx),
        ResultEvent::AppReady
    );
    let input_method = spawn_input_method(socket_path, result_tx, input_method_command_rx);
    runtime.state.input_devices.insert(
        tensor_input::DeviceId::new(1),
        crate::protocol::state::InputDeviceCapabilities {
            keyboard: true,
            ..Default::default()
        },
    );
    runtime.state.reconcile_seat_capabilities();
    assert_eq!(
        dispatch_until_result(&mut runtime, &result_rx),
        ResultEvent::InputMethodIdleReady
    );
    app_command_tx.send(AppCommand::Enable).unwrap();

    assert_eq!(
        dispatch_until_result(&mut runtime, &result_rx),
        ResultEvent::InputMethodActive {
            serial: 1,
            rectangle: (11, 13, 7, 9),
            preferred_scale: 150,
        }
    );
    let root = runtime
        .state
        .space
        .elements()
        .next()
        .and_then(|window| window.wl_surface())
        .map(std::borrow::Cow::into_owned)
        .expect("application is mapped");
    assert_eq!(runtime.state.surface_tree_member_count(&root), 2);

    runtime
        .state
        .process_input_event(crate::backend::LibinputEvent::Input(
            tensor_input::BackendInputEvent::Keyboard(tensor_input::KeyboardEvent {
                key: 30,
                pressed: true,
                time_ns: 10_000_000,
            }),
        ));
    runtime
        .state
        .process_input_event(crate::backend::LibinputEvent::Input(
            tensor_input::BackendInputEvent::Keyboard(tensor_input::KeyboardEvent {
                key: 30,
                pressed: false,
                time_ns: 11_000_000,
            }),
        ));
    runtime.state.display_handle.flush_clients().unwrap();

    let mut keyboard_routed = false;
    let mut stale_rejected = false;
    for _ in 0..2 {
        match dispatch_until_result(&mut runtime, &result_rx) {
            ResultEvent::KeyboardRouted {
                key,
                press_time,
                release_time,
            } => {
                assert_eq!((key, press_time, release_time), (30, 10, 11));
                keyboard_routed = true;
            }
            ResultEvent::AppRejectedStaleEdit {
                serial,
                commit,
                preedit,
                delete,
            } => {
                assert_eq!(serial, 2);
                assert_eq!(commit, "stale");
                assert_eq!(preedit, None);
                assert_eq!(delete, None);
                stale_rejected = true;
            }
            event => panic!("unexpected input-method result: {event:?}"),
        }
    }
    assert!(keyboard_routed && stale_rejected);
    input_method_command_tx
        .send(InputMethodCommand::CommitCurrent)
        .unwrap();

    assert_eq!(
        dispatch_until_result(&mut runtime, &result_rx),
        ResultEvent::AppEdited {
            serial: 1,
            commit: "中".to_owned(),
            preedit: "文".to_owned(),
            delete: (1, 2),
        }
    );

    app_command_tx.send(AppCommand::Disable).unwrap();
    assert_eq!(
        dispatch_until_result(&mut runtime, &result_rx),
        ResultEvent::InputMethodInactive
    );
    assert_eq!(runtime.state.surface_tree_member_count(&root), 1);

    app_command_tx.send(AppCommand::Shutdown).unwrap();
    app.join().unwrap();
    input_method.join().unwrap();
}

fn spawn_application(
    socket_path: PathBuf,
    results: mpsc::Sender<ResultEvent>,
    commands: mpsc::Receiver<AppCommand>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<AppClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let shell = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let text_manager = globals
            .bind::<zwp_text_input_manager_v3::ZwpTextInputManagerV3, _, _>(&handle, 1..=1, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let xdg = shell.get_xdg_surface(&surface, &handle, ());
        let _toplevel = xdg.get_toplevel(&handle, ());
        let text_input = text_manager.get_text_input(&seat, &handle, ());
        let buffer = create_buffer(&shm, &handle, 96, 64, "tensor-text-input-test");
        let mut state = AppClient::default();

        surface.commit();
        while state.configure.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        xdg.ack_configure(state.configure.unwrap());
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        queue.roundtrip(&mut state).unwrap();
        results.send(ResultEvent::AppReady).unwrap();

        assert!(matches!(commands.recv().unwrap(), AppCommand::Enable));
        while !state.entered {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        text_input.enable();
        text_input.set_surrounding_text("a中b".to_owned(), 4, 1);
        text_input.set_text_change_cause(zwp_text_input_v3::ChangeCause::Other);
        text_input.set_content_type(
            zwp_text_input_v3::ContentHint::Completion,
            zwp_text_input_v3::ContentPurpose::Normal,
        );
        text_input.set_cursor_rectangle(11, 13, 7, 9);
        text_input.commit();
        connection.flush().unwrap();

        while state.done.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        results
            .send(ResultEvent::AppRejectedStaleEdit {
                serial: state.done.take().unwrap(),
                commit: state.commit.take().unwrap(),
                preedit: state.preedit.take(),
                delete: state.delete.take(),
            })
            .unwrap();

        while state.done.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        results
            .send(ResultEvent::AppEdited {
                serial: state.done.unwrap(),
                commit: state.commit.take().unwrap(),
                preedit: state.preedit.take().unwrap(),
                delete: state.delete.unwrap(),
            })
            .unwrap();

        assert!(matches!(commands.recv().unwrap(), AppCommand::Disable));
        text_input.disable();
        text_input.commit();
        connection.flush().unwrap();
        assert!(matches!(commands.recv().unwrap(), AppCommand::Shutdown));
    })
}

fn spawn_input_method(
    socket_path: PathBuf,
    results: mpsc::Sender<ResultEvent>,
    commands: mpsc::Receiver<InputMethodCommand>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<InputMethodClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<zwp_input_method_manager_v2::ZwpInputMethodManagerV2, _, _>(&handle, 1..=1, ())
            .unwrap();
        let fractional = globals
            .bind::<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let input_method = manager.get_input_method(&seat, &handle, ());
        let unavailable_input_method = manager.get_input_method(&seat, &handle, ());
        unavailable_input_method.commit_string("x".repeat(4_001));
        unavailable_input_method.set_preedit_string("中".to_owned(), 1, 3);
        let popup_surface = compositor.create_surface(&handle, ());
        let _popup_scale = fractional.get_fractional_scale(&popup_surface, &handle, ());
        let _popup = input_method.get_input_popup_surface(&popup_surface, &handle, ());
        let _keyboard = input_method.grab_keyboard(&handle, ());
        let buffer = create_buffer(&shm, &handle, 80, 32, "tensor-input-popup-test");
        let mut state = InputMethodClient::default();
        input_method.set_preedit_string("inactive".to_owned(), 0, 8);
        input_method.delete_surrounding_text(99, 98);
        queue.roundtrip(&mut state).unwrap();
        results.send(ResultEvent::InputMethodIdleReady).unwrap();

        while !(state.active
            && state.done_count == 1
            && state.rectangle.is_some()
            && state.unavailable
            && state.keymap
            && state.repeat.is_some()
            && state.modifiers)
        {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        popup_surface.attach(Some(&buffer), 0, 0);
        popup_surface.commit();
        queue.roundtrip(&mut state).unwrap();
        results
            .send(ResultEvent::InputMethodActive {
                serial: state.done_count,
                rectangle: state.rectangle.unwrap(),
                preferred_scale: state.preferred_scale.unwrap(),
            })
            .unwrap();

        input_method.commit_string("stale".to_owned());
        input_method.commit(0);
        connection.flush().unwrap();

        while state.keys.len() < 2 {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let pressed = &state.keys[0];
        let released = &state.keys[1];
        assert_eq!(
            pressed.2,
            wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed)
        );
        assert_eq!(
            released.2,
            wayland_client::WEnum::Value(wl_keyboard::KeyState::Released)
        );
        results
            .send(ResultEvent::KeyboardRouted {
                key: pressed.1,
                press_time: pressed.0,
                release_time: released.0,
            })
            .unwrap();

        assert!(matches!(
            commands.recv().unwrap(),
            InputMethodCommand::CommitCurrent
        ));
        input_method.delete_surrounding_text(1, 2);
        input_method.commit_string("中".to_owned());
        input_method.set_preedit_string("文".to_owned(), 0, 3);
        input_method.commit(state.done_count);
        connection.flush().unwrap();

        while !state.inactive {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        results.send(ResultEvent::InputMethodInactive).unwrap();
    })
}

fn create_buffer<S>(
    shm: &wl_shm::WlShm,
    handle: &QueueHandle<S>,
    width: i32,
    height: i32,
    name: &str,
) -> wl_buffer::WlBuffer
where
    S: Dispatch<wl_shm_pool::WlShmPool, ()> + Dispatch<wl_buffer::WlBuffer, ()> + 'static,
{
    let size = width * height * 4;
    let fd = memfd_create(name, MemfdFlags::CLOEXEC).unwrap();
    ftruncate(&fd, u64::try_from(size).unwrap()).unwrap();
    let pool = shm.create_pool(fd.as_fd(), size, handle, ());
    let buffer = pool.create_buffer(
        0,
        width,
        height,
        width * 4,
        wl_shm::Format::Argb8888,
        handle,
        (),
    );
    pool.destroy();
    buffer
}

fn runtime_socket(runtime: &WaylandRuntime) -> PathBuf {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    PathBuf::from(runtime_dir).join(runtime.socket_name())
}

fn dispatch_until_result(
    runtime: &mut WaylandRuntime,
    results: &mpsc::Receiver<ResultEvent>,
) -> ResultEvent {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("input-method clients did not complete before the dispatch limit");
}

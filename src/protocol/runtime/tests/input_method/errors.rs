use super::*;

#[derive(Clone, Copy)]
enum InvalidRequest {
    SurroundingUtf8Boundary,
    PopupRoleConflict,
    PopupCapacity,
    KeyboardGrabCapacity,
    PopupSurfaceDestroyedFirst,
}

#[derive(Debug, Eq, PartialEq)]
enum FailureEvent {
    RequestSent,
    ClientRejected,
}

#[derive(Default)]
struct FailureClient;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for FailureClient {
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

delegate_noop!(FailureClient: ignore wl_compositor::WlCompositor);
delegate_noop!(FailureClient: ignore wl_surface::WlSurface);
delegate_noop!(FailureClient: ignore wl_seat::WlSeat);
delegate_noop!(FailureClient: ignore xdg_wm_base::XdgWmBase);
delegate_noop!(FailureClient: ignore xdg_surface::XdgSurface);
delegate_noop!(FailureClient: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(FailureClient: ignore zwp_text_input_manager_v3::ZwpTextInputManagerV3);
delegate_noop!(FailureClient: ignore zwp_text_input_v3::ZwpTextInputV3);
delegate_noop!(FailureClient: ignore zwp_input_method_manager_v2::ZwpInputMethodManagerV2);
delegate_noop!(FailureClient: ignore zwp_input_method_v2::ZwpInputMethodV2);
delegate_noop!(FailureClient: ignore zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2);
delegate_noop!(FailureClient: ignore zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2);

#[test]
fn text_input_rejects_non_boundary_utf8_indices_on_the_wire() {
    assert_protocol_failure(InvalidRequest::SurroundingUtf8Boundary);
}

#[test]
fn input_popup_rejects_a_surface_with_an_existing_role() {
    assert_protocol_failure(InvalidRequest::PopupRoleConflict);
}

#[test]
fn input_popup_rejects_the_seventeenth_live_role() {
    assert_protocol_failure(InvalidRequest::PopupCapacity);
}

#[test]
fn input_method_rejects_the_seventeenth_live_keyboard_grab() {
    assert_protocol_failure(InvalidRequest::KeyboardGrabCapacity);
}

#[test]
fn input_popup_rejects_destroying_its_surface_first() {
    assert_protocol_failure(InvalidRequest::PopupSurfaceDestroyedFirst);
}

fn assert_protocol_failure(request: InvalidRequest) {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let socket_path = runtime_socket(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::channel();

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<FailureClient>(&connection).unwrap();
        let handle = queue.handle();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();

        match request {
            InvalidRequest::SurroundingUtf8Boundary => {
                let manager = globals
                    .bind::<zwp_text_input_manager_v3::ZwpTextInputManagerV3, _, _>(
                        &handle,
                        1..=1,
                        (),
                    )
                    .unwrap();
                let text_input = manager.get_text_input(&seat, &handle, ());
                text_input.set_surrounding_text("中".to_owned(), 1, 0);
            }
            InvalidRequest::PopupRoleConflict => {
                let compositor = globals
                    .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
                    .unwrap();
                let shell = globals
                    .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
                    .unwrap();
                let manager = globals
                    .bind::<zwp_input_method_manager_v2::ZwpInputMethodManagerV2, _, _>(
                        &handle,
                        1..=1,
                        (),
                    )
                    .unwrap();
                let surface = compositor.create_surface(&handle, ());
                let xdg_surface = shell.get_xdg_surface(&surface, &handle, ());
                let _toplevel = xdg_surface.get_toplevel(&handle, ());
                let input_method = manager.get_input_method(&seat, &handle, ());
                let _popup = input_method.get_input_popup_surface(&surface, &handle, ());
            }
            InvalidRequest::PopupCapacity => {
                let compositor = globals
                    .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
                    .unwrap();
                let manager = globals
                    .bind::<zwp_input_method_manager_v2::ZwpInputMethodManagerV2, _, _>(
                        &handle,
                        1..=1,
                        (),
                    )
                    .unwrap();
                let input_method = manager.get_input_method(&seat, &handle, ());
                for _ in 0..=16 {
                    let surface = compositor.create_surface(&handle, ());
                    let _popup = input_method.get_input_popup_surface(&surface, &handle, ());
                }
            }
            InvalidRequest::KeyboardGrabCapacity => {
                let manager = globals
                    .bind::<zwp_input_method_manager_v2::ZwpInputMethodManagerV2, _, _>(
                        &handle,
                        1..=1,
                        (),
                    )
                    .unwrap();
                let input_method = manager.get_input_method(&seat, &handle, ());
                for _ in 0..=16 {
                    let _keyboard = input_method.grab_keyboard(&handle, ());
                }
            }
            InvalidRequest::PopupSurfaceDestroyedFirst => {
                let compositor = globals
                    .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
                    .unwrap();
                let manager = globals
                    .bind::<zwp_input_method_manager_v2::ZwpInputMethodManagerV2, _, _>(
                        &handle,
                        1..=1,
                        (),
                    )
                    .unwrap();
                let surface = compositor.create_surface(&handle, ());
                let input_method = manager.get_input_method(&seat, &handle, ());
                let _popup = input_method.get_input_popup_surface(&surface, &handle, ());
                surface.destroy();
            }
        }
        connection.flush().unwrap();
        result_tx.send(FailureEvent::RequestSent).unwrap();
        loop {
            if queue.blocking_dispatch(&mut FailureClient).is_err() {
                result_tx.send(FailureEvent::ClientRejected).unwrap();
                return;
            }
        }
    });

    assert_eq!(
        dispatch_until_failure(&mut runtime, &result_rx),
        FailureEvent::RequestSent
    );
    assert_eq!(
        dispatch_until_failure(&mut runtime, &result_rx),
        FailureEvent::ClientRejected
    );
    client.join().unwrap();
}

fn dispatch_until_failure(
    runtime: &mut WaylandRuntime,
    results: &mpsc::Receiver<FailureEvent>,
) -> FailureEvent {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("invalid input-method request did not complete before the dispatch limit");
}

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleEvent {
    ChildrenReady,
    ParentDestroyed,
}

#[test]
fn destroying_input_method_destroys_popup_and_keyboard_children() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let socket_path = runtime_socket(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::channel();
    let (command_tx, command_rx) = mpsc::channel();

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<InputMethodClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<zwp_input_method_manager_v2::ZwpInputMethodManagerV2, _, _>(&handle, 1..=1, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let input_method = manager.get_input_method(&seat, &handle, ());
        let popup = input_method.get_input_popup_surface(&surface, &handle, ());
        let keyboard = input_method.grab_keyboard(&handle, ());
        let mut state = InputMethodClient::default();
        queue.roundtrip(&mut state).unwrap();
        result_tx.send(LifecycleEvent::ChildrenReady).unwrap();

        command_rx.recv().unwrap();
        input_method.destroy();
        queue.roundtrip(&mut state).unwrap();
        result_tx.send(LifecycleEvent::ParentDestroyed).unwrap();
        let _ = (popup, keyboard, surface);
    });

    assert_eq!(
        dispatch_until_lifecycle_result(&mut runtime, &result_rx),
        LifecycleEvent::ChildrenReady
    );
    let child_ids = runtime.state.protocol_globals.input_method.child_ids();
    assert_eq!(child_ids.len(), 2);
    let backend = runtime.state.display_handle.backend_handle();
    assert!(
        child_ids
            .iter()
            .all(|id| backend.object_info(id.clone()).is_ok())
    );

    command_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until_lifecycle_result(&mut runtime, &result_rx),
        LifecycleEvent::ParentDestroyed
    );
    assert!(
        child_ids
            .iter()
            .all(|id| backend.object_info(id.clone()).is_err())
    );
    client.join().unwrap();
}

#[test]
fn child_destructors_release_fixed_capacity_before_the_next_dispatch() {
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
        let (globals, mut queue) = registry_queue_init::<InputMethodClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<zwp_input_method_manager_v2::ZwpInputMethodManagerV2, _, _>(&handle, 1..=1, ())
            .unwrap();
        let input_method = manager.get_input_method(&seat, &handle, ());
        let mut surfaces = Vec::with_capacity(17);
        let mut popups = Vec::with_capacity(16);
        let mut keyboards = Vec::with_capacity(16);
        for _ in 0..16 {
            let surface = compositor.create_surface(&handle, ());
            popups.push(input_method.get_input_popup_surface(&surface, &handle, ()));
            keyboards.push(input_method.grab_keyboard(&handle, ()));
            surfaces.push(surface);
        }
        let mut state = InputMethodClient::default();
        queue.roundtrip(&mut state).unwrap();

        popups.remove(0).destroy();
        keyboards.remove(0).release();
        let replacement_surface = compositor.create_surface(&handle, ());
        let _replacement_popup =
            input_method.get_input_popup_surface(&replacement_surface, &handle, ());
        let _replacement_keyboard = input_method.grab_keyboard(&handle, ());
        surfaces.push(replacement_surface);
        result_tx.send(LifecycleEvent::ChildrenReady).unwrap();
        queue.roundtrip(&mut state).unwrap();
    });

    assert_eq!(
        dispatch_until_lifecycle_result(&mut runtime, &result_rx),
        LifecycleEvent::ChildrenReady
    );
    while !client.is_finished() {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
    }
    client.join().unwrap();
}

fn dispatch_until_lifecycle_result(
    runtime: &mut WaylandRuntime,
    results: &mpsc::Receiver<LifecycleEvent>,
) -> LifecycleEvent {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("input-method child destruction did not complete before the dispatch limit");
}

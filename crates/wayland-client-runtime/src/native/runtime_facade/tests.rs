use super::*;
use crate::geometry::LogicalSize;
use crate::surface::{DecorationPreference, ToplevelAttributes};
use wayland_client::Proxy;

#[test]
fn native_runtime_connects_and_creates_toplevel_when_display_present() {
    let Ok(mut runtime) = NativeRuntime::connect() else {
        return;
    };
    let caps = runtime.capabilities();
    // Core desktop stack should always be present on a real compositor.
    let _ = (
        caps.fractional_scale,
        caps.cursor_shape,
        caps.text_input_v3,
        caps.pointer_gestures_v1,
        caps.pointer_constraints_v1,
        caps.xdg_dialog_v1,
        caps.xdg_toplevel_icon_v1,
        caps.ext_background_effect,
    );

    let surface = runtime
        .create_toplevel(ToplevelAttributes {
            title: "native-runtime-smoke".into(),
            app_id: "dev.fika.NativeRuntimeSmoke".into(),
            initial_size: Some(LogicalSize::new(320, 240)),
            min_size: Some(LogicalSize::new(160, 120)),
            max_size: None,
            decorations: DecorationPreference::Server,
        })
        .expect("create toplevel");
    let renderer_handle = runtime.surface_handle(surface).expect("renderer handle");
    let leased_surface = renderer_handle.native().wl_surface().clone();
    runtime
        .set_title(surface, "native-runtime-retitled".into())
        .expect("set title");
    runtime.request_frame(surface).expect("frame");
    assert!(runtime.is_frame_pending(surface));
    // Arm the preferred present path (presentation feedback or frame).
    runtime.arm_present_notify(surface).expect("arm present");
    assert!(runtime.is_present_pending(surface));
    // Coalesce while pending.
    runtime.request_frame(surface).expect("frame again");
    assert!(runtime.is_frame_pending(surface));
    runtime
        .arm_present_notify(surface)
        .expect("arm present again");
    assert!(runtime.is_present_pending(surface));
    assert_eq!(
        runtime.logical_size(surface),
        Some(LogicalSize::new(320, 240))
    );
    assert!(runtime.buffer_size(surface).is_some());
    assert_eq!(
        runtime.surface_kind(surface),
        Some(crate::surface::SurfaceKind::Toplevel)
    );
    runtime.commit(surface).expect("commit");
    // Non-blocking poll should not hang.
    runtime
        .dispatch(Some(Duration::from_millis(0)))
        .expect("dispatch");
    let mut events = Vec::new();
    runtime.drain_events_into(&mut events);
    runtime.destroy_surface(surface).expect("destroy");
    assert!(
        leased_surface.is_alive(),
        "runtime close must retain wl_surface while a renderer handle exists"
    );
    drop(renderer_handle);
    assert!(
        !leased_surface.is_alive(),
        "last renderer handle drop must destroy the retired wl_surface"
    );
}

#[test]
fn native_runtime_interactive_apis_require_serial() {
    let Ok(mut runtime) = NativeRuntime::connect() else {
        return;
    };
    let surface = runtime
        .create_toplevel(ToplevelAttributes {
            title: "serial".into(),
            app_id: "dev.fika.Serial".into(),
            ..Default::default()
        })
        .expect("toplevel");
    assert!(runtime.begin_interactive_move(surface).is_err());
    assert!(
        runtime
            .begin_interactive_resize(surface, crate::ResizeEdge::Bottom)
            .is_err()
    );
    let _ = runtime.destroy_surface(surface);
}

#[test]
fn native_runtime_popup_layer_outputs_and_app_id() {
    use crate::layer_shell::{LayerSurfaceAttributes, LayerSurfaceState};
    use crate::surface::{PopupAttributes, PopupPositioner};

    let Ok(mut runtime) = NativeRuntime::connect() else {
        return;
    };
    let _ = runtime.outputs();
    let parent = runtime
        .create_toplevel(ToplevelAttributes {
            title: "parent".into(),
            app_id: "dev.fika.Parent".into(),
            initial_size: Some(LogicalSize::new(400, 300)),
            ..Default::default()
        })
        .expect("parent");
    runtime
        .set_app_id(parent, "dev.fika.ParentRenamed")
        .expect("app_id");

    let positioner = PopupPositioner {
        size: LogicalSize::new(120, 80),
        anchor_rect: crate::geometry::LogicalRect::new(0, 0, 40, 20),
        ..PopupPositioner::default()
    };
    let popup = runtime
        .create_popup(
            parent,
            PopupAttributes {
                positioner: positioner.clone(),
                grab: None,
            },
        )
        .expect("popup");
    if runtime.capabilities().popup_reposition {
        let _ = runtime.reposition_popup(popup, &positioner, 1);
    }
    runtime.destroy_surface(popup).expect("destroy popup");

    if runtime.capabilities().layer_shell_v1 {
        let layer = runtime
            .create_layer_surface(LayerSurfaceAttributes {
                namespace: "fika-native-layer".into(),
                output: None,
                state: LayerSurfaceState {
                    size: LogicalSize::new(200, 32),
                    ..Default::default()
                },
            })
            .expect("layer");
        let state = runtime.layer_surface_state(layer).expect("state");
        assert_eq!(state.size.width, 200);
        runtime.destroy_surface(layer).expect("destroy layer");
    }

    if runtime.capabilities().xdg_activation_v1 {
        let _ =
            runtime.request_activation_token(parent, crate::ActivationTokenAttributes::default());
    }

    runtime.destroy_surface(parent).expect("destroy parent");
}

#[test]
fn dispatch_compio_wait_returns_on_timeout_and_zero() {
    let Ok(mut runtime) = NativeRuntime::connect() else {
        return;
    };
    let surface = runtime
        .create_toplevel(ToplevelAttributes {
            title: "dispatch-wait".into(),
            app_id: "dev.fika.DispatchWait".into(),
            initial_size: Some(LogicalSize::new(320, 240)),
            ..Default::default()
        })
        .expect("toplevel");
    // Non-blocking must return.
    runtime
        .dispatch(Some(Duration::from_millis(0)))
        .expect("zero");
    // Short timeout must return (Compio timer), not hang forever.
    let start = std::time::Instant::now();
    runtime
        .dispatch(Some(Duration::from_millis(50)))
        .expect("timeout");
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "dispatch timeout took {elapsed:?}"
    );
    // Infinite wait would hang; only exercise with wake from another thread.
    let wake = runtime.wake_handle();
    let handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        wake.wake();
    });
    let start = std::time::Instant::now();
    runtime.dispatch(None).expect("wake");
    let elapsed = start.elapsed();
    handle.join().unwrap();
    assert!(
        elapsed < Duration::from_millis(500),
        "dispatch None + wake took {elapsed:?}"
    );
    let _ = runtime.destroy_surface(surface);
}

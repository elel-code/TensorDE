use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_region, wl_registry, wl_surface},
};
use wayland_protocols::{
    ext::background_effect::v1::client::{
        ext_background_effect_manager_v1, ext_background_effect_surface_v1,
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EffectEvent {
    Enabled,
    Disabled,
}

#[derive(Default)]
struct EffectClient {
    configured: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for EffectClient {
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

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for EffectClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for EffectClient {
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

delegate_noop!(EffectClient: ignore wl_compositor::WlCompositor);
delegate_noop!(EffectClient: ignore wl_region::WlRegion);
delegate_noop!(EffectClient: ignore wl_surface::WlSurface);
delegate_noop!(EffectClient: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(EffectClient: ignore ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1);
delegate_noop!(EffectClient: ignore ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1);

#[test]
fn background_effect_wire_applies_only_on_surface_commit() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (event_tx, event_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<EffectClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let manager = globals
            .bind::<ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        surface.commit();

        let mut state = EffectClient::default();
        while !state.configured {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        let region = compositor.create_region(&handle, ());
        region.add(0, 0, 100, 50);
        let effect = manager.get_background_effect(&surface, &handle, ());
        effect.set_blur_region(Some(&region));
        surface.commit();
        connection.roundtrip().unwrap();
        event_tx.send(EffectEvent::Enabled).unwrap();
        release_rx.recv().unwrap();

        effect.set_blur_region(None);
        surface.commit();
        connection.roundtrip().unwrap();
        event_tx.send(EffectEvent::Disabled).unwrap();

        effect.destroy();
        region.destroy();
        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
        manager.destroy();
    });

    assert_eq!(
        dispatch_until_effect(&mut runtime, &event_rx),
        EffectEvent::Enabled
    );
    assert!(mapped_protocol_effect_enabled(&runtime));
    assert!(mapped_backdrop_enabled(&runtime));
    release_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until_effect(&mut runtime, &event_rx),
        EffectEvent::Disabled
    );
    assert!(!mapped_backdrop_enabled(&runtime));
    client.join().unwrap();
}

fn dispatch_until_effect(
    runtime: &mut WaylandRuntime,
    events: &mpsc::Receiver<EffectEvent>,
) -> EffectEvent {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(event) = events.try_recv() {
            return event;
        }
    }
    panic!("background-effect client did not complete before the dispatch limit");
}

fn mapped_backdrop_enabled(runtime: &WaylandRuntime) -> bool {
    let Some(root) = runtime
        .state
        .space
        .elements()
        .next()
        .and_then(|window| window.wl_surface())
    else {
        return false;
    };
    let Some(view) = runtime.state.view_for_surface(&root) else {
        return false;
    };
    runtime
        .state
        .world
        .view_effects(view)
        .is_some_and(|effects| effects.backdrop_blur.is_some())
}

fn mapped_protocol_effect_enabled(runtime: &WaylandRuntime) -> bool {
    runtime
        .state
        .space
        .elements()
        .next()
        .and_then(|window| window.wl_surface())
        .is_some_and(|surface| {
            runtime
                .state
                .protocol_globals
                .committed_background_has_area(&surface)
        })
}

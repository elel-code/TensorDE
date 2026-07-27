use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use tensor_protocol::{SurfaceAlpha, SurfaceContentType};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::{
    wp::{
        alpha_modifier::v1::client::{wp_alpha_modifier_surface_v1, wp_alpha_modifier_v1},
        content_type::v1::client::{wp_content_type_manager_v1, wp_content_type_v1},
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;
use crate::protocol::state::test_surface_tree_states;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MetadataStep {
    Applied,
    Cleared,
}

#[derive(Default)]
struct MetadataClient {
    configured: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for MetadataClient {
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

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for MetadataClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for MetadataClient {
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

delegate_noop!(MetadataClient: ignore wl_buffer::WlBuffer);
delegate_noop!(MetadataClient: ignore wl_compositor::WlCompositor);
delegate_noop!(MetadataClient: ignore wl_surface::WlSurface);
delegate_noop!(MetadataClient: ignore wp_alpha_modifier_v1::WpAlphaModifierV1);
delegate_noop!(MetadataClient: ignore wp_alpha_modifier_surface_v1::WpAlphaModifierSurfaceV1);
delegate_noop!(MetadataClient: ignore wp_content_type_manager_v1::WpContentTypeManagerV1);
delegate_noop!(MetadataClient: ignore wp_content_type_v1::WpContentTypeV1);
delegate_noop!(MetadataClient: ignore wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1);
delegate_noop!(MetadataClient: ignore xdg_toplevel::XdgToplevel);

#[test]
fn surface_metadata_wire_is_double_buffered_and_resets_on_destroy() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<MetadataClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let content_manager = globals
            .bind::<wp_content_type_manager_v1::WpContentTypeManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let alpha_manager = globals
            .bind::<wp_alpha_modifier_v1::WpAlphaModifierV1, _, _>(&handle, 1..=1, ())
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
        let mut state = MetadataClient::default();
        while !state.configured {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        let content = content_manager.get_surface_content_type(&surface, &handle, ());
        let alpha = alpha_manager.get_surface(&surface, &handle, ());
        let pixel = single_pixel.create_u32_rgba_buffer(
            u32::MAX,
            u32::MAX,
            u32::MAX,
            u32::MAX,
            &handle,
            (),
        );
        content.set_content_type(wp_content_type_v1::Type::Video);
        alpha.set_multiplier(0x1234_5678);
        surface.attach(Some(&pixel), 0, 0);
        surface.damage_buffer(0, 0, 1, 1);
        surface.commit();
        queue.roundtrip(&mut state).unwrap();
        step_tx.send(MetadataStep::Applied).unwrap();
        release_rx.recv().unwrap();

        content.destroy();
        alpha.destroy();
        surface.commit();
        queue.roundtrip(&mut state).unwrap();
        step_tx.send(MetadataStep::Cleared).unwrap();
        release_rx.recv().unwrap();

        pixel.destroy();
        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
        single_pixel.destroy();
        alpha_manager.destroy();
        content_manager.destroy();
    });

    assert_eq!(
        dispatch_until_metadata_step(&mut runtime, &step_rx),
        MetadataStep::Applied
    );
    let root = mapped_metadata_root(&runtime);
    let state = test_surface_tree_states(&root)
        .into_iter()
        .find(|state| state.surface == root.id().protocol_id())
        .expect("mapped root has a render snapshot");
    assert_eq!(state.alpha, SurfaceAlpha::from_raw(0x1234_5678));
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .surface_metadata
            .committed_content_type(&root),
        SurfaceContentType::Video
    );
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_metadata_step(&mut runtime, &step_rx),
        MetadataStep::Cleared
    );
    let state = test_surface_tree_states(&root)
        .into_iter()
        .find(|state| state.surface == root.id().protocol_id())
        .expect("mapped root remains live until client cleanup");
    assert_eq!(state.alpha, SurfaceAlpha::OPAQUE);
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .surface_metadata
            .committed_content_type(&root),
        SurfaceContentType::None
    );
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

fn dispatch_until_metadata_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<MetadataStep>,
) -> MetadataStep {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("surface metadata client did not complete before the dispatch limit");
}

fn mapped_metadata_root(
    runtime: &WaylandRuntime,
) -> wayland_server::protocol::wl_surface::WlSurface {
    runtime
        .state
        .space
        .elements()
        .next()
        .and_then(|window| window.wl_surface())
        .map(std::borrow::Cow::into_owned)
        .expect("metadata test toplevel is mapped")
}

use std::{os::fd::AsFd, os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use rustix::fs::{MemfdFlags, ftruncate, memfd_create};
use tensor_protocol::SurfaceTransform;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_subcompositor,
        wl_subsurface, wl_surface,
    },
};
use wayland_protocols::{
    wp::viewporter::client::{wp_viewport, wp_viewporter},
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;
use crate::protocol::state::test_surface_tree_states;

#[derive(Debug, Eq, PartialEq)]
enum Step {
    RootAttached { surface: u32, buffer: u32 },
    RootDamaged,
    ViewportApplied,
    ChildDeferred { surface: u32, buffer: u32 },
    ChildApplied,
    RootDetached { releases: u8 },
}

#[derive(Default)]
struct SurfaceClient {
    configured: bool,
    buffer_releases: u8,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for SurfaceClient {
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

impl Dispatch<wl_buffer::WlBuffer, ()> for SurfaceClient {
    fn event(
        state: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_buffer::Event::Release) {
            state.buffer_releases = state.buffer_releases.saturating_add(1);
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for SurfaceClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for SurfaceClient {
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

delegate_noop!(SurfaceClient: ignore wl_compositor::WlCompositor);
delegate_noop!(SurfaceClient: ignore wl_shm::WlShm);
delegate_noop!(SurfaceClient: ignore wl_shm_pool::WlShmPool);
delegate_noop!(SurfaceClient: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(SurfaceClient: ignore wl_subsurface::WlSubsurface);
delegate_noop!(SurfaceClient: ignore wl_surface::WlSurface);
delegate_noop!(SurfaceClient: ignore wp_viewport::WpViewport);
delegate_noop!(SurfaceClient: ignore wp_viewporter::WpViewporter);
delegate_noop!(SurfaceClient: ignore xdg_toplevel::XdgToplevel);

#[test]
fn surface_state_applies_buffer_damage_viewport_and_synchronized_child_commits() {
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
        let (globals, mut queue) = registry_queue_init::<SurfaceClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let subcompositor = globals
            .bind::<wl_subcompositor::WlSubcompositor, _, _>(&handle, 1..=1, ())
            .unwrap();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=1, ())
            .unwrap();
        let viewporter = globals
            .bind::<wp_viewporter::WpViewporter, _, _>(&handle, 1..=1, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();

        let root = compositor.create_surface(&handle, ());
        let root_xdg = wm_base.get_xdg_surface(&root, &handle, ());
        let root_toplevel = root_xdg.get_toplevel(&handle, ());
        root.commit();
        let mut state = SurfaceClient::default();
        while !state.configured {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        let root_buffer = create_shm_buffer(&shm, &handle, 64, 32);
        root.attach(Some(&root_buffer), 0, 0);
        root.damage_buffer(0, 0, 64, 32);
        root.commit();
        queue.roundtrip(&mut state).unwrap();
        event_tx
            .send(Step::RootAttached {
                surface: root.id().protocol_id(),
                buffer: root_buffer.id().protocol_id(),
            })
            .unwrap();
        release_rx.recv().unwrap();

        root.damage_buffer(4, 3, 8, 7);
        root.commit();
        queue.roundtrip(&mut state).unwrap();
        event_tx.send(Step::RootDamaged).unwrap();
        release_rx.recv().unwrap();

        let viewport = viewporter.get_viewport(&root, &handle, ());
        viewport.set_destination(40, 20);
        root.set_buffer_scale(2);
        root.set_buffer_transform(wl_output::Transform::_90);
        root.commit();
        queue.roundtrip(&mut state).unwrap();
        event_tx.send(Step::ViewportApplied).unwrap();
        release_rx.recv().unwrap();

        let child = compositor.create_surface(&handle, ());
        let child_buffer = create_shm_buffer(&shm, &handle, 16, 8);
        let subsurface = subcompositor.get_subsurface(&child, &root, &handle, ());
        subsurface.set_position(7, 5);
        child.attach(Some(&child_buffer), 0, 0);
        child.damage_buffer(0, 0, 16, 8);
        child.commit();
        queue.roundtrip(&mut state).unwrap();
        event_tx
            .send(Step::ChildDeferred {
                surface: child.id().protocol_id(),
                buffer: child_buffer.id().protocol_id(),
            })
            .unwrap();
        release_rx.recv().unwrap();

        root.commit();
        queue.roundtrip(&mut state).unwrap();
        event_tx.send(Step::ChildApplied).unwrap();
        release_rx.recv().unwrap();

        root.attach(None, 0, 0);
        root.commit();
        queue.roundtrip(&mut state).unwrap();
        event_tx
            .send(Step::RootDetached {
                releases: state.buffer_releases,
            })
            .unwrap();

        subsurface.destroy();
        child.destroy();
        viewport.destroy();
        root_toplevel.destroy();
        root_xdg.destroy();
        root.destroy();
    });

    let (root_id, root_buffer_id) = match dispatch_until_step(&mut runtime, &event_rx) {
        Step::RootAttached { surface, buffer } => (surface, buffer),
        step => panic!("expected root attach, got {step:?}"),
    };
    let root = mapped_root(&runtime);
    let states = test_surface_tree_states(&root);
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].surface, root_id);
    assert_eq!(states[0].buffer, Some(root_buffer_id));
    assert_eq!(states[0].offset, (0, 0));
    assert_eq!(states[0].size, (64, 32));
    assert_eq!(states[0].commit, 1);
    assert_eq!(states[0].buffer_scale, 1);
    assert_eq!(states[0].transform, SurfaceTransform::Normal);
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_step(&mut runtime, &event_rx),
        Step::RootDamaged
    );
    assert_eq!(test_surface_tree_states(&root)[0].commit, 2);
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_step(&mut runtime, &event_rx),
        Step::ViewportApplied
    );
    let state = test_surface_tree_states(&root)[0];
    assert_eq!(state.size, (40, 20));
    assert_eq!(state.commit, 2);
    assert_eq!(state.buffer_scale, 2);
    assert_eq!(state.transform, SurfaceTransform::Rotate90);
    release_tx.send(()).unwrap();

    let (child_id, child_buffer_id) = match dispatch_until_step(&mut runtime, &event_rx) {
        Step::ChildDeferred { surface, buffer } => (surface, buffer),
        step => panic!("expected deferred child, got {step:?}"),
    };
    assert_eq!(test_surface_tree_states(&root).len(), 1);
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_step(&mut runtime, &event_rx),
        Step::ChildApplied
    );
    let states = test_surface_tree_states(&root);
    let child = states
        .iter()
        .find(|state| state.surface == child_id)
        .expect("parent commit applies synchronized child state");
    assert_eq!(child.buffer, Some(child_buffer_id));
    assert_eq!(child.offset, (7, 5));
    assert_eq!(child.size, (16, 8));
    assert_eq!(child.commit, 1);
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_step(&mut runtime, &event_rx),
        Step::RootDetached { releases: 1 }
    );
    let states = test_surface_tree_states(&root);
    assert!(states.iter().all(|state| state.surface != root_id));
    client.join().unwrap();
}

fn create_shm_buffer(
    shm: &wl_shm::WlShm,
    handle: &QueueHandle<SurfaceClient>,
    width: i32,
    height: i32,
) -> wl_buffer::WlBuffer {
    let size = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .expect("test buffer size fits i32");
    let fd = memfd_create("tensor-surface-state-test", MemfdFlags::CLOEXEC).unwrap();
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

fn mapped_root(runtime: &WaylandRuntime) -> wayland_server::protocol::wl_surface::WlSurface {
    runtime
        .state
        .space
        .elements()
        .next()
        .and_then(|window| window.wl_surface())
        .map(std::borrow::Cow::into_owned)
        .expect("test toplevel is mapped")
}

fn dispatch_until_step(runtime: &mut WaylandRuntime, events: &mpsc::Receiver<Step>) -> Step {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(event) = events.try_recv() {
            return event;
        }
    }
    panic!("Wayland surface client did not complete before the dispatch limit");
}

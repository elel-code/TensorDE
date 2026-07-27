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
    wp::{
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
        viewporter::client::{wp_viewport, wp_viewporter},
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::*;
use crate::protocol::{
    globals::single_pixel_buffer::single_pixel_rgba,
    state::{test_surface_buffer, test_surface_tree_states},
};

#[derive(Debug, Eq, PartialEq)]
enum Step {
    RootAttached { surface: u32, buffer: u32 },
    RootDamaged,
    ViewportApplied,
    ChildDeferred { surface: u32, buffer: u32 },
    ChildApplied,
    SinglePixelAttached { buffer: u32 },
    ViewportRemoved,
    ViewportRecreated,
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
delegate_noop!(SurfaceClient: ignore wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1);
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
        let single_pixel = globals
            .bind::<wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
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
        viewport.set_source(2.0, 4.0, 10.0, 20.0);
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

        let pixel = single_pixel.create_u32_rgba_buffer(
            0x1122_3344,
            0x5566_7788,
            0x99aa_bbcc,
            0xddee_ff00,
            &handle,
            (),
        );
        viewport.set_source(-1.0, -1.0, -1.0, -1.0);
        viewport.set_destination(9, 7);
        root.set_buffer_scale(1);
        root.set_buffer_transform(wl_output::Transform::Normal);
        root.attach(Some(&pixel), 0, 0);
        root.damage_buffer(0, 0, 1, 1);
        root.commit();
        queue.roundtrip(&mut state).unwrap();
        event_tx
            .send(Step::SinglePixelAttached {
                buffer: pixel.id().protocol_id(),
            })
            .unwrap();
        release_rx.recv().unwrap();

        viewport.destroy();
        root.commit();
        queue.roundtrip(&mut state).unwrap();
        event_tx.send(Step::ViewportRemoved).unwrap();
        release_rx.recv().unwrap();

        let replacement_viewport = viewporter.get_viewport(&root, &handle, ());
        replacement_viewport.set_destination(5, 4);
        root.commit();
        queue.roundtrip(&mut state).unwrap();
        event_tx.send(Step::ViewportRecreated).unwrap();
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
        replacement_viewport.destroy();
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
    assert_eq!(states[0].source, None);
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
    assert_eq!(
        state.source.expect("source crop was committed").raw_fixed(),
        [512, 1024, 2560, 5120]
    );
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

    let pixel_id = match dispatch_until_step(&mut runtime, &event_rx) {
        Step::SinglePixelAttached { buffer } => buffer,
        step => panic!("expected single-pixel attach, got {step:?}"),
    };
    let state = test_surface_tree_states(&root)
        .into_iter()
        .find(|state| state.surface == root_id)
        .expect("single-pixel root remains in the surface tree");
    assert_eq!(state.buffer, Some(pixel_id));
    assert_eq!(state.size, (9, 7));
    assert_eq!(state.commit, 3);
    assert_eq!(state.buffer_scale, 1);
    assert_eq!(state.transform, SurfaceTransform::Normal);
    assert_eq!(state.source, None);
    let pixel = test_surface_buffer(&root).expect("single-pixel buffer is current");
    assert_eq!(
        single_pixel_rgba(&pixel),
        Some(&[0x1122_3344, 0x5566_7788, 0x99aa_bbcc, 0xddee_ff00])
    );
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_step(&mut runtime, &event_rx),
        Step::ViewportRemoved
    );
    let state = test_surface_tree_states(&root)
        .into_iter()
        .find(|state| state.surface == root_id)
        .expect("root remains mapped after viewport removal");
    assert_eq!(state.size, (1, 1));
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_step(&mut runtime, &event_rx),
        Step::ViewportRecreated
    );
    let state = test_surface_tree_states(&root)
        .into_iter()
        .find(|state| state.surface == root_id)
        .expect("root remains mapped after viewport recreation");
    assert_eq!(state.size, (5, 4));
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_step(&mut runtime, &event_rx),
        Step::RootDetached { releases: 2 }
    );
    let states = test_surface_tree_states(&root);
    assert!(states.iter().all(|state| state.surface != root_id));
    client.join().unwrap();
}

#[derive(Clone, Copy, Debug)]
enum ShmViolation {
    FileTooSmall,
    InvalidStride,
    UnsupportedFormat,
    Shrink,
}

#[test]
fn shm_reports_wire_errors_on_the_owning_protocol_objects() {
    let cases = [
        (
            ShmViolation::FileTooSmall,
            "wl_shm",
            u32::from(wl_shm::Error::InvalidFd),
        ),
        (
            ShmViolation::InvalidStride,
            "wl_shm_pool",
            u32::from(wl_shm::Error::InvalidStride),
        ),
        (
            ShmViolation::UnsupportedFormat,
            "wl_shm_pool",
            u32::from(wl_shm::Error::InvalidFormat),
        ),
        (
            ShmViolation::Shrink,
            "wl_shm_pool",
            u32::from(wl_shm::Error::InvalidFd),
        ),
    ];

    for (violation, expected_interface, expected_code) in cases {
        let (interface, code) = shm_protocol_error(violation);
        assert_eq!(interface, expected_interface, "{violation:?}");
        assert_eq!(code, expected_code, "{violation:?}");
    }
}

fn shm_protocol_error(violation: ShmViolation) -> (String, u32) {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(1);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<SurfaceClient>(&connection).unwrap();
        let handle = queue.handle();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
            .unwrap();
        let fd = memfd_create("tensor-shm-wire-error-test", MemfdFlags::CLOEXEC).unwrap();
        ftruncate(&fd, 4096).unwrap();
        let pool_size = if matches!(violation, ShmViolation::FileTooSmall) {
            8192
        } else {
            4096
        };
        let pool = shm.create_pool(fd.as_fd(), pool_size, &handle, ());
        match violation {
            ShmViolation::FileTooSmall => {}
            ShmViolation::InvalidStride => {
                let _buffer =
                    pool.create_buffer(0, 16, 16, 63, wl_shm::Format::Argb8888, &handle, ());
            }
            ShmViolation::UnsupportedFormat => {
                let _buffer = pool.create_buffer(0, 16, 16, 64, wl_shm::Format::C8, &handle, ());
            }
            ShmViolation::Shrink => pool.resize(2048),
        }

        assert!(queue.roundtrip(&mut SurfaceClient::default()).is_err());
        let error = connection
            .protocol_error()
            .expect("expected protocol error");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    let result = dispatch_until_protocol_error(&mut runtime, &result_rx);
    client.join().unwrap();
    result
}

#[derive(Clone, Copy, Debug)]
enum ViewportViolation {
    Duplicate,
    BadValue,
    BadSize,
    OutOfBuffer,
    NoSurface,
}

#[test]
fn viewporter_reports_wire_errors_on_the_owning_protocol_objects() {
    let cases = [
        (
            ViewportViolation::Duplicate,
            "wp_viewporter",
            u32::from(wp_viewporter::Error::ViewportExists),
        ),
        (
            ViewportViolation::BadValue,
            "wp_viewport",
            u32::from(wp_viewport::Error::BadValue),
        ),
        (
            ViewportViolation::BadSize,
            "wp_viewport",
            u32::from(wp_viewport::Error::BadSize),
        ),
        (
            ViewportViolation::OutOfBuffer,
            "wp_viewport",
            u32::from(wp_viewport::Error::OutOfBuffer),
        ),
        (
            ViewportViolation::NoSurface,
            "wp_viewport",
            u32::from(wp_viewport::Error::NoSurface),
        ),
    ];

    for (violation, expected_interface, expected_code) in cases {
        let (interface, code) = viewport_protocol_error(violation);
        assert_eq!(interface, expected_interface, "{violation:?}");
        assert_eq!(code, expected_code, "{violation:?}");
    }
}

fn viewport_protocol_error(violation: ViewportViolation) -> (String, u32) {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(1);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<SurfaceClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
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
        let viewport = viewporter.get_viewport(&surface, &handle, ());

        match violation {
            ViewportViolation::Duplicate => {
                let _duplicate = viewporter.get_viewport(&surface, &handle, ());
            }
            ViewportViolation::BadValue => viewport.set_destination(0, 1),
            ViewportViolation::BadSize => {
                viewport.set_source(0.0, 0.0, 1.5, 1.0);
                surface.commit();
            }
            ViewportViolation::OutOfBuffer => {
                let buffer = single_pixel.create_u32_rgba_buffer(
                    u32::MAX,
                    u32::MAX,
                    u32::MAX,
                    u32::MAX,
                    &handle,
                    (),
                );
                viewport.set_source(0.0, 0.0, 2.0, 1.0);
                surface.attach(Some(&buffer), 0, 0);
                surface.commit();
            }
            ViewportViolation::NoSurface => {
                surface.destroy();
                viewport.set_destination(1, 1);
            }
        }

        assert!(queue.roundtrip(&mut SurfaceClient::default()).is_err());
        let error = connection
            .protocol_error()
            .expect("expected protocol error");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    let result = dispatch_until_protocol_error(&mut runtime, &result_rx);
    client.join().unwrap();
    result
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

fn dispatch_until_protocol_error(
    runtime: &mut WaylandRuntime,
    results: &mpsc::Receiver<(String, u32)>,
) -> (String, u32) {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("Wayland protocol-error client did not complete before the dispatch limit");
}

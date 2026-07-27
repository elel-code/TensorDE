use std::{
    os::{fd::AsFd, unix::net::UnixStream},
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

use rustix::{
    fs::{MemfdFlags, ftruncate, memfd_create},
    io::pwrite,
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface},
};
use wayland_protocols::{
    wp::{
        pointer_warp::v1::client::wp_pointer_warp_v1,
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1,
    },
    xdg::{
        shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
        system_bell::v1::client::xdg_system_bell_v1,
        toplevel_icon::v1::client::{xdg_toplevel_icon_manager_v1, xdg_toplevel_icon_v1},
        toplevel_tag::v1::client::xdg_toplevel_tag_manager_v1,
    },
};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DesktopStep {
    Pending,
    Applied,
}

#[derive(Clone, Copy, Debug)]
enum IconViolation {
    NonShm,
    NonSquare,
    BufferDestroyed,
    ImmutableName,
    ImmutableBuffer,
}

#[derive(Default)]
struct DesktopClient {
    configured: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for DesktopClient {
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

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for DesktopClient {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for DesktopClient {
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

delegate_noop!(DesktopClient: ignore wl_compositor::WlCompositor);
delegate_noop!(DesktopClient: ignore wl_surface::WlSurface);
delegate_noop!(DesktopClient: ignore wl_shm::WlShm);
delegate_noop!(DesktopClient: ignore wl_shm_pool::WlShmPool);
delegate_noop!(DesktopClient: ignore wl_buffer::WlBuffer);
delegate_noop!(DesktopClient: ignore wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1);
delegate_noop!(DesktopClient: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(DesktopClient: ignore xdg_system_bell_v1::XdgSystemBellV1);
delegate_noop!(DesktopClient: ignore xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1);
delegate_noop!(DesktopClient: ignore xdg_toplevel_icon_v1::XdgToplevelIconV1);
delegate_noop!(DesktopClient: ignore xdg_toplevel_tag_manager_v1::XdgToplevelTagManagerV1);
delegate_noop!(DesktopClient: ignore wp_pointer_warp_v1::WpPointerWarpV1);

#[test]
fn desktop_control_globals_bind_and_publish_toplevel_text() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<DesktopClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let tags = globals
            .bind::<xdg_toplevel_tag_manager_v1::XdgToplevelTagManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let bell = globals
            .bind::<xdg_system_bell_v1::XdgSystemBellV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let icons = globals
            .bind::<xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let warp = globals
            .bind::<wp_pointer_warp_v1::WpPointerWarpV1, _, _>(&handle, 1..=1, ())
            .unwrap();

        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        surface.commit();
        let mut state = DesktopClient::default();
        while !state.configured {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        tags.set_toplevel_tag(&toplevel, "settings".to_owned());
        tags.set_toplevel_description(&toplevel, "Application settings".to_owned());
        let icon = icons.create_icon(&handle, ());
        icon.set_name("org.tensor.Settings".to_owned());
        let replaced = create_icon_buffer(&shm, &handle, 2, [1, 2, 3, 4]);
        let buffer = create_icon_buffer(&shm, &handle, 2, [0x54, 0x45, 0x4e, 0x53]);
        icon.add_buffer(&replaced, 2);
        icon.add_buffer(&buffer, 2);
        replaced.destroy();
        icons.set_icon(&toplevel, Some(&icon));
        bell.ring(Some(&surface));
        queue.roundtrip(&mut state).unwrap();
        step_tx.send(DesktopStep::Pending).unwrap();
        release_rx.recv().unwrap();

        surface.commit();
        icon.destroy();
        buffer.destroy();
        queue.roundtrip(&mut state).unwrap();
        step_tx.send(DesktopStep::Applied).unwrap();
        release_rx.recv().unwrap();

        warp.destroy();
        icons.destroy();
        bell.destroy();
        tags.destroy();
        toplevel.destroy();
        xdg_surface.destroy();
        surface.destroy();
    });

    assert_eq!(
        dispatch_until_desktop_step(&mut runtime, &step_rx),
        DesktopStep::Pending
    );
    let root = runtime
        .state
        .space
        .elements()
        .next()
        .and_then(|window| window.wl_surface())
        .map(std::borrow::Cow::into_owned)
        .expect("desktop-control toplevel is mapped");
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .desktop_controls
            .toplevel_text(&root),
        Some(("settings".to_owned(), "Application settings".to_owned()))
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .desktop_controls
            .toplevel_icon_name(&root),
        None
    );
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_desktop_step(&mut runtime, &step_rx),
        DesktopStep::Applied
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .desktop_controls
            .toplevel_icon_name(&root)
            .as_deref(),
        Some("org.tensor.Settings")
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .desktop_controls
            .toplevel_icon_buffer_sample(&root),
        Some((2, 2, 2, [0x54, 0x45, 0x4e, 0x53]))
    );
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

#[test]
fn toplevel_icon_reports_invalid_immutable_and_lifetime_errors() {
    use xdg_toplevel_icon_v1::Error;

    for (violation, expected) in [
        (IconViolation::NonShm, Error::InvalidBuffer),
        (IconViolation::NonSquare, Error::InvalidBuffer),
        (IconViolation::BufferDestroyed, Error::NoBuffer),
        (IconViolation::ImmutableName, Error::Immutable),
        (IconViolation::ImmutableBuffer, Error::Immutable),
    ] {
        assert_eq!(
            icon_protocol_error(violation),
            ("xdg_toplevel_icon_v1".to_owned(), expected as u32),
            "unexpected result for {violation:?}"
        );
    }
}

fn icon_protocol_error(violation: IconViolation) -> (String, u32) {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<DesktopClient>(&connection).unwrap();
        let handle = queue.handle();
        let icons = globals
            .bind::<xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
            .unwrap();
        let icon = icons.create_icon(&handle, ());

        match violation {
            IconViolation::NonShm => {
                let single_pixel = globals
                    .bind::<wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1, _, _>(
                        &handle,
                        1..=1,
                        (),
                    )
                    .unwrap();
                let buffer = single_pixel.create_u32_rgba_buffer(
                    u32::MAX,
                    u32::MAX,
                    u32::MAX,
                    u32::MAX,
                    &handle,
                    (),
                );
                icon.add_buffer(&buffer, 1);
            }
            IconViolation::NonSquare => {
                let buffer = create_shm_icon_buffer(&shm, &handle, 2, 1, [1, 2, 3, 4]);
                icon.add_buffer(&buffer, 1);
            }
            IconViolation::BufferDestroyed => {
                let buffer = create_icon_buffer(&shm, &handle, 2, [1, 2, 3, 4]);
                icon.add_buffer(&buffer, 1);
                buffer.destroy();
            }
            IconViolation::ImmutableName | IconViolation::ImmutableBuffer => {
                let compositor = globals
                    .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
                    .unwrap();
                let wm_base = globals
                    .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
                    .unwrap();
                let surface = compositor.create_surface(&handle, ());
                let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
                let toplevel = xdg_surface.get_toplevel(&handle, ());
                icons.set_icon(&toplevel, Some(&icon));
                match violation {
                    IconViolation::ImmutableName => icon.set_name("changed".to_owned()),
                    IconViolation::ImmutableBuffer => {
                        let buffer = create_icon_buffer(&shm, &handle, 2, [1, 2, 3, 4]);
                        icon.add_buffer(&buffer, 1);
                    }
                    _ => unreachable!(),
                }
            }
        }

        assert!(queue.roundtrip(&mut DesktopClient::default()).is_err());
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

fn create_icon_buffer(
    shm: &wl_shm::WlShm,
    handle: &QueueHandle<DesktopClient>,
    edge: i32,
    marker: [u8; 4],
) -> wl_buffer::WlBuffer {
    create_shm_icon_buffer(shm, handle, edge, edge, marker)
}

fn create_shm_icon_buffer(
    shm: &wl_shm::WlShm,
    handle: &QueueHandle<DesktopClient>,
    width: i32,
    height: i32,
    marker: [u8; 4],
) -> wl_buffer::WlBuffer {
    let size = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .expect("test icon buffer size fits i32");
    let fd = memfd_create("tensor-desktop-icon-test", MemfdFlags::CLOEXEC).unwrap();
    ftruncate(&fd, u64::try_from(size).unwrap()).unwrap();
    assert_eq!(pwrite(&fd, &marker, 0).unwrap(), marker.len());
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

fn dispatch_until_desktop_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<DesktopStep>,
) -> DesktopStep {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("desktop-control client did not complete before the dispatch limit");
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
    panic!("desktop-control protocol error did not arrive before the dispatch limit");
}

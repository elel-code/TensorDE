use std::{
    os::{fd::AsFd, unix::net::UnixStream},
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

use rustix::{
    fs::{MemfdFlags, ftruncate, memfd_create},
    io::pread,
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool},
};
use wayland_protocols::ext::{
    image_capture_source::v1::client::{
        ext_image_capture_source_v1, ext_output_image_capture_source_manager_v1,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_cursor_session_v1, ext_image_copy_capture_frame_v1,
        ext_image_copy_capture_manager_v1, ext_image_copy_capture_session_v1,
    },
};

use super::*;

const WIDTH: u32 = 1000;
const HEIGHT: u32 = 800;
const STRIDE: i32 = WIDTH as i32 * 4;
const BUFFER_LEN: usize = WIDTH as usize * HEIGHT as usize * 4;

#[derive(Default)]
struct CaptureClient {
    size: Option<(u32, u32)>,
    shm_formats: Vec<wl_shm::Format>,
    constraints_done: bool,
    damage: Vec<(i32, i32, i32, i32)>,
    presented: Option<Duration>,
    ready: bool,
    failed: Option<ext_image_copy_capture_frame_v1::FailureReason>,
    cursor_entered: bool,
    cursor_left: bool,
    cursor_position: Option<(i32, i32)>,
    cursor_hotspot: Option<(i32, i32)>,
}

impl Dispatch<ext_image_copy_capture_cursor_session_v1::ExtImageCopyCaptureCursorSessionV1, ()>
    for CaptureClient
{
    fn event(
        state: &mut Self,
        _: &ext_image_copy_capture_cursor_session_v1::ExtImageCopyCaptureCursorSessionV1,
        event: ext_image_copy_capture_cursor_session_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_image_copy_capture_cursor_session_v1::Event::Enter => {
                state.cursor_entered = true;
            }
            ext_image_copy_capture_cursor_session_v1::Event::Leave => {
                state.cursor_left = true;
            }
            ext_image_copy_capture_cursor_session_v1::Event::Position { x, y } => {
                state.cursor_position = Some((x, y));
            }
            ext_image_copy_capture_cursor_session_v1::Event::Hotspot { x, y } => {
                state.cursor_hotspot = Some((x, y));
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for CaptureClient {
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

impl Dispatch<ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1, ()>
    for CaptureClient
{
    fn event(
        state: &mut Self,
        _: &ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_image_copy_capture_session_v1::Event::BufferSize { width, height } => {
                state.size = Some((width, height));
            }
            ext_image_copy_capture_session_v1::Event::ShmFormat {
                format: WEnum::Value(format),
            } => state.shm_formats.push(format),
            ext_image_copy_capture_session_v1::Event::Done => {
                state.constraints_done = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1, ()> for CaptureClient {
    fn event(
        state: &mut Self,
        _: &ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_image_copy_capture_frame_v1::Event::Damage {
                x,
                y,
                width,
                height,
            } => state.damage.push((x, y, width, height)),
            ext_image_copy_capture_frame_v1::Event::PresentationTime {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
            } => {
                let seconds = (u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo);
                state.presented = Some(Duration::new(seconds, tv_nsec));
            }
            ext_image_copy_capture_frame_v1::Event::Ready => state.ready = true,
            ext_image_copy_capture_frame_v1::Event::Failed { reason } => {
                state.failed = reason.into_result().ok();
            }
            _ => {}
        }
    }
}

delegate_noop!(CaptureClient: ignore wl_buffer::WlBuffer);
delegate_noop!(CaptureClient: ignore wl_output::WlOutput);
delegate_noop!(CaptureClient: ignore wl_pointer::WlPointer);
delegate_noop!(CaptureClient: ignore wl_seat::WlSeat);
delegate_noop!(CaptureClient: ignore wl_shm::WlShm);
delegate_noop!(CaptureClient: ignore wl_shm_pool::WlShmPool);
delegate_noop!(CaptureClient: ignore ext_image_capture_source_v1::ExtImageCaptureSourceV1);
delegate_noop!(CaptureClient: ignore ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1);
delegate_noop!(CaptureClient: ignore ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1);

#[cfg(feature = "tty")]
#[derive(Debug, Eq, PartialEq)]
struct CapturedCursor {
    size: (u32, u32),
    formats: Vec<wl_shm::Format>,
    position: (i32, i32),
    hotspot: (i32, i32),
    first_pixel: [u8; 4],
}

#[cfg(feature = "tty")]
#[test]
fn pointer_cursor_capture_writes_separate_alpha_image_and_metadata() {
    let mut runtime = capture_runtime();
    runtime.state.cursor.install_test_capture_image(
        tensor_util::OutputScale::from_f64(1.25).unwrap(),
        tensor_util::Size::new(2, 2),
        tensor_util::Point::new(1, 1),
        std::sync::Arc::from([
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ]),
    );
    let socket_path = socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    runtime.state.input_seat.enable_pointer();
    runtime
        .state
        .input_seat
        .set_pointer_location((100.0, 80.0).into());
    runtime
        .state
        .protocol_globals
        .seat
        .set_pointer_enabled(true);
    let (capture_tx, capture_rx) = mpsc::sync_channel(1);
    let (leave_tx, leave_rx) = mpsc::sync_channel(1);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<CaptureClient>(&connection).unwrap();
        let handle = queue.handle();
        let output = globals
            .bind::<wl_output::WlOutput, _, _>(&handle, 1..=4, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let pointer = seat.get_pointer(&handle, ());
        let source_manager = globals
            .bind::<
                ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
                _,
                _,
            >(&handle, 1..=1, ())
            .unwrap();
        let copy_manager = globals
            .bind::<ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
            .unwrap();
        let source = source_manager.create_source(&output, &handle, ());
        let cursor = copy_manager.create_pointer_cursor_session(&source, &pointer, &handle, ());
        let session = cursor.get_capture_session(&handle, ());
        let mut state = CaptureClient::default();
        while !state.constraints_done || !state.cursor_entered {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        let fd = memfd_create("tensor-cursor-capture-wire-test", MemfdFlags::CLOEXEC).unwrap();
        ftruncate(&fd, 16).unwrap();
        let pool = shm.create_pool(fd.as_fd(), 16, &handle, ());
        let buffer = pool.create_buffer(0, 2, 2, 8, wl_shm::Format::Argb8888, &handle, ());
        pool.destroy();
        let frame = session.create_frame(&handle, ());
        frame.attach_buffer(&buffer);
        frame.damage_buffer(0, 0, 2, 2);
        frame.capture();
        while !state.ready && state.failed.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        assert_eq!(state.failed, None);
        let mut first_pixel = [0_u8; 4];
        assert_eq!(pread(&fd, &mut first_pixel, 0).unwrap(), 4);
        capture_tx
            .send(CapturedCursor {
                size: state.size.unwrap(),
                formats: state.shm_formats.clone(),
                position: state.cursor_position.unwrap(),
                hotspot: state.cursor_hotspot.unwrap(),
                first_pixel,
            })
            .unwrap();
        while !state.cursor_left {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        leave_tx.send(()).unwrap();
    });

    let captured = dispatch_until_capture_result(&mut runtime, &capture_rx);
    assert_eq!(
        captured,
        CapturedCursor {
            size: (2, 2),
            formats: vec![wl_shm::Format::Argb8888],
            position: (125, 100),
            hotspot: (1, 1),
            first_pixel: [0, 0, 255, 255],
        }
    );
    runtime
        .state
        .input_seat
        .set_pointer_location((2_000.0, 2_000.0).into());
    runtime.state.refresh_cursor_surface_outputs();
    runtime.state.flush_wayland_clients();
    dispatch_until_capture_result(&mut runtime, &leave_rx);
    client.join().unwrap();
}

#[derive(Debug, Eq, PartialEq)]
struct CapturedFrame {
    size: (u32, u32),
    formats: Vec<wl_shm::Format>,
    damage: Vec<(i32, i32, i32, i32)>,
    presented: Duration,
    nonzero_pixel: bool,
}

#[test]
fn output_capture_writes_tensor_shm_and_reports_complete_metadata() {
    let mut runtime = capture_runtime();
    let socket_path = socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(1);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<CaptureClient>(&connection).unwrap();
        let handle = queue.handle();
        let output = globals
            .bind::<wl_output::WlOutput, _, _>(&handle, 1..=4, ())
            .unwrap();
        let source_manager = globals
            .bind::<
                ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
                _,
                _,
            >(&handle, 1..=1, ())
            .unwrap();
        let copy_manager = globals
            .bind::<ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let shm = globals
            .bind::<wl_shm::WlShm, _, _>(&handle, 1..=2, ())
            .unwrap();
        let source = source_manager.create_source(&output, &handle, ());
        let session = copy_manager.create_session(
            &source,
            ext_image_copy_capture_manager_v1::Options::empty(),
            &handle,
            (),
        );
        let mut state = CaptureClient::default();
        while !state.constraints_done {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        let fd = memfd_create("tensor-capture-wire-test", MemfdFlags::CLOEXEC).unwrap();
        ftruncate(&fd, BUFFER_LEN as u64).unwrap();
        let pool = shm.create_pool(fd.as_fd(), BUFFER_LEN as i32, &handle, ());
        let buffer = pool.create_buffer(
            0,
            WIDTH as i32,
            HEIGHT as i32,
            STRIDE,
            wl_shm::Format::Xrgb8888,
            &handle,
            (),
        );
        pool.destroy();
        let frame = session.create_frame(&handle, ());
        frame.attach_buffer(&buffer);
        frame.damage_buffer(0, 0, WIDTH as i32, HEIGHT as i32);
        frame.capture();
        while !state.ready && state.failed.is_none() {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        assert_eq!(state.failed, None);

        let mut first_pixel = [0_u8; 4];
        assert_eq!(pread(&fd, &mut first_pixel, 0).unwrap(), first_pixel.len());
        result_tx
            .send(CapturedFrame {
                size: state.size.unwrap(),
                formats: state.shm_formats,
                damage: state.damage,
                presented: state.presented.unwrap(),
                nonzero_pixel: first_pixel != [0; 4],
            })
            .unwrap();
    });

    let captured = dispatch_until_capture_result(&mut runtime, &result_rx);
    assert_eq!(captured.size, (WIDTH, HEIGHT));
    assert_eq!(
        captured.formats,
        [wl_shm::Format::Xrgb8888, wl_shm::Format::Argb8888]
    );
    assert_eq!(captured.damage, [(0, 0, WIDTH as i32, HEIGHT as i32)]);
    assert!(captured.presented > Duration::ZERO);
    assert!(captured.nonzero_pixel);
    client.join().unwrap();
}

#[derive(Clone, Copy, Debug)]
enum CaptureViolation {
    DuplicateFrame,
    NoBuffer,
}

#[test]
fn capture_reports_session_and_frame_wire_errors() {
    let cases = [
        (
            CaptureViolation::DuplicateFrame,
            "ext_image_copy_capture_session_v1",
            u32::from(ext_image_copy_capture_session_v1::Error::DuplicateFrame),
        ),
        (
            CaptureViolation::NoBuffer,
            "ext_image_copy_capture_frame_v1",
            u32::from(ext_image_copy_capture_frame_v1::Error::NoBuffer),
        ),
    ];
    for (violation, expected_interface, expected_code) in cases {
        let (interface, code) = capture_protocol_error(violation);
        assert_eq!(interface, expected_interface, "{violation:?}");
        assert_eq!(code, expected_code, "{violation:?}");
    }
}

fn capture_protocol_error(violation: CaptureViolation) -> (String, u32) {
    let mut runtime = capture_runtime();
    let socket_path = socket_path(&runtime);
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(1);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<CaptureClient>(&connection).unwrap();
        let handle = queue.handle();
        let output = globals
            .bind::<wl_output::WlOutput, _, _>(&handle, 1..=4, ())
            .unwrap();
        let source_manager = globals
            .bind::<
                ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
                _,
                _,
            >(&handle, 1..=1, ())
            .unwrap();
        let copy_manager = globals
            .bind::<ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1, _, _>(
                &handle,
                1..=1,
                (),
            )
            .unwrap();
        let source = source_manager.create_source(&output, &handle, ());
        let session = copy_manager.create_session(
            &source,
            ext_image_copy_capture_manager_v1::Options::empty(),
            &handle,
            (),
        );
        let mut state = CaptureClient::default();
        while !state.constraints_done {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let frame = session.create_frame(&handle, ());
        match violation {
            CaptureViolation::DuplicateFrame => {
                let _duplicate = session.create_frame(&handle, ());
            }
            CaptureViolation::NoBuffer => frame.capture(),
        }

        assert!(queue.roundtrip(&mut state).is_err());
        let error = connection
            .protocol_error()
            .expect("expected protocol error");
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
    });

    let result = dispatch_until_capture_result(&mut runtime, &result_rx);
    client.join().unwrap();
    result
}

fn capture_runtime() -> WaylandRuntime {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    runtime
}

fn socket_path(runtime: &WaylandRuntime) -> PathBuf {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    PathBuf::from(runtime_dir).join(runtime.socket_name())
}

fn dispatch_until_capture_result<T>(
    runtime: &mut WaylandRuntime,
    results: &mpsc::Receiver<T>,
) -> T {
    for _ in 0..400 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        runtime.state.on_loop_idle();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("Wayland capture client did not complete before the dispatch limit");
}

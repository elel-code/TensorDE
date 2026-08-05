use std::{
    fs::File,
    io::Read,
    os::{
        fd::{AsFd, OwnedFd},
        unix::net::UnixStream,
    },
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

use rustix::fs::{MemfdFlags, SealFlags, fcntl_get_seals, ftruncate, memfd_create};
use tensor_host::{DrmFormat, Fourcc, Modifier};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_registry},
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1, zwp_linux_dmabuf_feedback_v1, zwp_linux_dmabuf_v1,
};

use super::*;

const MAIN_DEVICE: u64 = 0x1234_5678;
const FORMAT_ENTRY_SIZE: usize = 16;

#[derive(Default)]
struct DmabufClient {
    feedback: FeedbackObservation,
    params_failed: bool,
}

#[derive(Default)]
struct FeedbackObservation {
    table: Option<(OwnedFd, u32)>,
    main_device_events: u8,
    target_device: Option<Vec<u8>>,
    flags: Option<u32>,
    indices: Option<Vec<u8>>,
    tranche_done: u8,
    done: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for DmabufClient {
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

impl Dispatch<zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1, ()> for DmabufClient {
    fn event(
        state: &mut Self,
        _: &zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
        event: zwp_linux_dmabuf_feedback_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_linux_dmabuf_feedback_v1::Event::Done => state.feedback.done = true,
            zwp_linux_dmabuf_feedback_v1::Event::FormatTable { fd, size } => {
                state.feedback.table = Some((fd, size));
            }
            zwp_linux_dmabuf_feedback_v1::Event::MainDevice { .. } => {
                state.feedback.main_device_events =
                    state.feedback.main_device_events.saturating_add(1);
            }
            zwp_linux_dmabuf_feedback_v1::Event::TrancheTargetDevice { device } => {
                state.feedback.target_device = Some(device);
            }
            zwp_linux_dmabuf_feedback_v1::Event::TrancheFlags { flags } => {
                state.feedback.flags = Some(flags.into());
            }
            zwp_linux_dmabuf_feedback_v1::Event::TrancheFormats { indices } => {
                state.feedback.indices = Some(indices);
            }
            zwp_linux_dmabuf_feedback_v1::Event::TrancheDone => {
                state.feedback.tranche_done = state.feedback.tranche_done.saturating_add(1);
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, ()> for DmabufClient {
    fn event(
        state: &mut Self,
        _: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, zwp_linux_buffer_params_v1::Event::Failed) {
            state.params_failed = true;
        }
    }

    wayland_client::event_created_child!(
        DmabufClient,
        zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        [zwp_linux_buffer_params_v1::EVT_CREATED_OPCODE => (wl_buffer::WlBuffer, ())]
    );
}

delegate_noop!(DmabufClient: ignore zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1);
delegate_noop!(DmabufClient: ignore wl_buffer::WlBuffer);

#[test]
fn dmabuf_v6_feedback_publishes_sealed_sampling_table() {
    let formats = [
        DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9)),
        DrmFormat::new(Fourcc::ARGB8888, Modifier::LINEAR),
    ];
    let (mut runtime, socket_path, _completions) = dmabuf_runtime(formats);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<DmabufClient>(&connection).unwrap();
        let handle = queue.handle();
        let dmabuf = globals
            .bind::<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1, _, _>(&handle, 6..=6, ())
            .unwrap();
        let _feedback = dmabuf.get_default_feedback(&handle, ());
        let mut state = DmabufClient::default();
        while !state.feedback.done {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        result_tx.send(state.feedback).unwrap();
    });

    let mut feedback = dispatch_until_result(&mut runtime, &result_rx);
    assert_eq!(feedback.main_device_events, 0);
    assert_eq!(
        feedback.target_device.take().unwrap(),
        MAIN_DEVICE.to_ne_bytes()
    );
    assert_eq!(
        feedback.flags,
        Some(u32::from(
            zwp_linux_dmabuf_feedback_v1::TrancheFlags::Sampling
        ))
    );
    assert_eq!(
        feedback.indices.take().unwrap(),
        [0_u16.to_ne_bytes(), 1_u16.to_ne_bytes()].concat()
    );
    assert_eq!(feedback.tranche_done, 1);

    let (table_fd, table_size) = feedback.table.take().unwrap();
    assert_eq!(table_size as usize, formats.len() * FORMAT_ENTRY_SIZE);
    let seals = fcntl_get_seals(&table_fd).unwrap();
    assert!(
        seals.contains(SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE | SealFlags::SEAL)
    );
    let mut table = vec![0; table_size as usize];
    File::from(table_fd).read_exact(&mut table).unwrap();
    assert_eq!(&table[0..4], &formats[0].code.raw().to_ne_bytes());
    assert_eq!(&table[8..16], &formats[0].modifier.raw().to_ne_bytes());
    assert_eq!(&table[16..20], &formats[1].code.raw().to_ne_bytes());
    assert_eq!(&table[24..32], &formats[1].modifier.raw().to_ne_bytes());
    client.join().unwrap();
}

#[test]
fn dmabuf_params_reject_duplicate_plane_on_the_wire() {
    assert_eq!(
        params_protocol_error(ParamsViolation::DuplicatePlane),
        u32::from(zwp_linux_buffer_params_v1::Error::PlaneSet)
    );
}

#[test]
fn dmabuf_params_reject_mismatched_v6_modifiers_on_the_wire() {
    assert_eq!(
        params_protocol_error(ParamsViolation::MismatchedModifiers),
        u32::from(zwp_linux_buffer_params_v1::Error::InvalidFormat)
    );
}

#[test]
fn dmabuf_params_reject_extra_plane_for_single_plane_contract() {
    assert_eq!(
        params_protocol_error(ParamsViolation::ExtraPlane),
        u32::from(zwp_linux_buffer_params_v1::Error::Incomplete)
    );
}

#[test]
fn dmabuf_params_reject_reuse_after_create_on_the_wire() {
    assert_eq!(
        params_protocol_error(ParamsViolation::AlreadyUsed),
        u32::from(zwp_linux_buffer_params_v1::Error::AlreadyUsed)
    );
}

#[test]
fn dmabuf_create_reports_nonfatal_import_failure_without_renderer() {
    let format = DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9));
    let (mut runtime, socket_path, _completions) = dmabuf_runtime([format]);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<DmabufClient>(&connection).unwrap();
        let handle = queue.handle();
        let dmabuf = globals
            .bind::<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1, _, _>(&handle, 6..=6, ())
            .unwrap();
        let params = dmabuf.create_params(&handle, ());
        let fd = plane_fd();
        params.add(fd.as_fd(), 0, 0, 64, 0, 9);
        params.create(
            16,
            16,
            Fourcc::XRGB8888.raw(),
            zwp_linux_buffer_params_v1::Flags::empty(),
        );
        let mut state = DmabufClient::default();
        while !state.params_failed {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        result_tx.send(connection.protocol_error()).unwrap();
    });

    assert!(dispatch_until_result(&mut runtime, &result_rx).is_none());
    client.join().unwrap();
}

#[test]
fn dmabuf_create_immed_rejects_import_failure_without_renderer() {
    assert_eq!(
        params_protocol_error(ParamsViolation::ImmediateImportFailure),
        u32::from(zwp_linux_buffer_params_v1::Error::InvalidWlBuffer)
    );
}

#[derive(Clone, Copy)]
enum ParamsViolation {
    DuplicatePlane,
    MismatchedModifiers,
    ExtraPlane,
    AlreadyUsed,
    ImmediateImportFailure,
}

fn params_protocol_error(violation: ParamsViolation) -> u32 {
    let format = DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9));
    let (mut runtime, socket_path, _completions) = dmabuf_runtime([format]);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<DmabufClient>(&connection).unwrap();
        let handle = queue.handle();
        let dmabuf = globals
            .bind::<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1, _, _>(&handle, 6..=6, ())
            .unwrap();
        let params = dmabuf.create_params(&handle, ());
        let first = plane_fd();
        params.add(first.as_fd(), 0, 0, 64, 0, 9);

        let second = plane_fd();
        match violation {
            ParamsViolation::DuplicatePlane => params.add(second.as_fd(), 0, 0, 64, 0, 9),
            ParamsViolation::MismatchedModifiers => params.add(second.as_fd(), 1, 0, 64, 0, 10),
            ParamsViolation::ExtraPlane => {
                params.add(second.as_fd(), 1, 0, 64, 0, 9);
                params.create(
                    16,
                    16,
                    Fourcc::XRGB8888.raw(),
                    zwp_linux_buffer_params_v1::Flags::empty(),
                );
            }
            ParamsViolation::AlreadyUsed => {
                params.create(
                    16,
                    16,
                    Fourcc::XRGB8888.raw(),
                    zwp_linux_buffer_params_v1::Flags::empty(),
                );
                params.create(
                    16,
                    16,
                    Fourcc::XRGB8888.raw(),
                    zwp_linux_buffer_params_v1::Flags::empty(),
                );
            }
            ParamsViolation::ImmediateImportFailure => {
                let _buffer = params.create_immed(
                    16,
                    16,
                    Fourcc::XRGB8888.raw(),
                    zwp_linux_buffer_params_v1::Flags::empty(),
                    &handle,
                    (),
                );
            }
        }
        assert!(queue.roundtrip(&mut DmabufClient::default()).is_err());
        let error = connection
            .protocol_error()
            .expect("expected protocol error");
        assert_eq!(
            error.object_interface,
            "zwp_linux_buffer_params_v1".to_owned()
        );
        result_tx.send(error.code).unwrap();
    });

    let code = dispatch_until_result(&mut runtime, &result_rx);
    client.join().unwrap();
    code
}

fn dmabuf_runtime(
    formats: impl IntoIterator<Item = DrmFormat>,
) -> (
    WaylandRuntime,
    PathBuf,
    tensor_runtime::EventfdCompletionRelay,
) {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let display = runtime.state.display_handle.clone();
    assert!(
        runtime
            .state
            .protocol_globals
            .install_dmabuf(&display, MAIN_DEVICE, formats)
            .unwrap()
    );
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let completions = runtime.prepare_for_test(false).unwrap();
    (runtime, socket_path, completions)
}

fn dispatch_until_result<T>(runtime: &mut WaylandRuntime, results: &mpsc::Receiver<T>) -> T {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("Wayland dma-buf client did not complete before the dispatch limit");
}

fn plane_fd() -> OwnedFd {
    let fd = memfd_create("tensor-dmabuf-wire-plane", MemfdFlags::CLOEXEC).unwrap();
    ftruncate(&fd, 4096).unwrap();
    fd
}

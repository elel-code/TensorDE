use std::{
    fs::File,
    io::{Read, Write},
    os::fd::AsFd,
};

#[cfg(feature = "tty")]
use wayland_client::protocol::{
    wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
    wl_surface,
};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop, globals::GlobalListContents,
};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
    ext_data_control_source_v1::{self, ExtDataControlSourceV1},
};
#[cfg(feature = "tty")]
use wayland_protocols::{
    wp::primary_selection::zv1::client::{
        zwp_primary_selection_device_manager_v1, zwp_primary_selection_device_v1,
        zwp_primary_selection_offer_v1, zwp_primary_selection_source_v1,
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
};

use super::*;

const MIME: &str = "text/plain;charset=utf-8";
const PAYLOAD: &[u8] = b"tensor-selection-fd-direct";

#[derive(Default)]
struct DataControlClient {
    initial_clipboard: usize,
    initial_primary: usize,
    clipboard: Option<ExtDataControlOfferV1>,
    offered_mime: bool,
    source_sent: bool,
    first_source_cancelled: bool,
    wlr_clipboard: usize,
    wlr_primary: usize,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for DataControlClient {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(DataControlClient: ignore wl_seat::WlSeat);
delegate_noop!(DataControlClient: ignore ExtDataControlManagerV1);
delegate_noop!(DataControlClient: ignore ZwlrDataControlManagerV1);

impl Dispatch<ExtDataControlDeviceV1, ()> for DataControlClient {
    fn event(
        state: &mut Self,
        _device: &ExtDataControlDeviceV1,
        event: ext_data_control_device_v1::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_device_v1::Event::Selection { id: None } => {
                state.initial_clipboard += 1;
            }
            ext_data_control_device_v1::Event::Selection { id: Some(offer) } => {
                state.clipboard = Some(offer);
            }
            ext_data_control_device_v1::Event::PrimarySelection { id: None } => {
                state.initial_primary += 1;
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(DataControlClient, ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ExtDataControlOfferV1, ()> for DataControlClient {
    fn event(
        state: &mut Self,
        _offer: &ExtDataControlOfferV1,
        event: ext_data_control_offer_v1::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
            state.offered_mime |= mime_type == MIME;
        }
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for DataControlClient {
    fn event(
        state: &mut Self,
        _device: &ZwlrDataControlDeviceV1,
        event: zwlr_data_control_device_v1::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::Selection { id: None } => {
                state.wlr_clipboard += 1;
            }
            zwlr_data_control_device_v1::Event::PrimarySelection { id: None } => {
                state.wlr_primary += 1;
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(DataControlClient, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for DataControlClient {
    fn event(
        _state: &mut Self,
        _offer: &ZwlrDataControlOfferV1,
        _event: <ZwlrDataControlOfferV1 as Proxy>::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlSourceV1, bool> for DataControlClient {
    fn event(
        state: &mut Self,
        _source: &ExtDataControlSourceV1,
        event: ext_data_control_source_v1::Event,
        first: &bool,
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_source_v1::Event::Send { mime_type, fd } => {
                assert_eq!(mime_type, MIME);
                File::from(fd).write_all(PAYLOAD).unwrap();
                state.source_sent = true;
            }
            ext_data_control_source_v1::Event::Cancelled if *first => {
                state.first_source_cancelled = true;
            }
            _ => {}
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct TransferResult {
    initial_clipboard: usize,
    initial_primary: usize,
    offered_mime: bool,
    payload: Vec<u8>,
    first_source_cancelled: bool,
}

#[cfg(feature = "tty")]
#[derive(Default)]
struct FocusedSelectionClient {
    configured: bool,
    core_null: usize,
    core_offer: Option<wl_data_offer::WlDataOffer>,
    core_mime: bool,
    primary_null: usize,
    primary_offer: Option<zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1>,
    primary_mime: bool,
}

#[cfg(feature = "tty")]
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for FocusedSelectionClient {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(feature = "tty")]
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for FocusedSelectionClient {
    fn event(
        _state: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

#[cfg(feature = "tty")]
impl Dispatch<xdg_surface::XdgSurface, ()> for FocusedSelectionClient {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.configured = true;
        }
    }
}

#[cfg(feature = "tty")]
impl Dispatch<wl_data_device::WlDataDevice, ()> for FocusedSelectionClient {
    fn event(
        state: &mut Self,
        _device: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::Selection { id: None } => state.core_null += 1,
            wl_data_device::Event::Selection { id: Some(offer) } => {
                state.core_offer = Some(offer);
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(FocusedSelectionClient, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ()),
    ]);
}

#[cfg(feature = "tty")]
impl Dispatch<wl_data_offer::WlDataOffer, ()> for FocusedSelectionClient {
    fn event(
        state: &mut Self,
        _offer: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            state.core_mime |= mime_type == MIME;
        }
    }
}

#[cfg(feature = "tty")]
impl Dispatch<zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1, ()>
    for FocusedSelectionClient
{
    fn event(
        state: &mut Self,
        _device: &zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
        event: zwp_primary_selection_device_v1::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        match event {
            zwp_primary_selection_device_v1::Event::Selection { id: None } => {
                state.primary_null += 1;
            }
            zwp_primary_selection_device_v1::Event::Selection { id: Some(offer) } => {
                state.primary_offer = Some(offer);
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(FocusedSelectionClient, zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1, [
        zwp_primary_selection_device_v1::EVT_DATA_OFFER_OPCODE => (zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1, ()),
    ]);
}

#[cfg(feature = "tty")]
impl Dispatch<zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1, ()>
    for FocusedSelectionClient
{
    fn event(
        state: &mut Self,
        _offer: &zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1,
        event: zwp_primary_selection_offer_v1::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        if let zwp_primary_selection_offer_v1::Event::Offer { mime_type } = event {
            state.primary_mime |= mime_type == MIME;
        }
    }
}

#[cfg(feature = "tty")]
delegate_noop!(FocusedSelectionClient: ignore wl_compositor::WlCompositor);
#[cfg(feature = "tty")]
delegate_noop!(FocusedSelectionClient: ignore wl_surface::WlSurface);
#[cfg(feature = "tty")]
delegate_noop!(FocusedSelectionClient: ignore wl_seat::WlSeat);
#[cfg(feature = "tty")]
delegate_noop!(FocusedSelectionClient: ignore wl_data_device_manager::WlDataDeviceManager);
#[cfg(feature = "tty")]
delegate_noop!(FocusedSelectionClient: ignore wl_data_source::WlDataSource);
#[cfg(feature = "tty")]
delegate_noop!(FocusedSelectionClient: ignore zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1);
#[cfg(feature = "tty")]
delegate_noop!(FocusedSelectionClient: ignore zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1);
#[cfg(feature = "tty")]
delegate_noop!(FocusedSelectionClient: ignore xdg_toplevel::XdgToplevel);

#[test]
fn ext_data_control_transfers_fd_and_cancels_replaced_source() {
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
        let (globals, mut queue) = registry_queue_init::<DataControlClient>(&connection).unwrap();
        let handle = queue.handle();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<ExtDataControlManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let first = manager.create_data_source(&handle, true);
        first.offer(MIME.to_owned());
        let device = manager.get_data_device(&seat, &handle, ());
        let mut state = DataControlClient::default();

        while state.initial_clipboard == 0 || state.initial_primary == 0 {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        device.set_selection(Some(&first));
        while state.clipboard.is_none() || !state.offered_mime {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        let (read, write) = rustix::pipe::pipe().unwrap();
        state
            .clipboard
            .as_ref()
            .unwrap()
            .receive(MIME.to_owned(), write.as_fd());
        drop(write);
        while !state.source_sent {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        let mut payload = Vec::new();
        File::from(read).read_to_end(&mut payload).unwrap();

        let replacement = manager.create_data_source(&handle, false);
        replacement.offer(MIME.to_owned());
        device.set_selection(Some(&replacement));
        while !state.first_source_cancelled {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        result_tx
            .send(TransferResult {
                initial_clipboard: state.initial_clipboard,
                initial_primary: state.initial_primary,
                offered_mime: state.offered_mime,
                payload,
                first_source_cancelled: state.first_source_cancelled,
            })
            .unwrap();
    });

    let result = dispatch_selection_result(&mut runtime, &result_rx);
    assert_eq!(
        result,
        TransferResult {
            initial_clipboard: 1,
            initial_primary: 1,
            offered_mime: true,
            payload: PAYLOAD.to_vec(),
            first_source_cancelled: true,
        }
    );
    client.join().unwrap();
}

#[cfg(feature = "tty")]
#[test]
fn focused_core_and_primary_devices_share_tensor_authority() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    install_test_output(&mut runtime);
    runtime.state.input_devices.insert(
        tensor_event::DeviceId::new(1),
        crate::protocol::state::InputDeviceCapabilities {
            keyboard: true,
            ..Default::default()
        },
    );
    runtime.state.reconcile_seat_capabilities();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (result_tx, result_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) =
            registry_queue_init::<FocusedSelectionClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let wm_base = globals
            .bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=7, ())
            .unwrap();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let data_manager = globals
            .bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(&handle, 1..=3, ())
            .unwrap();
        let primary_manager = globals
            .bind::<
                zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1,
                _,
                _,
            >(&handle, 1..=1, ())
            .unwrap();
        let core_source = data_manager.create_data_source(&handle, ());
        core_source.offer(MIME.to_owned());
        let core_device = data_manager.get_data_device(&seat, &handle, ());
        let primary_source = primary_manager.create_source(&handle, ());
        primary_source.offer(MIME.to_owned());
        let primary_device = primary_manager.get_device(&seat, &handle, ());
        let mut state = FocusedSelectionClient::default();

        core_device.set_selection(Some(&core_source), 1);
        primary_device.set_selection(Some(&primary_source), 1);
        queue.roundtrip(&mut state).unwrap();
        let rejected_before_focus = state.core_offer.is_none() && state.primary_offer.is_none();

        let surface = compositor.create_surface(&handle, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
        let _toplevel = xdg_surface.get_toplevel(&handle, ());
        surface.commit();
        while !state.configured {
            queue.blocking_dispatch(&mut state).unwrap();
        }

        core_device.set_selection(Some(&core_source), 2);
        primary_device.set_selection(Some(&primary_source), 2);
        while state.core_offer.is_none()
            || state.primary_offer.is_none()
            || !state.core_mime
            || !state.primary_mime
        {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        result_tx
            .send((
                rejected_before_focus,
                state.core_null,
                state.primary_null,
                state.core_mime,
                state.primary_mime,
            ))
            .unwrap();
    });

    let result = dispatch_focus_result(&mut runtime, &result_rx);
    assert!(result.0);
    assert_eq!(result.1, 1);
    assert_eq!(result.2, 1);
    assert!(result.3);
    assert!(result.4);
    client.join().unwrap();
}

#[derive(Clone, Copy)]
enum DataControlViolation {
    ReuseSource,
    LateOffer,
}

#[test]
fn ext_data_control_rejects_source_reuse() {
    assert_ext_data_control_violation(DataControlViolation::ReuseSource);
}

#[test]
fn ext_data_control_rejects_late_mime_offer() {
    assert_ext_data_control_violation(DataControlViolation::LateOffer);
}

#[test]
fn wlr_v1_device_never_receives_primary_selection_events() {
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
        let (globals, mut queue) = registry_queue_init::<DataControlClient>(&connection).unwrap();
        let handle = queue.handle();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<ZwlrDataControlManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let _device = manager.get_data_device(&seat, &handle, ());
        let mut state = DataControlClient::default();
        queue.roundtrip(&mut state).unwrap();
        result_tx
            .send((state.wlr_clipboard, state.wlr_primary))
            .unwrap();
    });

    assert_eq!(dispatch_pair(&mut runtime, &result_rx), (1, 0));
    client.join().unwrap();
}

fn assert_ext_data_control_violation(violation: DataControlViolation) {
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
        let (globals, mut queue) = registry_queue_init::<DataControlClient>(&connection).unwrap();
        let handle = queue.handle();
        let seat = globals
            .bind::<wl_seat::WlSeat, _, _>(&handle, 1..=9, ())
            .unwrap();
        let manager = globals
            .bind::<ExtDataControlManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let source = manager.create_data_source(&handle, true);
        source.offer(MIME.to_owned());
        let device = manager.get_data_device(&seat, &handle, ());
        let mut state = DataControlClient::default();
        while state.initial_clipboard == 0 || state.initial_primary == 0 {
            queue.blocking_dispatch(&mut state).unwrap();
        }
        device.set_selection(Some(&source));
        queue.roundtrip(&mut state).unwrap();
        match violation {
            DataControlViolation::ReuseSource => device.set_primary_selection(Some(&source)),
            DataControlViolation::LateOffer => source.offer("text/html".to_owned()),
        }
        result_tx
            .send(queue.roundtrip(&mut state).is_err())
            .unwrap();
    });

    assert!(dispatch_bool(&mut runtime, &result_rx));
    client.join().unwrap();
}

fn dispatch_selection_result(
    runtime: &mut WaylandRuntime,
    result: &mpsc::Receiver<TransferResult>,
) -> TransferResult {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = result.try_recv() {
            return result;
        }
    }
    panic!("selection client did not complete before the dispatch limit");
}

fn dispatch_bool(runtime: &mut WaylandRuntime, result: &mpsc::Receiver<bool>) -> bool {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = result.try_recv() {
            return result;
        }
    }
    panic!("selection client did not complete before the dispatch limit");
}

fn dispatch_pair(
    runtime: &mut WaylandRuntime,
    result: &mpsc::Receiver<(usize, usize)>,
) -> (usize, usize) {
    for _ in 0..300 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = result.try_recv() {
            return result;
        }
    }
    panic!("selection client did not complete before the dispatch limit");
}

#[cfg(feature = "tty")]
fn dispatch_focus_result(
    runtime: &mut WaylandRuntime,
    result: &mpsc::Receiver<(bool, usize, usize, bool, bool)>,
) -> (bool, usize, usize, bool, bool) {
    for _ in 0..400 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = result.try_recv() {
            return result;
        }
    }
    panic!("focused selection client did not complete before the dispatch limit");
}

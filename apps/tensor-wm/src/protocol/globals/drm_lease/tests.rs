use std::{os::unix::net::UnixStream, sync::mpsc, time::Duration};

use tensor_drm::LeaseConnector;
use tensor_host::ConnectorId;
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol::wl_registry,
};
use wayland_protocols::wp::drm_lease::v1::client::{
    wp_drm_lease_connector_v1, wp_drm_lease_device_v1, wp_drm_lease_request_v1, wp_drm_lease_v1,
};
use wayland_server::Display;

use super::*;
use crate::{
    layout::{LayoutEngine, LayoutKind},
    scene::SceneAppearance,
};

#[derive(Default)]
struct LeaseClient {
    connectors: Vec<wp_drm_lease_connector_v1::WpDrmLeaseConnectorV1>,
    connector_ids: Vec<u32>,
    device_done: usize,
    device_released: usize,
    lease_finished: usize,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for LeaseClient {
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

impl Dispatch<wp_drm_lease_device_v1::WpDrmLeaseDeviceV1, ()> for LeaseClient {
    fn event(
        state: &mut Self,
        _: &wp_drm_lease_device_v1::WpDrmLeaseDeviceV1,
        event: wp_drm_lease_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wp_drm_lease_device_v1::Event::Connector { id } => state.connectors.push(id),
            wp_drm_lease_device_v1::Event::Done => state.device_done += 1,
            wp_drm_lease_device_v1::Event::Released => state.device_released += 1,
            wp_drm_lease_device_v1::Event::DrmFd { .. } => {}
            _ => unreachable!(),
        }
    }

    wayland_client::event_created_child!(LeaseClient, wp_drm_lease_device_v1::WpDrmLeaseDeviceV1, [
        wp_drm_lease_device_v1::EVT_CONNECTOR_OPCODE => (wp_drm_lease_connector_v1::WpDrmLeaseConnectorV1, ())
    ]);
}

impl Dispatch<wp_drm_lease_connector_v1::WpDrmLeaseConnectorV1, ()> for LeaseClient {
    fn event(
        state: &mut Self,
        _: &wp_drm_lease_connector_v1::WpDrmLeaseConnectorV1,
        event: wp_drm_lease_connector_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_drm_lease_connector_v1::Event::ConnectorId { connector_id } = event {
            state.connector_ids.push(connector_id);
        }
    }
}

impl Dispatch<wp_drm_lease_v1::WpDrmLeaseV1, ()> for LeaseClient {
    fn event(
        state: &mut Self,
        _: &wp_drm_lease_v1::WpDrmLeaseV1,
        event: wp_drm_lease_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wp_drm_lease_v1::Event::Finished) {
            state.lease_finished += 1;
        }
    }
}

wayland_client::delegate_noop!(LeaseClient: ignore wp_drm_lease_request_v1::WpDrmLeaseRequestV1);

fn connector(device_id: u64, connector_id: u32) -> LeaseConnector {
    LeaseConnector {
        id: ConnectorId::new(device_id, connector_id),
        name: format!("DP-{connector_id}"),
        description: format!("test lease connector {connector_id}"),
        crtc_id: connector_id + 100,
        primary_plane_id: connector_id + 200,
    }
}

fn state_with_device(device_id: u64) -> (RuntimeState, UnixStream) {
    let display = Display::<RuntimeState>::new().unwrap();
    let mut display_handle = display.handle();
    let mut state = RuntimeState::with_appearance(
        display,
        LayoutEngine::new(LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    );
    state.protocol_globals.drm_lease.install_for_test(
        &display_handle,
        device_id,
        vec![connector(device_id, 7)],
    );
    let (server, socket) = UnixStream::pair().unwrap();
    display_handle
        .insert_client(server, std::sync::Arc::new(WaylandClientState::default()))
        .unwrap();
    (state, socket)
}

#[test]
fn bind_publishes_connector_metadata_and_release_acknowledgement() {
    let (mut state, socket) = state_with_device(1);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client = std::thread::spawn(move || {
        let connection = Connection::from_socket(socket).unwrap();
        let (globals, mut queue) = registry_queue_init::<LeaseClient>(&connection).unwrap();
        let handle = queue.handle();
        let device = globals
            .bind::<wp_drm_lease_device_v1::WpDrmLeaseDeviceV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let mut client_state = LeaseClient::default();
        queue.roundtrip(&mut client_state).unwrap();
        device.release();
        queue.roundtrip(&mut client_state).unwrap();
        result_tx
            .send((
                client_state.connector_ids,
                client_state.device_done,
                client_state.device_released,
            ))
            .unwrap();
    });

    assert_eq!(dispatch_until(&mut state, &result_rx), (vec![7], 1, 1));
    client.join().unwrap();
}

#[test]
fn empty_and_duplicate_requests_are_wire_errors() {
    for (duplicate, expected_code) in [(false, 2), (true, 1)] {
        let (mut state, socket) = state_with_device(1);
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let client = std::thread::spawn(move || {
            let connection = Connection::from_socket(socket).unwrap();
            let (globals, mut queue) = registry_queue_init::<LeaseClient>(&connection).unwrap();
            let handle = queue.handle();
            let device = globals
                .bind::<wp_drm_lease_device_v1::WpDrmLeaseDeviceV1, _, _>(&handle, 1..=1, ())
                .unwrap();
            let mut client_state = LeaseClient::default();
            queue.roundtrip(&mut client_state).unwrap();
            let request = device.create_lease_request(&handle, ());
            if duplicate {
                let connector = &client_state.connectors[0];
                request.request_connector(connector);
                request.request_connector(connector);
            } else {
                let _lease = request.submit(&handle, ());
            }
            assert!(queue.roundtrip(&mut client_state).is_err());
            let error = connection.protocol_error().unwrap();
            result_tx
                .send((error.object_interface, error.code))
                .unwrap();
        });

        assert_eq!(
            dispatch_until(&mut state, &result_rx),
            ("wp_drm_lease_request_v1".to_owned(), expected_code)
        );
        client.join().unwrap();
    }
}

#[test]
fn connector_from_replaced_device_is_a_wrong_device_error() {
    let (mut state, socket) = state_with_device(1);
    let (ready_tx, ready_rx) = mpsc::sync_channel(0);
    let (continue_tx, continue_rx) = mpsc::sync_channel(0);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let client = std::thread::spawn(move || {
        let connection = Connection::from_socket(socket).unwrap();
        let (globals, mut queue) = registry_queue_init::<LeaseClient>(&connection).unwrap();
        let handle = queue.handle();
        let first = globals
            .bind::<wp_drm_lease_device_v1::WpDrmLeaseDeviceV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let mut client_state = LeaseClient::default();
        queue.roundtrip(&mut client_state).unwrap();
        let stale_connector = client_state.connectors[0].clone();
        ready_tx.send(()).unwrap();
        continue_rx.recv().unwrap();
        queue.roundtrip(&mut client_state).unwrap();
        let second = globals
            .bind::<wp_drm_lease_device_v1::WpDrmLeaseDeviceV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let request = second.create_lease_request(&handle, ());
        request.request_connector(&stale_connector);
        assert!(queue.roundtrip(&mut client_state).is_err());
        let error = connection.protocol_error().unwrap();
        result_tx
            .send((error.object_interface, error.code))
            .unwrap();
        drop(first);
    });

    dispatch_until(&mut state, &ready_rx);
    let display = state.display_handle.clone();
    state
        .protocol_globals
        .drm_lease
        .install_for_test(&display, 2, vec![connector(2, 9)]);
    state.flush_wayland_clients();
    continue_tx.send(()).unwrap();
    assert_eq!(
        dispatch_until(&mut state, &result_rx),
        ("wp_drm_lease_request_v1".to_owned(), 0)
    );
    client.join().unwrap();
}

fn dispatch_until<T>(state: &mut RuntimeState, receiver: &mpsc::Receiver<T>) -> T {
    for _ in 0..300 {
        state.dispatch_wayland_clients().unwrap();
        state.flush_wayland_clients();
        if let Ok(value) = receiver.try_recv() {
            return value;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("DRM lease client did not complete before the dispatch limit");
}

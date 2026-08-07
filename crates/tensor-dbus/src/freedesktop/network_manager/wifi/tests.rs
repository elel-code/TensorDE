use super::*;
use crate::{MessageKind, wire};
use zvariant::Array;

const DEVICE_PATH: &str = "/org/freedesktop/NetworkManager/Devices/1";
const AP_PATH: &str = "/org/freedesktop/NetworkManager/AccessPoint/1";

fn value<T: Into<OwnedValue>>(value: T) -> OwnedValue {
    value.into()
}

fn text(value: &str) -> OwnedValue {
    zvariant::Str::from(value).into()
}

fn bytes(value: &[u8]) -> OwnedValue {
    OwnedValue::try_from(Array::from(value.to_vec())).unwrap()
}

fn object_path(value: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(value).unwrap()
}

fn path_value(value: &str) -> OwnedValue {
    OwnedValue::try_from(zvariant::Value::from(object_path(value))).unwrap()
}

fn properties(entries: impl IntoIterator<Item = (&'static str, OwnedValue)>) -> Properties {
    entries
        .into_iter()
        .map(|(property, value)| (property.to_owned(), value))
        .collect()
}

fn access_point_properties(ssid: &[u8]) -> Properties {
    properties([
        ("Ssid", bytes(ssid)),
        ("HwAddress", text("02:00:00:00:00:01")),
        ("Strength", value(83_u8)),
        ("Frequency", value(5_180_u32)),
        ("MaxBitrate", value(866_700_u32)),
        ("Mode", value(2_u32)),
        ("Flags", value(0_u32)),
        ("WpaFlags", value(0_u32)),
        ("RsnFlags", value(AP_SEC_KEY_MGMT_802_1X)),
    ])
}

fn access_point() -> WifiAccessPointSnapshot {
    read::decode_access_point(
        object_path(AP_PATH),
        object_path(DEVICE_PATH),
        &access_point_properties(b"Tensor"),
    )
    .unwrap()
}

fn snapshot() -> NetworkManagerDetailsSnapshot {
    NetworkManagerDetailsSnapshot {
        root: NetworkManagerSnapshot::from_parts(
            true,
            true,
            true,
            super::super::NetworkState::ConnectedGlobal,
            super::super::Connectivity::Full,
            super::super::PrimaryConnectionKind::Wifi,
        ),
        wifi: WifiInventorySnapshot {
            devices: vec![WifiDeviceSnapshot {
                path: object_path(DEVICE_PATH),
                interface_name: "wlan0".to_owned(),
                state: DeviceState::Activated,
                hardware_address: "02:00:00:00:00:02".to_owned(),
                active_access_point: Some(object_path(AP_PATH)),
            }],
            access_points: vec![access_point()],
        },
    }
}

fn properties_changed(
    path: &str,
    interface: &str,
    changed: Properties,
    invalidated: Vec<String>,
) -> Message {
    let encoded = wire::encode_outgoing(
        wire::Outgoing {
            kind: MessageKind::Signal,
            flags: 0,
            serial: 11,
            reply_serial: None,
            path: Some(path),
            interface: Some(PROPERTIES_INTERFACE),
            member: Some("PropertiesChanged"),
            error_name: None,
            destination: None,
        },
        &(interface, changed, invalidated),
    )
    .unwrap();
    wire::decode_message(encoded.bytes, Vec::new()).unwrap()
}

fn signal(path: &str, interface: &str, member: &str) -> Message {
    let encoded = wire::encode_outgoing(
        wire::Outgoing {
            kind: MessageKind::Signal,
            flags: 0,
            serial: 12,
            reply_serial: None,
            path: Some(path),
            interface: Some(interface),
            member: Some(member),
            error_name: None,
            destination: None,
        },
        &(),
    )
    .unwrap();
    wire::decode_message(encoded.bytes, Vec::new()).unwrap()
}

#[test]
fn access_point_snapshot_preserves_raw_ssid_and_security_semantics() {
    let point = read::decode_access_point(
        object_path(AP_PATH),
        object_path(DEVICE_PATH),
        &access_point_properties(&[b'T', 0xff, b'N']),
    )
    .unwrap();
    assert_eq!(point.ssid(), &[b'T', 0xff, b'N']);
    assert_eq!(point.ssid_display(), "T\u{fffd}N");
    assert_eq!(point.signal_strength(), 83);
    assert_eq!(point.frequency_mhz(), 5_180);
    assert_eq!(point.max_bitrate_kbps(), 866_700);
    assert_eq!(point.mode(), WifiMode::Infrastructure);
    assert!(point.secured());
    assert!(point.enterprise());
}

#[test]
fn oversized_ssids_and_inventories_are_rejected() {
    assert!(matches!(
        read::decode_access_point(
            object_path(AP_PATH),
            object_path(DEVICE_PATH),
            &access_point_properties(&[b'x'; MAX_SSID_BYTES + 1]),
        ),
        Err(NetworkManagerDetailsError::LimitExceeded {
            collection: "SSID bytes",
            count: 33,
            maximum: MAX_SSID_BYTES,
        })
    ));
    assert!(matches!(
        enforce_limit("access points", MAX_ACCESS_POINTS + 1, MAX_ACCESS_POINTS),
        Err(NetworkManagerDetailsError::LimitExceeded { .. })
    ));
}

#[test]
fn hot_access_point_updates_are_atomic() {
    let mut snapshot = snapshot();
    let changed = properties_changed(
        AP_PATH,
        ACCESS_POINT_INTERFACE,
        properties([("Strength", value(47_u8))]),
        Vec::new(),
    );
    assert_eq!(
        observe_snapshot(&mut snapshot, &changed).unwrap(),
        NetworkManagerDetailsEvent::Changed
    );
    assert_eq!(
        snapshot
            .wifi()
            .active_access_point()
            .unwrap()
            .signal_strength(),
        47
    );

    let before = snapshot.clone();
    let invalid = properties_changed(
        AP_PATH,
        ACCESS_POINT_INTERFACE,
        properties([
            ("HwAddress", text("02:00:00:00:00:03")),
            ("Strength", text("invalid")),
        ]),
        Vec::new(),
    );
    assert!(observe_snapshot(&mut snapshot, &invalid).is_err());
    assert_eq!(snapshot, before);
}

#[test]
fn active_access_point_changes_are_applied_without_rebuilding_inventory() {
    let mut snapshot = snapshot();
    let disconnected = properties_changed(
        DEVICE_PATH,
        WIRELESS_INTERFACE,
        properties([("ActiveAccessPoint", path_value("/"))]),
        Vec::new(),
    );
    assert_eq!(
        observe_snapshot(&mut snapshot, &disconnected).unwrap(),
        NetworkManagerDetailsEvent::Changed
    );
    assert!(snapshot.wifi().active_access_point().is_none());
    assert_eq!(snapshot.wifi().access_points().len(), 1);
}

#[test]
fn topology_and_scan_completion_require_complete_refreshes() {
    let mut snapshot = snapshot();
    let ethernet = properties_changed(
        "/org/freedesktop/NetworkManager/Devices/2",
        DEVICE_INTERFACE,
        properties([("State", value(100_u32))]),
        Vec::new(),
    );
    assert_eq!(
        observe_snapshot(&mut snapshot, &ethernet).unwrap(),
        NetworkManagerDetailsEvent::Ignored
    );
    assert_eq!(
        observe_snapshot(
            &mut snapshot,
            &signal(DEVICE_PATH, WIRELESS_INTERFACE, "AccessPointAdded"),
        )
        .unwrap(),
        NetworkManagerDetailsEvent::RefreshRequired
    );
    let scan = properties_changed(
        DEVICE_PATH,
        WIRELESS_INTERFACE,
        properties([("LastScan", value(42_i64))]),
        Vec::new(),
    );
    assert_eq!(
        observe_snapshot(&mut snapshot, &scan).unwrap(),
        NetworkManagerDetailsEvent::RefreshRequired
    );
}

#[test]
fn details_rule_tracks_one_owner_across_the_network_manager_namespace() {
    let rule = signal_rule().unwrap();
    assert!(
        rule.bus_expression()
            .contains("sender='org.freedesktop.NetworkManager'")
    );
    assert!(
        rule.bus_expression()
            .contains("path_namespace='/org/freedesktop/NetworkManager'")
    );
    assert!(!rule.sender_available());
}

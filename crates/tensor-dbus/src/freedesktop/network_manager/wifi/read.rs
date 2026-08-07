use std::collections::HashSet;

use zvariant::OwnedObjectPath;

use super::{
    ACCESS_POINT_INTERFACE, DEVICE_INTERFACE, MAX_ACCESS_POINTS, MAX_DEVICES, MAX_SSID_BYTES,
    MAX_WIFI_DEVICES, NetworkManagerDetailsError, NetworkManagerDetailsSnapshot,
    PROPERTIES_INTERFACE, Properties, ROOT_INTERFACE, ROOT_PATH, WIFI_DEVICE_TYPE,
    WIRELESS_INTERFACE, WifiAccessPointSnapshot, WifiDeviceSnapshot, WifiInventorySnapshot,
    WifiMode, enforce_limit, required_bytes, required_optional_path, required_string, required_u8,
    required_u32,
};
use crate::{Connection, PendingReply};

use super::super::decode_snapshot;
use super::DESTINATION;

pub async fn read_details(
    connection: &mut Connection,
) -> Result<NetworkManagerDetailsSnapshot, NetworkManagerDetailsError> {
    let root = connection
        .send_call::<_, Properties>(
            Some(DESTINATION),
            ROOT_PATH,
            Some(PROPERTIES_INTERFACE),
            "GetAll",
            &(ROOT_INTERFACE,),
        )
        .await?;
    let devices = match connection
        .send_call::<_, Vec<OwnedObjectPath>>(
            Some(DESTINATION),
            ROOT_PATH,
            Some(ROOT_INTERFACE),
            "GetDevices",
            &(),
        )
        .await
    {
        Ok(devices) => devices,
        Err(error) => {
            let _ = root.abandon(connection);
            return Err(error.into());
        }
    };
    let root_properties = match root.wait(connection).await {
        Ok(properties) => properties,
        Err(error) => {
            let _ = devices.abandon(connection);
            return Err(error.into());
        }
    };
    let mut device_paths = devices.wait(connection).await?;
    enforce_limit("devices", device_paths.len(), MAX_DEVICES)?;
    device_paths.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    device_paths.dedup();

    let root = decode_snapshot(&root_properties)?;
    let pending_devices = send_device_properties(connection, device_paths).await?;
    let devices = wait_wifi_devices(connection, pending_devices).await?;
    let pending_wifi = send_wifi_reads(connection, devices).await?;
    let (devices, access_point_paths) = wait_wifi_reads(connection, pending_wifi).await?;
    let pending_access_points =
        send_access_point_properties(connection, access_point_paths).await?;
    let access_points = wait_access_points(connection, pending_access_points).await?;
    Ok(NetworkManagerDetailsSnapshot {
        root,
        wifi: WifiInventorySnapshot {
            devices,
            access_points,
        },
    })
}

struct PendingDevice {
    path: OwnedObjectPath,
    properties: PendingReply<Properties>,
}

struct PendingWifi {
    device: WifiDeviceSnapshot,
    wireless: PendingReply<Properties>,
    access_points: PendingReply<Vec<OwnedObjectPath>>,
}

struct PendingAccessPoint {
    device_path: OwnedObjectPath,
    path: OwnedObjectPath,
    properties: PendingReply<Properties>,
}

async fn send_device_properties(
    connection: &mut Connection,
    paths: Vec<OwnedObjectPath>,
) -> Result<Vec<PendingDevice>, NetworkManagerDetailsError> {
    let mut pending = Vec::with_capacity(paths.len());
    for path in paths {
        let properties = match connection
            .send_call::<_, Properties>(
                Some(DESTINATION),
                path.as_str(),
                Some(PROPERTIES_INTERFACE),
                "GetAll",
                &(DEVICE_INTERFACE,),
            )
            .await
        {
            Ok(properties) => properties,
            Err(error) => {
                abandon_devices(connection, pending);
                return Err(error.into());
            }
        };
        pending.push(PendingDevice { path, properties });
    }
    Ok(pending)
}

async fn wait_wifi_devices(
    connection: &mut Connection,
    pending: Vec<PendingDevice>,
) -> Result<Vec<WifiDeviceSnapshot>, NetworkManagerDetailsError> {
    let mut devices = Vec::new();
    let mut pending = pending.into_iter();
    while let Some(item) = pending.next() {
        let properties = match item.properties.wait(connection).await {
            Ok(properties) => properties,
            Err(error) => {
                abandon_devices(connection, pending);
                return Err(error.into());
            }
        };
        let device_type = match required_u32(&properties, DEVICE_INTERFACE, "DeviceType") {
            Ok(device_type) => device_type,
            Err(error) => {
                abandon_devices(connection, pending);
                return Err(error);
            }
        };
        if device_type == WIFI_DEVICE_TYPE {
            let device = match decode_device(item.path, &properties) {
                Ok(device) => device,
                Err(error) => {
                    abandon_devices(connection, pending);
                    return Err(error);
                }
            };
            devices.push(device);
            if let Err(error) = enforce_limit("Wi-Fi devices", devices.len(), MAX_WIFI_DEVICES) {
                abandon_devices(connection, pending);
                return Err(error);
            }
        }
    }
    Ok(devices)
}

async fn send_wifi_reads(
    connection: &mut Connection,
    devices: Vec<WifiDeviceSnapshot>,
) -> Result<Vec<PendingWifi>, NetworkManagerDetailsError> {
    let mut pending = Vec::with_capacity(devices.len());
    for device in devices {
        let wireless = match connection
            .send_call::<_, Properties>(
                Some(DESTINATION),
                device.path.as_str(),
                Some(PROPERTIES_INTERFACE),
                "GetAll",
                &(WIRELESS_INTERFACE,),
            )
            .await
        {
            Ok(properties) => properties,
            Err(error) => {
                abandon_wifi(connection, pending);
                return Err(error.into());
            }
        };
        let access_points = match connection
            .send_call::<_, Vec<OwnedObjectPath>>(
                Some(DESTINATION),
                device.path.as_str(),
                Some(WIRELESS_INTERFACE),
                "GetAllAccessPoints",
                &(),
            )
            .await
        {
            Ok(points) => points,
            Err(error) => {
                let _ = wireless.abandon(connection);
                abandon_wifi(connection, pending);
                return Err(error.into());
            }
        };
        pending.push(PendingWifi {
            device,
            wireless,
            access_points,
        });
    }
    Ok(pending)
}

async fn wait_wifi_reads(
    connection: &mut Connection,
    pending: Vec<PendingWifi>,
) -> Result<
    (
        Vec<WifiDeviceSnapshot>,
        Vec<(OwnedObjectPath, OwnedObjectPath)>,
    ),
    NetworkManagerDetailsError,
> {
    let mut devices = Vec::with_capacity(pending.len());
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    let mut pending = pending.into_iter();
    while let Some(mut item) = pending.next() {
        let wireless = match item.wireless.wait(connection).await {
            Ok(properties) => properties,
            Err(error) => {
                let _ = item.access_points.abandon(connection);
                abandon_wifi(connection, pending);
                return Err(error.into());
            }
        };
        item.device.active_access_point =
            match required_optional_path(&wireless, WIRELESS_INTERFACE, "ActiveAccessPoint") {
                Ok(path) => path,
                Err(error) => {
                    let _ = item.access_points.abandon(connection);
                    abandon_wifi(connection, pending);
                    return Err(error);
                }
            };
        let access_points = match item.access_points.wait(connection).await {
            Ok(points) => points,
            Err(error) => {
                abandon_wifi(connection, pending);
                return Err(error.into());
            }
        };
        for path in access_points {
            if seen.insert(path.clone()) {
                paths.push((item.device.path.clone(), path));
                if let Err(error) = enforce_limit("access points", paths.len(), MAX_ACCESS_POINTS) {
                    abandon_wifi(connection, pending);
                    return Err(error);
                }
            }
        }
        devices.push(item.device);
    }
    paths.sort_by(|left, right| left.1.as_str().cmp(right.1.as_str()));
    Ok((devices, paths))
}

async fn send_access_point_properties(
    connection: &mut Connection,
    paths: Vec<(OwnedObjectPath, OwnedObjectPath)>,
) -> Result<Vec<PendingAccessPoint>, NetworkManagerDetailsError> {
    let mut pending = Vec::with_capacity(paths.len());
    for (device_path, path) in paths {
        let properties = match connection
            .send_call::<_, Properties>(
                Some(DESTINATION),
                path.as_str(),
                Some(PROPERTIES_INTERFACE),
                "GetAll",
                &(ACCESS_POINT_INTERFACE,),
            )
            .await
        {
            Ok(properties) => properties,
            Err(error) => {
                abandon_access_points(connection, pending);
                return Err(error.into());
            }
        };
        pending.push(PendingAccessPoint {
            device_path,
            path,
            properties,
        });
    }
    Ok(pending)
}

async fn wait_access_points(
    connection: &mut Connection,
    pending: Vec<PendingAccessPoint>,
) -> Result<Vec<WifiAccessPointSnapshot>, NetworkManagerDetailsError> {
    let mut access_points = Vec::with_capacity(pending.len());
    let mut pending = pending.into_iter();
    while let Some(item) = pending.next() {
        let properties = match item.properties.wait(connection).await {
            Ok(properties) => properties,
            Err(error) => {
                abandon_access_points(connection, pending);
                return Err(error.into());
            }
        };
        let point = match decode_access_point(item.path, item.device_path, &properties) {
            Ok(point) => point,
            Err(error) => {
                abandon_access_points(connection, pending);
                return Err(error);
            }
        };
        access_points.push(point);
    }
    Ok(access_points)
}

fn decode_device(
    path: OwnedObjectPath,
    properties: &Properties,
) -> Result<WifiDeviceSnapshot, NetworkManagerDetailsError> {
    Ok(WifiDeviceSnapshot {
        path,
        interface_name: required_string(properties, DEVICE_INTERFACE, "Interface")?,
        state: super::DeviceState::from_code(required_u32(properties, DEVICE_INTERFACE, "State")?),
        hardware_address: required_string(properties, DEVICE_INTERFACE, "HwAddress")?,
        active_access_point: None,
    })
}

pub(super) fn decode_access_point(
    path: OwnedObjectPath,
    device_path: OwnedObjectPath,
    properties: &Properties,
) -> Result<WifiAccessPointSnapshot, NetworkManagerDetailsError> {
    let ssid = required_bytes(properties, ACCESS_POINT_INTERFACE, "Ssid")?;
    enforce_limit("SSID bytes", ssid.len(), MAX_SSID_BYTES)?;
    Ok(WifiAccessPointSnapshot {
        path,
        device_path,
        ssid_display: String::from_utf8_lossy(&ssid).into_owned(),
        ssid,
        bssid: required_string(properties, ACCESS_POINT_INTERFACE, "HwAddress")?,
        signal_strength: required_u8(properties, ACCESS_POINT_INTERFACE, "Strength")?.min(100),
        frequency_mhz: required_u32(properties, ACCESS_POINT_INTERFACE, "Frequency")?,
        max_bitrate_kbps: required_u32(properties, ACCESS_POINT_INTERFACE, "MaxBitrate")?,
        mode: WifiMode::from_code(required_u32(properties, ACCESS_POINT_INTERFACE, "Mode")?),
        flags: required_u32(properties, ACCESS_POINT_INTERFACE, "Flags")?,
        wpa_flags: required_u32(properties, ACCESS_POINT_INTERFACE, "WpaFlags")?,
        rsn_flags: required_u32(properties, ACCESS_POINT_INTERFACE, "RsnFlags")?,
    })
}

fn abandon_devices(connection: &mut Connection, pending: impl IntoIterator<Item = PendingDevice>) {
    for item in pending {
        let _ = item.properties.abandon(connection);
    }
}

fn abandon_wifi(connection: &mut Connection, pending: impl IntoIterator<Item = PendingWifi>) {
    for item in pending {
        let _ = item.wireless.abandon(connection);
        let _ = item.access_points.abandon(connection);
    }
}

fn abandon_access_points(
    connection: &mut Connection,
    pending: impl IntoIterator<Item = PendingAccessPoint>,
) {
    for item in pending {
        let _ = item.properties.abandon(connection);
    }
}

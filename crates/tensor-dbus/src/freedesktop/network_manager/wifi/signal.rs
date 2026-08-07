use super::{
    ACCESS_POINT_INTERFACE, DEVICE_INTERFACE, NetworkManagerDetailsError,
    NetworkManagerDetailsEvent, NetworkManagerDetailsSnapshot, PROPERTIES_INTERFACE, Properties,
    ROOT_INTERFACE, ROOT_PATH, WIRELESS_INTERFACE, WifiInventorySnapshot,
    apply_access_point_properties, optional_path_value, property_string, property_u32,
};
use crate::Message;

use super::super::{NetworkManagerMonitorEvent, apply_properties_changed};

pub(super) fn observe_snapshot(
    snapshot: &mut NetworkManagerDetailsSnapshot,
    message: &Message,
) -> Result<NetworkManagerDetailsEvent, NetworkManagerDetailsError> {
    if message.path() == Some(ROOT_PATH) && message.interface() == Some(ROOT_INTERFACE) {
        return Ok(match message.member() {
            Some("DeviceAdded" | "DeviceRemoved") => NetworkManagerDetailsEvent::RefreshRequired,
            _ => NetworkManagerDetailsEvent::Ignored,
        });
    }
    if message.interface() == Some(WIRELESS_INTERFACE) {
        return Ok(match message.member() {
            Some("AccessPointAdded" | "AccessPointRemoved") => {
                NetworkManagerDetailsEvent::RefreshRequired
            }
            _ => NetworkManagerDetailsEvent::Ignored,
        });
    }
    if message.interface() != Some(PROPERTIES_INTERFACE)
        || message.member() != Some("PropertiesChanged")
    {
        return Ok(NetworkManagerDetailsEvent::Ignored);
    }
    if message.path() == Some(ROOT_PATH) {
        return apply_root_changed(snapshot, message);
    }
    apply_child_changed(snapshot, message)
}

fn apply_root_changed(
    snapshot: &mut NetworkManagerDetailsSnapshot,
    message: &Message,
) -> Result<NetworkManagerDetailsEvent, NetworkManagerDetailsError> {
    Ok(
        match apply_properties_changed(&mut snapshot.root, message)? {
            NetworkManagerMonitorEvent::Ignored => NetworkManagerDetailsEvent::Ignored,
            NetworkManagerMonitorEvent::Unchanged => NetworkManagerDetailsEvent::Unchanged,
            NetworkManagerMonitorEvent::Changed => NetworkManagerDetailsEvent::Changed,
            NetworkManagerMonitorEvent::RefreshRequired => {
                NetworkManagerDetailsEvent::RefreshRequired
            }
            NetworkManagerMonitorEvent::OwnerChanged => NetworkManagerDetailsEvent::OwnerChanged,
        },
    )
}

fn apply_child_changed(
    snapshot: &mut NetworkManagerDetailsSnapshot,
    message: &Message,
) -> Result<NetworkManagerDetailsEvent, NetworkManagerDetailsError> {
    let Some(path) = message.path() else {
        return Ok(NetworkManagerDetailsEvent::Ignored);
    };
    let (interface, changed, invalidated): (String, Properties, Vec<String>) = message.body()?;
    let mut candidate = snapshot.wifi.clone();
    let event = match interface.as_str() {
        DEVICE_INTERFACE => apply_device_changed(&mut candidate, path, &changed, &invalidated)?,
        WIRELESS_INTERFACE => apply_wireless_changed(&mut candidate, path, &changed, &invalidated)?,
        ACCESS_POINT_INTERFACE => {
            apply_access_point_changed(&mut candidate, path, &changed, &invalidated)?
        }
        _ => NetworkManagerDetailsEvent::Ignored,
    };
    if event != NetworkManagerDetailsEvent::Changed {
        return Ok(event);
    }
    if candidate == snapshot.wifi {
        return Ok(NetworkManagerDetailsEvent::Unchanged);
    }
    snapshot.wifi = candidate;
    Ok(NetworkManagerDetailsEvent::Changed)
}

fn apply_device_changed(
    inventory: &mut WifiInventorySnapshot,
    path: &str,
    changed: &Properties,
    invalidated: &[String],
) -> Result<NetworkManagerDetailsEvent, NetworkManagerDetailsError> {
    let Some(device) = inventory
        .devices
        .iter_mut()
        .find(|device| device.path.as_str() == path)
    else {
        // Ethernet and other non-Wi-Fi devices share the generic Device
        // interface but are outside this retained inventory. DeviceAdded and
        // DeviceRemoved already cover topology changes.
        return Ok(NetworkManagerDetailsEvent::Ignored);
    };
    if invalidated.iter().any(|property| {
        matches!(
            property.as_str(),
            "Interface" | "State" | "HwAddress" | "DeviceType"
        )
    }) || changed.contains_key("DeviceType")
    {
        return Ok(NetworkManagerDetailsEvent::RefreshRequired);
    }
    if let Some(value) = changed.get("Interface") {
        device.interface_name = property_string(value, DEVICE_INTERFACE, "Interface")?;
    }
    if let Some(value) = changed.get("State") {
        device.state =
            super::DeviceState::from_code(property_u32(value, DEVICE_INTERFACE, "State")?);
    }
    if let Some(value) = changed.get("HwAddress") {
        device.hardware_address = property_string(value, DEVICE_INTERFACE, "HwAddress")?;
    }
    Ok(NetworkManagerDetailsEvent::Changed)
}

fn apply_wireless_changed(
    inventory: &mut WifiInventorySnapshot,
    path: &str,
    changed: &Properties,
    invalidated: &[String],
) -> Result<NetworkManagerDetailsEvent, NetworkManagerDetailsError> {
    let Some(device) = inventory
        .devices
        .iter_mut()
        .find(|device| device.path.as_str() == path)
    else {
        return Ok(NetworkManagerDetailsEvent::RefreshRequired);
    };
    if invalidated.iter().any(|property| {
        matches!(
            property.as_str(),
            "ActiveAccessPoint" | "AccessPoints" | "LastScan"
        )
    }) || changed.contains_key("AccessPoints")
        || changed.contains_key("LastScan")
    {
        return Ok(NetworkManagerDetailsEvent::RefreshRequired);
    }
    if let Some(value) = changed.get("ActiveAccessPoint") {
        device.active_access_point =
            optional_path_value(value, WIRELESS_INTERFACE, "ActiveAccessPoint")?;
    }
    Ok(NetworkManagerDetailsEvent::Changed)
}

fn apply_access_point_changed(
    inventory: &mut WifiInventorySnapshot,
    path: &str,
    changed: &Properties,
    invalidated: &[String],
) -> Result<NetworkManagerDetailsEvent, NetworkManagerDetailsError> {
    let Some(point) = inventory
        .access_points
        .iter_mut()
        .find(|point| point.path.as_str() == path)
    else {
        return Ok(NetworkManagerDetailsEvent::RefreshRequired);
    };
    if invalidated
        .iter()
        .any(|property| super::is_access_point_property(property))
    {
        return Ok(NetworkManagerDetailsEvent::RefreshRequired);
    }
    apply_access_point_properties(point, changed)?;
    Ok(NetworkManagerDetailsEvent::Changed)
}

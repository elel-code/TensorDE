//! Typed, bounded NetworkManager Wi-Fi discovery and live state.

mod read;
mod signal;
#[cfg(test)]
mod tests;

pub use read::read_details;

use std::collections::HashMap;

use zvariant::{OwnedObjectPath, OwnedValue};

use super::{
    DESTINATION, NetworkManagerError, NetworkManagerSnapshot, PROPERTIES_INTERFACE, Properties,
    ROOT_INTERFACE, ROOT_PATH,
};
use crate::{Connection, Error, MatchRule, Message, Result as TransportResult};

use signal::observe_snapshot;

pub const DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";
pub const WIRELESS_INTERFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
pub const ACCESS_POINT_INTERFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
pub const MAX_DEVICES: usize = 32;
pub const MAX_WIFI_DEVICES: usize = 16;
pub const MAX_ACCESS_POINTS: usize = 256;
pub const MAX_SSID_BYTES: usize = 32;

const WIFI_DEVICE_TYPE: u32 = 2;
const AP_FLAGS_PRIVACY: u32 = 0x1;
const AP_SEC_KEY_MGMT_802_1X: u32 = 0x200;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DeviceState {
    #[default]
    Unknown,
    Unmanaged,
    Unavailable,
    Disconnected,
    Prepare,
    Config,
    NeedAuth,
    IpConfig,
    IpCheck,
    Secondaries,
    Activated,
    Deactivating,
    Failed,
}

impl DeviceState {
    const fn from_code(code: u32) -> Self {
        match code {
            10 => Self::Unmanaged,
            20 => Self::Unavailable,
            30 => Self::Disconnected,
            40 => Self::Prepare,
            50 => Self::Config,
            60 => Self::NeedAuth,
            70 => Self::IpConfig,
            80 => Self::IpCheck,
            90 => Self::Secondaries,
            100 => Self::Activated,
            110 => Self::Deactivating,
            120 => Self::Failed,
            _ => Self::Unknown,
        }
    }

    pub const fn connected(self) -> bool {
        matches!(self, Self::Activated)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WifiMode {
    #[default]
    Unknown,
    AdHoc,
    Infrastructure,
    AccessPoint,
    Mesh,
}

impl WifiMode {
    const fn from_code(code: u32) -> Self {
        match code {
            1 => Self::AdHoc,
            2 => Self::Infrastructure,
            3 => Self::AccessPoint,
            4 => Self::Mesh,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WifiDeviceSnapshot {
    path: OwnedObjectPath,
    interface_name: String,
    state: DeviceState,
    hardware_address: String,
    active_access_point: Option<OwnedObjectPath>,
}

impl WifiDeviceSnapshot {
    pub fn path(&self) -> &OwnedObjectPath {
        &self.path
    }

    pub fn interface_name(&self) -> &str {
        &self.interface_name
    }

    pub const fn state(&self) -> DeviceState {
        self.state
    }

    pub fn hardware_address(&self) -> &str {
        &self.hardware_address
    }

    pub fn active_access_point(&self) -> Option<&OwnedObjectPath> {
        self.active_access_point.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WifiAccessPointSnapshot {
    path: OwnedObjectPath,
    device_path: OwnedObjectPath,
    ssid: Vec<u8>,
    ssid_display: String,
    bssid: String,
    signal_strength: u8,
    frequency_mhz: u32,
    max_bitrate_kbps: u32,
    mode: WifiMode,
    flags: u32,
    wpa_flags: u32,
    rsn_flags: u32,
}

impl WifiAccessPointSnapshot {
    pub fn path(&self) -> &OwnedObjectPath {
        &self.path
    }

    pub fn device_path(&self) -> &OwnedObjectPath {
        &self.device_path
    }

    pub fn ssid(&self) -> &[u8] {
        &self.ssid
    }

    pub fn ssid_display(&self) -> &str {
        &self.ssid_display
    }

    pub fn bssid(&self) -> &str {
        &self.bssid
    }

    pub const fn signal_strength(&self) -> u8 {
        self.signal_strength
    }

    pub const fn frequency_mhz(&self) -> u32 {
        self.frequency_mhz
    }

    pub const fn max_bitrate_kbps(&self) -> u32 {
        self.max_bitrate_kbps
    }

    pub const fn mode(&self) -> WifiMode {
        self.mode
    }

    pub const fn flags(&self) -> u32 {
        self.flags
    }

    pub const fn wpa_flags(&self) -> u32 {
        self.wpa_flags
    }

    pub const fn rsn_flags(&self) -> u32 {
        self.rsn_flags
    }

    pub const fn secured(&self) -> bool {
        self.flags & AP_FLAGS_PRIVACY != 0 || self.wpa_flags != 0 || self.rsn_flags != 0
    }

    pub const fn enterprise(&self) -> bool {
        (self.wpa_flags | self.rsn_flags) & AP_SEC_KEY_MGMT_802_1X != 0
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WifiInventorySnapshot {
    devices: Vec<WifiDeviceSnapshot>,
    access_points: Vec<WifiAccessPointSnapshot>,
}

impl WifiInventorySnapshot {
    pub fn devices(&self) -> &[WifiDeviceSnapshot] {
        &self.devices
    }

    pub fn access_points(&self) -> &[WifiAccessPointSnapshot] {
        &self.access_points
    }

    pub fn active_access_point(&self) -> Option<&WifiAccessPointSnapshot> {
        self.devices.iter().find_map(|device| {
            let active = device.active_access_point.as_ref()?;
            self.access_points
                .iter()
                .find(|point| point.path == *active)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkManagerDetailsSnapshot {
    root: NetworkManagerSnapshot,
    wifi: WifiInventorySnapshot,
}

impl NetworkManagerDetailsSnapshot {
    pub const fn root(&self) -> &NetworkManagerSnapshot {
        &self.root
    }

    pub const fn wifi(&self) -> &WifiInventorySnapshot {
        &self.wifi
    }

    pub fn from_parts(root: NetworkManagerSnapshot, wifi: WifiInventorySnapshot) -> Self {
        Self { root, wifi }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkManagerDetailsEvent {
    Ignored,
    Unchanged,
    Changed,
    RefreshRequired,
    OwnerChanged,
}

pub struct NetworkManagerDetailsMonitor {
    snapshot: NetworkManagerDetailsSnapshot,
    signal_rule: MatchRule,
}

impl NetworkManagerDetailsMonitor {
    pub async fn start(connection: &mut Connection) -> Result<Self, NetworkManagerDetailsError> {
        let mut signal_rule = signal_rule()?;
        connection.add_match(&mut signal_rule).await?;
        let snapshot = match read_details(connection).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = connection.remove_match(&signal_rule).await;
                return Err(error);
            }
        };
        Ok(Self {
            snapshot,
            signal_rule,
        })
    }

    pub const fn snapshot(&self) -> &NetworkManagerDetailsSnapshot {
        &self.snapshot
    }

    pub fn observe(
        &mut self,
        message: &Message,
    ) -> Result<NetworkManagerDetailsEvent, NetworkManagerDetailsError> {
        if self.signal_rule.observe(message)? {
            return Ok(NetworkManagerDetailsEvent::OwnerChanged);
        }
        if !self.signal_rule.matches(message) {
            return Ok(NetworkManagerDetailsEvent::Ignored);
        }
        observe_snapshot(&mut self.snapshot, message)
    }

    pub async fn refresh(
        &mut self,
        connection: &mut Connection,
    ) -> Result<NetworkManagerDetailsEvent, NetworkManagerDetailsError> {
        let snapshot = read_details(connection).await?;
        if snapshot == self.snapshot {
            return Ok(NetworkManagerDetailsEvent::Unchanged);
        }
        self.snapshot = snapshot;
        Ok(NetworkManagerDetailsEvent::Changed)
    }

    pub async fn close(self, connection: &mut Connection) -> TransportResult<()> {
        connection.remove_match(&self.signal_rule).await
    }
}

pub async fn request_scan(
    connection: &mut Connection,
    device_path: &OwnedObjectPath,
) -> Result<(), NetworkManagerDetailsError> {
    let options = HashMap::<String, OwnedValue>::new();
    let (): () = connection
        .call(
            Some(DESTINATION),
            device_path.as_str(),
            Some(WIRELESS_INTERFACE),
            "RequestScan",
            &(options,),
        )
        .await?;
    Ok(())
}

fn apply_access_point_properties(
    point: &mut WifiAccessPointSnapshot,
    properties: &Properties,
) -> Result<(), NetworkManagerDetailsError> {
    if let Some(value) = properties.get("Ssid") {
        let ssid = property_bytes(value, ACCESS_POINT_INTERFACE, "Ssid")?;
        enforce_limit("SSID bytes", ssid.len(), MAX_SSID_BYTES)?;
        point.ssid_display = String::from_utf8_lossy(&ssid).into_owned();
        point.ssid = ssid;
    }
    if let Some(value) = properties.get("HwAddress") {
        point.bssid = property_string(value, ACCESS_POINT_INTERFACE, "HwAddress")?;
    }
    if let Some(value) = properties.get("Strength") {
        point.signal_strength = property_u8(value, ACCESS_POINT_INTERFACE, "Strength")?.min(100);
    }
    if let Some(value) = properties.get("Frequency") {
        point.frequency_mhz = property_u32(value, ACCESS_POINT_INTERFACE, "Frequency")?;
    }
    if let Some(value) = properties.get("MaxBitrate") {
        point.max_bitrate_kbps = property_u32(value, ACCESS_POINT_INTERFACE, "MaxBitrate")?;
    }
    if let Some(value) = properties.get("Mode") {
        point.mode = WifiMode::from_code(property_u32(value, ACCESS_POINT_INTERFACE, "Mode")?);
    }
    if let Some(value) = properties.get("Flags") {
        point.flags = property_u32(value, ACCESS_POINT_INTERFACE, "Flags")?;
    }
    if let Some(value) = properties.get("WpaFlags") {
        point.wpa_flags = property_u32(value, ACCESS_POINT_INTERFACE, "WpaFlags")?;
    }
    if let Some(value) = properties.get("RsnFlags") {
        point.rsn_flags = property_u32(value, ACCESS_POINT_INTERFACE, "RsnFlags")?;
    }
    Ok(())
}

fn required<'a>(
    properties: &'a Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<&'a OwnedValue, NetworkManagerDetailsError> {
    properties
        .get(property)
        .ok_or(NetworkManagerDetailsError::MissingProperty {
            interface,
            property,
        })
}

fn required_u32(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<u32, NetworkManagerDetailsError> {
    property_u32(
        required(properties, interface, property)?,
        interface,
        property,
    )
}

fn required_u8(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<u8, NetworkManagerDetailsError> {
    property_u8(
        required(properties, interface, property)?,
        interface,
        property,
    )
}

fn required_string(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<String, NetworkManagerDetailsError> {
    property_string(
        required(properties, interface, property)?,
        interface,
        property,
    )
}

fn required_bytes(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<Vec<u8>, NetworkManagerDetailsError> {
    property_bytes(
        required(properties, interface, property)?,
        interface,
        property,
    )
}

fn required_optional_path(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<Option<OwnedObjectPath>, NetworkManagerDetailsError> {
    optional_path_value(
        required(properties, interface, property)?,
        interface,
        property,
    )
}

fn property_u32(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<u32, NetworkManagerDetailsError> {
    u32::try_from(value).map_err(|source| invalid_property(interface, property, source))
}

fn property_u8(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<u8, NetworkManagerDetailsError> {
    u8::try_from(value).map_err(|source| invalid_property(interface, property, source))
}

fn property_string(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<String, NetworkManagerDetailsError> {
    <&str>::try_from(value)
        .map(str::to_owned)
        .map_err(|source| invalid_property(interface, property, source))
}

fn property_bytes(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<Vec<u8>, NetworkManagerDetailsError> {
    let value = value
        .try_clone()
        .map_err(|source| invalid_property(interface, property, source))?;
    Vec::<u8>::try_from(value).map_err(|source| invalid_property(interface, property, source))
}

fn optional_path_value(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<Option<OwnedObjectPath>, NetworkManagerDetailsError> {
    let value = value
        .try_clone()
        .map_err(|source| invalid_property(interface, property, source))?;
    let path = OwnedObjectPath::try_from(value)
        .map_err(|source| invalid_property(interface, property, source))?;
    Ok((path.as_str() != "/").then_some(path))
}

fn invalid_property(
    interface: &'static str,
    property: &'static str,
    source: zvariant::Error,
) -> NetworkManagerDetailsError {
    NetworkManagerDetailsError::InvalidProperty {
        interface,
        property,
        source,
    }
}

fn is_access_point_property(property: &str) -> bool {
    matches!(
        property,
        "Ssid"
            | "HwAddress"
            | "Strength"
            | "Frequency"
            | "MaxBitrate"
            | "Mode"
            | "Flags"
            | "WpaFlags"
            | "RsnFlags"
    )
}

fn enforce_limit(
    collection: &'static str,
    count: usize,
    maximum: usize,
) -> Result<(), NetworkManagerDetailsError> {
    if count > maximum {
        Err(NetworkManagerDetailsError::LimitExceeded {
            collection,
            count,
            maximum,
        })
    } else {
        Ok(())
    }
}

fn signal_rule() -> TransportResult<MatchRule> {
    MatchRule::signal(Some(DESTINATION), None, None, None)?.path_namespace(ROOT_PATH)
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkManagerDetailsError {
    #[error(transparent)]
    Transport(#[from] Error),
    #[error(transparent)]
    Root(#[from] NetworkManagerError),
    #[error("NetworkManager {interface} response omitted required property `{property}`")]
    MissingProperty {
        interface: &'static str,
        property: &'static str,
    },
    #[error("NetworkManager {interface} property `{property}` has the wrong D-Bus type: {source}")]
    InvalidProperty {
        interface: &'static str,
        property: &'static str,
        source: zvariant::Error,
    },
    #[error("NetworkManager returned {count} {collection}; maximum is {maximum}")]
    LimitExceeded {
        collection: &'static str,
        count: usize,
        maximum: usize,
    },
}

impl NetworkManagerDetailsError {
    pub fn is_service_unavailable(&self) -> bool {
        match self {
            Self::Transport(Error::AddressUnavailable(_) | Error::Io(_)) => true,
            Self::Transport(Error::Method { name, .. }) => matches!(
                name.as_str(),
                "org.freedesktop.DBus.Error.NameHasNoOwner"
                    | "org.freedesktop.DBus.Error.ServiceUnknown"
                    | "org.freedesktop.DBus.Error.Spawn.ServiceNotFound"
            ),
            Self::Transport(_) => false,
            Self::Root(error) => error.is_service_unavailable(),
            Self::MissingProperty { .. }
            | Self::InvalidProperty { .. }
            | Self::LimitExceeded { .. } => false,
        }
    }
}

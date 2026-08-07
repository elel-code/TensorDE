//! Typed, caller-driven access to NetworkManager's aggregate network state.

pub mod wifi;

use std::collections::HashMap;

use zvariant::{OwnedValue, Value};

use crate::{Connection, Error, MatchRule, Message, Result as TransportResult};

pub const DESTINATION: &str = "org.freedesktop.NetworkManager";
pub const ROOT_PATH: &str = "/org/freedesktop/NetworkManager";
pub const ROOT_INTERFACE: &str = "org.freedesktop.NetworkManager";
pub const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

type Properties = HashMap<String, OwnedValue>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NetworkState {
    #[default]
    Unknown,
    Disabled,
    Disconnected,
    Disconnecting,
    Connecting,
    ConnectedLocal,
    ConnectedSite,
    ConnectedGlobal,
}

impl NetworkState {
    const fn from_code(code: u32) -> Self {
        match code {
            10 => Self::Disabled,
            20 => Self::Disconnected,
            30 => Self::Disconnecting,
            40 => Self::Connecting,
            50 => Self::ConnectedLocal,
            60 => Self::ConnectedSite,
            70 => Self::ConnectedGlobal,
            _ => Self::Unknown,
        }
    }

    pub const fn connected(self) -> bool {
        matches!(
            self,
            Self::ConnectedLocal | Self::ConnectedSite | Self::ConnectedGlobal
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Connectivity {
    #[default]
    Unknown,
    None,
    Portal,
    Limited,
    Full,
}

impl Connectivity {
    const fn from_code(code: u32) -> Self {
        match code {
            1 => Self::None,
            2 => Self::Portal,
            3 => Self::Limited,
            4 => Self::Full,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PrimaryConnectionKind {
    #[default]
    None,
    Ethernet,
    Wifi,
    Vpn,
    Other,
}

impl PrimaryConnectionKind {
    fn parse(value: &str) -> Self {
        match value {
            "" => Self::None,
            "802-3-ethernet" => Self::Ethernet,
            "802-11-wireless" => Self::Wifi,
            "vpn" | "wireguard" => Self::Vpn,
            _ => Self::Other,
        }
    }
}

/// Complete root-object state used by shells and settings surfaces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkManagerSnapshot {
    networking_enabled: bool,
    wireless_enabled: bool,
    wireless_hardware_enabled: bool,
    state: NetworkState,
    connectivity: Connectivity,
    primary_connection: PrimaryConnectionKind,
}

impl NetworkManagerSnapshot {
    pub fn from_parts(
        networking_enabled: bool,
        wireless_enabled: bool,
        wireless_hardware_enabled: bool,
        state: NetworkState,
        connectivity: Connectivity,
        primary_connection: PrimaryConnectionKind,
    ) -> Self {
        Self {
            networking_enabled,
            wireless_enabled,
            wireless_hardware_enabled,
            state,
            connectivity,
            primary_connection,
        }
    }

    pub const fn networking_enabled(&self) -> bool {
        self.networking_enabled
    }

    pub const fn wireless_enabled(&self) -> bool {
        self.wireless_enabled
    }

    pub const fn wireless_hardware_enabled(&self) -> bool {
        self.wireless_hardware_enabled
    }

    pub const fn effective_wireless_enabled(&self) -> bool {
        self.networking_enabled && self.wireless_enabled && self.wireless_hardware_enabled
    }

    pub const fn state(&self) -> NetworkState {
        self.state
    }

    pub const fn connectivity(&self) -> Connectivity {
        self.connectivity
    }

    pub const fn primary_connection(&self) -> PrimaryConnectionKind {
        self.primary_connection
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkManagerError {
    #[error(transparent)]
    Transport(#[from] Error),
    #[error("NetworkManager response omitted required property `{property}`")]
    MissingProperty { property: &'static str },
    #[error("NetworkManager property `{property}` has the wrong D-Bus type: {source}")]
    InvalidProperty {
        property: &'static str,
        source: zvariant::Error,
    },
}

impl NetworkManagerError {
    pub fn is_service_unavailable(&self) -> bool {
        match self {
            Self::Transport(Error::AddressUnavailable(_) | Error::Io(_)) => true,
            Self::Transport(Error::Method { name, .. }) => matches!(
                name.as_str(),
                "org.freedesktop.DBus.Error.NameHasNoOwner"
                    | "org.freedesktop.DBus.Error.ServiceUnknown"
                    | "org.freedesktop.DBus.Error.Spawn.ServiceNotFound"
            ),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkManagerMonitorEvent {
    Ignored,
    Unchanged,
    Changed,
    RefreshRequired,
    OwnerChanged,
}

/// Installed root-property match plus the latest complete snapshot.
pub struct NetworkManagerMonitor {
    snapshot: NetworkManagerSnapshot,
    properties_rule: MatchRule,
}

impl NetworkManagerMonitor {
    pub async fn start(connection: &mut Connection) -> Result<Self, NetworkManagerError> {
        let mut properties_rule = properties_rule()?;
        connection.add_match(&mut properties_rule).await?;
        let snapshot = match read_snapshot(connection).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = connection.remove_match(&properties_rule).await;
                return Err(error);
            }
        };
        Ok(Self {
            snapshot,
            properties_rule,
        })
    }

    pub const fn snapshot(&self) -> &NetworkManagerSnapshot {
        &self.snapshot
    }

    pub fn observe(
        &mut self,
        message: &Message,
    ) -> Result<NetworkManagerMonitorEvent, NetworkManagerError> {
        if self.properties_rule.observe(message)? {
            return Ok(NetworkManagerMonitorEvent::OwnerChanged);
        }
        if !self.properties_rule.matches(message) {
            return Ok(NetworkManagerMonitorEvent::Ignored);
        }
        apply_properties_changed(&mut self.snapshot, message)
    }

    pub async fn refresh(
        &mut self,
        connection: &mut Connection,
    ) -> Result<NetworkManagerMonitorEvent, NetworkManagerError> {
        let snapshot = read_snapshot(connection).await?;
        if snapshot == self.snapshot {
            return Ok(NetworkManagerMonitorEvent::Unchanged);
        }
        self.snapshot = snapshot;
        Ok(NetworkManagerMonitorEvent::Changed)
    }

    pub async fn close(self, connection: &mut Connection) -> TransportResult<()> {
        connection.remove_match(&self.properties_rule).await
    }
}

pub async fn read_snapshot(
    connection: &mut Connection,
) -> Result<NetworkManagerSnapshot, NetworkManagerError> {
    let properties: Properties = connection
        .call(
            Some(DESTINATION),
            ROOT_PATH,
            Some(PROPERTIES_INTERFACE),
            "GetAll",
            &(ROOT_INTERFACE,),
        )
        .await?;
    decode_snapshot(&properties)
}

pub async fn set_networking_enabled(
    connection: &mut Connection,
    enabled: bool,
) -> Result<(), NetworkManagerError> {
    let (): () = connection
        .call(
            Some(DESTINATION),
            ROOT_PATH,
            Some(ROOT_INTERFACE),
            "Enable",
            &(enabled,),
        )
        .await?;
    Ok(())
}

pub async fn set_wireless_enabled(
    connection: &mut Connection,
    enabled: bool,
) -> Result<(), NetworkManagerError> {
    let value = Value::new(enabled).try_into_owned().map_err(Error::from)?;
    let (): () = connection
        .call(
            Some(DESTINATION),
            ROOT_PATH,
            Some(PROPERTIES_INTERFACE),
            "Set",
            &(ROOT_INTERFACE, "WirelessEnabled", value),
        )
        .await?;
    Ok(())
}

pub fn apply_properties_changed(
    snapshot: &mut NetworkManagerSnapshot,
    message: &Message,
) -> Result<NetworkManagerMonitorEvent, NetworkManagerError> {
    if message.path() != Some(ROOT_PATH)
        || message.interface() != Some(PROPERTIES_INTERFACE)
        || message.member() != Some("PropertiesChanged")
    {
        return Ok(NetworkManagerMonitorEvent::Ignored);
    }
    let (interface, changed, invalidated): (String, Properties, Vec<String>) = message.body()?;
    if interface != ROOT_INTERFACE {
        return Ok(NetworkManagerMonitorEvent::Ignored);
    }
    if invalidated
        .iter()
        .any(|property| is_snapshot_property(property))
    {
        return Ok(NetworkManagerMonitorEvent::RefreshRequired);
    }

    let mut candidate = snapshot.clone();
    apply_changed(&changed, &mut candidate)?;
    if candidate == *snapshot {
        return Ok(NetworkManagerMonitorEvent::Unchanged);
    }
    *snapshot = candidate;
    Ok(NetworkManagerMonitorEvent::Changed)
}

fn decode_snapshot(properties: &Properties) -> Result<NetworkManagerSnapshot, NetworkManagerError> {
    Ok(NetworkManagerSnapshot {
        networking_enabled: required_bool(properties, "NetworkingEnabled")?,
        wireless_enabled: required_bool(properties, "WirelessEnabled")?,
        wireless_hardware_enabled: required_bool(properties, "WirelessHardwareEnabled")?,
        state: NetworkState::from_code(required_u32(properties, "State")?),
        connectivity: Connectivity::from_code(required_u32(properties, "Connectivity")?),
        primary_connection: PrimaryConnectionKind::parse(&required_string(
            properties,
            "PrimaryConnectionType",
        )?),
    })
}

fn apply_changed(
    changed: &Properties,
    snapshot: &mut NetworkManagerSnapshot,
) -> Result<(), NetworkManagerError> {
    if let Some(value) = changed.get("NetworkingEnabled") {
        snapshot.networking_enabled = property_bool(value, "NetworkingEnabled")?;
    }
    if let Some(value) = changed.get("WirelessEnabled") {
        snapshot.wireless_enabled = property_bool(value, "WirelessEnabled")?;
    }
    if let Some(value) = changed.get("WirelessHardwareEnabled") {
        snapshot.wireless_hardware_enabled = property_bool(value, "WirelessHardwareEnabled")?;
    }
    if let Some(value) = changed.get("State") {
        snapshot.state = NetworkState::from_code(property_u32(value, "State")?);
    }
    if let Some(value) = changed.get("Connectivity") {
        snapshot.connectivity = Connectivity::from_code(property_u32(value, "Connectivity")?);
    }
    if let Some(value) = changed.get("PrimaryConnectionType") {
        snapshot.primary_connection =
            PrimaryConnectionKind::parse(property_string(value, "PrimaryConnectionType")?);
    }
    Ok(())
}

fn properties_rule() -> TransportResult<MatchRule> {
    MatchRule::signal(
        Some(DESTINATION),
        Some(ROOT_PATH),
        Some(PROPERTIES_INTERFACE),
        Some("PropertiesChanged"),
    )
}

fn is_snapshot_property(property: &str) -> bool {
    matches!(
        property,
        "NetworkingEnabled"
            | "WirelessEnabled"
            | "WirelessHardwareEnabled"
            | "State"
            | "Connectivity"
            | "PrimaryConnectionType"
    )
}

fn required<'a>(
    properties: &'a Properties,
    property: &'static str,
) -> Result<&'a OwnedValue, NetworkManagerError> {
    properties
        .get(property)
        .ok_or(NetworkManagerError::MissingProperty { property })
}

fn required_bool(
    properties: &Properties,
    property: &'static str,
) -> Result<bool, NetworkManagerError> {
    property_bool(required(properties, property)?, property)
}

fn required_u32(
    properties: &Properties,
    property: &'static str,
) -> Result<u32, NetworkManagerError> {
    property_u32(required(properties, property)?, property)
}

fn required_string(
    properties: &Properties,
    property: &'static str,
) -> Result<String, NetworkManagerError> {
    property_string(required(properties, property)?, property).map(str::to_owned)
}

fn property_bool(value: &OwnedValue, property: &'static str) -> Result<bool, NetworkManagerError> {
    bool::try_from(value)
        .map_err(|source| NetworkManagerError::InvalidProperty { property, source })
}

fn property_u32(value: &OwnedValue, property: &'static str) -> Result<u32, NetworkManagerError> {
    u32::try_from(value).map_err(|source| NetworkManagerError::InvalidProperty { property, source })
}

fn property_string<'a>(
    value: &'a OwnedValue,
    property: &'static str,
) -> Result<&'a str, NetworkManagerError> {
    <&str>::try_from(value)
        .map_err(|source| NetworkManagerError::InvalidProperty { property, source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MessageKind, wire};

    fn value<T: Into<OwnedValue>>(value: T) -> OwnedValue {
        value.into()
    }

    fn string(value: &'static str) -> OwnedValue {
        OwnedValue::from(zvariant::Str::from(value))
    }

    fn properties(entries: impl IntoIterator<Item = (&'static str, OwnedValue)>) -> Properties {
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn snapshot_properties() -> Properties {
        properties([
            ("NetworkingEnabled", value(true)),
            ("WirelessEnabled", value(true)),
            ("WirelessHardwareEnabled", value(true)),
            ("State", value(70_u32)),
            ("Connectivity", value(4_u32)),
            ("PrimaryConnectionType", string("802-11-wireless")),
        ])
    }

    fn snapshot() -> NetworkManagerSnapshot {
        decode_snapshot(&snapshot_properties()).unwrap()
    }

    fn changed_message(changed: Properties, invalidated: Vec<String>) -> Message {
        let encoded = wire::encode_outgoing(
            wire::Outgoing {
                kind: MessageKind::Signal,
                flags: 0,
                serial: 9,
                reply_serial: None,
                path: Some(ROOT_PATH),
                interface: Some(PROPERTIES_INTERFACE),
                member: Some("PropertiesChanged"),
                error_name: None,
                destination: None,
            },
            &(ROOT_INTERFACE, changed, invalidated),
        )
        .unwrap();
        wire::decode_message(encoded.bytes, Vec::new()).unwrap()
    }

    #[test]
    fn full_snapshot_is_typed_and_preserves_connectivity_semantics() {
        let snapshot = snapshot();
        assert!(snapshot.networking_enabled());
        assert!(snapshot.effective_wireless_enabled());
        assert_eq!(snapshot.state(), NetworkState::ConnectedGlobal);
        assert_eq!(snapshot.connectivity(), Connectivity::Full);
        assert_eq!(snapshot.primary_connection(), PrimaryConnectionKind::Wifi);
    }

    #[test]
    fn property_updates_are_atomic_and_unknown_enums_are_explicit() {
        let mut snapshot = snapshot();
        let before = snapshot.clone();
        let invalid = changed_message(
            properties([("State", value(40_u32)), ("WirelessEnabled", value(1_u32))]),
            Vec::new(),
        );
        assert!(matches!(
            apply_properties_changed(&mut snapshot, &invalid),
            Err(NetworkManagerError::InvalidProperty {
                property: "WirelessEnabled",
                ..
            })
        ));
        assert_eq!(snapshot, before);

        let unknown = changed_message(
            properties([
                ("State", value(999_u32)),
                ("Connectivity", value(999_u32)),
                ("PrimaryConnectionType", string("tensor-link")),
            ]),
            Vec::new(),
        );
        assert_eq!(
            apply_properties_changed(&mut snapshot, &unknown).unwrap(),
            NetworkManagerMonitorEvent::Changed
        );
        assert_eq!(snapshot.state(), NetworkState::Unknown);
        assert_eq!(snapshot.connectivity(), Connectivity::Unknown);
        assert_eq!(snapshot.primary_connection(), PrimaryConnectionKind::Other);
    }

    #[test]
    fn relevant_invalidation_requests_refresh_without_mutation() {
        let mut snapshot = snapshot();
        let before = snapshot.clone();
        let invalidated = changed_message(
            properties([("WirelessEnabled", value(false))]),
            vec!["Connectivity".to_owned()],
        );
        assert_eq!(
            apply_properties_changed(&mut snapshot, &invalidated).unwrap(),
            NetworkManagerMonitorEvent::RefreshRequired
        );
        assert_eq!(snapshot, before);
    }

    #[test]
    fn irrelevant_and_duplicate_changes_do_not_advance_state() {
        let mut snapshot = snapshot();
        let duplicate = changed_message(
            properties([
                ("WirelessEnabled", value(true)),
                ("Version", string("1.54.0")),
            ]),
            Vec::new(),
        );
        assert_eq!(
            apply_properties_changed(&mut snapshot, &duplicate).unwrap(),
            NetworkManagerMonitorEvent::Unchanged
        );
    }
}

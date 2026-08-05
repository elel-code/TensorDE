//! Typed, caller-driven access to UPower's display-device snapshot.

use std::collections::HashMap;

use zvariant::{OwnedObjectPath, OwnedValue};

use crate::{Connection, Error, MatchRule, Message, Result as TransportResult};

pub const DESTINATION: &str = "org.freedesktop.UPower";
pub const ROOT_PATH: &str = "/org/freedesktop/UPower";
pub const ROOT_INTERFACE: &str = "org.freedesktop.UPower";
pub const DEVICE_INTERFACE: &str = "org.freedesktop.UPower.Device";
pub const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

type Properties = HashMap<String, OwnedValue>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerSource {
    Ac,
    Battery,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BatteryState {
    #[default]
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

impl BatteryState {
    const fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::FullyCharged,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BatteryWarning {
    #[default]
    Unknown,
    None,
    Discharging,
    Low,
    Critical,
    Action,
}

impl BatteryWarning {
    const fn from_code(code: u32) -> Self {
        match code {
            1 => Self::None,
            2 => Self::Discharging,
            3 => Self::Low,
            4 => Self::Critical,
            5 => Self::Action,
            _ => Self::Unknown,
        }
    }
}

/// A complete, validated snapshot of UPower's aggregate display device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UPowerSnapshot {
    display_device: OwnedObjectPath,
    source: PowerSource,
    battery_present: bool,
    percentage: Option<u8>,
    state: BatteryState,
    warning: BatteryWarning,
}

impl UPowerSnapshot {
    pub fn from_parts(
        display_device: OwnedObjectPath,
        source: PowerSource,
        battery_present: bool,
        percentage: Option<u8>,
        state: BatteryState,
        warning: BatteryWarning,
    ) -> Self {
        let mut snapshot = Self {
            display_device,
            source,
            battery_present,
            percentage: percentage.map(|value| value.min(100)),
            state,
            warning,
        };
        normalize(&mut snapshot);
        snapshot
    }

    pub fn display_device(&self) -> &OwnedObjectPath {
        &self.display_device
    }

    pub const fn source(&self) -> PowerSource {
        self.source
    }

    pub const fn battery_present(&self) -> bool {
        self.battery_present
    }

    pub const fn percentage(&self) -> Option<u8> {
        self.percentage
    }

    pub const fn state(&self) -> BatteryState {
        self.state
    }

    pub const fn warning(&self) -> BatteryWarning {
        self.warning
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UPowerError {
    #[error(transparent)]
    Transport(#[from] Error),
    #[error("UPower {interface} response omitted required property `{property}`")]
    MissingProperty {
        interface: &'static str,
        property: &'static str,
    },
    #[error("UPower {interface} property `{property}` has the wrong D-Bus type: {source}")]
    InvalidProperty {
        interface: &'static str,
        property: &'static str,
        source: zvariant::Error,
    },
    #[error("UPower display-device percentage is not finite: {value}")]
    InvalidPercentage { value: f64 },
}

impl UPowerError {
    /// Whether no usable UPower provider is currently reachable.
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
pub enum UPowerMonitorEvent {
    Ignored,
    Unchanged,
    Changed,
    RefreshRequired,
    OwnerChanged,
}

/// Installed UPower signal rules plus the latest complete retained snapshot.
///
/// Construction subscribes before reading properties, so changes racing the
/// initial read remain queued on the caller-owned connection.
pub struct UPowerMonitor {
    snapshot: UPowerSnapshot,
    root_rule: MatchRule,
    device_rule: MatchRule,
}

impl UPowerMonitor {
    pub async fn start(connection: &mut Connection) -> Result<Self, UPowerError> {
        let mut root_rule = properties_rule(ROOT_PATH)?;
        connection.add_match(&mut root_rule).await?;

        let display_device = match get_display_device(connection).await {
            Ok(path) => path,
            Err(error) => {
                let _ = connection.remove_match(&root_rule).await;
                return Err(error);
            }
        };
        let mut device_rule = properties_rule(display_device.as_str())?;
        if let Err(error) = connection.add_match(&mut device_rule).await {
            let _ = connection.remove_match(&root_rule).await;
            return Err(error.into());
        }
        let snapshot = match read_snapshot_at(connection, display_device).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = connection.remove_match(&device_rule).await;
                let _ = connection.remove_match(&root_rule).await;
                return Err(error);
            }
        };
        Ok(Self {
            snapshot,
            root_rule,
            device_rule,
        })
    }

    pub const fn snapshot(&self) -> &UPowerSnapshot {
        &self.snapshot
    }

    pub fn observe(&mut self, message: &Message) -> Result<UPowerMonitorEvent, UPowerError> {
        let root_owner = self.root_rule.observe(message)?;
        let device_owner = self.device_rule.observe(message)?;
        if root_owner || device_owner {
            return Ok(UPowerMonitorEvent::OwnerChanged);
        }
        if !self.root_rule.matches(message) && !self.device_rule.matches(message) {
            return Ok(UPowerMonitorEvent::Ignored);
        }
        apply_properties_changed(&mut self.snapshot, message)
    }

    pub async fn close(self, connection: &mut Connection) -> TransportResult<()> {
        let device = connection.remove_match(&self.device_rule).await;
        let root = connection.remove_match(&self.root_rule).await;
        device.and(root)
    }
}

pub async fn read_snapshot(connection: &mut Connection) -> Result<UPowerSnapshot, UPowerError> {
    let display_device = get_display_device(connection).await?;
    read_snapshot_at(connection, display_device).await
}

pub fn apply_properties_changed(
    snapshot: &mut UPowerSnapshot,
    message: &Message,
) -> Result<UPowerMonitorEvent, UPowerError> {
    let interface = match message.path() {
        Some(ROOT_PATH) => ROOT_INTERFACE,
        Some(path) if path == snapshot.display_device.as_str() => DEVICE_INTERFACE,
        _ => return Ok(UPowerMonitorEvent::Ignored),
    };
    if message.interface() != Some(PROPERTIES_INTERFACE)
        || message.member() != Some("PropertiesChanged")
    {
        return Ok(UPowerMonitorEvent::Ignored);
    }
    let (changed_interface, changed, invalidated): (String, Properties, Vec<String>) =
        message.body()?;
    if changed_interface != interface {
        return Ok(UPowerMonitorEvent::Ignored);
    }
    if invalidated
        .iter()
        .any(|property| is_snapshot_property(interface, property))
    {
        return Ok(UPowerMonitorEvent::RefreshRequired);
    }

    let mut candidate = snapshot.clone();
    apply_changed(interface, &changed, &mut candidate)?;
    normalize(&mut candidate);
    if candidate.battery_present && candidate.percentage.is_none() {
        return Ok(UPowerMonitorEvent::RefreshRequired);
    }
    if candidate == *snapshot {
        return Ok(UPowerMonitorEvent::Unchanged);
    }
    *snapshot = candidate;
    Ok(UPowerMonitorEvent::Changed)
}

async fn get_display_device(connection: &mut Connection) -> Result<OwnedObjectPath, UPowerError> {
    connection
        .call(
            Some(DESTINATION),
            ROOT_PATH,
            Some(ROOT_INTERFACE),
            "GetDisplayDevice",
            &(),
        )
        .await
        .map_err(Into::into)
}

async fn read_snapshot_at(
    connection: &mut Connection,
    display_device: OwnedObjectPath,
) -> Result<UPowerSnapshot, UPowerError> {
    let root = connection
        .send_call::<_, Properties>(
            Some(DESTINATION),
            ROOT_PATH,
            Some(PROPERTIES_INTERFACE),
            "GetAll",
            &(ROOT_INTERFACE,),
        )
        .await?;
    let device = match connection
        .send_call::<_, Properties>(
            Some(DESTINATION),
            display_device.as_str(),
            Some(PROPERTIES_INTERFACE),
            "GetAll",
            &(DEVICE_INTERFACE,),
        )
        .await
    {
        Ok(device) => device,
        Err(error) => {
            let _ = root.abandon(connection);
            return Err(error.into());
        }
    };
    let root_properties = match root.wait(connection).await {
        Ok(properties) => properties,
        Err(error) => {
            let _ = device.abandon(connection);
            return Err(error.into());
        }
    };
    let device_properties = device.wait(connection).await?;
    decode_snapshot(display_device, &root_properties, &device_properties)
}

fn decode_snapshot(
    display_device: OwnedObjectPath,
    root: &Properties,
    device: &Properties,
) -> Result<UPowerSnapshot, UPowerError> {
    let on_battery = required_bool(root, ROOT_INTERFACE, "OnBattery")?;
    let battery_present = required_bool(device, DEVICE_INTERFACE, "IsPresent")?;
    let percentage = percentage(required_f64(device, DEVICE_INTERFACE, "Percentage")?)?;
    let state = required_u32(device, DEVICE_INTERFACE, "State")?;
    let warning = required_u32(device, DEVICE_INTERFACE, "WarningLevel")?;
    let mut snapshot = UPowerSnapshot {
        display_device,
        source: if on_battery {
            PowerSource::Battery
        } else {
            PowerSource::Ac
        },
        battery_present,
        percentage: Some(percentage),
        state: BatteryState::from_code(state),
        warning: BatteryWarning::from_code(warning),
    };
    normalize(&mut snapshot);
    Ok(snapshot)
}

fn apply_changed(
    interface: &'static str,
    changed: &Properties,
    snapshot: &mut UPowerSnapshot,
) -> Result<(), UPowerError> {
    if interface == ROOT_INTERFACE {
        if let Some(value) = changed.get("OnBattery") {
            snapshot.source = if property_bool(value, interface, "OnBattery")? {
                PowerSource::Battery
            } else {
                PowerSource::Ac
            };
        }
        return Ok(());
    }
    if let Some(value) = changed.get("IsPresent") {
        snapshot.battery_present = property_bool(value, interface, "IsPresent")?;
    }
    if let Some(value) = changed.get("Percentage") {
        snapshot.percentage = Some(percentage(property_f64(value, interface, "Percentage")?)?);
    }
    if let Some(value) = changed.get("State") {
        snapshot.state = BatteryState::from_code(property_u32(value, interface, "State")?);
    }
    if let Some(value) = changed.get("WarningLevel") {
        snapshot.warning =
            BatteryWarning::from_code(property_u32(value, interface, "WarningLevel")?);
    }
    Ok(())
}

fn normalize(snapshot: &mut UPowerSnapshot) {
    if !snapshot.battery_present {
        snapshot.percentage = None;
        snapshot.state = BatteryState::Unknown;
        snapshot.warning = BatteryWarning::Unknown;
    }
}

fn properties_rule(path: &str) -> TransportResult<MatchRule> {
    MatchRule::signal(
        Some(DESTINATION),
        Some(path),
        Some(PROPERTIES_INTERFACE),
        Some("PropertiesChanged"),
    )
}

fn is_snapshot_property(interface: &str, property: &str) -> bool {
    match interface {
        ROOT_INTERFACE => property == "OnBattery",
        DEVICE_INTERFACE => matches!(
            property,
            "IsPresent" | "Percentage" | "State" | "WarningLevel"
        ),
        _ => false,
    }
}

fn required_bool(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<bool, UPowerError> {
    property_bool(
        required(properties, interface, property)?,
        interface,
        property,
    )
}

fn required_u32(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<u32, UPowerError> {
    property_u32(
        required(properties, interface, property)?,
        interface,
        property,
    )
}

fn required_f64(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<f64, UPowerError> {
    property_f64(
        required(properties, interface, property)?,
        interface,
        property,
    )
}

fn required<'a>(
    properties: &'a Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<&'a OwnedValue, UPowerError> {
    properties
        .get(property)
        .ok_or(UPowerError::MissingProperty {
            interface,
            property,
        })
}

fn property_bool(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<bool, UPowerError> {
    bool::try_from(value).map_err(|source| UPowerError::InvalidProperty {
        interface,
        property,
        source,
    })
}

fn property_u32(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<u32, UPowerError> {
    u32::try_from(value).map_err(|source| UPowerError::InvalidProperty {
        interface,
        property,
        source,
    })
}

fn property_f64(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<f64, UPowerError> {
    f64::try_from(value).map_err(|source| UPowerError::InvalidProperty {
        interface,
        property,
        source,
    })
}

fn percentage(value: f64) -> Result<u8, UPowerError> {
    if !value.is_finite() {
        return Err(UPowerError::InvalidPercentage { value });
    }
    Ok(value.clamp(0.0, 100.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MessageKind, wire};

    fn value<T: Into<OwnedValue>>(value: T) -> OwnedValue {
        value.into()
    }

    fn properties(entries: impl IntoIterator<Item = (&'static str, OwnedValue)>) -> Properties {
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn snapshot() -> UPowerSnapshot {
        decode_snapshot(
            OwnedObjectPath::try_from("/org/freedesktop/UPower/devices/DisplayDevice").unwrap(),
            &properties([("OnBattery", value(true))]),
            &properties([
                ("IsPresent", value(true)),
                ("Percentage", value(72.6_f64)),
                ("State", value(2_u32)),
                ("WarningLevel", value(3_u32)),
            ]),
        )
        .unwrap()
    }

    fn changed_message(
        path: &str,
        interface: &str,
        changed: Properties,
        invalidated: Vec<String>,
    ) -> Message {
        let encoded = wire::encode_outgoing(
            wire::Outgoing {
                kind: MessageKind::Signal,
                flags: 0,
                serial: 7,
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

    #[test]
    fn full_snapshot_is_typed_rounded_and_bounded() {
        let snapshot = snapshot();
        assert_eq!(snapshot.source(), PowerSource::Battery);
        assert!(snapshot.battery_present());
        assert_eq!(snapshot.percentage(), Some(73));
        assert_eq!(snapshot.state(), BatteryState::Discharging);
        assert_eq!(snapshot.warning(), BatteryWarning::Low);

        assert_eq!(percentage(-3.0).unwrap(), 0);
        assert_eq!(percentage(120.0).unwrap(), 100);
        assert!(matches!(
            percentage(f64::NAN),
            Err(UPowerError::InvalidPercentage { .. })
        ));
    }

    #[test]
    fn absent_battery_does_not_retain_stale_device_values() {
        let mut snapshot = snapshot();
        let path = snapshot.display_device().as_str().to_owned();
        let message = changed_message(
            &path,
            DEVICE_INTERFACE,
            properties([("IsPresent", value(false))]),
            Vec::new(),
        );
        assert_eq!(
            apply_properties_changed(&mut snapshot, &message).unwrap(),
            UPowerMonitorEvent::Changed
        );
        assert!(!snapshot.battery_present());
        assert_eq!(snapshot.percentage(), None);
        assert_eq!(snapshot.state(), BatteryState::Unknown);
        assert_eq!(snapshot.warning(), BatteryWarning::Unknown);
    }

    #[test]
    fn updates_are_atomic_and_unknown_enum_values_are_explicit() {
        let mut snapshot = snapshot();
        let before = snapshot.clone();
        let path = snapshot.display_device().as_str().to_owned();
        let invalid = changed_message(
            &path,
            DEVICE_INTERFACE,
            properties([("State", value(99_u32)), ("WarningLevel", value(true))]),
            Vec::new(),
        );
        assert!(matches!(
            apply_properties_changed(&mut snapshot, &invalid),
            Err(UPowerError::InvalidProperty {
                property: "WarningLevel",
                ..
            })
        ));
        assert_eq!(snapshot, before);

        let unknown = changed_message(
            &path,
            DEVICE_INTERFACE,
            properties([("State", value(99_u32)), ("WarningLevel", value(99_u32))]),
            Vec::new(),
        );
        assert_eq!(
            apply_properties_changed(&mut snapshot, &unknown).unwrap(),
            UPowerMonitorEvent::Changed
        );
        assert_eq!(snapshot.state(), BatteryState::Unknown);
        assert_eq!(snapshot.warning(), BatteryWarning::Unknown);
    }

    #[test]
    fn relevant_invalidations_request_a_complete_refresh() {
        let mut snapshot = snapshot();
        let message = changed_message(
            ROOT_PATH,
            ROOT_INTERFACE,
            Properties::new(),
            vec!["OnBattery".to_owned()],
        );
        assert_eq!(
            apply_properties_changed(&mut snapshot, &message).unwrap(),
            UPowerMonitorEvent::RefreshRequired
        );
    }

    #[test]
    fn irrelevant_and_duplicate_changes_do_not_dirty_the_snapshot() {
        let mut snapshot = snapshot();
        let irrelevant = changed_message(
            ROOT_PATH,
            "org.freedesktop.UPower.KbdBacklight",
            properties([("Brightness", value(3_u32))]),
            Vec::new(),
        );
        assert_eq!(
            apply_properties_changed(&mut snapshot, &irrelevant).unwrap(),
            UPowerMonitorEvent::Ignored
        );
        let duplicate = changed_message(
            ROOT_PATH,
            ROOT_INTERFACE,
            properties([("OnBattery", value(true))]),
            Vec::new(),
        );
        assert_eq!(
            apply_properties_changed(&mut snapshot, &duplicate).unwrap(),
            UPowerMonitorEvent::Unchanged
        );
    }
}

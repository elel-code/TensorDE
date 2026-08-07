use zvariant::OwnedValue;

use super::{
    MprisError, MprisMonitorEvent, MprisPlayerSnapshot, OBJECT_PATH, PLAYER_INTERFACE, Properties,
    required,
};
use crate::{MatchRule, Message, Result as TransportResult};

pub(super) fn apply_seeked(
    player: &mut MprisPlayerSnapshot,
    message: &Message,
) -> Result<MprisMonitorEvent, MprisError> {
    let position = message.body::<i64>()?;
    let position = position_value(position, "Position")?;
    if player.position_micros == Some(position) {
        return Ok(MprisMonitorEvent::Unchanged);
    }
    player.position_micros = Some(position);
    Ok(MprisMonitorEvent::Changed)
}

pub(super) fn required_position(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<u64, MprisError> {
    position_value(
        property_i64(
            required(properties, interface, property)?,
            interface,
            property,
        )?,
        property,
    )
}

pub(super) fn property_i64(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<i64, MprisError> {
    i64::try_from(value).map_err(|source| MprisError::InvalidProperty {
        interface,
        property,
        source,
    })
}

pub(super) fn position_value(value: i64, property: &'static str) -> Result<u64, MprisError> {
    u64::try_from(value).map_err(|_| MprisError::InvalidPosition { property, value })
}

pub(super) fn seeked_rule() -> TransportResult<MatchRule> {
    MatchRule::signal(
        None,
        Some(OBJECT_PATH),
        Some(PLAYER_INTERFACE),
        Some("Seeked"),
    )
}

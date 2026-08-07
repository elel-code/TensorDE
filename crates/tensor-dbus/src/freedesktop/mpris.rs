//! Typed, caller-driven access to MPRIS player discovery and controls.

use std::collections::HashMap;

use zvariant::OwnedValue;

use crate::{Connection, Error, MatchRule, Message, Result as TransportResult};

mod position;

use position::{apply_seeked, position_value, property_i64, required_position, seeked_rule};

pub const NAME_PREFIX: &str = "org.mpris.MediaPlayer2.";
pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
pub const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
pub const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
pub const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
pub const MAX_PLAYERS: usize = 32;
pub const MAX_METADATA_TEXT_BYTES: usize = 4 * 1024;
pub const MAX_ARTISTS: usize = 32;

const DBUS_DESTINATION: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const DBUS_INTERFACE: &str = "org.freedesktop.DBus";

type Properties = HashMap<String, OwnedValue>;

struct DecodedMetadata {
    title: Option<String>,
    artists: Vec<String>,
    album: Option<String>,
    art_url: Option<String>,
    duration_micros: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl PlaybackStatus {
    fn parse(value: &str) -> Result<Self, MprisError> {
        match value {
            "Playing" => Ok(Self::Playing),
            "Paused" => Ok(Self::Paused),
            "Stopped" => Ok(Self::Stopped),
            value => Err(MprisError::UnknownPlaybackStatus(value.to_owned())),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MprisAction {
    Previous,
    PlayPause,
    Next,
}

impl MprisAction {
    const fn member(self) -> &'static str {
        match self {
            Self::Previous => "Previous",
            Self::PlayPause => "PlayPause",
            Self::Next => "Next",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MprisPlayerSnapshot {
    bus_name: String,
    owner: String,
    identity: String,
    desktop_entry: Option<String>,
    playback: PlaybackStatus,
    title: Option<String>,
    artists: Vec<String>,
    album: Option<String>,
    art_url: Option<String>,
    position_micros: Option<u64>,
    duration_micros: Option<u64>,
    can_control: bool,
    can_play: bool,
    can_pause: bool,
    can_go_previous: bool,
    can_go_next: bool,
}

impl MprisPlayerSnapshot {
    pub fn bus_name(&self) -> &str {
        &self.bus_name
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn desktop_entry(&self) -> Option<&str> {
        self.desktop_entry.as_deref()
    }

    pub const fn playback(&self) -> PlaybackStatus {
        self.playback
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn artists(&self) -> &[String] {
        &self.artists
    }

    pub fn album(&self) -> Option<&str> {
        self.album.as_deref()
    }

    pub fn art_url(&self) -> Option<&str> {
        self.art_url.as_deref()
    }

    /// Current playback position in MPRIS microseconds, when advertised.
    pub const fn position_micros(&self) -> Option<u64> {
        self.position_micros
    }

    /// Track duration from `mpris:length`, in microseconds, when available.
    pub const fn duration_micros(&self) -> Option<u64> {
        self.duration_micros
    }

    pub const fn can_control(&self) -> bool {
        self.can_control
    }

    pub const fn can_play(&self) -> bool {
        self.can_play
    }

    pub const fn can_pause(&self) -> bool {
        self.can_pause
    }

    pub const fn can_go_previous(&self) -> bool {
        self.can_go_previous
    }

    pub const fn can_go_next(&self) -> bool {
        self.can_go_next
    }

    pub const fn can_play_pause(&self) -> bool {
        self.can_control
            && match self.playback {
                PlaybackStatus::Playing => self.can_pause,
                PlaybackStatus::Paused | PlaybackStatus::Stopped => self.can_play,
            }
    }

    pub const fn supports(&self, action: MprisAction) -> bool {
        match action {
            MprisAction::Previous => self.can_control && self.can_go_previous,
            MprisAction::PlayPause => self.can_play_pause(),
            MprisAction::Next => self.can_control && self.can_go_next,
        }
    }

    fn is_idle(&self) -> bool {
        self.playback == PlaybackStatus::Stopped && self.title.is_none() && self.artists.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MprisMonitorEvent {
    Ignored,
    Unchanged,
    Changed,
    RefreshRequired,
}

pub struct MprisMonitor {
    players: Vec<MprisPlayerSnapshot>,
    properties_rule: MatchRule,
    seeked_rule: MatchRule,
    owner_rule: MatchRule,
}

impl MprisMonitor {
    pub async fn start(connection: &mut Connection) -> Result<Self, MprisError> {
        let mut owner_rule = owner_rule()?;
        connection.add_match(&mut owner_rule).await?;
        let mut properties_rule = properties_rule()?;
        if let Err(error) = connection.add_match(&mut properties_rule).await {
            let _ = connection.remove_match(&owner_rule).await;
            return Err(error.into());
        }
        let mut seeked_rule = match seeked_rule() {
            Ok(rule) => rule,
            Err(error) => {
                let _ = connection.remove_match(&properties_rule).await;
                let _ = connection.remove_match(&owner_rule).await;
                return Err(error.into());
            }
        };
        if let Err(error) = connection.add_match(&mut seeked_rule).await {
            let _ = connection.remove_match(&properties_rule).await;
            let _ = connection.remove_match(&owner_rule).await;
            return Err(error.into());
        }
        let players = match read_players(connection).await {
            Ok(players) => players,
            Err(error) => {
                let _ = connection.remove_match(&seeked_rule).await;
                let _ = connection.remove_match(&properties_rule).await;
                let _ = connection.remove_match(&owner_rule).await;
                return Err(error);
            }
        };
        Ok(Self {
            players,
            properties_rule,
            seeked_rule,
            owner_rule,
        })
    }

    pub fn players(&self) -> &[MprisPlayerSnapshot] {
        &self.players
    }

    pub fn active_player(&self, previous: Option<&str>) -> Option<&MprisPlayerSnapshot> {
        select_active(&self.players, previous)
    }

    pub fn observe(&mut self, message: &Message) -> Result<MprisMonitorEvent, MprisError> {
        if self.owner_rule.matches(message) {
            return Ok(MprisMonitorEvent::RefreshRequired);
        }
        if self.seeked_rule.matches(message) {
            return self.observe_seeked(message);
        }
        if !self.properties_rule.matches(message) {
            return Ok(MprisMonitorEvent::Ignored);
        }
        let Some(sender) = message.sender() else {
            return Ok(MprisMonitorEvent::Ignored);
        };
        let Some(index) = self
            .players
            .iter()
            .position(|player| player.owner == sender)
        else {
            return Ok(MprisMonitorEvent::RefreshRequired);
        };
        apply_properties_changed(&mut self.players[index], message)
    }

    fn observe_seeked(&mut self, message: &Message) -> Result<MprisMonitorEvent, MprisError> {
        let Some(sender) = message.sender() else {
            return Ok(MprisMonitorEvent::Ignored);
        };
        let Some(index) = self
            .players
            .iter()
            .position(|player| player.owner == sender)
        else {
            return Ok(MprisMonitorEvent::RefreshRequired);
        };
        apply_seeked(&mut self.players[index], message)
    }

    pub async fn perform(
        &self,
        connection: &mut Connection,
        bus_name: &str,
        action: MprisAction,
    ) -> Result<(), MprisError> {
        let player = self
            .players
            .iter()
            .find(|player| player.bus_name == bus_name)
            .ok_or_else(|| MprisError::UnknownPlayer(bus_name.to_owned()))?;
        if !player.supports(action) {
            return Err(MprisError::UnsupportedAction {
                player: bus_name.to_owned(),
                action,
            });
        }
        let (): () = connection
            .call(
                Some(player.bus_name.as_str()),
                OBJECT_PATH,
                Some(PLAYER_INTERFACE),
                action.member(),
                &(),
            )
            .await?;
        Ok(())
    }

    pub async fn close(self, connection: &mut Connection) -> TransportResult<()> {
        let properties = connection.remove_match(&self.properties_rule).await;
        let seeked = connection.remove_match(&self.seeked_rule).await;
        let owner = connection.remove_match(&self.owner_rule).await;
        properties.and(seeked).and(owner)
    }
}

pub async fn read_players(
    connection: &mut Connection,
) -> Result<Vec<MprisPlayerSnapshot>, MprisError> {
    let mut names: Vec<String> = connection
        .call(
            Some(DBUS_DESTINATION),
            DBUS_PATH,
            Some(DBUS_INTERFACE),
            "ListNames",
            &(),
        )
        .await?;
    names.retain(|name| is_player_name(name));
    names.sort();
    names.dedup();
    names.truncate(MAX_PLAYERS);

    let mut players = Vec::with_capacity(names.len());
    for name in names {
        match read_player(connection, name).await {
            Ok(player) => players.push(player),
            Err(error) if error.is_service_unavailable() => {}
            Err(error) => return Err(error),
        }
    }
    Ok(players)
}

fn select_active<'a>(
    players: &'a [MprisPlayerSnapshot],
    previous: Option<&str>,
) -> Option<&'a MprisPlayerSnapshot> {
    let best_priority = players
        .iter()
        .filter(|player| !player.is_idle())
        .map(player_priority)
        .min()?;
    previous
        .and_then(|name| players.iter().find(|player| player.bus_name == name))
        .filter(|player| !player.is_idle() && player_priority(player) == best_priority)
        .or_else(|| {
            players
                .iter()
                .find(|player| !player.is_idle() && player_priority(player) == best_priority)
        })
}

const fn player_priority(player: &MprisPlayerSnapshot) -> u8 {
    match (player.playback, player.can_control) {
        (PlaybackStatus::Playing, true) => 0,
        (PlaybackStatus::Playing, false) => 1,
        (PlaybackStatus::Paused | PlaybackStatus::Stopped, true) => 2,
        (PlaybackStatus::Paused | PlaybackStatus::Stopped, false) => 3,
    }
}

async fn read_player(
    connection: &mut Connection,
    bus_name: String,
) -> Result<MprisPlayerSnapshot, MprisError> {
    let owner: String = connection
        .call(
            Some(DBUS_DESTINATION),
            DBUS_PATH,
            Some(DBUS_INTERFACE),
            "GetNameOwner",
            &(bus_name.as_str(),),
        )
        .await?;
    let root = connection
        .send_call::<_, Properties>(
            Some(bus_name.as_str()),
            OBJECT_PATH,
            Some(PROPERTIES_INTERFACE),
            "GetAll",
            &(ROOT_INTERFACE,),
        )
        .await?;
    let player = match connection
        .send_call::<_, Properties>(
            Some(bus_name.as_str()),
            OBJECT_PATH,
            Some(PROPERTIES_INTERFACE),
            "GetAll",
            &(PLAYER_INTERFACE,),
        )
        .await
    {
        Ok(player) => player,
        Err(error) => {
            let _ = root.abandon(connection);
            return Err(error.into());
        }
    };
    let root = match root.wait(connection).await {
        Ok(root) => root,
        Err(error) => {
            let _ = player.abandon(connection);
            return Err(error.into());
        }
    };
    let player = player.wait(connection).await?;
    decode_player(bus_name, owner, &root, &player)
}

fn decode_player(
    bus_name: String,
    owner: String,
    root: &Properties,
    player: &Properties,
) -> Result<MprisPlayerSnapshot, MprisError> {
    let identity = required_string(root, ROOT_INTERFACE, "Identity")?;
    let desktop_entry = optional_string(root, ROOT_INTERFACE, "DesktopEntry")?;
    let playback = PlaybackStatus::parse(&required_string(
        player,
        PLAYER_INTERFACE,
        "PlaybackStatus",
    )?)?;
    let metadata = required_metadata(player)?;
    let metadata = decode_metadata(&metadata)?;
    Ok(MprisPlayerSnapshot {
        bus_name,
        owner,
        identity,
        desktop_entry,
        playback,
        title: metadata.title,
        artists: metadata.artists,
        album: metadata.album,
        art_url: metadata.art_url,
        position_micros: Some(required_position(player, PLAYER_INTERFACE, "Position")?),
        duration_micros: metadata.duration_micros,
        can_control: required_bool(player, PLAYER_INTERFACE, "CanControl")?,
        can_play: required_bool(player, PLAYER_INTERFACE, "CanPlay")?,
        can_pause: required_bool(player, PLAYER_INTERFACE, "CanPause")?,
        can_go_previous: required_bool(player, PLAYER_INTERFACE, "CanGoPrevious")?,
        can_go_next: required_bool(player, PLAYER_INTERFACE, "CanGoNext")?,
    })
}

fn apply_properties_changed(
    player: &mut MprisPlayerSnapshot,
    message: &Message,
) -> Result<MprisMonitorEvent, MprisError> {
    let (interface, changed, invalidated): (String, Properties, Vec<String>) = message.body()?;
    if interface != PLAYER_INTERFACE {
        return Ok(MprisMonitorEvent::Ignored);
    }
    if invalidated.iter().any(|name| is_player_property(name)) {
        return Ok(MprisMonitorEvent::RefreshRequired);
    }
    let mut candidate = player.clone();
    if let Some(value) = changed.get("PlaybackStatus") {
        candidate.playback =
            PlaybackStatus::parse(&property_string(value, PLAYER_INTERFACE, "PlaybackStatus")?)?;
    }
    if let Some(value) = changed.get("Metadata") {
        let metadata = property_metadata(value, PLAYER_INTERFACE, "Metadata")?;
        let metadata = decode_metadata(&metadata)?;
        candidate.title = metadata.title;
        candidate.artists = metadata.artists;
        candidate.album = metadata.album;
        candidate.art_url = metadata.art_url;
        candidate.duration_micros = metadata.duration_micros;
    }
    if let Some(value) = changed.get("Position") {
        candidate.position_micros = Some(position_value(
            property_i64(value, PLAYER_INTERFACE, "Position")?,
            "Position",
        )?);
    }
    apply_optional_bool(&changed, "CanControl", &mut candidate.can_control)?;
    apply_optional_bool(&changed, "CanPlay", &mut candidate.can_play)?;
    apply_optional_bool(&changed, "CanPause", &mut candidate.can_pause)?;
    apply_optional_bool(&changed, "CanGoPrevious", &mut candidate.can_go_previous)?;
    apply_optional_bool(&changed, "CanGoNext", &mut candidate.can_go_next)?;
    if candidate == *player {
        return Ok(MprisMonitorEvent::Unchanged);
    }
    *player = candidate;
    Ok(MprisMonitorEvent::Changed)
}

fn required_metadata(properties: &Properties) -> Result<Properties, MprisError> {
    property_metadata(
        required(properties, PLAYER_INTERFACE, "Metadata")?,
        PLAYER_INTERFACE,
        "Metadata",
    )
}

fn decode_metadata(metadata: &Properties) -> Result<DecodedMetadata, MprisError> {
    let title = metadata_string(metadata, "xesam:title")?;
    let album = metadata_string(metadata, "xesam:album")?;
    let art_url = metadata_string(metadata, "mpris:artUrl")?;
    let duration_micros = metadata
        .get("mpris:length")
        .map(|value| {
            position_value(
                property_i64(value, PLAYER_INTERFACE, "Metadata[mpris:length]")?,
                "Metadata[mpris:length]",
            )
        })
        .transpose()?;
    let artists = match metadata.get("xesam:artist") {
        Some(value) => {
            let owned = value
                .try_clone()
                .map_err(|source| MprisError::InvalidProperty {
                    interface: PLAYER_INTERFACE,
                    property: "Metadata[xesam:artist]",
                    source,
                })?;
            let artists =
                Vec::<String>::try_from(owned).map_err(|source| MprisError::InvalidProperty {
                    interface: PLAYER_INTERFACE,
                    property: "Metadata[xesam:artist]",
                    source,
                })?;
            if artists.len() > MAX_ARTISTS {
                return Err(MprisError::TooManyArtists {
                    count: artists.len(),
                    maximum: MAX_ARTISTS,
                });
            }
            artists
                .into_iter()
                .map(|artist| bounded_text("Metadata[xesam:artist]", artist))
                .collect::<Result<_, _>>()?
        }
        None => Vec::new(),
    };
    Ok(DecodedMetadata {
        title,
        artists,
        album,
        art_url,
        duration_micros,
    })
}

fn metadata_string(
    metadata: &Properties,
    property: &'static str,
) -> Result<Option<String>, MprisError> {
    metadata
        .get(property)
        .map(|value| property_string(value, PLAYER_INTERFACE, property))
        .transpose()
        .and_then(|value| {
            value
                .filter(|value| !value.is_empty())
                .map(|value| bounded_text(property, value))
                .transpose()
        })
}

fn apply_optional_bool(
    changed: &Properties,
    property: &'static str,
    target: &mut bool,
) -> Result<(), MprisError> {
    if let Some(value) = changed.get(property) {
        *target = property_bool(value, PLAYER_INTERFACE, property)?;
    }
    Ok(())
}

fn required_bool(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<bool, MprisError> {
    property_bool(
        required(properties, interface, property)?,
        interface,
        property,
    )
}

fn required_string(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<String, MprisError> {
    let value = property_string(
        required(properties, interface, property)?,
        interface,
        property,
    )?;
    bounded_text(property, value)
}

fn optional_string(
    properties: &Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<Option<String>, MprisError> {
    properties
        .get(property)
        .map(|value| property_string(value, interface, property))
        .transpose()?
        .filter(|value| !value.is_empty())
        .map(|value| bounded_text(property, value))
        .transpose()
}

fn required<'a>(
    properties: &'a Properties,
    interface: &'static str,
    property: &'static str,
) -> Result<&'a OwnedValue, MprisError> {
    properties.get(property).ok_or(MprisError::MissingProperty {
        interface,
        property,
    })
}

fn property_bool(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<bool, MprisError> {
    bool::try_from(value).map_err(|source| MprisError::InvalidProperty {
        interface,
        property,
        source,
    })
}

fn property_string(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<String, MprisError> {
    <&str>::try_from(value)
        .map(str::to_owned)
        .map_err(|source| MprisError::InvalidProperty {
            interface,
            property,
            source,
        })
}

fn property_metadata(
    value: &OwnedValue,
    interface: &'static str,
    property: &'static str,
) -> Result<Properties, MprisError> {
    let owned = value
        .try_clone()
        .map_err(|source| MprisError::InvalidProperty {
            interface,
            property,
            source,
        })?;
    Properties::try_from(owned).map_err(|source| MprisError::InvalidProperty {
        interface,
        property,
        source,
    })
}

fn bounded_text(property: &'static str, value: String) -> Result<String, MprisError> {
    if value.len() > MAX_METADATA_TEXT_BYTES {
        Err(MprisError::MetadataTextTooLong {
            property,
            bytes: value.len(),
            maximum: MAX_METADATA_TEXT_BYTES,
        })
    } else {
        Ok(value)
    }
}

fn is_player_property(property: &str) -> bool {
    matches!(
        property,
        "PlaybackStatus"
            | "Metadata"
            | "Position"
            | "CanControl"
            | "CanPlay"
            | "CanPause"
            | "CanGoPrevious"
            | "CanGoNext"
    )
}

fn is_player_name(name: &str) -> bool {
    name.strip_prefix(NAME_PREFIX)
        .is_some_and(|suffix| !suffix.is_empty())
}

fn properties_rule() -> TransportResult<MatchRule> {
    MatchRule::signal(
        None,
        Some(OBJECT_PATH),
        Some(PROPERTIES_INTERFACE),
        Some("PropertiesChanged"),
    )
}

fn owner_rule() -> TransportResult<MatchRule> {
    MatchRule::signal(
        None,
        Some(DBUS_PATH),
        Some(DBUS_INTERFACE),
        Some("NameOwnerChanged"),
    )?
    .arg0_namespace("org.mpris.MediaPlayer2")
}

#[derive(Debug, thiserror::Error)]
pub enum MprisError {
    #[error(transparent)]
    Transport(#[from] Error),
    #[error("MPRIS {interface} response omitted required property `{property}`")]
    MissingProperty {
        interface: &'static str,
        property: &'static str,
    },
    #[error("MPRIS {interface} property `{property}` has the wrong D-Bus type: {source}")]
    InvalidProperty {
        interface: &'static str,
        property: &'static str,
        source: zvariant::Error,
    },
    #[error("MPRIS returned unknown PlaybackStatus `{0}`")]
    UnknownPlaybackStatus(String),
    #[error("MPRIS property `{property}` contains {bytes} bytes; maximum is {maximum}")]
    MetadataTextTooLong {
        property: &'static str,
        bytes: usize,
        maximum: usize,
    },
    #[error("MPRIS metadata contains {count} artists; maximum is {maximum}")]
    TooManyArtists { count: usize, maximum: usize },
    #[error("MPRIS property `{property}` contains a negative position {value}")]
    InvalidPosition { property: &'static str, value: i64 },
    #[error("MPRIS player `{0}` is no longer present")]
    UnknownPlayer(String),
    #[error("MPRIS player `{player}` does not support {action:?}")]
    UnsupportedAction { player: String, action: MprisAction },
}

impl MprisError {
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

#[cfg(test)]
mod tests;

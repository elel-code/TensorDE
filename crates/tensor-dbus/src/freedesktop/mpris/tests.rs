use super::*;
use crate::{MessageKind, wire};

fn value<T: Into<OwnedValue>>(value: T) -> OwnedValue {
    value.into()
}

fn text(value: &str) -> OwnedValue {
    zvariant::Str::from(value).into()
}

fn string_array(values: &[&str]) -> OwnedValue {
    let values = values
        .iter()
        .map(|value| zvariant::Str::from(*value))
        .collect::<Vec<_>>();
    OwnedValue::try_from(zvariant::Array::from(values)).unwrap()
}

fn properties(entries: impl IntoIterator<Item = (&'static str, OwnedValue)>) -> Properties {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

fn player(name: &str, playback: PlaybackStatus, control: bool) -> MprisPlayerSnapshot {
    MprisPlayerSnapshot {
        bus_name: format!("{NAME_PREFIX}{name}"),
        owner: format!(":1.{name}"),
        identity: name.to_owned(),
        desktop_entry: None,
        playback,
        title: Some("Track".into()),
        artists: vec!["Artist".into()],
        album: None,
        art_url: None,
        position_micros: None,
        duration_micros: None,
        can_control: control,
        can_play: control,
        can_pause: control,
        can_go_previous: control,
        can_go_next: control,
    }
}

fn changed_message(changed: Properties, invalidated: Vec<String>) -> Message {
    let encoded = wire::encode_outgoing(
        wire::Outgoing {
            kind: MessageKind::Signal,
            flags: 0,
            serial: 7,
            reply_serial: None,
            path: Some(OBJECT_PATH),
            interface: Some(PROPERTIES_INTERFACE),
            member: Some("PropertiesChanged"),
            error_name: None,
            destination: None,
        },
        &(PLAYER_INTERFACE, changed, invalidated),
    )
    .unwrap();
    wire::decode_message(encoded.bytes, Vec::new()).unwrap()
}

fn seeked_message(position: i64) -> Message {
    let encoded = wire::encode_outgoing(
        wire::Outgoing {
            kind: MessageKind::Signal,
            flags: 0,
            serial: 8,
            reply_serial: None,
            path: Some(OBJECT_PATH),
            interface: Some(PLAYER_INTERFACE),
            member: Some("Seeked"),
            error_name: None,
            destination: None,
        },
        &(position,),
    )
    .unwrap();
    wire::decode_message(encoded.bytes, Vec::new()).unwrap()
}

#[test]
fn complete_player_snapshot_decodes_metadata_and_capabilities() {
    let root = properties([
        ("Identity", text("Demo Player")),
        ("DesktopEntry", text("demo")),
    ]);
    let metadata = properties([
        ("xesam:title", text("A Track")),
        ("xesam:artist", string_array(&["An Artist"])),
        ("xesam:album", text("An Album")),
        ("mpris:length", value(180_000_000_i64)),
    ]);
    let player = properties([
        ("PlaybackStatus", text("Playing")),
        ("Position", value(42_000_000_i64)),
        ("Metadata", value(metadata)),
        ("CanControl", value(true)),
        ("CanPlay", value(true)),
        ("CanPause", value(true)),
        ("CanGoPrevious", value(true)),
        ("CanGoNext", value(false)),
    ]);
    let snapshot =
        decode_player(format!("{NAME_PREFIX}demo"), ":1.20".into(), &root, &player).unwrap();
    assert_eq!(snapshot.playback(), PlaybackStatus::Playing);
    assert_eq!(snapshot.title(), Some("A Track"));
    assert_eq!(snapshot.artists(), ["An Artist"]);
    assert_eq!(snapshot.position_micros(), Some(42_000_000));
    assert_eq!(snapshot.duration_micros(), Some(180_000_000));
    assert!(snapshot.can_play_pause());
    assert!(!snapshot.can_go_next());
}

#[test]
fn active_selection_prefers_playing_and_retains_previous_player() {
    let paused = player("paused", PlaybackStatus::Paused, true);
    let playing = player("playing", PlaybackStatus::Playing, true);
    let players = vec![paused.clone(), playing.clone()];
    assert_eq!(
        select_active(&players, Some(paused.bus_name())).unwrap(),
        &playing
    );
    let players = vec![
        paused.clone(),
        player("stopped", PlaybackStatus::Stopped, true),
    ];
    assert_eq!(
        select_active(&players, Some(paused.bus_name())).unwrap(),
        &paused
    );

    let uncontrollable = player("uncontrollable", PlaybackStatus::Playing, false);
    let controllable = player("controllable", PlaybackStatus::Playing, true);
    let players = vec![uncontrollable.clone(), controllable.clone()];
    assert_eq!(
        select_active(&players, Some(uncontrollable.bus_name())).unwrap(),
        &controllable
    );
}

#[test]
fn property_updates_are_atomic_and_invalidations_refresh() {
    let mut snapshot = player("demo", PlaybackStatus::Paused, true);
    let changed = changed_message(
        properties([("PlaybackStatus", text("Playing"))]),
        Vec::new(),
    );
    assert_eq!(
        apply_properties_changed(&mut snapshot, &changed).unwrap(),
        MprisMonitorEvent::Changed
    );
    assert_eq!(snapshot.playback(), PlaybackStatus::Playing);

    let position = changed_message(properties([("Position", value(12_i64))]), Vec::new());
    assert_eq!(
        apply_properties_changed(&mut snapshot, &position).unwrap(),
        MprisMonitorEvent::Changed
    );
    assert_eq!(snapshot.position_micros(), Some(12));

    let before = snapshot.clone();
    let invalid = changed_message(properties([("CanPlay", text("wrong"))]), Vec::new());
    assert!(apply_properties_changed(&mut snapshot, &invalid).is_err());
    assert_eq!(snapshot, before);

    let invalidated = changed_message(Properties::new(), vec!["Metadata".into()]);
    assert_eq!(
        apply_properties_changed(&mut snapshot, &invalidated).unwrap(),
        MprisMonitorEvent::RefreshRequired
    );
}

#[test]
fn negative_position_is_rejected_without_mutating_the_snapshot() {
    let mut snapshot = player("demo", PlaybackStatus::Paused, true);
    let before = snapshot.clone();
    let changed = changed_message(properties([("Position", value(-1_i64))]), Vec::new());
    assert!(matches!(
        apply_properties_changed(&mut snapshot, &changed),
        Err(MprisError::InvalidPosition {
            property: "Position",
            value: -1
        })
    ));
    assert_eq!(snapshot, before);
}

#[test]
fn seeked_signal_updates_position_atomically() {
    let mut snapshot = player("demo", PlaybackStatus::Playing, true);
    assert_eq!(
        apply_seeked(&mut snapshot, &seeked_message(75_000_000)).unwrap(),
        MprisMonitorEvent::Changed
    );
    assert_eq!(snapshot.position_micros(), Some(75_000_000));
    assert_eq!(
        apply_seeked(&mut snapshot, &seeked_message(75_000_000)).unwrap(),
        MprisMonitorEvent::Unchanged
    );
    let before = snapshot.clone();
    assert!(matches!(
        apply_seeked(&mut snapshot, &seeked_message(-1)),
        Err(MprisError::InvalidPosition { .. })
    ));
    assert_eq!(snapshot, before);
}

#[test]
fn discovery_and_action_contracts_are_closed() {
    assert!(is_player_name("org.mpris.MediaPlayer2.demo"));
    assert!(!is_player_name("org.mpris.MediaPlayer2"));
    let playing = player("demo", PlaybackStatus::Playing, true);
    assert!(playing.supports(MprisAction::Previous));
    assert!(playing.supports(MprisAction::PlayPause));
    assert!(playing.supports(MprisAction::Next));
    assert_eq!(MprisAction::PlayPause.member(), "PlayPause");
}

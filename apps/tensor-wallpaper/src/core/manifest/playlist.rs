use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{ManifestError, WallpaperEntry, validate_required_text};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub id: String,
    pub entry: Box<WallpaperEntry>,
    #[serde(default)]
    pub conditions: PlaylistConditions,
    #[serde(default = "default_playlist_item_weight")]
    pub weight: u32,
}

impl PlaylistItem {
    pub(super) fn validate(&self, index: usize) -> Result<(), ManifestError> {
        validate_required_text("playlist item id", &self.id)?;
        if self.weight == 0 {
            return Err(ManifestError::InvalidEntry(format!(
                "playlist item {:?} weight must be greater than 0",
                self.id
            )));
        }
        if matches!(self.entry.as_ref(), WallpaperEntry::Playlist { .. }) {
            return Err(ManifestError::InvalidEntry(format!(
                "playlist item {:?} must not contain a nested playlist",
                self.id
            )));
        }
        self.conditions.validate(&self.id)?;
        self.entry
            .validate()
            .map_err(|err| ManifestError::InvalidEntry(format!("playlist item {index}: {err}")))
    }
}

fn default_playlist_item_weight() -> u32 {
    1
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistConditions {
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub power: Option<PlaylistPowerCondition>,
    #[serde(default)]
    pub local_time: Option<PlaylistLocalTimeCondition>,
    #[serde(default)]
    pub weekdays: Vec<PlaylistWeekday>,
    #[serde(default)]
    pub focused: Option<bool>,
    #[serde(default)]
    pub visible: Option<bool>,
    #[serde(default)]
    pub fullscreen: Option<bool>,
    #[serde(default)]
    pub session_active: Option<bool>,
    #[serde(default)]
    pub session_locked: Option<bool>,
}

impl PlaylistConditions {
    fn validate(&self, item_id: &str) -> Result<(), ManifestError> {
        if self.outputs.iter().any(|output| output.trim().is_empty()) {
            return Err(ManifestError::InvalidEntry(format!(
                "playlist item {item_id:?} output condition must not contain empty names"
            )));
        }
        if let Some(local_time) = &self.local_time {
            local_time.validate(item_id)?;
        }
        let unique_weekdays = self.weekdays.iter().collect::<BTreeSet<_>>();
        if unique_weekdays.len() != self.weekdays.len() {
            return Err(ManifestError::InvalidEntry(format!(
                "playlist item {item_id:?} weekdays condition must not contain duplicates"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistLocalTimeCondition {
    pub start: String,
    pub end: String,
}

impl PlaylistLocalTimeCondition {
    fn validate(&self, item_id: &str) -> Result<(), ManifestError> {
        let Some(start) = parse_playlist_local_time_minute(&self.start) else {
            return Err(ManifestError::InvalidEntry(format!(
                "playlist item {item_id:?} local_time.start must use HH:MM"
            )));
        };
        let Some(end) = parse_playlist_local_time_minute(&self.end) else {
            return Err(ManifestError::InvalidEntry(format!(
                "playlist item {item_id:?} local_time.end must use HH:MM"
            )));
        };
        if start == end {
            return Err(ManifestError::InvalidEntry(format!(
                "playlist item {item_id:?} local_time start and end must differ"
            )));
        }
        Ok(())
    }

    pub(crate) fn contains_minute_of_day(&self, minute: u16) -> bool {
        let Some(start) = parse_playlist_local_time_minute(&self.start) else {
            return false;
        };
        let Some(end) = parse_playlist_local_time_minute(&self.end) else {
            return false;
        };
        if start < end {
            start <= minute && minute < end
        } else {
            minute >= start || minute < end
        }
    }
}

pub(crate) fn parse_playlist_local_time_minute(value: &str) -> Option<u16> {
    let (hour, minute) = value.split_once(':')?;
    if hour.len() != 2 || minute.len() != 2 {
        return None;
    }
    let hour = hour.parse::<u16>().ok()?;
    let minute = minute.parse::<u16>().ok()?;
    if hour >= 24 || minute >= 60 {
        return None;
    }
    Some(hour * 60 + minute)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaylistPowerCondition {
    Unknown,
    Ac,
    Battery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaylistWeekday {
    #[serde(alias = "mon")]
    Monday,
    #[serde(alias = "tue")]
    Tuesday,
    #[serde(alias = "wed")]
    Wednesday,
    #[serde(alias = "thu")]
    Thursday,
    #[serde(alias = "fri")]
    Friday,
    #[serde(alias = "sat")]
    Saturday,
    #[serde(alias = "sun")]
    Sunday,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaylistSelection {
    #[default]
    FirstMatch,
    WeightedRandom,
}

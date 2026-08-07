use tensor_dbus::freedesktop::mpris::{MprisAction, MprisPlayerSnapshot, PlaybackStatus};

use crate::MediaServiceSnapshot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MediaOsdContent {
    pub(crate) title: String,
    pub(crate) artists: Vec<String>,
    pub(crate) album: Option<String>,
    pub(crate) position_micros: Option<u64>,
    pub(crate) duration_micros: Option<u64>,
    pub(crate) playback: PlaybackStatus,
    pub(crate) previous: bool,
    pub(crate) play_pause: bool,
    pub(crate) next: bool,
}

impl MediaOsdContent {
    fn from_player(player: &MprisPlayerSnapshot) -> Self {
        Self {
            title: player.title().unwrap_or_default().to_owned(),
            artists: player.artists().to_vec(),
            album: player.album().map(str::to_owned),
            position_micros: player.position_micros(),
            duration_micros: player.duration_micros(),
            playback: player.playback(),
            previous: player.supports(MprisAction::Previous),
            play_pause: player.supports(MprisAction::PlayPause),
            next: player.supports(MprisAction::Next),
        }
    }

    pub(crate) const fn supports(&self, action: MprisAction) -> bool {
        match action {
            MprisAction::Previous => self.previous,
            MprisAction::PlayPause => self.play_pause,
            MprisAction::Next => self.next,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MediaOsdTrigger {
    title: String,
    artists: Vec<String>,
    album: Option<String>,
    playback: PlaybackStatus,
}

impl From<&MediaOsdContent> for MediaOsdTrigger {
    fn from(content: &MediaOsdContent) -> Self {
        Self {
            title: content.title.clone(),
            artists: content.artists.clone(),
            album: content.album.clone(),
            playback: content.playback,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MediaOsdState {
    observed: Option<MediaOsdTrigger>,
    visible: Option<MediaOsdContent>,
    deadline_ms: Option<u64>,
    position_anchor_ms: Option<u64>,
    hovered: bool,
    revision: u64,
}

impl MediaOsdState {
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn content(&self) -> Option<&MediaOsdContent> {
        self.visible.as_ref()
    }

    pub(crate) const fn visible(&self) -> bool {
        self.visible.is_some()
    }

    pub(crate) fn observe(
        &mut self,
        snapshot: &MediaServiceSnapshot,
        now_ms: u64,
        enabled: bool,
        timeout_ms: u64,
    ) {
        match snapshot {
            MediaServiceSnapshot::Pending => {}
            MediaServiceSnapshot::Ready(player) => self.observe_content(
                player.as_deref().map(MediaOsdContent::from_player),
                now_ms,
                enabled,
                timeout_ms,
            ),
            MediaServiceSnapshot::Unavailable | MediaServiceSnapshot::Failed => {
                self.observed = None;
                self.hide();
            }
        }
    }

    pub(crate) fn expire(&mut self, now_ms: u64) {
        if !self.hovered && self.deadline_ms.is_some_and(|deadline| now_ms >= deadline) {
            self.hide();
        }
    }

    /// Advances a playing track locally between sparse MPRIS position signals.
    /// The next service snapshot resets the anchor, so seeks and pauses remain
    /// authoritative and the position never exceeds the advertised duration.
    pub(crate) fn advance(&mut self, now_ms: u64) -> bool {
        let Some(content) = self.visible.as_mut() else {
            return false;
        };
        if content.playback != PlaybackStatus::Playing {
            return false;
        }
        let (Some(position), Some(duration), Some(anchor)) = (
            content.position_micros,
            content.duration_micros,
            self.position_anchor_ms,
        ) else {
            return false;
        };
        if duration == 0 {
            return false;
        }
        let elapsed_micros = u128::from(now_ms.saturating_sub(anchor)).saturating_mul(1_000);
        let next = u128::from(position)
            .saturating_add(elapsed_micros)
            .min(u128::from(duration)) as u64;
        self.position_anchor_ms = Some(now_ms);
        if next == position {
            return false;
        }
        content.position_micros = Some(next);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub(crate) fn set_hovered(&mut self, hovered: bool, now_ms: u64, timeout_ms: u64) {
        if !self.visible() || self.hovered == hovered {
            return;
        }
        self.hovered = hovered;
        if !hovered {
            self.deadline_ms = Some(now_ms.saturating_add(timeout_ms));
        }
    }

    pub(crate) fn update_policy(&mut self, enabled: bool, now_ms: u64, timeout_ms: u64) {
        if !enabled {
            self.hide();
        } else if self.visible() {
            self.deadline_ms = Some(now_ms.saturating_add(timeout_ms));
        }
    }

    pub(crate) fn dismiss(&mut self) {
        self.hide();
    }

    fn observe_content(
        &mut self,
        content: Option<MediaOsdContent>,
        now_ms: u64,
        enabled: bool,
        timeout_ms: u64,
    ) {
        let Some(content) = content else {
            self.observed = None;
            self.hide();
            return;
        };
        let trigger = MediaOsdTrigger::from(&content);
        let initial = self.observed.is_none();
        let changed = self
            .observed
            .as_ref()
            .is_some_and(|observed| observed != &trigger);
        self.observed = Some(trigger);
        self.position_anchor_ms = Some(now_ms);

        if !enabled || content.title.is_empty() {
            self.hide();
        } else if !initial {
            if changed {
                self.show(content, now_ms, timeout_ms);
            } else if self
                .visible
                .as_ref()
                .is_some_and(|visible| visible != &content)
            {
                self.visible = Some(content);
                self.revision = self.revision.wrapping_add(1);
            }
        }
    }

    fn show(&mut self, content: MediaOsdContent, now_ms: u64, timeout_ms: u64) {
        if self.visible.as_ref() != Some(&content) {
            self.visible = Some(content);
            self.revision = self.revision.wrapping_add(1);
        }
        self.deadline_ms = Some(now_ms.saturating_add(timeout_ms));
    }

    fn hide(&mut self) {
        if self.visible.take().is_some() {
            self.revision = self.revision.wrapping_add(1);
        }
        self.deadline_ms = None;
        self.position_anchor_ms = None;
        self.hovered = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(title: &str, playback: PlaybackStatus) -> MediaOsdContent {
        MediaOsdContent {
            title: title.into(),
            artists: vec!["Artist".into()],
            album: Some("Album".into()),
            position_micros: Some(30),
            duration_micros: Some(100),
            playback,
            previous: true,
            play_pause: true,
            next: false,
        }
    }

    #[test]
    fn first_snapshot_is_a_baseline_and_track_change_expires_exactly() {
        let mut state = MediaOsdState::default();
        state.observe_content(
            Some(content("First", PlaybackStatus::Playing)),
            100,
            true,
            3_000,
        );
        assert!(!state.visible());
        state.observe_content(
            Some(content("Second", PlaybackStatus::Playing)),
            200,
            true,
            3_000,
        );
        assert_eq!(
            state.content().map(|content| content.title.as_str()),
            Some("Second")
        );
        state.expire(3_199);
        assert!(state.visible());
        state.expire(3_200);
        assert!(!state.visible());
    }

    #[test]
    fn playback_change_triggers_but_capability_change_only_refreshes_content() {
        let mut state = MediaOsdState::default();
        state.observe_content(
            Some(content("Track", PlaybackStatus::Paused)),
            0,
            true,
            3_000,
        );
        state.observe_content(
            Some(content("Track", PlaybackStatus::Playing)),
            10,
            true,
            3_000,
        );
        let revision = state.revision();
        let mut capability = content("Track", PlaybackStatus::Playing);
        capability.next = true;
        state.observe_content(Some(capability), 20, true, 3_000);
        assert!(state.revision() > revision);
        assert!(state.content().unwrap().supports(MprisAction::Next));
        state.expire(3_010);
        assert!(!state.visible());
    }

    #[test]
    fn hover_pauses_and_leave_restarts_the_full_timeout() {
        let mut state = MediaOsdState::default();
        state.observe_content(
            Some(content("First", PlaybackStatus::Playing)),
            0,
            true,
            3_000,
        );
        state.observe_content(
            Some(content("Second", PlaybackStatus::Playing)),
            10,
            true,
            3_000,
        );
        state.set_hovered(true, 100, 3_000);
        state.expire(10_000);
        assert!(state.visible());
        state.set_hovered(false, 10_000, 3_000);
        state.expire(12_999);
        assert!(state.visible());
        state.expire(13_000);
        assert!(!state.visible());
    }

    #[test]
    fn blank_metadata_and_disabled_policy_never_show() {
        let mut state = MediaOsdState::default();
        state.observe_content(
            Some(content("Track", PlaybackStatus::Playing)),
            0,
            true,
            3_000,
        );
        state.observe_content(Some(content("", PlaybackStatus::Paused)), 10, true, 3_000);
        assert!(!state.visible());
        state.observe_content(
            Some(content("Visible", PlaybackStatus::Playing)),
            20,
            true,
            3_000,
        );
        assert!(state.visible());
        state.observe_content(Some(content("", PlaybackStatus::Playing)), 30, true, 3_000);
        assert!(!state.visible());
        state.observe_content(
            Some(content("Another", PlaybackStatus::Playing)),
            40,
            false,
            3_000,
        );
        assert!(!state.visible());
    }

    #[test]
    fn playing_progress_advances_until_duration_without_extending_osd_deadline() {
        let mut state = MediaOsdState::default();
        state.observe_content(
            Some(content("Baseline", PlaybackStatus::Playing)),
            0,
            true,
            3_000,
        );
        let playing = MediaOsdContent {
            title: "Track".into(),
            position_micros: Some(1_000_000),
            duration_micros: Some(10_000_000),
            ..content("Track", PlaybackStatus::Playing)
        };
        state.observe_content(Some(playing), 100, true, 3_000);
        let revision = state.revision();
        assert!(state.advance(600));
        assert!(state.revision() > revision);
        assert_eq!(state.content().unwrap().position_micros, Some(1_500_000));
        state.advance(20_000);
        assert_eq!(state.content().unwrap().position_micros, Some(10_000_000));
        state.expire(3_099);
        assert!(state.visible());
        state.expire(3_100);
        assert!(!state.visible());
    }

    #[test]
    fn policy_updates_hide_immediately_and_retime_only_an_existing_osd() {
        let mut state = MediaOsdState::default();
        state.observe_content(
            Some(content("Baseline", PlaybackStatus::Playing)),
            0,
            true,
            3_000,
        );
        state.update_policy(true, 10, 500);
        assert!(!state.visible(), "enabling must not synthesize an OSD");

        state.observe_content(
            Some(content("Changed", PlaybackStatus::Playing)),
            20,
            true,
            3_000,
        );
        state.update_policy(true, 100, 500);
        state.expire(599);
        assert!(state.visible());
        state.expire(600);
        assert!(!state.visible());

        state.observe_content(
            Some(content("Another", PlaybackStatus::Playing)),
            700,
            true,
            3_000,
        );
        assert!(state.visible());
        state.update_policy(false, 701, 3_000);
        assert!(!state.visible());
    }
}

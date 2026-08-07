mod service;

use std::sync::Arc;

pub use service::MediaServiceError;
pub(crate) use service::MediaServiceHandle;

use tensor_dbus::freedesktop::mpris::{MprisAction, MprisPlayerSnapshot, PlaybackStatus};

use crate::{PanelAppletEmphasis, PanelAppletState};

/// Retained product-facing lifecycle for session MPRIS state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum MediaServiceSnapshot {
    #[default]
    Pending,
    Ready(Option<Arc<MprisPlayerSnapshot>>),
    Unavailable,
    Failed,
}

impl MediaServiceSnapshot {
    pub fn player(&self) -> Option<&MprisPlayerSnapshot> {
        match self {
            Self::Ready(Some(player)) => Some(player.as_ref()),
            Self::Pending | Self::Ready(None) | Self::Unavailable | Self::Failed => None,
        }
    }

    pub fn supports(&self, action: MprisAction) -> bool {
        self.player().is_some_and(|player| player.supports(action))
    }

    pub fn panel_state(&self) -> PanelAppletState {
        match self {
            Self::Pending => PanelAppletState::pending(),
            Self::Ready(Some(player)) => media_applet_state(player.as_ref()),
            Self::Ready(None) => PanelAppletState::ready(),
            Self::Unavailable => PanelAppletState::unavailable(),
            Self::Failed => PanelAppletState::failed(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MediaActionState {
    #[default]
    Idle,
    Pending(MprisAction),
    Succeeded(MprisAction),
    Failed(MprisAction),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MediaServiceStore {
    snapshot: MediaServiceSnapshot,
    action: MediaActionState,
    revision: u64,
}

impl MediaServiceStore {
    pub(crate) fn publish_snapshot(&mut self, snapshot: MediaServiceSnapshot) -> bool {
        if self.snapshot == snapshot {
            return false;
        }
        self.snapshot = snapshot;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub(crate) fn publish_external_snapshot(&mut self, snapshot: MediaServiceSnapshot) -> bool {
        let changed = self.publish_snapshot(snapshot);
        if changed {
            self.publish_action(MediaActionState::Idle);
        }
        changed
    }

    pub(crate) fn publish_action(&mut self, action: MediaActionState) -> bool {
        if self.action == action {
            return false;
        }
        self.action = action;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub(crate) fn begin_action(&mut self, action: MprisAction) -> Result<(), MediaServiceError> {
        if matches!(self.action, MediaActionState::Pending(_)) {
            return Err(MediaServiceError::Busy);
        }
        let Some(player) = self.snapshot.player() else {
            return Err(MediaServiceError::Unavailable);
        };
        if !player.supports(action) {
            return Err(MediaServiceError::Unsupported(action));
        }
        self.publish_action(MediaActionState::Pending(action));
        Ok(())
    }

    pub(crate) fn read(&self) -> (u64, MediaServiceSnapshot, MediaActionState) {
        (self.revision, self.snapshot.clone(), self.action)
    }
}

/// Lowers one validated MPRIS player into the bounded panel applet ABI.
pub fn media_applet_state(player: &MprisPlayerSnapshot) -> PanelAppletState {
    playback_applet_state(player.playback())
}

const fn playback_applet_state(playback: PlaybackStatus) -> PanelAppletState {
    let emphasis = match playback {
        PlaybackStatus::Playing => PanelAppletEmphasis::Active,
        PlaybackStatus::Paused | PlaybackStatus::Stopped => PanelAppletEmphasis::Normal,
    };
    PanelAppletState::ready().with_emphasis(emphasis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PanelAppletAvailability;

    #[test]
    fn lifecycle_distinguishes_no_player_from_an_unavailable_bus() {
        assert_eq!(
            MediaServiceSnapshot::Ready(None)
                .panel_state()
                .availability(),
            PanelAppletAvailability::Ready
        );
        assert_eq!(
            MediaServiceSnapshot::Unavailable
                .panel_state()
                .availability(),
            PanelAppletAvailability::Unavailable
        );
    }

    #[test]
    fn playing_is_the_only_active_panel_state() {
        assert_eq!(
            playback_applet_state(PlaybackStatus::Playing).emphasis(),
            PanelAppletEmphasis::Active
        );
        assert_eq!(
            playback_applet_state(PlaybackStatus::Paused).emphasis(),
            PanelAppletEmphasis::Normal
        );
    }

    #[test]
    fn duplicate_snapshots_do_not_advance_revision() {
        let mut store = MediaServiceStore::default();
        assert!(!store.publish_snapshot(MediaServiceSnapshot::Pending));
        assert!(store.publish_snapshot(MediaServiceSnapshot::Ready(None)));
        assert!(!store.publish_snapshot(MediaServiceSnapshot::Ready(None)));
        assert_eq!(store.read().0, 1);
    }

    #[test]
    fn no_player_supports_no_transport_action() {
        let snapshot = MediaServiceSnapshot::Ready(None);
        assert!(!snapshot.supports(MprisAction::Previous));
        assert!(!snapshot.supports(MprisAction::PlayPause));
        assert!(!snapshot.supports(MprisAction::Next));
    }

    #[test]
    fn external_snapshot_change_clears_stale_action_feedback() {
        let mut store = MediaServiceStore::default();
        store.publish_action(MediaActionState::Succeeded(MprisAction::Next));
        assert!(store.publish_external_snapshot(MediaServiceSnapshot::Ready(None)));
        assert_eq!(store.action, MediaActionState::Idle);

        store.publish_action(MediaActionState::Failed(MprisAction::Previous));
        assert!(!store.publish_external_snapshot(MediaServiceSnapshot::Ready(None)));
        assert_eq!(
            store.action,
            MediaActionState::Failed(MprisAction::Previous)
        );
    }

    #[test]
    fn action_reservation_rejects_a_missing_player_without_mutating_state() {
        let mut store = MediaServiceStore::default();
        assert_eq!(
            store.begin_action(MprisAction::PlayPause),
            Err(MediaServiceError::Unavailable)
        );
        assert_eq!(store.action, MediaActionState::Idle);
        assert_eq!(store.revision, 0);
    }
}

use thiserror::Error;
use xkbcommon::xkb::{self, keysyms};

use crate::media::MediaAction;

pub(crate) const MAX_MEDIA_KEYSYM_NAME_BYTES: usize = 128;

/// Tensorland-owned mapping from physical media keysyms to Shell actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MediaKeyConfig {
    enabled: bool,
    previous: u32,
    play_pause: u32,
    next: u32,
}

impl MediaKeyConfig {
    pub(crate) const fn action_for(self, keysym: u32) -> Option<MediaAction> {
        if !self.enabled {
            return None;
        }
        if keysym == self.previous {
            Some(MediaAction::Previous)
        } else if keysym == self.play_pause {
            Some(MediaAction::PlayPause)
        } else if keysym == self.next {
            Some(MediaAction::Next)
        } else {
            None
        }
    }
}

impl Default for MediaKeyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            previous: keysyms::KEY_XF86AudioPrev,
            play_pause: keysyms::KEY_XF86AudioPlay,
            next: keysyms::KEY_XF86AudioNext,
        }
    }
}

pub(super) fn resolve(
    enabled: Option<bool>,
    previous: Option<String>,
    play_pause: Option<String>,
    next: Option<String>,
) -> Result<MediaKeyConfig, MediaKeyConfigError> {
    let defaults = MediaKeyConfig::default();
    let previous = parse_keysym("previous", previous, defaults.previous)?;
    let play_pause = parse_keysym("play-pause", play_pause, defaults.play_pause)?;
    let next = parse_keysym("next", next, defaults.next)?;
    if previous == play_pause || previous == next || play_pause == next {
        return Err(MediaKeyConfigError::DuplicateKeysym);
    }
    Ok(MediaKeyConfig {
        enabled: enabled.unwrap_or(defaults.enabled),
        previous,
        play_pause,
        next,
    })
}

fn parse_keysym(
    action: &'static str,
    configured: Option<String>,
    default: u32,
) -> Result<u32, MediaKeyConfigError> {
    let Some(name) = configured else {
        return Ok(default);
    };
    let bytes = name.len();
    if bytes == 0 || bytes > MAX_MEDIA_KEYSYM_NAME_BYTES || name.as_bytes().contains(&0) {
        return Err(MediaKeyConfigError::InvalidKeysymName {
            action,
            name,
            maximum: MAX_MEDIA_KEYSYM_NAME_BYTES,
        });
    }
    let keysym = xkb::keysym_from_name(&name, xkb::KEYSYM_NO_FLAGS).raw();
    if keysym == keysyms::KEY_NoSymbol {
        return Err(MediaKeyConfigError::UnknownKeysym { action, name });
    }
    Ok(keysym)
}

#[derive(Debug, Error)]
pub enum MediaKeyConfigError {
    #[error("media key for {action} must contain 1..={maximum} non-NUL bytes, got {name:?}")]
    InvalidKeysymName {
        action: &'static str,
        name: String,
        maximum: usize,
    },
    #[error("media key for {action} names an unknown XKB keysym {name:?}")]
    UnknownKeysym { action: &'static str, name: String },
    #[error("previous, play-pause, and next must use distinct media keysyms")]
    DuplicateKeysym,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_media_keys_are_standard_xf86_keysyms() {
        let config = MediaKeyConfig::default();
        assert_eq!(
            config.action_for(keysyms::KEY_XF86AudioPrev),
            Some(MediaAction::Previous)
        );
        assert_eq!(
            config.action_for(keysyms::KEY_XF86AudioPlay),
            Some(MediaAction::PlayPause)
        );
        assert_eq!(
            config.action_for(keysyms::KEY_XF86AudioNext),
            Some(MediaAction::Next)
        );
    }
}

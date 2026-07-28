//! Session activity detection through systemd-logind when available.

use std::env;
use std::ffi::OsString;
use std::io;
use std::process::Command;

const SESSION_STATE_OVERRIDE: &str = "GILDER_SESSION_STATE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionState {
    pub active: bool,
    pub locked: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            active: true,
            locked: false,
        }
    }
}

pub fn read_session_state() -> SessionState {
    if let Some(state) = read_session_state_override() {
        return state;
    }
    read_session_state_from_env(env::var_os("XDG_SESSION_ID")).unwrap_or_default()
}

fn read_session_state_override() -> Option<SessionState> {
    env::var(SESSION_STATE_OVERRIDE)
        .ok()
        .and_then(|value| parse_session_state_override(&value))
}

fn parse_session_state_override(value: &str) -> Option<SessionState> {
    match value.trim().to_ascii_lowercase().as_str() {
        "active" | "unlocked" | "focused" => Some(SessionState::default()),
        "inactive" | "background" => Some(SessionState {
            active: false,
            locked: false,
        }),
        "locked" | "lock" | "screen-locked" => Some(SessionState {
            active: true,
            locked: true,
        }),
        "inactive-locked" | "locked-inactive" => Some(SessionState {
            active: false,
            locked: true,
        }),
        "" | "auto" | "logind" => None,
        _ => None,
    }
}

fn read_session_state_from_env(session_id: Option<OsString>) -> io::Result<SessionState> {
    let Some(session_id) = session_id.and_then(|value| value.into_string().ok()) else {
        return Ok(SessionState::default());
    };
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Ok(SessionState::default());
    }
    read_logind_session_state(session_id)
}

fn read_logind_session_state(session_id: &str) -> io::Result<SessionState> {
    let output = Command::new("loginctl")
        .args([
            "show-session",
            session_id,
            "-p",
            "Active",
            "-p",
            "LockedHint",
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let message = if stderr.is_empty() {
            format!("loginctl exited with status {}", output.status)
        } else {
            stderr
        };
        return Err(io::Error::other(message));
    }

    parse_logind_session_state(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid logind session state"))
}

fn parse_logind_session_state(value: &str) -> Option<SessionState> {
    let mut active = None;
    let mut locked = None;
    for line in value.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Active" => active = parse_bool_value(value),
            "LockedHint" => locked = parse_bool_value(value),
            _ => {}
        }
    }

    Some(SessionState {
        active: active?,
        locked: locked.unwrap_or(false),
    })
}

fn parse_bool_value(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" => Some(true),
        "no" | "false" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treats_missing_session_id_as_active_and_unlocked() {
        assert_eq!(
            read_session_state_from_env(None).unwrap(),
            SessionState::default()
        );
    }

    #[test]
    fn treats_empty_session_id_as_active_and_unlocked() {
        assert_eq!(
            read_session_state_from_env(Some(OsString::from("  "))).unwrap(),
            SessionState::default()
        );
    }

    #[test]
    fn parses_session_state_overrides() {
        assert_eq!(
            parse_session_state_override("active"),
            Some(SessionState::default())
        );
        assert_eq!(
            parse_session_state_override("inactive"),
            Some(SessionState {
                active: false,
                locked: false,
            })
        );
        assert_eq!(
            parse_session_state_override("locked"),
            Some(SessionState {
                active: true,
                locked: true,
            })
        );
        assert_eq!(
            parse_session_state_override("inactive-locked"),
            Some(SessionState {
                active: false,
                locked: true,
            })
        );
        assert_eq!(parse_session_state_override("auto"), None);
        assert_eq!(parse_session_state_override("unknown"), None);
    }

    #[test]
    fn parses_logind_session_state() {
        assert_eq!(
            parse_logind_session_state("Active=yes\nLockedHint=no\n"),
            Some(SessionState::default())
        );
        assert_eq!(
            parse_logind_session_state("Active=true\nLockedHint=1\n"),
            Some(SessionState {
                active: true,
                locked: true,
            })
        );
        assert_eq!(
            parse_logind_session_state("LockedHint=yes\nActive=no\n"),
            Some(SessionState {
                active: false,
                locked: true,
            })
        );
        assert_eq!(
            parse_logind_session_state("Active=yes\n"),
            Some(SessionState::default())
        );
        assert_eq!(parse_logind_session_state("LockedHint=no\n"), None);
        assert_eq!(parse_logind_session_state("Active=unknown\n"), None);
    }
}

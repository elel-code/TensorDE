//! Session activity detection through systemd-logind when available.

use std::env;
use std::ffi::OsString;
use std::io;
use std::process::Command;

pub fn read_session_active() -> bool {
    read_session_active_from_env(env::var_os("XDG_SESSION_ID")).unwrap_or(true)
}

fn read_session_active_from_env(session_id: Option<OsString>) -> io::Result<bool> {
    let Some(session_id) = session_id.and_then(|value| value.into_string().ok()) else {
        return Ok(true);
    };
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Ok(true);
    }
    read_logind_session_active(session_id)
}

fn read_logind_session_active(session_id: &str) -> io::Result<bool> {
    let output = Command::new("loginctl")
        .args(["show-session", session_id, "-p", "Active", "--value"])
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_logind_active_value(&stdout)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid logind Active value"))
}

fn parse_logind_active_value(value: &str) -> Option<bool> {
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
    fn treats_missing_session_id_as_active() {
        assert!(read_session_active_from_env(None).unwrap());
    }

    #[test]
    fn treats_empty_session_id_as_active() {
        assert!(read_session_active_from_env(Some(OsString::from("  "))).unwrap());
    }

    #[test]
    fn parses_logind_active_values() {
        assert_eq!(parse_logind_active_value("yes\n"), Some(true));
        assert_eq!(parse_logind_active_value("true"), Some(true));
        assert_eq!(parse_logind_active_value("1"), Some(true));
        assert_eq!(parse_logind_active_value("no\n"), Some(false));
        assert_eq!(parse_logind_active_value("false"), Some(false));
        assert_eq!(parse_logind_active_value("0"), Some(false));
        assert_eq!(parse_logind_active_value("unknown"), None);
    }
}

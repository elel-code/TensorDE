use std::{ffi::OsStr, fmt::Write as _, os::unix::ffi::OsStrExt, path::Path, sync::OnceLock};

use thiserror::Error;
use zbus::zvariant::{OwnedObjectPath, Value};

const DESTINATION: &str = "org.freedesktop.systemd1";
const PATH: &str = "/org/freedesktop/systemd1";
const INTERFACE: &str = "org.freedesktop.systemd1.Manager";

pub(super) fn start(
    command: &OsStr,
    intermediate_pid: u32,
    client_pid: u32,
) -> Result<(), ScopeError> {
    let connection = connection()?;
    let proxy = zbus::blocking::Proxy::new(connection, DESTINATION, PATH, INTERFACE)?;
    let signals = proxy.receive_signal("JobRemoved")?;

    let scope_name = scope_name(command, client_pid);
    let pids = [intermediate_pid, client_pid];
    let properties = [
        ("PIDs", Value::new(pids.as_slice())),
        ("CollectMode", Value::new("inactive-or-failed")),
    ];
    let aux: &[(&str, &[(&str, Value<'_>)])] = &[];
    let job: OwnedObjectPath = proxy.call(
        "StartTransientUnit",
        &(scope_name.as_str(), "fail", properties.as_slice(), aux),
    )?;

    for message in signals {
        let body = message.body();
        let (_, path, unit, result): (u32, OwnedObjectPath, &str, &str) = body.deserialize()?;
        if path == job {
            if result == "done" {
                return Ok(());
            }
            return Err(ScopeError::JobFailed {
                unit: unit.to_owned(),
                result: result.to_owned(),
            });
        }
    }
    Err(ScopeError::SignalStreamEnded)
}

fn connection() -> Result<&'static zbus::blocking::Connection, ScopeError> {
    static CONNECTION: OnceLock<Result<zbus::blocking::Connection, String>> = OnceLock::new();
    CONNECTION
        .get_or_init(|| zbus::blocking::Connection::session().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|message| ScopeError::Connection(message.clone()))
}

fn scope_name(command: &OsStr, client_pid: u32) -> String {
    let basename = Path::new(command).file_name().unwrap_or(command);
    let mut name = String::from("app-tensor-");
    for &byte in basename.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.') {
            name.push(char::from(byte));
        } else {
            let _ = write!(name, "\\x{byte:02x}");
        }
    }
    let _ = write!(name, "-{client_pid}.scope");
    name
}

#[derive(Debug, Error)]
pub enum ScopeError {
    #[error("failed to connect to the user D-Bus: {0}")]
    Connection(String),
    #[error("systemd D-Bus request failed: {0}")]
    Dbus(#[from] zbus::Error),
    #[error("systemd job for {unit} failed with result {result}")]
    JobFailed { unit: String, result: String },
    #[error("systemd JobRemoved signal stream ended before the client scope was ready")]
    SignalStreamEnded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_name_escapes_unit_sensitive_bytes() {
        assert_eq!(
            scope_name(OsStr::new("/usr/bin/app name@beta"), 42),
            "app-tensor-app\\x20name\\x40beta-42.scope"
        );
    }

    #[test]
    fn scope_name_keeps_systemd_safe_bytes() {
        assert_eq!(
            scope_name(OsStr::new("org.example_App:1.0"), 7),
            "app-tensor-org.example_App:1.0-7.scope"
        );
    }
}

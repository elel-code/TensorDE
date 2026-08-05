use std::{ffi::OsStr, fmt::Write as _, os::unix::ffi::OsStrExt, path::Path};

use tensor_dbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, Value},
};
use thiserror::Error;

const DESTINATION: &str = "org.freedesktop.systemd1";
const PATH: &str = "/org/freedesktop/systemd1";
const INTERFACE: &str = "org.freedesktop.systemd1.Manager";

pub(super) async fn start(
    connection: &mut Connection,
    command: &OsStr,
    intermediate_pid: u32,
    client_pid: u32,
) -> Result<(), ScopeError> {
    let mut proxy = Proxy::new(connection, Some(DESTINATION), PATH, Some(INTERFACE))?;
    let job_removed = proxy.subscribe("JobRemoved").await?;

    let scope_name = scope_name(command, client_pid);
    let pids = [intermediate_pid, client_pid];
    let properties = [
        ("PIDs", Value::new(pids.as_slice())),
        ("CollectMode", Value::new("inactive-or-failed")),
    ];
    let aux: &[(&str, &[(&str, Value<'_>)])] = &[];
    let job: OwnedObjectPath = proxy
        .call(
            "StartTransientUnit",
            &(scope_name.as_str(), "fail", properties.as_slice(), aux),
        )
        .await?;

    let mut signals = proxy.signal_stream(job_removed);
    loop {
        let message = signals.next().await?;
        let (_, path, unit, result): (u32, OwnedObjectPath, String, String) = message.body()?;
        if path == job {
            let _ = signals.close().await?;
            if result == "done" {
                return Ok(());
            }
            return Err(ScopeError::JobFailed { unit, result });
        }
    }
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
    #[error("systemd D-Bus request failed: {0}")]
    Dbus(#[from] tensor_dbus::Error),
    #[error("systemd job for {unit} failed with result {result}")]
    JobFailed { unit: String, result: String },
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

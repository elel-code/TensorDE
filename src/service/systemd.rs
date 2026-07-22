use std::{
    ffi::OsString,
    io,
    process::{Command, ExitStatus},
};

use thiserror::Error;
use tracing::warn;

use super::policy::EnvironmentValue;

const SESSION_ENVIRONMENT: &[&str] = &[
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "TENSOR_IPC_SOCKET",
];

pub fn notify_ready() -> io::Result<()> {
    sd_notify::notify(&[
        sd_notify::NotifyState::Ready,
        sd_notify::NotifyState::Status("Tensor compositor initialized"),
    ])
}

pub fn import_environment(values: &[EnvironmentValue]) -> Result<(), SystemdError> {
    let mut args = vec![OsString::from("--user"), OsString::from("set-environment")];
    args.extend(values.iter().map(|(name, value)| {
        let mut item = name.clone();
        item.push("=");
        item.push(value);
        item
    }));
    run_systemctl(&args)?;

    let mut dbus_args = Vec::new();
    dbus_args.extend(values.iter().map(|(name, value)| {
        let mut item = name.clone();
        item.push("=");
        item.push(value);
        item
    }));
    match Command::new("dbus-update-activation-environment")
        .args(&dbus_args)
        .status()
    {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => {
            warn!(%status, "D-Bus activation environment update failed");
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(SystemdError::Command {
            command: "dbus-update-activation-environment",
            source,
        }),
    }
}

pub fn unset_environment() -> Result<(), SystemdError> {
    let args = [
        OsString::from("--user"),
        OsString::from("unset-environment"),
    ]
    .into_iter()
    .chain(SESSION_ENVIRONMENT.iter().map(OsString::from))
    .collect::<Vec<_>>();
    run_systemctl(&args)
}

fn run_systemctl(args: &[OsString]) -> Result<(), SystemdError> {
    let command = "systemctl --user";
    let status = Command::new("systemctl")
        .args(args)
        .status()
        .map_err(|source| SystemdError::Command { command, source })?;
    if status.success() {
        Ok(())
    } else {
        Err(SystemdError::CommandFailed { command, status })
    }
}

#[derive(Debug, Error)]
pub enum SystemdError {
    #[error("failed to execute {command}: {source}")]
    Command {
        command: &'static str,
        source: io::Error,
    },
    #[error("{command} exited unsuccessfully: {status}")]
    CommandFailed {
        command: &'static str,
        status: ExitStatus,
    },
}

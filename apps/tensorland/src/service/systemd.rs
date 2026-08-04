use std::{
    ffi::{OsStr, OsString},
    io,
    process::{Command, ExitStatus},
};

use thiserror::Error;
use tracing::warn;

use super::policy::{EnvironmentValue, SESSION_ENVIRONMENT_NAMES};

pub fn notify_ready() -> io::Result<()> {
    sd_notify::notify(&[
        sd_notify::NotifyState::Ready,
        sd_notify::NotifyState::Status("Tensor compositor initialized"),
    ])
}

/// Tell the user manager that orderly shutdown has begun (Hyprland-style STOPPING).
pub fn notify_stopping() -> io::Result<()> {
    sd_notify::notify(&[
        sd_notify::NotifyState::Stopping,
        sd_notify::NotifyState::Status("Tensor compositor stopping"),
    ])
}

pub struct ImportedEnvironment {
    _private: (),
}

impl Drop for ImportedEnvironment {
    fn drop(&mut self) {
        if let Err(error) = unset_environment() {
            warn!(%error, "failed to clear session environment");
        }
        if let Err(error) = update_activation_environment(&[]) {
            warn!(%error, "failed to clear D-Bus activation environment");
        }
    }
}

pub fn import_environment(
    values: &[EnvironmentValue],
) -> Result<ImportedEnvironment, SystemdError> {
    unset_environment()?;
    let imported = ImportedEnvironment { _private: () };
    let mut args = vec![OsString::from("--user"), OsString::from("set-environment")];
    args.extend(values.iter().map(|(name, value)| {
        let mut item = name.clone();
        item.push("=");
        item.push(value);
        item
    }));
    run_systemctl(&args)?;

    update_activation_environment(values)?;
    Ok(imported)
}

fn update_activation_environment(values: &[EnvironmentValue]) -> Result<(), SystemdError> {
    let args = activation_environment_args(values);
    match Command::new("dbus-update-activation-environment")
        .args(&args)
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

fn activation_environment_args(values: &[EnvironmentValue]) -> Vec<OsString> {
    SESSION_ENVIRONMENT_NAMES
        .iter()
        .map(|name| {
            let value = values
                .iter()
                .find(|(candidate, _)| candidate == OsStr::new(name))
                .map(|(_, value)| value.as_os_str())
                .unwrap_or_default();
            let mut item = OsString::from(name);
            item.push("=");
            item.push(value);
            item
        })
        .collect()
}

fn unset_environment() -> Result<(), SystemdError> {
    let args = [
        OsString::from("--user"),
        OsString::from("unset-environment"),
    ]
    .into_iter()
    .chain(SESSION_ENVIRONMENT_NAMES.iter().map(OsString::from))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_snapshot_clears_missing_managed_values() {
        let values = [(
            OsString::from("WAYLAND_DISPLAY"),
            OsString::from("tensor-0"),
        )];
        let args = activation_environment_args(&values);

        assert!(args.contains(&OsString::from("WAYLAND_DISPLAY=tensor-0")));
        assert!(args.contains(&OsString::from("DISPLAY=")));
        assert!(args.contains(&OsString::from("TENSOR_IPC_SOCKET=")));
    }
}

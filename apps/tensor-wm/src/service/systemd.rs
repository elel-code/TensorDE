use std::{
    collections::HashMap,
    env,
    ffi::{OsStr, OsString},
    io,
    process::{Command, ExitStatus},
};

use tensor_dbus::Connection;
use tensor_runtime::io_uring_runtime;
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

pub fn import_process_activation_environment() -> Result<(), SystemdError> {
    publish_activation_environment(process_environment_values()?)
}

fn update_activation_environment(values: &[EnvironmentValue]) -> Result<(), SystemdError> {
    publish_activation_environment(activation_environment_values(values)?)
}

fn publish_activation_environment(
    environment: HashMap<String, String>,
) -> Result<(), SystemdError> {
    let runtime = io_uring_runtime(2).map_err(SystemdError::Runtime)?;
    if let Err(error) = runtime.block_on(async {
        let mut connection = Connection::session_bus().await?;
        update_activation_environment_on(&mut connection, &environment).await
    }) {
        warn!(%error, "D-Bus activation environment update failed");
    }
    Ok(())
}

async fn update_activation_environment_on(
    connection: &mut Connection,
    environment: &HashMap<String, String>,
) -> tensor_dbus::Result<()> {
    connection
        .call(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "UpdateActivationEnvironment",
            environment,
        )
        .await
}

fn activation_environment_values(
    values: &[EnvironmentValue],
) -> Result<HashMap<String, String>, SystemdError> {
    SESSION_ENVIRONMENT_NAMES
        .iter()
        .map(|name| {
            let value = values
                .iter()
                .find(|(candidate, _)| candidate == OsStr::new(name))
                .map(|(_, value)| value.as_os_str())
                .unwrap_or_default();
            let value = value
                .to_str()
                .ok_or_else(|| SystemdError::NonUnicodeEnvironment {
                    name: OsString::from(name),
                })?;
            Ok(((*name).to_owned(), value.to_owned()))
        })
        .collect()
}

fn process_environment_values() -> Result<HashMap<String, String>, SystemdError> {
    env::vars_os()
        .map(|(name, value)| {
            let name = name
                .into_string()
                .map_err(|name| SystemdError::NonUnicodeEnvironment { name })?;
            let value = value
                .into_string()
                .map_err(|_| SystemdError::NonUnicodeEnvironment {
                    name: OsString::from(&name),
                })?;
            Ok((name, value))
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
    #[error("failed to create the Compio runtime for D-Bus activation: {0}")]
    Runtime(#[source] io::Error),
    #[error("D-Bus activation environment value {name:?} is not valid UTF-8")]
    NonUnicodeEnvironment { name: OsString },
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
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Stdio},
        sync::atomic::{AtomicU64, Ordering},
    };

    use tensor_dbus::BusAddress;

    use super::*;

    struct PrivateBus {
        child: Child,
        address: String,
    }

    impl PrivateBus {
        fn start() -> Option<Self> {
            let address = format!(
                "unix:abstract=tensorland-systemd-test-{}-{}",
                std::process::id(),
                next_bus_id()
            );
            let mut child = match Command::new("dbus-daemon")
                .args([
                    "--session",
                    "--nofork",
                    "--nopidfile",
                    "--print-address=1",
                    "--address",
                    &address,
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
            {
                Ok(child) => child,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
                Err(error) => panic!("failed to start private dbus-daemon: {error}"),
            };
            let mut announced = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut announced)
                .expect("dbus-daemon did not announce its address");
            assert!(announced.trim().starts_with(&address));
            Some(Self {
                child,
                address: announced.trim().to_owned(),
            })
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn next_bus_id() -> u64 {
        static NEXT_BUS: AtomicU64 = AtomicU64::new(1);
        NEXT_BUS.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn activation_snapshot_clears_missing_managed_values() {
        let values = [(
            OsString::from("WAYLAND_DISPLAY"),
            OsString::from("tensor-0"),
        )];
        let environment = activation_environment_values(&values).unwrap();

        assert_eq!(environment.get("WAYLAND_DISPLAY").unwrap(), "tensor-0");
        assert_eq!(environment.get("DISPLAY").unwrap(), "");
        assert_eq!(environment.get("TENSOR_IPC_SOCKET").unwrap(), "");
    }

    #[cfg(unix)]
    #[test]
    fn activation_snapshot_rejects_non_unicode_values() {
        use std::os::unix::ffi::OsStringExt;

        let values = [(
            OsString::from("WAYLAND_DISPLAY"),
            OsString::from_vec(vec![0xff]),
        )];
        assert!(matches!(
            activation_environment_values(&values),
            Err(SystemdError::NonUnicodeEnvironment { ref name })
                if name == "WAYLAND_DISPLAY"
        ));
    }

    #[test]
    fn activation_environment_is_sent_over_tensor_dbus() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipping private bus test: dbus-daemon is unavailable");
            return;
        };
        io_uring_runtime(2).unwrap().block_on(async {
            let mut connection = Connection::connect_bus(BusAddress::parse(&bus.address).unwrap())
                .await
                .unwrap();
            update_activation_environment_on(
                &mut connection,
                &HashMap::from([("TENSOR_TEST_VALUE".to_owned(), "active".to_owned())]),
            )
            .await
            .unwrap();
        });
    }
}

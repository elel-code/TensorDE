mod policy;

use std::path::PathBuf;

#[cfg(feature = "systemd")]
mod systemd;

pub(crate) use policy::SESSION_ENVIRONMENT_NAMES;
pub use policy::{EnvironmentValue, ParseSystemdModeError, SystemdMode, session_environment};

#[cfg(feature = "systemd")]
pub use systemd::{SystemdError, import_environment, notify_ready, notify_stopping};

pub fn configured_mode(
    cli_path: Option<PathBuf>,
) -> Result<SystemdMode, crate::config::ConfigError> {
    let path = crate::config::Config::resolve_path(cli_path);
    crate::config::Config::load_with_environment(&path).map(|config| config.systemd)
}

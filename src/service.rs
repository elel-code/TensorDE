mod policy;

use std::path::PathBuf;

#[cfg(feature = "systemd")]
mod systemd;

pub use policy::{ParseSystemdModeError, SystemdMode};

#[cfg(feature = "systemd")]
pub use policy::{EnvironmentValue, session_environment};

#[cfg(feature = "systemd")]
pub use systemd::{SystemdError, import_environment, notify_ready, unset_environment};

pub fn configured_mode(cli_path: Option<PathBuf>) -> Result<SystemdMode, String> {
    let path = crate::config::Config::resolve_path(cli_path);
    crate::config::Config::load_with_environment(&path)
        .map(|config| config.systemd)
        .map_err(|error| error.to_string())
}

use std::{env, ffi::OsString, str::FromStr};

use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SystemdMode {
    #[default]
    Auto,
    Enabled,
    Disabled,
}

impl SystemdMode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }

    pub fn active(self) -> bool {
        self.resolve(Self::detected())
    }

    pub const fn resolve(self, detected: bool) -> bool {
        match self {
            Self::Auto => detected,
            Self::Enabled => true,
            Self::Disabled => false,
        }
    }

    pub fn detected() -> bool {
        env::var_os("NOTIFY_SOCKET").is_some()
            || env::var_os("SYSTEMD_EXEC_PID").is_some()
            || env::var_os("MANAGERPID").is_some()
    }
}

impl FromStr for SystemdMode {
    type Err = ParseSystemdModeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "enabled" | "on" => Ok(Self::Enabled),
            "disabled" | "off" => Ok(Self::Disabled),
            _ => Err(ParseSystemdModeError(value.to_owned())),
        }
    }
}

pub type EnvironmentValue = (OsString, OsString);

pub(crate) const SESSION_ENVIRONMENT_NAMES: &[&str] = &[
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "TENSOR_IPC_SOCKET",
];

pub fn session_environment(
    wayland_display: impl Into<OsString>,
    ipc_socket: impl Into<OsString>,
    xwayland_display: Option<OsString>,
) -> Vec<EnvironmentValue> {
    let mut environment = vec![
        (OsString::from("WAYLAND_DISPLAY"), wayland_display.into()),
        (
            OsString::from("XDG_CURRENT_DESKTOP"),
            OsString::from("tensor"),
        ),
        (
            OsString::from("XDG_SESSION_TYPE"),
            OsString::from("wayland"),
        ),
        (OsString::from("TENSOR_IPC_SOCKET"), ipc_socket.into()),
    ];
    if let Some(display) = xwayland_display {
        environment.push((OsString::from("DISPLAY"), display));
    }
    environment
}

#[derive(Debug, Error, Eq, PartialEq)]
#[error("unknown systemd mode '{0}'; expected auto, enabled, or disabled")]
pub struct ParseSystemdModeError(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_modes() {
        assert_eq!("auto".parse(), Ok(SystemdMode::Auto));
        assert_eq!("on".parse(), Ok(SystemdMode::Enabled));
        assert_eq!("off".parse(), Ok(SystemdMode::Disabled));
    }

    #[test]
    fn disabled_mode_never_activates() {
        assert!(!SystemdMode::Disabled.active());
    }

    #[test]
    fn auto_mode_follows_detection() {
        assert!(SystemdMode::Auto.resolve(true));
        assert!(!SystemdMode::Auto.resolve(false));
        assert!(SystemdMode::Enabled.resolve(false));
        assert!(!SystemdMode::Disabled.resolve(true));
    }

    #[test]
    fn session_environment_includes_only_an_allocated_xwayland_display() {
        let without_xwayland = session_environment("wayland-1", "/tmp/tensor.sock", None);
        assert!(!without_xwayland.iter().any(|(name, _)| name == "DISPLAY"));

        let with_xwayland =
            session_environment("wayland-1", "/tmp/tensor.sock", Some(OsString::from(":7")));
        assert!(
            with_xwayland
                .iter()
                .any(|(name, value)| name == "DISPLAY" && value == ":7")
        );
    }
}

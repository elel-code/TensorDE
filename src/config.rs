use std::{
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use thiserror::Error;

use crate::{
    layout::LayoutKind,
    render::{GpuPreference, ParseGpuPreferenceError},
    service::{ParseSystemdModeError, SystemdMode},
    xwayland::XWaylandConfig,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub initial_layout: LayoutKind,
    pub ipc_socket: PathBuf,
    pub gpu_preference: GpuPreference,
    pub systemd: SystemdMode,
    pub xwayland: XWaylandConfig,
    pub startup_commands: Vec<StartupCommand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupCommand {
    pub argv: Vec<String>,
}

impl Config {
    pub fn load_with_environment(path: &Path) -> Result<Self, ConfigError> {
        let mut config = Self::load_or_default(&path)?;

        if let Some(layout) = env::var_os("TENSOR_LAYOUT") {
            let layout = layout.to_str().ok_or(ConfigError::NonUnicodeLayout)?;
            config.initial_layout = LayoutKind::from_str(layout)?;
        }
        if let Some(socket) = env::var_os("TENSOR_IPC_SOCKET") {
            config.ipc_socket = PathBuf::from(socket);
        }
        if let Some(preference) = env::var_os("TENSOR_GPU") {
            let preference = preference.to_str().ok_or(ConfigError::NonUnicodeGpu)?;
            config.gpu_preference = GpuPreference::from_str(preference)?;
        }
        if let Some(mode) = env::var_os("TENSOR_SYSTEMD") {
            let mode = mode.to_str().ok_or(ConfigError::NonUnicodeSystemd)?;
            config.systemd = SystemdMode::from_str(mode)?;
        }
        if let Some(enabled) = env::var_os("TENSOR_XWAYLAND") {
            config.xwayland = XWaylandConfig::from_environment(enabled.to_str())?;
        }

        Ok(config)
    }

    pub fn resolve_path(cli_path: Option<PathBuf>) -> PathBuf {
        cli_path
            .or_else(|| env::var_os("TENSOR_CONFIG").map(PathBuf::from))
            .or_else(|| {
                let config_dir =
                    env::var_os("XDG_CONFIG_HOME")
                        .map(PathBuf::from)
                        .or_else(|| {
                            env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
                        })?;
                Some(config_dir.join("tensor/config.kdl"))
            })
            .unwrap_or_else(|| PathBuf::from("/etc/tensor/config.kdl"))
    }

    pub fn load_or_default(path: &Path) -> Result<Self, ConfigError> {
        match fs::read_to_string(path) {
            Ok(document) => Self::from_kdl(path, &document),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    fn from_kdl(path: &Path, document: &str) -> Result<Self, ConfigError> {
        let path_name = path.to_string_lossy();
        let parsed: FileConfig =
            knus::parse(&path_name, document).map_err(|error| ConfigError::Parse {
                path: path.to_owned(),
                message: error.to_string(),
            })?;
        let initial_layout = parsed
            .layout
            .as_deref()
            .map(LayoutKind::from_str)
            .transpose()?
            .unwrap_or_default();
        let ipc_socket = parsed
            .ipc_socket
            .map(PathBuf::from)
            .unwrap_or_else(|| Self::default().ipc_socket);
        let gpu_preference = parsed
            .gpu
            .as_deref()
            .map(GpuPreference::from_str)
            .transpose()?
            .unwrap_or_default();
        let systemd = parsed
            .systemd
            .as_deref()
            .map(SystemdMode::from_str)
            .transpose()?
            .unwrap_or_default();
        let xwayland = parsed
            .xwayland
            .unwrap_or_else(|| XWaylandConfig::default().enabled());
        let startup_commands = parsed
            .spawn_at_startup
            .into_iter()
            .enumerate()
            .map(|(index, argv)| {
                if argv.is_empty() {
                    Err(ConfigError::EmptyStartupCommand { index })
                } else {
                    Ok(StartupCommand { argv })
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            initial_layout,
            ipc_socket,
            gpu_preference,
            systemd,
            xwayland: XWaylandConfig::new(xwayland),
            startup_commands,
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initial_layout: LayoutKind::default(),
            ipc_socket: env::var_os("XDG_RUNTIME_DIR")
                .map(|path| PathBuf::from(path).join("tensor.sock"))
                .unwrap_or_else(|| PathBuf::from("/tmp/tensor.sock")),
            gpu_preference: GpuPreference::default(),
            systemd: SystemdMode::default(),
            xwayland: XWaylandConfig::default(),
            startup_commands: Vec::new(),
        }
    }
}

#[derive(Debug, knus::Decode)]
struct FileConfig {
    #[knus(child, unwrap(argument))]
    layout: Option<String>,
    #[knus(child, unwrap(argument))]
    ipc_socket: Option<String>,
    #[knus(child, unwrap(argument))]
    gpu: Option<String>,
    #[knus(child, unwrap(argument))]
    systemd: Option<String>,
    #[knus(child, unwrap(argument))]
    xwayland: Option<bool>,
    #[knus(children(name = "spawn-at-startup"), unwrap(arguments))]
    spawn_at_startup: Vec<Vec<String>>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("TENSOR_LAYOUT is not valid Unicode")]
    NonUnicodeLayout,
    #[error("TENSOR_GPU is not valid Unicode")]
    NonUnicodeGpu,
    #[error("TENSOR_SYSTEMD is not valid Unicode")]
    NonUnicodeSystemd,
    #[error("spawn-at-startup entry {index} must contain a program")]
    EmptyStartupCommand { index: usize },
    #[error("failed to read config {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error(transparent)]
    UnknownLayout(#[from] crate::layout::ParseLayoutError),
    #[error(transparent)]
    UnknownGpu(#[from] ParseGpuPreferenceError),
    #[error(transparent)]
    UnknownSystemd(#[from] ParseSystemdModeError),
    #[error(transparent)]
    XWayland(#[from] crate::xwayland::XWaylandConfigError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kdl_layout_and_ipc_socket() {
        let config = Config::from_kdl(
            Path::new("test.kdl"),
            "layout \"spatial-2d\"\nipc-socket \"/run/user/1000/tensor.sock\"\ngpu \"integrated\"\nsystemd \"disabled\"\nxwayland true\nspawn-at-startup \"waybar\"\nspawn-at-startup \"foot\" \"--server\"",
        )
        .unwrap();

        assert_eq!(config.initial_layout, LayoutKind::Spatial2D);
        assert_eq!(
            config.ipc_socket,
            PathBuf::from("/run/user/1000/tensor.sock")
        );
        assert_eq!(config.gpu_preference, GpuPreference::Integrated);
        assert_eq!(config.systemd, SystemdMode::Disabled);
        assert!(config.xwayland.enabled());
        assert_eq!(
            config.startup_commands,
            vec![
                StartupCommand {
                    argv: vec!["waybar".to_owned()]
                },
                StartupCommand {
                    argv: vec!["foot".to_owned(), "--server".to_owned()]
                }
            ]
        );
    }

    #[test]
    fn rejects_unknown_systemd_mode() {
        assert!(matches!(
            Config::from_kdl(Path::new("test.kdl"), "systemd \"launchd\""),
            Err(ConfigError::UnknownSystemd(_))
        ));
    }

    #[test]
    fn rejects_unknown_kdl_nodes() {
        assert!(matches!(
            Config::from_kdl(Path::new("test.kdl"), "compatibility true"),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn rejects_empty_startup_commands() {
        assert!(matches!(
            Config::from_kdl(Path::new("test.kdl"), "spawn-at-startup"),
            Err(ConfigError::EmptyStartupCommand { index: 0 })
        ));
    }
}

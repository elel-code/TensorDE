use std::{
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use thiserror::Error;

use crate::layout::LayoutKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub initial_layout: LayoutKind,
    pub ipc_socket: PathBuf,
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
            knuffel::parse(&path_name, document).map_err(|error| ConfigError::Parse {
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

        Ok(Self {
            initial_layout,
            ipc_socket,
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
        }
    }
}

#[derive(Debug, knuffel::Decode)]
struct FileConfig {
    #[knuffel(child, unwrap(argument))]
    layout: Option<String>,
    #[knuffel(child, unwrap(argument))]
    ipc_socket: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("TENSOR_LAYOUT is not valid Unicode")]
    NonUnicodeLayout,
    #[error("failed to read config {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error(transparent)]
    UnknownLayout(#[from] crate::layout::ParseLayoutError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kdl_layout_and_ipc_socket() {
        let config = Config::from_kdl(
            Path::new("test.kdl"),
            "layout \"nourish-2d\"\nipc-socket \"/run/user/1000/tensor.sock\"",
        )
        .unwrap();

        assert_eq!(config.initial_layout, LayoutKind::Nourish2D);
        assert_eq!(
            config.ipc_socket,
            PathBuf::from("/run/user/1000/tensor.sock")
        );
    }
}

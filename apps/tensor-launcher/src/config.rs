#![allow(unexpected_cfgs)] // `tensor-kdl` derive emits optional downstream DOM impls.

use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

use tensor_kdl::Decode;

pub const MAX_QUERY_RESULTS: usize = 64;
pub const MAX_CATALOG_ENTRIES: usize = 131_072;
pub const MAX_CATALOG_DIAGNOSTICS: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LauncherConfig {
    pub application_directories: Vec<PathBuf>,
    pub max_results: usize,
    pub max_catalog_entries: usize,
    pub max_diagnostics: usize,
    pub systemd: SystemdMode,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SystemdMode {
    #[default]
    Auto,
    Enabled,
    Disabled,
}

impl LauncherConfig {
    pub fn resolve_path() -> PathBuf {
        env::var_os("TENSOR_LAUNCHER_CONFIG")
            .map(PathBuf::from)
            .or_else(|| xdg_config_home().map(|path| path.join("tensor/launcher.kdl")))
            .unwrap_or_else(|| PathBuf::from("/etc/tensor/launcher.kdl"))
    }

    pub fn load_default_path() -> Result<Self, LauncherConfigError> {
        Self::load_or_default(&Self::resolve_path())
    }

    pub fn load_or_default(path: &Path) -> Result<Self, LauncherConfigError> {
        match fs::read_to_string(path) {
            Ok(document) => Self::from_kdl(path, &document),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(LauncherConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    fn from_kdl(path: &Path, document: &str) -> Result<Self, LauncherConfigError> {
        let parsed: FileConfig =
            tensor_kdl::read(document).map_err(|error| LauncherConfigError::Parse {
                path: path.to_owned(),
                message: tensor_kdl::format_error_named(
                    &error,
                    document,
                    &path.display().to_string(),
                ),
            })?;
        parsed.resolve()
    }
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            application_directories: default_application_directories(),
            max_results: 10,
            max_catalog_entries: 32_768,
            max_diagnostics: 32,
            systemd: SystemdMode::Auto,
        }
    }
}

#[derive(Debug, Default, Decode)]
struct FileConfig {
    #[kdl(child(name = "max-results"), unwrap(argument))]
    max_results: Option<u64>,
    #[kdl(child(name = "max-catalog-entries"), unwrap(argument))]
    max_catalog_entries: Option<u64>,
    #[kdl(child(name = "max-diagnostics"), unwrap(argument))]
    max_diagnostics: Option<u64>,
    #[kdl(child(name = "systemd-mode"), unwrap(argument))]
    systemd: Option<String>,
    #[kdl(children(name = "application-directory"))]
    application_directories: Vec<ApplicationDirectory>,
}

#[derive(Debug, Decode)]
struct ApplicationDirectory {
    #[kdl(argument)]
    path: String,
}

impl FileConfig {
    fn resolve(self) -> Result<LauncherConfig, LauncherConfigError> {
        let defaults = LauncherConfig::default();
        let max_results = bounded(
            "max-results",
            self.max_results,
            defaults.max_results,
            MAX_QUERY_RESULTS,
        )?;
        let max_catalog_entries = bounded(
            "max-catalog-entries",
            self.max_catalog_entries,
            defaults.max_catalog_entries,
            MAX_CATALOG_ENTRIES,
        )?;
        let max_diagnostics = bounded(
            "max-diagnostics",
            self.max_diagnostics,
            defaults.max_diagnostics,
            MAX_CATALOG_DIAGNOSTICS,
        )?;
        let application_directories = if self.application_directories.is_empty() {
            defaults.application_directories
        } else {
            deduplicate(
                self.application_directories
                    .into_iter()
                    .map(|directory| PathBuf::from(directory.path)),
            )
        };
        if application_directories.is_empty() {
            return Err(LauncherConfigError::NoApplicationDirectories);
        }
        let systemd = match self.systemd.as_deref() {
            None | Some("auto") => SystemdMode::Auto,
            Some("enabled") => SystemdMode::Enabled,
            Some("disabled") => SystemdMode::Disabled,
            Some(value) => return Err(LauncherConfigError::UnknownSystemdMode(value.to_owned())),
        };
        Ok(LauncherConfig {
            application_directories,
            max_results,
            max_catalog_entries,
            max_diagnostics,
            systemd,
        })
    }
}

fn bounded(
    field: &'static str,
    configured: Option<u64>,
    default: usize,
    maximum: usize,
) -> Result<usize, LauncherConfigError> {
    let value = configured.unwrap_or(default as u64);
    if value == 0 || value > maximum as u64 {
        return Err(LauncherConfigError::OutOfRange {
            field,
            value,
            maximum,
        });
    }
    Ok(value as usize)
}

fn default_application_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    {
        directories.push(data_home.join("applications"));
    }
    let data_dirs =
        env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    directories.extend(env::split_paths(&data_dirs).map(|path| path.join("applications")));
    deduplicate(directories)
}

fn deduplicate(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn xdg_config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

#[derive(Debug, thiserror::Error)]
pub enum LauncherConfigError {
    #[error("failed to read Tensor Launcher configuration {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse Tensor Launcher configuration {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("{field} must be in 1..={maximum}, got {value}")]
    OutOfRange {
        field: &'static str,
        value: u64,
        maximum: usize,
    },
    #[error("at least one application-directory is required")]
    NoApplicationDirectories,
    #[error("unknown systemd-mode `{0}`; expected auto, enabled, or disabled")]
    UnknownSystemdMode(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(document: &str) -> Result<LauncherConfig, LauncherConfigError> {
        LauncherConfig::from_kdl(Path::new("launcher.kdl"), document)
    }

    #[test]
    fn typed_kdl_overrides_bounds_and_directories() {
        let config = parse(
            r#"
                max-results 24
                max-catalog-entries 4096
                max-diagnostics 12
                systemd-mode "enabled"
                application-directory "/home/test/.local/share/applications"
                application-directory "/usr/share/applications"
            "#,
        )
        .unwrap();
        assert_eq!(config.max_results, 24);
        assert_eq!(config.max_catalog_entries, 4096);
        assert_eq!(config.max_diagnostics, 12);
        assert_eq!(config.systemd, SystemdMode::Enabled);
        assert_eq!(config.application_directories.len(), 2);
    }

    #[test]
    fn invalid_bound_keeps_a_named_source_diagnostic() {
        let error = parse("max-results 65").unwrap_err();
        assert!(error.to_string().contains("max-results"));
    }

    #[test]
    fn unknown_systemd_mode_is_rejected() {
        assert!(matches!(
            parse("systemd-mode \"sometimes\"").unwrap_err(),
            LauncherConfigError::UnknownSystemdMode(_)
        ));
    }

    #[test]
    fn malformed_kdl_names_the_configuration() {
        let error = parse("max-results {").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("launcher.kdl"));
        assert!(message.contains("failed to parse"));
    }
}

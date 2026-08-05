#![allow(unexpected_cfgs)] // `tensor-kdl` derive emits optional downstream DOM impls.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use tensor_kdl::Decode;

pub const MAX_DIAGNOSTICS: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsConfig {
    pub max_diagnostics: usize,
    pub read_only: bool,
    pub confirm_privileged_changes: bool,
}

impl SettingsConfig {
    pub fn resolve_path() -> PathBuf {
        env::var_os("TENSOR_SETTINGS_CONFIG")
            .map(PathBuf::from)
            .or_else(|| xdg_config_home().map(|path| path.join("tensor/settings.kdl")))
            .unwrap_or_else(|| PathBuf::from("/etc/tensor/settings.kdl"))
    }

    pub fn load_default_path() -> Result<Self, SettingsConfigError> {
        Self::load_or_default(&Self::resolve_path())
    }

    pub fn load_or_default(path: &Path) -> Result<Self, SettingsConfigError> {
        match fs::read_to_string(path) {
            Ok(document) => Self::from_kdl(path, &document),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(SettingsConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    fn from_kdl(path: &Path, document: &str) -> Result<Self, SettingsConfigError> {
        let parsed: FileConfig =
            tensor_kdl::read(document).map_err(|error| SettingsConfigError::Parse {
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

impl Default for SettingsConfig {
    fn default() -> Self {
        Self {
            max_diagnostics: 32,
            read_only: false,
            confirm_privileged_changes: true,
        }
    }
}

#[derive(Debug, Default, Decode)]
struct FileConfig {
    #[kdl(child(name = "max-diagnostics"), unwrap(argument))]
    max_diagnostics: Option<u64>,
    #[kdl(child(name = "read-only"), unwrap(argument))]
    read_only: Option<bool>,
    #[kdl(child(name = "confirm-privileged-changes"), unwrap(argument))]
    confirm_privileged_changes: Option<bool>,
}

impl FileConfig {
    fn resolve(self) -> Result<SettingsConfig, SettingsConfigError> {
        let defaults = SettingsConfig::default();
        let value = self
            .max_diagnostics
            .unwrap_or(defaults.max_diagnostics as u64);
        if value == 0 || value > MAX_DIAGNOSTICS as u64 {
            return Err(SettingsConfigError::DiagnosticsOutOfRange(value));
        }
        Ok(SettingsConfig {
            max_diagnostics: value as usize,
            read_only: self.read_only.unwrap_or(defaults.read_only),
            confirm_privileged_changes: self
                .confirm_privileged_changes
                .unwrap_or(defaults.confirm_privileged_changes),
        })
    }
}

fn xdg_config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsConfigError {
    #[error("failed to read Tensor Settings configuration {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse Tensor Settings configuration {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("max-diagnostics must be in 1..={MAX_DIAGNOSTICS}, got {0}")]
    DiagnosticsOutOfRange(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(document: &str) -> Result<SettingsConfig, SettingsConfigError> {
        SettingsConfig::from_kdl(Path::new("settings.kdl"), document)
    }

    #[test]
    fn empty_document_uses_complete_defaults() {
        assert_eq!(parse("").unwrap(), SettingsConfig::default());
    }

    #[test]
    fn typed_kdl_controls_safety_and_diagnostic_policy() {
        let config =
            parse("max-diagnostics 64\nread-only #true\nconfirm-privileged-changes #false\n")
                .unwrap();
        assert_eq!(config.max_diagnostics, 64);
        assert!(config.read_only);
        assert!(!config.confirm_privileged_changes);
    }

    #[test]
    fn invalid_bound_and_malformed_kdl_keep_named_errors() {
        assert!(matches!(
            parse("max-diagnostics 0").unwrap_err(),
            SettingsConfigError::DiagnosticsOutOfRange(0)
        ));
        assert!(
            parse("read-only {")
                .unwrap_err()
                .to_string()
                .contains("settings.kdl")
        );
    }
}

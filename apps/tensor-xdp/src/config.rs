#![allow(unexpected_cfgs)] // `tensor-kdl` derives optional downstream DOM implementations.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use tensor_kdl::Decode;
use thiserror::Error;

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TensorXdpConfig {
    pub appearance: AppearanceSettings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppearanceSettings {
    pub color_scheme: ColorScheme,
    pub contrast: Contrast,
    pub reduced_motion: bool,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            color_scheme: ColorScheme::NoPreference,
            contrast: Contrast::Normal,
            reduced_motion: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorScheme {
    NoPreference,
    Dark,
    Light,
}

impl ColorScheme {
    pub const fn portal_value(self) -> u32 {
        match self {
            Self::NoPreference => 0,
            Self::Dark => 1,
            Self::Light => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Contrast {
    Normal,
    High,
}

impl Contrast {
    pub const fn portal_value(self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::High => 1,
        }
    }
}

impl TensorXdpConfig {
    pub fn resolve_path() -> PathBuf {
        env::var_os("TENSOR_XDP_CONFIG")
            .map(PathBuf::from)
            .or_else(|| xdg_config_home().map(|path| path.join("tensor/xdp.kdl")))
            .unwrap_or_else(|| PathBuf::from("/etc/tensor/xdp.kdl"))
    }

    pub fn load_default_path() -> Result<Self, ConfigError> {
        Self::load_or_default(&Self::resolve_path())
    }

    pub fn load_or_default(path: &Path) -> Result<Self, ConfigError> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        if metadata.len() > MAX_CONFIG_BYTES {
            return Err(ConfigError::TooLarge {
                path: path.to_owned(),
                bytes: metadata.len(),
            });
        }
        let document = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        Self::from_kdl(path, &document)
    }

    fn from_kdl(path: &Path, document: &str) -> Result<Self, ConfigError> {
        let parsed: FileConfig =
            tensor_kdl::read(document).map_err(|error| ConfigError::Parse {
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

#[derive(Debug, Default, Decode)]
struct FileConfig {
    #[kdl(child)]
    appearance: Option<AppearanceFileConfig>,
}

#[derive(Debug, Default, Decode)]
struct AppearanceFileConfig {
    #[kdl(property(name = "color-scheme"))]
    color_scheme: Option<String>,
    #[kdl(property)]
    contrast: Option<String>,
    #[kdl(property(name = "reduced-motion"))]
    reduced_motion: Option<bool>,
}

impl FileConfig {
    fn resolve(self) -> Result<TensorXdpConfig, ConfigError> {
        Ok(TensorXdpConfig {
            appearance: self
                .appearance
                .map(AppearanceFileConfig::resolve)
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

impl AppearanceFileConfig {
    fn resolve(self) -> Result<AppearanceSettings, ConfigError> {
        let defaults = AppearanceSettings::default();
        Ok(AppearanceSettings {
            color_scheme: self
                .color_scheme
                .map(|value| parse_color_scheme(&value))
                .transpose()?
                .unwrap_or(defaults.color_scheme),
            contrast: self
                .contrast
                .map(|value| parse_contrast(&value))
                .transpose()?
                .unwrap_or(defaults.contrast),
            reduced_motion: self.reduced_motion.unwrap_or(defaults.reduced_motion),
        })
    }
}

fn parse_color_scheme(value: &str) -> Result<ColorScheme, ConfigError> {
    match value {
        "no-preference" => Ok(ColorScheme::NoPreference),
        "dark" => Ok(ColorScheme::Dark),
        "light" => Ok(ColorScheme::Light),
        _ => Err(ConfigError::ColorScheme(value.to_owned())),
    }
}

fn parse_contrast(value: &str) -> Result<Contrast, ConfigError> {
    match value {
        "normal" => Ok(Contrast::Normal),
        "high" => Ok(Contrast::High),
        _ => Err(ConfigError::Contrast(value.to_owned())),
    }
}

fn xdg_config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read Tensor XDP configuration {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Tensor XDP configuration {path} is {bytes} bytes; maximum is {MAX_CONFIG_BYTES}")]
    TooLarge { path: PathBuf, bytes: u64 },
    #[error("invalid Tensor XDP configuration {path}:\n{message}")]
    Parse { path: PathBuf, message: String },
    #[error("appearance.color-scheme must be no-preference, dark, or light; got {0:?}")]
    ColorScheme(String),
    #[error("appearance.contrast must be normal or high; got {0:?}")]
    Contrast(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_kdl_resolves_all_standardized_appearance_values() {
        let config = TensorXdpConfig::from_kdl(
            Path::new("appearance.kdl"),
            r#"appearance color-scheme="dark" contrast="high" reduced-motion=#true"#,
        )
        .unwrap();

        assert_eq!(config.appearance.color_scheme, ColorScheme::Dark);
        assert_eq!(config.appearance.contrast, Contrast::High);
        assert!(config.appearance.reduced_motion);
    }

    #[test]
    fn unknown_values_and_toml_shapes_fail_closed() {
        assert!(
            TensorXdpConfig::from_kdl(Path::new("bad.kdl"), r#"appearance color-scheme="sepia""#,)
                .is_err()
        );
        assert!(
            TensorXdpConfig::from_kdl(
                Path::new("bad.toml"),
                "[appearance]\ncolor_scheme = \"dark\"",
            )
            .is_err()
        );
    }
}

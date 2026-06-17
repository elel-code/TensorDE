use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GilderConfig {
    #[serde(default)]
    pub default_wallpaper: Option<String>,
    #[serde(default)]
    pub outputs: BTreeMap<String, OutputConfig>,
    #[serde(default)]
    pub performance: PerformanceConfig,
    #[serde(default)]
    pub adapters: AdapterConfig,
}

impl Default for GilderConfig {
    fn default() -> Self {
        Self {
            default_wallpaper: None,
            outputs: BTreeMap::new(),
            performance: PerformanceConfig::default(),
            adapters: AdapterConfig::default(),
        }
    }
}

impl GilderConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigLoadError> {
        let path = path.as_ref();
        match fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents).map_err(ConfigLoadError::Parse),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(ConfigLoadError::Read(err)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(default)]
    pub wallpaper: Option<String>,
    #[serde(default)]
    pub fit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceConfig {
    #[serde(default = "default_interactive_fps")]
    pub interactive_max_fps: u32,
    #[serde(default = "default_background_fps")]
    pub background_max_fps: u32,
    #[serde(default = "default_battery_fps")]
    pub battery_max_fps: u32,
    #[serde(default)]
    pub fullscreen: ThrottlePolicy,
    #[serde(default)]
    pub unfocused: ThrottlePolicy,
    #[serde(default)]
    pub battery: PowerPolicy,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            interactive_max_fps: default_interactive_fps(),
            background_max_fps: default_background_fps(),
            battery_max_fps: default_battery_fps(),
            fullscreen: ThrottlePolicy::Pause,
            unfocused: ThrottlePolicy::Throttle,
            battery: PowerPolicy::Throttle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThrottlePolicy {
    Continue,
    Throttle,
    Pause,
}

impl Default for ThrottlePolicy {
    fn default() -> Self {
        Self::Pause
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PowerPolicy {
    Continue,
    Throttle,
    Pause,
}

impl Default for PowerPolicy {
    fn default() -> Self {
        Self::Throttle
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterConfig {
    #[serde(default = "default_true")]
    pub generic_wayland: bool,
    #[serde(default = "default_true")]
    pub hyprland: bool,
    #[serde(default = "default_true")]
    pub niri: bool,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            generic_wayland: true,
            hyprland: true,
            niri: true,
        }
    }
}

#[derive(Debug)]
pub enum ConfigLoadError {
    Read(io::Error),
    Parse(toml::de::Error),
}

impl fmt::Display for ConfigLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => write!(f, "failed to read config: {source}"),
            Self::Parse(source) => write!(f, "failed to parse config TOML: {source}"),
        }
    }
}

impl std::error::Error for ConfigLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Parse(source) => Some(source),
        }
    }
}

fn default_interactive_fps() -> u32 {
    60
}

fn default_background_fps() -> u32 {
    30
}

fn default_battery_fps() -> u32 {
    24
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_loads_defaults() {
        let config = GilderConfig::load("/tmp/gilder-config-that-should-not-exist.toml").unwrap();
        assert_eq!(config.performance.fullscreen, ThrottlePolicy::Pause);
        assert!(config.adapters.hyprland);
    }

    #[test]
    fn parses_performance_config() {
        let config: GilderConfig = toml::from_str(
            r#"
            [performance]
            interactive_max_fps = 75
            background_max_fps = 20
            battery_max_fps = 15
            fullscreen = "pause"
            unfocused = "throttle"
            battery = "pause"

            [adapters]
            niri = false
            "#,
        )
        .unwrap();
        assert_eq!(config.performance.interactive_max_fps, 75);
        assert_eq!(config.performance.battery, PowerPolicy::Pause);
        assert!(!config.adapters.niri);
        assert!(config.adapters.hyprland);
    }
}

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

    pub fn performance_for_output(&self, output_name: &str) -> PerformanceConfig {
        self.outputs
            .get(output_name)
            .map(|output| self.performance.with_output_overrides(&output.performance))
            .unwrap_or_else(|| self.performance.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(default)]
    pub wallpaper: Option<String>,
    #[serde(default)]
    pub fit: Option<String>,
    #[serde(default)]
    pub performance: OutputPerformanceConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceConfig {
    #[serde(default = "default_interactive_fps")]
    pub interactive_max_fps: u32,
    #[serde(default = "default_background_fps")]
    pub background_max_fps: u32,
    #[serde(default = "default_battery_fps")]
    pub battery_max_fps: u32,
    #[serde(default = "default_desktop_refresh_interval_ms")]
    pub desktop_refresh_interval_ms: u64,
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
            desktop_refresh_interval_ms: default_desktop_refresh_interval_ms(),
            fullscreen: ThrottlePolicy::Pause,
            unfocused: ThrottlePolicy::Throttle,
            battery: PowerPolicy::Throttle,
        }
    }
}

impl PerformanceConfig {
    pub fn with_output_overrides(&self, overrides: &OutputPerformanceConfig) -> Self {
        Self {
            interactive_max_fps: overrides
                .interactive_max_fps
                .unwrap_or(self.interactive_max_fps),
            background_max_fps: overrides
                .background_max_fps
                .unwrap_or(self.background_max_fps),
            battery_max_fps: overrides.battery_max_fps.unwrap_or(self.battery_max_fps),
            desktop_refresh_interval_ms: self.desktop_refresh_interval_ms,
            fullscreen: overrides.fullscreen.unwrap_or(self.fullscreen),
            unfocused: overrides.unfocused.unwrap_or(self.unfocused),
            battery: overrides.battery.unwrap_or(self.battery),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OutputPerformanceConfig {
    #[serde(default)]
    pub interactive_max_fps: Option<u32>,
    #[serde(default)]
    pub background_max_fps: Option<u32>,
    #[serde(default)]
    pub battery_max_fps: Option<u32>,
    #[serde(default)]
    pub fullscreen: Option<ThrottlePolicy>,
    #[serde(default)]
    pub unfocused: Option<ThrottlePolicy>,
    #[serde(default)]
    pub battery: Option<PowerPolicy>,
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

fn default_desktop_refresh_interval_ms() -> u64 {
    2000
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
            desktop_refresh_interval_ms = 1000
            fullscreen = "pause"
            unfocused = "throttle"
            battery = "throttle"

            [adapters]
            niri = false

            [outputs."HDMI-A-1".performance]
            background_max_fps = 12
            battery = "pause"
            "#,
        )
        .unwrap();
        assert_eq!(config.performance.interactive_max_fps, 75);
        assert_eq!(config.performance.desktop_refresh_interval_ms, 1000);
        assert_eq!(config.performance.battery, PowerPolicy::Throttle);
        let hdmi_performance = config.performance_for_output("HDMI-A-1");
        assert_eq!(hdmi_performance.interactive_max_fps, 75);
        assert_eq!(hdmi_performance.background_max_fps, 12);
        assert_eq!(hdmi_performance.battery, PowerPolicy::Pause);
        assert_eq!(config.performance_for_output("eDP-1"), config.performance);
        assert!(!config.adapters.niri);
        assert!(config.adapters.hyprland);
    }
}

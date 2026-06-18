use crate::core::FitMode;
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
    pub adaptive: AdaptiveConfig,
    #[serde(default)]
    pub video: VideoConfig,
    #[serde(default)]
    pub cache: CacheConfig,
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
            adaptive: AdaptiveConfig::default(),
            video: VideoConfig::default(),
            cache: CacheConfig::default(),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub kill_switch: bool,
    #[serde(default = "default_adaptive_refresh_interval_ms")]
    pub refresh_interval_ms: u64,
    #[serde(default = "default_adaptive_cooldown_ms")]
    pub cooldown_ms: u64,
    #[serde(default = "default_adaptive_throttle_max_fps")]
    pub throttle_max_fps: u32,
    #[serde(default)]
    pub action: AdaptiveAction,
    #[serde(default = "default_adaptive_cpu_pressure_threshold_percent")]
    pub cpu_pressure_threshold_percent: u32,
    #[serde(default = "default_adaptive_memory_pressure_threshold_percent")]
    pub memory_pressure_threshold_percent: u32,
    #[serde(default = "default_adaptive_temperature_threshold_celsius")]
    pub temperature_threshold_celsius: u32,
    #[serde(default = "default_adaptive_gpu_busy_threshold_percent")]
    pub gpu_busy_threshold_percent: u32,
    #[serde(default = "default_adaptive_battery_threshold_percent")]
    pub battery_capacity_threshold_percent: u32,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            kill_switch: false,
            refresh_interval_ms: default_adaptive_refresh_interval_ms(),
            cooldown_ms: default_adaptive_cooldown_ms(),
            throttle_max_fps: default_adaptive_throttle_max_fps(),
            action: AdaptiveAction::default(),
            cpu_pressure_threshold_percent: default_adaptive_cpu_pressure_threshold_percent(),
            memory_pressure_threshold_percent: default_adaptive_memory_pressure_threshold_percent(),
            temperature_threshold_celsius: default_adaptive_temperature_threshold_celsius(),
            gpu_busy_threshold_percent: default_adaptive_gpu_busy_threshold_percent(),
            battery_capacity_threshold_percent: default_adaptive_battery_threshold_percent(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VideoConfig {
    #[serde(default)]
    pub decoder: VideoDecoderPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_package_cache_max_entries")]
    pub package_cache_max_entries: usize,
    #[serde(default = "default_render_cache_max_entries")]
    pub render_cache_max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            package_cache_max_entries: default_package_cache_max_entries(),
            render_cache_max_entries: default_render_cache_max_entries(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoDecoderPolicy {
    #[default]
    Auto,
    HardwarePreferred,
    HardwareRequired,
    Software,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(default)]
    pub wallpaper: Option<String>,
    #[serde(default)]
    pub fit: Option<FitMode>,
    #[serde(default)]
    pub adaptive: OutputAdaptiveConfig,
    #[serde(default)]
    pub performance: OutputPerformanceConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OutputAdaptiveConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub throttle_max_fps: Option<u32>,
    #[serde(default)]
    pub action: Option<AdaptiveAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdaptiveAction {
    #[default]
    Throttle,
    PauseUnfocused,
    PauseDynamic,
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

fn default_render_cache_max_entries() -> usize {
    32
}

fn default_package_cache_max_entries() -> usize {
    16
}

fn default_adaptive_refresh_interval_ms() -> u64 {
    2000
}

fn default_adaptive_cooldown_ms() -> u64 {
    10_000
}

fn default_adaptive_throttle_max_fps() -> u32 {
    15
}

fn default_adaptive_cpu_pressure_threshold_percent() -> u32 {
    75
}

fn default_adaptive_memory_pressure_threshold_percent() -> u32 {
    20
}

fn default_adaptive_temperature_threshold_celsius() -> u32 {
    85
}

fn default_adaptive_gpu_busy_threshold_percent() -> u32 {
    90
}

fn default_adaptive_battery_threshold_percent() -> u32 {
    20
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
            default_wallpaper = "default.gwpdir"

            [performance]
            interactive_max_fps = 75
            background_max_fps = 20
            battery_max_fps = 15
            desktop_refresh_interval_ms = 1000
            fullscreen = "pause"
            unfocused = "throttle"
            battery = "throttle"

            [video]
            decoder = "hardware-preferred"

            [cache]
            package_cache_max_entries = 4
            render_cache_max_entries = 8

            [adaptive]
            enabled = true
            refresh_interval_ms = 1500
            cooldown_ms = 5000
            throttle_max_fps = 18
            action = "pause-dynamic"
            cpu_pressure_threshold_percent = 65
            memory_pressure_threshold_percent = 10
            temperature_threshold_celsius = 80
            gpu_busy_threshold_percent = 85
            battery_capacity_threshold_percent = 25

            [adapters]
            niri = false

            [outputs."HDMI-A-1"]
            wallpaper = "hdmi.gwpdir"
            fit = "contain"

            [outputs."HDMI-A-1".performance]
            background_max_fps = 12
            battery = "pause"

            [outputs."HDMI-A-1".adaptive]
            enabled = false
            throttle_max_fps = 9
            action = "throttle"
            "#,
        )
        .unwrap();
        assert_eq!(config.default_wallpaper.as_deref(), Some("default.gwpdir"));
        assert_eq!(
            config.outputs["HDMI-A-1"].wallpaper.as_deref(),
            Some("hdmi.gwpdir")
        );
        assert_eq!(config.outputs["HDMI-A-1"].fit, Some(FitMode::Contain));
        assert_eq!(config.performance.interactive_max_fps, 75);
        assert_eq!(config.performance.desktop_refresh_interval_ms, 1000);
        assert_eq!(config.performance.battery, PowerPolicy::Throttle);
        assert_eq!(config.video.decoder, VideoDecoderPolicy::HardwarePreferred);
        assert_eq!(config.cache.package_cache_max_entries, 4);
        assert_eq!(config.cache.render_cache_max_entries, 8);
        assert!(config.adaptive.enabled);
        assert_eq!(config.adaptive.refresh_interval_ms, 1500);
        assert_eq!(config.adaptive.cooldown_ms, 5000);
        assert_eq!(config.adaptive.throttle_max_fps, 18);
        assert_eq!(config.adaptive.action, AdaptiveAction::PauseDynamic);
        assert_eq!(config.adaptive.cpu_pressure_threshold_percent, 65);
        assert_eq!(config.adaptive.memory_pressure_threshold_percent, 10);
        assert_eq!(config.adaptive.temperature_threshold_celsius, 80);
        assert_eq!(config.adaptive.gpu_busy_threshold_percent, 85);
        assert_eq!(config.adaptive.battery_capacity_threshold_percent, 25);
        assert_eq!(config.outputs["HDMI-A-1"].adaptive.enabled, Some(false));
        assert_eq!(
            config.outputs["HDMI-A-1"].adaptive.throttle_max_fps,
            Some(9)
        );
        assert_eq!(
            config.outputs["HDMI-A-1"].adaptive.action,
            Some(AdaptiveAction::Throttle)
        );
        let hdmi_performance = config.performance_for_output("HDMI-A-1");
        assert_eq!(hdmi_performance.interactive_max_fps, 75);
        assert_eq!(hdmi_performance.background_max_fps, 12);
        assert_eq!(hdmi_performance.battery, PowerPolicy::Pause);
        assert_eq!(config.performance_for_output("eDP-1"), config.performance);
        assert!(!config.adapters.niri);
        assert!(config.adapters.hyprland);
    }
}

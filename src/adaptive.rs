//! Optional system-pressure monitor for conservative performance adaptation.

use crate::config::GilderConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

const PROC_PRESSURE_CPU: &str = "/proc/pressure/cpu";
const PROC_PRESSURE_MEMORY: &str = "/proc/pressure/memory";
const SYS_CLASS_THERMAL: &str = "/sys/class/thermal";

#[derive(Debug, Clone)]
pub struct AdaptiveMonitor {
    last_refresh: Option<Instant>,
    active_until: Option<Instant>,
    retained_triggers: Vec<AdaptiveTrigger>,
}

impl Default for AdaptiveMonitor {
    fn default() -> Self {
        Self {
            last_refresh: None,
            active_until: None,
            retained_triggers: Vec::new(),
        }
    }
}

impl AdaptiveMonitor {
    pub fn should_refresh(&self, interval: Duration) -> bool {
        self.last_refresh
            .map(|last_refresh| last_refresh.elapsed() >= interval)
            .unwrap_or(true)
    }

    pub fn refresh(&mut self, config: &GilderConfig) -> AdaptiveSnapshot {
        let now = Instant::now();
        self.last_refresh = Some(now);

        if !monitoring_enabled(config) {
            self.active_until = None;
            self.retained_triggers.clear();
            return AdaptiveSnapshot {
                monitoring_enabled: false,
                kill_switch: config.adaptive.kill_switch,
                sample: None,
                active_triggers: Vec::new(),
                last_error: None,
            };
        }

        let sample = read_system_sample();
        let triggers = triggers_for_sample(config, &sample);
        let active_triggers = if triggers.is_empty() {
            if self
                .active_until
                .is_some_and(|active_until| active_until > now)
            {
                self.retained_triggers.clone()
            } else {
                self.active_until = None;
                self.retained_triggers.clear();
                Vec::new()
            }
        } else {
            self.retained_triggers = triggers;
            self.active_until = Some(now + Duration::from_millis(config.adaptive.cooldown_ms));
            self.retained_triggers.clone()
        };

        AdaptiveSnapshot {
            monitoring_enabled: true,
            kill_switch: false,
            sample: Some(sample),
            active_triggers,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveSnapshot {
    pub monitoring_enabled: bool,
    pub kill_switch: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample: Option<AdaptiveSystemSample>,
    #[serde(default)]
    pub active_triggers: Vec<AdaptiveTrigger>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl AdaptiveSnapshot {
    pub fn affects_render_plan(&self) -> bool {
        self.monitoring_enabled && !self.kill_switch && !self.active_triggers.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveSystemSample {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_pressure_some_avg10_x100: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_pressure_some_avg10_x100: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_max_millicelsius: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveTrigger {
    pub metric: AdaptiveMetric,
    pub value_x100: u32,
    pub threshold_x100: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdaptiveMetric {
    CpuPressureSomeAvg10,
    MemoryPressureSomeAvg10,
    TemperatureMaxCelsius,
}

pub fn monitoring_enabled(config: &GilderConfig) -> bool {
    if config.adaptive.kill_switch {
        return false;
    }
    config.adaptive.enabled
        || config
            .outputs
            .values()
            .any(|output| output.adaptive.enabled == Some(true))
}

pub fn output_enabled(config: &GilderConfig, output_name: &str) -> bool {
    if config.adaptive.kill_switch {
        return false;
    }
    config
        .outputs
        .get(output_name)
        .and_then(|output| output.adaptive.enabled)
        .unwrap_or(config.adaptive.enabled)
}

pub fn output_throttle_max_fps(config: &GilderConfig, output_name: &str) -> u32 {
    config
        .outputs
        .get(output_name)
        .and_then(|output| output.adaptive.throttle_max_fps)
        .unwrap_or(config.adaptive.throttle_max_fps)
        .max(1)
}

fn read_system_sample() -> AdaptiveSystemSample {
    AdaptiveSystemSample {
        cpu_pressure_some_avg10_x100: read_pressure_some_avg10_x100(PROC_PRESSURE_CPU).ok(),
        memory_pressure_some_avg10_x100: read_pressure_some_avg10_x100(PROC_PRESSURE_MEMORY).ok(),
        temperature_max_millicelsius: read_temperature_max_millicelsius(SYS_CLASS_THERMAL)
            .ok()
            .flatten(),
    }
}

fn triggers_for_sample(
    config: &GilderConfig,
    sample: &AdaptiveSystemSample,
) -> Vec<AdaptiveTrigger> {
    let mut triggers = Vec::new();
    push_pressure_trigger(
        &mut triggers,
        AdaptiveMetric::CpuPressureSomeAvg10,
        sample.cpu_pressure_some_avg10_x100,
        config.adaptive.cpu_pressure_threshold_percent,
    );
    push_pressure_trigger(
        &mut triggers,
        AdaptiveMetric::MemoryPressureSomeAvg10,
        sample.memory_pressure_some_avg10_x100,
        config.adaptive.memory_pressure_threshold_percent,
    );
    push_temperature_trigger(
        &mut triggers,
        sample.temperature_max_millicelsius,
        config.adaptive.temperature_threshold_celsius,
    );
    triggers
}

fn push_pressure_trigger(
    triggers: &mut Vec<AdaptiveTrigger>,
    metric: AdaptiveMetric,
    value_x100: Option<u32>,
    threshold_percent: u32,
) {
    let Some(value_x100) = value_x100 else {
        return;
    };
    if threshold_percent == 0 {
        return;
    }
    let threshold_x100 = threshold_percent.saturating_mul(100);
    if value_x100 >= threshold_x100 {
        triggers.push(AdaptiveTrigger {
            metric,
            value_x100,
            threshold_x100,
        });
    }
}

fn push_temperature_trigger(
    triggers: &mut Vec<AdaptiveTrigger>,
    value_millicelsius: Option<i32>,
    threshold_celsius: u32,
) {
    let Some(value_millicelsius) = value_millicelsius else {
        return;
    };
    if threshold_celsius == 0 || value_millicelsius < 0 {
        return;
    }
    let value_x100 = (value_millicelsius as u32) / 10;
    let threshold_x100 = threshold_celsius.saturating_mul(100);
    if value_x100 >= threshold_x100 {
        triggers.push(AdaptiveTrigger {
            metric: AdaptiveMetric::TemperatureMaxCelsius,
            value_x100,
            threshold_x100,
        });
    }
}

fn read_pressure_some_avg10_x100(path: impl AsRef<Path>) -> io::Result<u32> {
    let contents = fs::read_to_string(path)?;
    parse_pressure_some_avg10_x100(&contents)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing PSI some avg10"))
}

fn read_temperature_max_millicelsius(root: impl AsRef<Path>) -> io::Result<Option<i32>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };

    let mut max_temperature = None;
    for entry in entries {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("thermal_zone") {
            continue;
        }
        let Some(temperature) = read_temperature_millicelsius(path.join("temp")) else {
            continue;
        };
        max_temperature = Some(
            max_temperature.map_or(temperature, |current| std::cmp::max(current, temperature)),
        );
    }
    Ok(max_temperature)
}

fn read_temperature_millicelsius(path: impl AsRef<Path>) -> Option<i32> {
    fs::read_to_string(path).ok()?.trim().parse::<i32>().ok()
}

fn parse_pressure_some_avg10_x100(contents: &str) -> Option<u32> {
    let line = contents.lines().find(|line| line.starts_with("some "))?;
    let token = line
        .split_whitespace()
        .find_map(|token| token.strip_prefix("avg10="))?;
    parse_percent_x100(token)
}

fn parse_percent_x100(value: &str) -> Option<u32> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let whole = whole.parse::<u32>().ok()?;
    let mut fraction_digits = fraction
        .bytes()
        .filter(|byte| byte.is_ascii_digit())
        .take(2)
        .map(|byte| u32::from(byte - b'0'))
        .collect::<Vec<_>>();
    while fraction_digits.len() < 2 {
        fraction_digits.push(0);
    }
    whole
        .checked_mul(100)?
        .checked_add(fraction_digits[0] * 10 + fraction_digits[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AdaptiveConfig, OutputAdaptiveConfig, OutputConfig};

    #[test]
    fn parses_psi_some_avg10_as_hundredths() {
        let contents = "some avg10=12.34 avg60=5.00 avg300=1.00 total=42\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        assert_eq!(parse_pressure_some_avg10_x100(contents), Some(1234));
    }

    #[test]
    fn output_enable_can_opt_in_without_global_enable() {
        let mut config = GilderConfig::default();
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                adaptive: OutputAdaptiveConfig {
                    enabled: Some(true),
                    throttle_max_fps: Some(9),
                },
                ..OutputConfig::default()
            },
        );

        assert!(monitoring_enabled(&config));
        assert!(output_enabled(&config, "eDP-1"));
        assert!(!output_enabled(&config, "HDMI-A-1"));
        assert_eq!(output_throttle_max_fps(&config, "eDP-1"), 9);
    }

    #[test]
    fn kill_switch_disables_global_and_output_adaptive_policy() {
        let mut config = GilderConfig {
            adaptive: AdaptiveConfig {
                enabled: true,
                kill_switch: true,
                ..AdaptiveConfig::default()
            },
            ..GilderConfig::default()
        };
        config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                adaptive: OutputAdaptiveConfig {
                    enabled: Some(true),
                    throttle_max_fps: None,
                },
                ..OutputConfig::default()
            },
        );

        assert!(!monitoring_enabled(&config));
        assert!(!output_enabled(&config, "eDP-1"));
    }

    #[test]
    fn pressure_thresholds_create_triggers() {
        let config = GilderConfig {
            adaptive: AdaptiveConfig {
                cpu_pressure_threshold_percent: 20,
                memory_pressure_threshold_percent: 5,
                temperature_threshold_celsius: 85,
                ..AdaptiveConfig::default()
            },
            ..GilderConfig::default()
        };
        let sample = AdaptiveSystemSample {
            cpu_pressure_some_avg10_x100: Some(2_001),
            memory_pressure_some_avg10_x100: Some(499),
            temperature_max_millicelsius: Some(84_000),
        };
        let triggers = triggers_for_sample(&config, &sample);

        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].metric, AdaptiveMetric::CpuPressureSomeAvg10);
    }

    #[test]
    fn temperature_threshold_creates_trigger() {
        let config = GilderConfig {
            adaptive: AdaptiveConfig {
                temperature_threshold_celsius: 80,
                ..AdaptiveConfig::default()
            },
            ..GilderConfig::default()
        };
        let sample = AdaptiveSystemSample {
            temperature_max_millicelsius: Some(80_500),
            ..AdaptiveSystemSample::default()
        };
        let triggers = triggers_for_sample(&config, &sample);

        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].metric, AdaptiveMetric::TemperatureMaxCelsius);
        assert_eq!(triggers[0].value_x100, 8_050);
        assert_eq!(triggers[0].threshold_x100, 8_000);
    }

    #[test]
    fn reads_max_temperature_from_thermal_zones() {
        let root = TempDir::new("adaptive-thermal");
        fs::create_dir_all(root.path().join("thermal_zone0")).unwrap();
        fs::write(root.path().join("thermal_zone0/temp"), "42000\n").unwrap();
        fs::create_dir_all(root.path().join("thermal_zone1")).unwrap();
        fs::write(root.path().join("thermal_zone1/temp"), "73500\n").unwrap();
        fs::create_dir_all(root.path().join("cooling_device0")).unwrap();
        fs::write(root.path().join("cooling_device0/temp"), "99000\n").unwrap();

        assert_eq!(
            read_temperature_max_millicelsius(root.path()).unwrap(),
            Some(73_500)
        );
    }

    struct TempDir {
        path: std::path::PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "gilder-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

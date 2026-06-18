//! Optional system-pressure monitor for conservative performance adaptation.

use crate::config::GilderConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

const PROC_PRESSURE_CPU: &str = "/proc/pressure/cpu";
const PROC_PRESSURE_MEMORY: &str = "/proc/pressure/memory";

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

fn read_pressure_some_avg10_x100(path: impl AsRef<Path>) -> io::Result<u32> {
    let contents = fs::read_to_string(path)?;
    parse_pressure_some_avg10_x100(&contents)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing PSI some avg10"))
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
                ..AdaptiveConfig::default()
            },
            ..GilderConfig::default()
        };
        let sample = AdaptiveSystemSample {
            cpu_pressure_some_avg10_x100: Some(2_001),
            memory_pressure_some_avg10_x100: Some(499),
        };
        let triggers = triggers_for_sample(&config, &sample);

        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].metric, AdaptiveMetric::CpuPressureSomeAvg10);
    }
}

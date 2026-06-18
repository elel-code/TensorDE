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
const SYS_CLASS_POWER_SUPPLY: &str = "/sys/class/power_supply";
const SYS_CLASS_DRM: &str = "/sys/class/drm";
const ENV_ADAPTIVE_STATE: &str = "GILDER_ADAPTIVE_STATE";

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

        let (sample, last_error) = read_sample(config);
        let triggers = triggers_for_sample(config, &sample);
        let active_triggers = if last_error.is_some() {
            self.active_until = None;
            self.retained_triggers.clear();
            Vec::new()
        } else if triggers.is_empty() {
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
            last_error,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_external_online: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_system_battery_present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_battery_discharging: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_battery_capacity_percent: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_battery_power_microwatts: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_busy_percent_avg: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_busy_percent_max: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gpu_busy_sources: Vec<String>,
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

pub fn output_action(config: &GilderConfig, output_name: &str) -> crate::config::AdaptiveAction {
    config
        .outputs
        .get(output_name)
        .and_then(|output| output.adaptive.action)
        .unwrap_or(config.adaptive.action)
}

fn read_system_sample() -> AdaptiveSystemSample {
    let mut sample = read_power_supply_sample(SYS_CLASS_POWER_SUPPLY).unwrap_or_default();
    sample.cpu_pressure_some_avg10_x100 = read_pressure_some_avg10_x100(PROC_PRESSURE_CPU).ok();
    sample.memory_pressure_some_avg10_x100 =
        read_pressure_some_avg10_x100(PROC_PRESSURE_MEMORY).ok();
    sample.temperature_max_millicelsius = read_temperature_max_millicelsius(SYS_CLASS_THERMAL)
        .ok()
        .flatten();
    if let Ok(Some(gpu_busy)) = read_gpu_busy_sample(SYS_CLASS_DRM) {
        sample.gpu_busy_percent_avg = Some(gpu_busy.avg);
        sample.gpu_busy_percent_max = Some(gpu_busy.max);
        sample.gpu_busy_sources = gpu_busy.sources;
    }
    sample
}

fn read_sample(config: &GilderConfig) -> (AdaptiveSystemSample, Option<String>) {
    match std::env::var(ENV_ADAPTIVE_STATE) {
        Ok(value) => match validation_sample_override(config, &value) {
            Ok(Some(sample)) => (sample, None),
            Ok(None) => (read_system_sample(), None),
            Err(message) => (AdaptiveSystemSample::default(), Some(message)),
        },
        Err(std::env::VarError::NotPresent) => (read_system_sample(), None),
        Err(err) => (
            AdaptiveSystemSample::default(),
            Some(format!("invalid {ENV_ADAPTIVE_STATE}: {err}")),
        ),
    }
}

fn validation_sample_override(
    config: &GilderConfig,
    value: &str,
) -> Result<Option<AdaptiveSystemSample>, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok(None);
    }

    let mut sample = AdaptiveSystemSample {
        cpu_pressure_some_avg10_x100: Some(0),
        memory_pressure_some_avg10_x100: Some(0),
        temperature_max_millicelsius: Some(0),
        power_external_online: Some(true),
        power_system_battery_present: Some(false),
        ..AdaptiveSystemSample::default()
    };

    match value.as_str() {
        "inactive" | "none" | "clear" => {}
        "cpu" | "cpu-pressure" => {
            sample.cpu_pressure_some_avg10_x100 = Some(pressure_override_value(
                config.adaptive.cpu_pressure_threshold_percent,
            ));
        }
        "memory" | "memory-pressure" => {
            sample.memory_pressure_some_avg10_x100 = Some(pressure_override_value(
                config.adaptive.memory_pressure_threshold_percent,
            ));
        }
        "temperature" | "thermal" => {
            sample.temperature_max_millicelsius = Some(temperature_override_millicelsius(
                config.adaptive.temperature_threshold_celsius,
            ));
        }
        "all" => {
            sample.cpu_pressure_some_avg10_x100 = Some(pressure_override_value(
                config.adaptive.cpu_pressure_threshold_percent,
            ));
            sample.memory_pressure_some_avg10_x100 = Some(pressure_override_value(
                config.adaptive.memory_pressure_threshold_percent,
            ));
            sample.temperature_max_millicelsius = Some(temperature_override_millicelsius(
                config.adaptive.temperature_threshold_celsius,
            ));
        }
        _ => {
            return Err(format!(
                "invalid {ENV_ADAPTIVE_STATE}: expected inactive, cpu-pressure, memory-pressure, temperature, or all"
            ));
        }
    }
    Ok(Some(sample))
}

fn pressure_override_value(threshold_percent: u32) -> u32 {
    threshold_percent.max(1).saturating_mul(100)
}

fn temperature_override_millicelsius(threshold_celsius: u32) -> i32 {
    let clamped_celsius = threshold_celsius.max(1).min((i32::MAX as u32) / 1_000);
    (clamped_celsius * 1_000) as i32
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

fn read_power_supply_sample(root: impl AsRef<Path>) -> io::Result<AdaptiveSystemSample> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(AdaptiveSystemSample::default());
        }
        Err(err) => return Err(err),
    };

    let mut sample = AdaptiveSystemSample::default();
    let mut battery_capacity_sum = 0u64;
    let mut battery_capacity_count = 0u64;

    for entry in entries {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }
        let supply_type = read_trimmed(path.join("type"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if supply_type == "battery" {
            if !is_system_battery(&path) {
                continue;
            }
            sample.power_system_battery_present = Some(true);
            match read_trimmed(path.join("status"))
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "discharging" => sample.power_battery_discharging = Some(true),
                "charging" | "full" | "not charging" => {
                    if sample.power_battery_discharging != Some(true) {
                        sample.power_battery_discharging = Some(false);
                    }
                }
                _ => {}
            }
            if let Some(capacity) = read_battery_capacity_percent(&path) {
                battery_capacity_sum += u64::from(capacity);
                battery_capacity_count += 1;
            }
            if sample.power_battery_power_microwatts.is_none() {
                sample.power_battery_power_microwatts = read_power_microwatts(&path);
            }
        } else if is_external_supply(&supply_type) {
            match read_bool(path.join("online")) {
                Some(true) => sample.power_external_online = Some(true),
                Some(false) => {
                    if sample.power_external_online != Some(true) {
                        sample.power_external_online = Some(false);
                    }
                }
                None => {}
            }
        }
    }

    if sample.power_system_battery_present.is_none() {
        sample.power_system_battery_present = Some(false);
    }
    if battery_capacity_count > 0 {
        sample.power_battery_capacity_percent =
            Some((battery_capacity_sum / battery_capacity_count) as u32);
    }
    Ok(sample)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GpuBusySample {
    avg: u32,
    max: u32,
    sources: Vec<String>,
}

fn read_gpu_busy_sample(root: impl AsRef<Path>) -> io::Result<Option<GpuBusySample>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };

    let mut values = Vec::new();
    let mut sources = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !(name.starts_with("card") || name.starts_with("renderD")) {
            continue;
        }
        let Some(value) = read_u32(path.join("device/gpu_busy_percent")) else {
            continue;
        };
        values.push(value);
        sources.push(name.to_owned());
    }

    if values.is_empty() {
        return Ok(None);
    }

    let sum: u64 = values.iter().map(|value| u64::from(*value)).sum();
    let avg = (sum / values.len() as u64) as u32;
    let max = values.iter().copied().max().unwrap_or(0);
    sources.sort();
    Ok(Some(GpuBusySample { avg, max, sources }))
}

fn read_battery_capacity_percent(path: &Path) -> Option<u32> {
    read_u32(path.join("capacity")).or_else(|| {
        let now =
            read_u64(path.join("energy_now")).or_else(|| read_u64(path.join("charge_now")))?;
        let full =
            read_u64(path.join("energy_full")).or_else(|| read_u64(path.join("charge_full")))?;
        if full == 0 {
            return None;
        }
        Some(((now.saturating_mul(100)) / full).min(100) as u32)
    })
}

fn read_power_microwatts(path: &Path) -> Option<u64> {
    read_u64(path.join("power_now")).or_else(|| {
        let current_microamps = read_u64(path.join("current_now"))?;
        let voltage_microvolts = read_u64(path.join("voltage_now"))?;
        Some(current_microamps.saturating_mul(voltage_microvolts) / 1_000_000)
    })
}

fn is_system_battery(path: &Path) -> bool {
    read_trimmed(path.join("scope"))
        .map(|scope| !scope.eq_ignore_ascii_case("device"))
        .unwrap_or(true)
}

fn is_external_supply(supply_type: &str) -> bool {
    matches!(
        supply_type,
        "mains"
            | "usb"
            | "usb_ac"
            | "usb-c"
            | "usb_c"
            | "usb_dcp"
            | "usb_cdp"
            | "usb_aca"
            | "usb_pd"
            | "wireless"
    )
}

fn read_bool(path: impl AsRef<Path>) -> Option<bool> {
    match read_trimmed(path)?.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "online" => Some(true),
        "0" | "false" | "no" | "offline" => Some(false),
        _ => None,
    }
}

fn read_u32(path: impl AsRef<Path>) -> Option<u32> {
    read_trimmed(path)?.parse::<u32>().ok()
}

fn read_u64(path: impl AsRef<Path>) -> Option<u64> {
    read_trimmed(path)?.parse::<u64>().ok()
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
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
    use crate::config::{AdaptiveAction, AdaptiveConfig, OutputAdaptiveConfig, OutputConfig};

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
                    action: Some(AdaptiveAction::PauseUnfocused),
                },
                ..OutputConfig::default()
            },
        );

        assert!(monitoring_enabled(&config));
        assert!(output_enabled(&config, "eDP-1"));
        assert!(!output_enabled(&config, "HDMI-A-1"));
        assert_eq!(output_throttle_max_fps(&config, "eDP-1"), 9);
        assert_eq!(
            output_action(&config, "eDP-1"),
            AdaptiveAction::PauseUnfocused
        );
        assert_eq!(output_action(&config, "HDMI-A-1"), AdaptiveAction::Throttle);
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
                    action: None,
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
            ..AdaptiveSystemSample::default()
        };
        let triggers = triggers_for_sample(&config, &sample);

        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].metric, AdaptiveMetric::CpuPressureSomeAvg10);
    }

    #[test]
    fn validation_override_can_create_cpu_pressure_trigger() {
        let config = GilderConfig {
            adaptive: AdaptiveConfig {
                cpu_pressure_threshold_percent: 20,
                ..AdaptiveConfig::default()
            },
            ..GilderConfig::default()
        };
        let sample = validation_sample_override(&config, "cpu-pressure")
            .unwrap()
            .unwrap();
        let triggers = triggers_for_sample(&config, &sample);

        assert_eq!(sample.cpu_pressure_some_avg10_x100, Some(2_000));
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].metric, AdaptiveMetric::CpuPressureSomeAvg10);
        assert_eq!(triggers[0].value_x100, 2_000);
        assert_eq!(triggers[0].threshold_x100, 2_000);
    }

    #[test]
    fn validation_override_inactive_does_not_trigger() {
        let config = GilderConfig::default();
        let sample = validation_sample_override(&config, "inactive")
            .unwrap()
            .unwrap();

        assert!(triggers_for_sample(&config, &sample).is_empty());
    }

    #[test]
    fn validation_override_rejects_unknown_state() {
        let err = validation_sample_override(&GilderConfig::default(), "busy").unwrap_err();

        assert!(err.contains("GILDER_ADAPTIVE_STATE"));
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

    #[test]
    fn reads_gpu_busy_from_drm_nodes() {
        let root = TempDir::new("adaptive-gpu");
        fs::create_dir_all(root.path().join("card0/device")).unwrap();
        fs::write(root.path().join("card0/device/gpu_busy_percent"), "30\n").unwrap();
        fs::create_dir_all(root.path().join("renderD128/device")).unwrap();
        fs::write(
            root.path().join("renderD128/device/gpu_busy_percent"),
            "70\n",
        )
        .unwrap();
        fs::create_dir_all(root.path().join("version/device")).unwrap();
        fs::write(root.path().join("version/device/gpu_busy_percent"), "99\n").unwrap();

        let sample = read_gpu_busy_sample(root.path()).unwrap().unwrap();

        assert_eq!(sample.avg, 50);
        assert_eq!(sample.max, 70);
        assert_eq!(
            sample.sources,
            vec!["card0".to_owned(), "renderD128".to_owned()]
        );
    }

    #[test]
    fn missing_gpu_busy_reports_no_sample() {
        let root = TempDir::new("adaptive-gpu-missing");
        fs::create_dir_all(root.path().join("card0/device")).unwrap();

        assert_eq!(read_gpu_busy_sample(root.path()).unwrap(), None);
    }

    #[test]
    fn reads_power_supply_details() {
        let root = TempDir::new("adaptive-power");
        write_supply(
            root.path(),
            "BAT0",
            &[
                ("type", "Battery"),
                ("scope", "System"),
                ("status", "Discharging"),
                ("capacity", "72"),
                ("power_now", "12345678"),
            ],
        );
        write_supply(
            root.path(),
            "mouse",
            &[
                ("type", "Battery"),
                ("scope", "Device"),
                ("status", "Discharging"),
                ("capacity", "10"),
            ],
        );
        write_supply(root.path(), "AC", &[("type", "Mains"), ("online", "1")]);

        let sample = read_power_supply_sample(root.path()).unwrap();

        assert_eq!(sample.power_external_online, Some(true));
        assert_eq!(sample.power_system_battery_present, Some(true));
        assert_eq!(sample.power_battery_discharging, Some(true));
        assert_eq!(sample.power_battery_capacity_percent, Some(72));
        assert_eq!(sample.power_battery_power_microwatts, Some(12_345_678));
    }

    #[test]
    fn estimates_battery_capacity_and_power_from_charge_current_voltage() {
        let root = TempDir::new("adaptive-power-estimated");
        write_supply(
            root.path(),
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Charging"),
                ("charge_now", "40"),
                ("charge_full", "80"),
                ("current_now", "1500000"),
                ("voltage_now", "12000000"),
            ],
        );
        write_supply(root.path(), "AC", &[("type", "USB-C"), ("online", "0")]);

        let sample = read_power_supply_sample(root.path()).unwrap();

        assert_eq!(sample.power_external_online, Some(false));
        assert_eq!(sample.power_system_battery_present, Some(true));
        assert_eq!(sample.power_battery_discharging, Some(false));
        assert_eq!(sample.power_battery_capacity_percent, Some(50));
        assert_eq!(sample.power_battery_power_microwatts, Some(18_000_000));
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

    fn write_supply(root: &std::path::Path, name: &str, fields: &[(&str, &str)]) {
        let path = root.join(name);
        fs::create_dir_all(&path).unwrap();
        for (field, value) in fields {
            fs::write(path.join(field), value).unwrap();
        }
    }
}

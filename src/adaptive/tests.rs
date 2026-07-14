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
fn gpu_busy_threshold_creates_trigger() {
    let config = GilderConfig {
        adaptive: AdaptiveConfig {
            gpu_busy_threshold_percent: 80,
            ..AdaptiveConfig::default()
        },
        ..GilderConfig::default()
    };
    let sample = AdaptiveSystemSample {
        gpu_busy_percent_avg: Some(70),
        gpu_busy_percent_max: Some(85),
        ..AdaptiveSystemSample::default()
    };
    let triggers = triggers_for_sample(&config, &sample);

    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].metric, AdaptiveMetric::GpuBusyPercent);
    assert_eq!(triggers[0].value_x100, 8_500);
    assert_eq!(triggers[0].threshold_x100, 8_000);
}

#[test]
fn low_battery_threshold_creates_trigger_only_while_discharging() {
    let config = GilderConfig {
        adaptive: AdaptiveConfig {
            battery_capacity_threshold_percent: 25,
            ..AdaptiveConfig::default()
        },
        ..GilderConfig::default()
    };
    let discharging_sample = AdaptiveSystemSample {
        power_system_battery_present: Some(true),
        power_battery_discharging: Some(true),
        power_battery_capacity_percent: Some(25),
        ..AdaptiveSystemSample::default()
    };
    let charging_sample = AdaptiveSystemSample {
        power_system_battery_present: Some(true),
        power_battery_discharging: Some(false),
        power_battery_capacity_percent: Some(10),
        ..AdaptiveSystemSample::default()
    };

    let triggers = triggers_for_sample(&config, &discharging_sample);
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].metric, AdaptiveMetric::BatteryCapacityPercent);
    assert_eq!(triggers[0].value_x100, 2_500);
    assert_eq!(triggers[0].threshold_x100, 2_500);
    assert!(triggers_for_sample(&config, &charging_sample).is_empty());
}

#[test]
fn validation_overrides_can_create_gpu_and_low_battery_triggers() {
    let config = GilderConfig {
        adaptive: AdaptiveConfig {
            gpu_busy_threshold_percent: 80,
            battery_capacity_threshold_percent: 30,
            ..AdaptiveConfig::default()
        },
        ..GilderConfig::default()
    };
    let gpu_sample = validation_sample_override(&config, "gpu-busy")
        .unwrap()
        .unwrap();
    let battery_sample = validation_sample_override(&config, "low-battery")
        .unwrap()
        .unwrap();

    let gpu_triggers = triggers_for_sample(&config, &gpu_sample);
    assert_eq!(gpu_sample.gpu_busy_percent_max, Some(80));
    assert_eq!(gpu_triggers.len(), 1);
    assert_eq!(gpu_triggers[0].metric, AdaptiveMetric::GpuBusyPercent);

    let battery_triggers = triggers_for_sample(&config, &battery_sample);
    assert_eq!(battery_sample.power_battery_capacity_percent, Some(30));
    assert_eq!(battery_triggers.len(), 1);
    assert_eq!(
        battery_triggers[0].metric,
        AdaptiveMetric::BatteryCapacityPercent
    );
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

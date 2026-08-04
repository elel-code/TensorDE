#![allow(unexpected_cfgs)] // `tensor-kdl` derive emits optional downstream DOM impls.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use tensor_kdl::Decode;

const MAX_TIMEOUT_SECONDS: u64 = u32::MAX as u64 / 1_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdleConfig {
    pub enabled: bool,
    pub respect_inhibitors: bool,
    pub ac: PowerPolicy,
    pub battery: PowerPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PowerPolicy {
    pub monitor_off_after_ms: Option<u32>,
    pub lock_after_ms: Option<u32>,
    pub suspend_after_ms: Option<u32>,
    pub post_lock_monitor_off_after_ms: Option<u32>,
}

impl IdleConfig {
    pub fn resolve_path() -> PathBuf {
        env::var_os("TENSOR_IDLE_CONFIG")
            .map(PathBuf::from)
            .or_else(|| xdg_config_home().map(|path| path.join("tensor/idle.kdl")))
            .unwrap_or_else(|| PathBuf::from("/etc/tensor/idle.kdl"))
    }

    pub fn load_default_path() -> Result<Self, IdleConfigError> {
        Self::load_or_default(&Self::resolve_path())
    }

    pub fn load_or_default(path: &Path) -> Result<Self, IdleConfigError> {
        match fs::read_to_string(path) {
            Ok(document) => Self::from_kdl(path, &document),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(IdleConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    fn from_kdl(path: &Path, document: &str) -> Result<Self, IdleConfigError> {
        let parsed: FileConfig =
            tensor_kdl::read(document).map_err(|error| IdleConfigError::Parse {
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

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            respect_inhibitors: true,
            ac: PowerPolicy {
                monitor_off_after_ms: Some(600_000),
                lock_after_ms: Some(900_000),
                suspend_after_ms: Some(1_800_000),
                post_lock_monitor_off_after_ms: Some(30_000),
            },
            battery: PowerPolicy {
                monitor_off_after_ms: Some(300_000),
                lock_after_ms: Some(600_000),
                suspend_after_ms: Some(900_000),
                post_lock_monitor_off_after_ms: Some(15_000),
            },
        }
    }
}

#[derive(Debug, Default, Decode)]
struct FileConfig {
    #[kdl(child, unwrap(argument))]
    enabled: Option<bool>,
    #[kdl(child(name = "respect-inhibitors"), unwrap(argument))]
    respect_inhibitors: Option<bool>,
    #[kdl(child)]
    ac: Option<PowerFileConfig>,
    #[kdl(child)]
    battery: Option<PowerFileConfig>,
}

#[derive(Debug, Default, Decode)]
struct PowerFileConfig {
    #[kdl(child(name = "monitor-off-after-seconds"), unwrap(argument))]
    monitor_off_after_seconds: Option<u64>,
    #[kdl(child(name = "lock-after-seconds"), unwrap(argument))]
    lock_after_seconds: Option<u64>,
    #[kdl(child(name = "suspend-after-seconds"), unwrap(argument))]
    suspend_after_seconds: Option<u64>,
    #[kdl(child(name = "post-lock-monitor-off-after-seconds"), unwrap(argument))]
    post_lock_monitor_off_after_seconds: Option<u64>,
}

impl FileConfig {
    fn resolve(self) -> Result<IdleConfig, IdleConfigError> {
        let defaults = IdleConfig::default();
        Ok(IdleConfig {
            enabled: self.enabled.unwrap_or(defaults.enabled),
            respect_inhibitors: self
                .respect_inhibitors
                .unwrap_or(defaults.respect_inhibitors),
            ac: self
                .ac
                .map(|policy| policy.resolve("ac", defaults.ac))
                .transpose()?
                .unwrap_or(defaults.ac),
            battery: self
                .battery
                .map(|policy| policy.resolve("battery", defaults.battery))
                .transpose()?
                .unwrap_or(defaults.battery),
        })
    }
}

impl PowerFileConfig {
    fn resolve(
        self,
        power_source: &'static str,
        defaults: PowerPolicy,
    ) -> Result<PowerPolicy, IdleConfigError> {
        Ok(PowerPolicy {
            monitor_off_after_ms: timeout(
                power_source,
                "monitor-off-after-seconds",
                self.monitor_off_after_seconds,
                defaults.monitor_off_after_ms,
            )?,
            lock_after_ms: timeout(
                power_source,
                "lock-after-seconds",
                self.lock_after_seconds,
                defaults.lock_after_ms,
            )?,
            suspend_after_ms: timeout(
                power_source,
                "suspend-after-seconds",
                self.suspend_after_seconds,
                defaults.suspend_after_ms,
            )?,
            post_lock_monitor_off_after_ms: timeout(
                power_source,
                "post-lock-monitor-off-after-seconds",
                self.post_lock_monitor_off_after_seconds,
                defaults.post_lock_monitor_off_after_ms,
            )?,
        })
    }
}

fn timeout(
    power_source: &'static str,
    field: &'static str,
    seconds: Option<u64>,
    default_ms: Option<u32>,
) -> Result<Option<u32>, IdleConfigError> {
    let Some(seconds) = seconds else {
        return Ok(default_ms);
    };
    if seconds == 0 {
        return Ok(None);
    }
    if seconds > MAX_TIMEOUT_SECONDS {
        return Err(IdleConfigError::TimeoutOutOfRange {
            power_source,
            field,
            seconds,
            maximum: MAX_TIMEOUT_SECONDS,
        });
    }
    Ok(Some((seconds * 1_000) as u32))
}

fn xdg_config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

#[derive(Debug, thiserror::Error)]
pub enum IdleConfigError {
    #[error("failed to read Tensor Idle configuration {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse Tensor Idle configuration {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("{power_source}.{field} must be 0..={maximum} seconds, got {seconds}")]
    TimeoutOutOfRange {
        power_source: &'static str,
        field: &'static str,
        seconds: u64,
        maximum: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(document: &str) -> Result<IdleConfig, IdleConfigError> {
        IdleConfig::from_kdl(Path::new("idle.kdl"), document)
    }

    #[test]
    fn typed_kdl_keeps_ac_and_battery_policy_independent() {
        let config = parse(
            r#"
                enabled #true
                respect-inhibitors #false
                ac {
                    monitor-off-after-seconds 120
                    lock-after-seconds 300
                    suspend-after-seconds 0
                    post-lock-monitor-off-after-seconds 10
                }
                battery {
                    monitor-off-after-seconds 60
                    lock-after-seconds 180
                    suspend-after-seconds 600
                }
            "#,
        )
        .unwrap();
        assert!(!config.respect_inhibitors);
        assert_eq!(config.ac.monitor_off_after_ms, Some(120_000));
        assert_eq!(config.ac.suspend_after_ms, None);
        assert_eq!(config.battery.lock_after_ms, Some(180_000));
    }

    #[test]
    fn zero_disables_one_stage_without_weakening_other_defaults() {
        let config = parse("ac { lock-after-seconds 0 }").unwrap();
        assert_eq!(config.ac.lock_after_ms, None);
        assert_eq!(config.ac.monitor_off_after_ms, Some(600_000));
    }

    #[test]
    fn timeout_must_fit_the_wayland_protocol_field() {
        let error = parse("ac { lock-after-seconds 4294968 }").unwrap_err();
        assert!(matches!(error, IdleConfigError::TimeoutOutOfRange { .. }));
    }

    #[test]
    fn malformed_kdl_keeps_the_named_source() {
        assert!(parse("ac {").unwrap_err().to_string().contains("idle.kdl"));
    }
}

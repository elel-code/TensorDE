use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::Deserialize;
use tensor_util::OutputScale;
use thiserror::Error;

use crate::{
    layout::{LayoutKind, LayoutLength, LayoutOptions},
    render::{GpuPreference, ParseGpuPreferenceError},
    scene::SceneAppearance,
    service::{ParseSystemdModeError, SystemdMode},
    xwayland::XWaylandConfig,
};

mod appearance;
pub use appearance::AppearanceConfigError;
use appearance::AppearanceFileConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub initial_layout: LayoutKind,
    pub layout_options: LayoutOptions,
    pub ipc_socket: PathBuf,
    pub gpu_preference: GpuPreference,
    pub render_device: Option<PathBuf>,
    pub output_rules: BTreeMap<String, OutputRule>,
    pub appearance: SceneAppearance,
    pub systemd: SystemdMode,
    pub xwayland: XWaylandConfig,
    pub startup_commands: Vec<StartupCommand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupCommand {
    pub argv: Vec<String>,
}

/// Per-connector policy resolved at the configuration boundary.
///
/// The DRM backend remains responsible for discovering the supported mode
/// list. This value only expresses the user's stable intent, so a hotplug or
/// a new EDID never leaks parser-specific state into the output policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputRule {
    pub scale: Option<OutputScale>,
    pub mode: Option<OutputMode>,
    /// Logical origin of this output when set; otherwise automatic placement.
    pub position: Option<(i32, i32)>,
    /// When false the connector is ignored for scanout (still discovered).
    pub enabled: bool,
    /// Cap automatic mode selection to this refresh (millihertz). Explicit
    /// `mode` with `@refresh` still wins when that exact mode exists.
    pub max_refresh_millihertz: Option<u32>,
}

impl Default for OutputRule {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputRule {
    pub const fn new() -> Self {
        Self {
            scale: None,
            mode: None,
            position: None,
            enabled: true,
            max_refresh_millihertz: None,
        }
    }
}

/// A DRM mode requested by its visible dimensions and optional exact refresh.
/// Refresh is stored in millihertz, the same unit Smithay exposes on
/// [`smithay::output::Mode`], which prevents a floating-point comparison at
/// the configuration-to-DRM boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputMode {
    pub width: u32,
    pub height: u32,
    pub refresh_millihertz: Option<u32>,
}

impl OutputMode {
    pub const fn new(width: u32, height: u32, refresh_millihertz: Option<u32>) -> Self {
        Self {
            width,
            height,
            refresh_millihertz,
        }
    }
}

impl Config {
    pub fn load_with_environment(path: &Path) -> Result<Self, ConfigError> {
        let mut config = Self::load_or_default(path)?;

        if let Some(layout) = env::var_os("TENSOR_LAYOUT") {
            let layout = layout.to_str().ok_or(ConfigError::NonUnicodeLayout)?;
            config.initial_layout = LayoutKind::from_str(layout)?;
        }
        if let Some(socket) = env::var_os("TENSOR_IPC_SOCKET") {
            config.ipc_socket = PathBuf::from(socket);
        }
        if let Some(preference) = env::var_os("TENSOR_GPU") {
            let preference = preference.to_str().ok_or(ConfigError::NonUnicodeGpu)?;
            config.gpu_preference = GpuPreference::from_str(preference)?;
        }
        if let Some(path) = env::var_os("TENSOR_RENDER_DEVICE") {
            config.render_device = Some(path.into());
        }
        if let Some(mode) = env::var_os("TENSOR_SYSTEMD") {
            let mode = mode.to_str().ok_or(ConfigError::NonUnicodeSystemd)?;
            config.systemd = SystemdMode::from_str(mode)?;
        }
        if let Some(enabled) = env::var_os("TENSOR_XWAYLAND") {
            config.xwayland = XWaylandConfig::from_environment(enabled.to_str())?;
        }

        Ok(config)
    }

    pub fn resolve_path(cli_path: Option<PathBuf>) -> PathBuf {
        cli_path
            .or_else(|| env::var_os("TENSOR_CONFIG").map(PathBuf::from))
            .or_else(|| {
                let config_dir =
                    env::var_os("XDG_CONFIG_HOME")
                        .map(PathBuf::from)
                        .or_else(|| {
                            env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
                        })?;
                Some(config_dir.join("tensor/config.toml"))
            })
            .unwrap_or_else(|| PathBuf::from("/etc/tensor/config.toml"))
    }

    pub fn load_or_default(path: &Path) -> Result<Self, ConfigError> {
        match fs::read_to_string(path) {
            Ok(document) => Self::from_toml(path, &document),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if path
                    .extension()
                    .is_some_and(|extension| extension == "toml")
                {
                    let kdl = path.with_extension("kdl");
                    if kdl.is_file() {
                        return Err(ConfigError::LegacyKdl { path: kdl });
                    }
                }
                Ok(Self::default())
            }
            Err(source) => Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    fn from_toml(path: &Path, document: &str) -> Result<Self, ConfigError> {
        if path.extension().is_some_and(|extension| extension == "kdl") {
            return Err(ConfigError::LegacyKdl {
                path: path.to_owned(),
            });
        }
        let parsed: FileConfig = toml::from_str(document).map_err(|error| ConfigError::Parse {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
        parsed.resolve()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initial_layout: LayoutKind::default(),
            layout_options: LayoutOptions::default(),
            ipc_socket: env::var_os("XDG_RUNTIME_DIR")
                .map(|path| PathBuf::from(path).join("tensor.sock"))
                .unwrap_or_else(|| PathBuf::from("/tmp/tensor.sock")),
            gpu_preference: GpuPreference::default(),
            render_device: None,
            output_rules: BTreeMap::new(),
            appearance: SceneAppearance::default(),
            systemd: SystemdMode::default(),
            xwayland: XWaylandConfig::default(),
            startup_commands: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    layout: Option<LayoutFileConfig>,
    ipc_socket: Option<String>,
    gpu: Option<String>,
    render_device: Option<String>,
    #[serde(default)]
    outputs: Vec<OutputFileConfig>,
    #[serde(default)]
    appearance: Option<AppearanceFileConfig>,
    systemd: Option<String>,
    xwayland: Option<bool>,
    #[serde(default)]
    spawn_at_startup: Vec<Vec<String>>,
}

impl FileConfig {
    fn resolve(self) -> Result<Config, ConfigError> {
        let (initial_layout, layout_options) = self
            .layout
            .map(LayoutFileConfig::resolve)
            .transpose()?
            .unwrap_or_else(|| (LayoutKind::default(), LayoutOptions::default()));
        let ipc_socket = self
            .ipc_socket
            .map(PathBuf::from)
            .unwrap_or_else(|| Config::default().ipc_socket);
        let gpu_preference = self
            .gpu
            .as_deref()
            .map(GpuPreference::from_str)
            .transpose()?
            .unwrap_or_default();
        let render_device = self.render_device.map(PathBuf::from);
        let output_rules = resolve_output_rules(self.outputs)?;
        let appearance = self
            .appearance
            .map(AppearanceFileConfig::resolve)
            .transpose()?
            .unwrap_or_default();
        let systemd = self
            .systemd
            .as_deref()
            .map(SystemdMode::from_str)
            .transpose()?
            .unwrap_or_default();
        let xwayland = self
            .xwayland
            .unwrap_or_else(|| XWaylandConfig::default().enabled());
        let startup_commands = self
            .spawn_at_startup
            .into_iter()
            .enumerate()
            .map(|(index, argv)| {
                if argv.is_empty() {
                    Err(ConfigError::EmptyStartupCommand { index })
                } else {
                    Ok(StartupCommand { argv })
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Config {
            initial_layout,
            layout_options,
            ipc_socket,
            gpu_preference,
            render_device,
            output_rules,
            appearance,
            systemd,
            xwayland: XWaylandConfig::new(xwayland),
            startup_commands,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputFileConfig {
    name: String,
    scale: Option<f64>,
    mode: Option<String>,
    position: Option<OutputPositionConfig>,
    enabled: Option<bool>,
    max_refresh_millihertz: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputPositionConfig {
    x: i32,
    y: i32,
}

fn resolve_output_rules(
    outputs: Vec<OutputFileConfig>,
) -> Result<BTreeMap<String, OutputRule>, ConfigError> {
    let mut rules = BTreeMap::new();
    for output in outputs {
        let scale = output
            .scale
            .map(|value| {
                OutputScale::from_f64(value).ok_or_else(|| ConfigError::InvalidOutputScale {
                    output: output.name.clone(),
                    message: "must be finite and between 0.1 and 10".to_owned(),
                })
            })
            .transpose()?;
        let mode = output
            .mode
            .as_deref()
            .map(parse_output_mode)
            .transpose()
            .map_err(|message| ConfigError::InvalidOutputMode {
                output: output.name.clone(),
                mode: output.mode.clone().expect("a parsed mode string exists"),
                message,
            })?;
        if let Some(cap) = output.max_refresh_millihertz
            && cap == 0
        {
            return Err(ConfigError::InvalidOutputRefreshCap {
                output: output.name,
            });
        }
        let rule = OutputRule {
            scale,
            mode,
            position: output.position.map(|value| (value.x, value.y)),
            enabled: output.enabled.unwrap_or(true),
            max_refresh_millihertz: output.max_refresh_millihertz,
        };
        if rules.insert(output.name.clone(), rule).is_some() {
            return Err(ConfigError::DuplicateOutput {
                output: output.name,
            });
        }
    }
    Ok(rules)
}

fn parse_output_mode(value: &str) -> Result<OutputMode, String> {
    let (resolution, refresh) = match value.split_once('@') {
        Some((resolution, refresh)) if !refresh.contains('@') => (resolution, Some(refresh)),
        Some(_) => return Err("contains more than one `@` separator".to_owned()),
        None => (value, None),
    };
    let (width, height) = resolution
        .split_once('x')
        .filter(|(_, height)| !height.contains('x'))
        .ok_or_else(|| {
            "must use the form `<width>x<height>` or `<width>x<height>@<Hz>`".to_owned()
        })?;
    let width = parse_mode_dimension(width, "width")?;
    let height = parse_mode_dimension(height, "height")?;
    let refresh_millihertz = refresh.map(parse_refresh_millihertz).transpose()?;
    Ok(OutputMode::new(width, height, refresh_millihertz))
}

fn parse_mode_dimension(value: &str, name: &str) -> Result<u32, String> {
    let dimension = value
        .parse::<u32>()
        .map_err(|_| format!("{name} must be a positive unsigned integer"))?;
    (dimension > 0)
        .then_some(dimension)
        .ok_or_else(|| format!("{name} must be greater than zero"))
}

fn parse_refresh_millihertz(value: &str) -> Result<u32, String> {
    let (whole, fraction) = match value.split_once('.') {
        Some((whole, fraction)) if !fraction.contains('.') => (whole, Some(fraction)),
        Some(_) => return Err("refresh rate has more than one decimal point".to_owned()),
        None => (value, None),
    };
    let whole = whole
        .parse::<u32>()
        .map_err(|_| "refresh rate must be a positive decimal number".to_owned())?;
    let fraction = match fraction {
        None => 0,
        Some("") => {
            return Err("refresh rate must contain digits after the decimal point".to_owned());
        }
        Some(value) if value.len() > 3 || !value.bytes().all(|byte| byte.is_ascii_digit()) => {
            return Err("refresh rate accepts at most three decimal places".to_owned());
        }
        Some(value) => value
            .parse::<u32>()
            .expect("a validated decimal fraction parses")
            .checked_mul(10_u32.pow(u32::try_from(3 - value.len()).unwrap_or(0)))
            .expect("a three-digit refresh fraction fits u32"),
    };
    let millihertz = whole
        .checked_mul(1_000)
        .and_then(|whole| whole.checked_add(fraction))
        .ok_or_else(|| "refresh rate is too large".to_owned())?;
    if millihertz == 0 {
        return Err("refresh rate must be greater than zero".to_owned());
    }
    if millihertz > i32::MAX as u32 {
        return Err("refresh rate exceeds the DRM millihertz range".to_owned());
    }
    Ok(millihertz)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayoutFileConfig {
    kind: String,
    gaps: Option<u32>,
    default_column_width: Option<LayoutLengthConfig>,
    master_width: Option<LayoutLengthConfig>,
}

impl LayoutFileConfig {
    fn resolve(self) -> Result<(LayoutKind, LayoutOptions), ConfigError> {
        let kind = LayoutKind::from_str(&self.kind)?;
        let defaults = LayoutOptions::default();
        let gap = self.gaps.unwrap_or(defaults.gap);
        if gap > 100_000 {
            return Err(ConfigError::InvalidLayoutOption {
                option: "gaps",
                message: "must be at most 100000 logical pixels".to_owned(),
            });
        }
        Ok((
            kind,
            LayoutOptions {
                gap,
                scrolling_default_width: resolve_layout_length(
                    "default_column_width",
                    self.default_column_width,
                    defaults.scrolling_default_width,
                )?,
                master_width: resolve_layout_length(
                    "master_width",
                    self.master_width,
                    defaults.master_width,
                )?,
            },
        ))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayoutLengthConfig {
    proportion: Option<f64>,
    fixed: Option<u32>,
}

fn resolve_layout_length(
    option: &'static str,
    configured: Option<LayoutLengthConfig>,
    default: LayoutLength,
) -> Result<LayoutLength, ConfigError> {
    let Some(configured) = configured else {
        return Ok(default);
    };
    match (configured.proportion, configured.fixed) {
        (Some(proportion), None) if proportion.is_finite() && proportion > 0.0 => {
            let scaled = (proportion * 10_000.0).round();
            if !(1.0..=100_000_000.0).contains(&scaled) {
                return Err(ConfigError::InvalidLayoutOption {
                    option,
                    message: "proportion must be between 0.0001 and 10000".to_owned(),
                });
            }
            Ok(LayoutLength::proportion(scaled as u32, 10_000))
        }
        (None, Some(fixed)) if fixed > 0 => Ok(LayoutLength::fixed(fixed)),
        (Some(_), Some(_)) => Err(ConfigError::InvalidLayoutOption {
            option,
            message: "set either proportion or fixed, not both".to_owned(),
        }),
        (Some(_), None) => Err(ConfigError::InvalidLayoutOption {
            option,
            message: "proportion must be finite and positive".to_owned(),
        }),
        (None, Some(_)) => Err(ConfigError::InvalidLayoutOption {
            option,
            message: "fixed width must be positive".to_owned(),
        }),
        (None, None) => Err(ConfigError::InvalidLayoutOption {
            option,
            message: "requires a proportion or fixed field".to_owned(),
        }),
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("TENSOR_LAYOUT is not valid Unicode")]
    NonUnicodeLayout,
    #[error("TENSOR_GPU is not valid Unicode")]
    NonUnicodeGpu,
    #[error("TENSOR_SYSTEMD is not valid Unicode")]
    NonUnicodeSystemd,
    #[error("spawn_at_startup entry {index} must contain a program")]
    EmptyStartupCommand { index: usize },
    #[error("invalid layout option {option}: {message}")]
    InvalidLayoutOption {
        option: &'static str,
        message: String,
    },
    #[error("invalid scale for output {output}: {message}")]
    InvalidOutputScale { output: String, message: String },
    #[error("output {output} has more than one rule")]
    DuplicateOutput { output: String },
    #[error("invalid mode {mode:?} for output {output}: {message}")]
    InvalidOutputMode {
        output: String,
        mode: String,
        message: String,
    },
    #[error("max_refresh_millihertz for output {output} must be greater than zero")]
    InvalidOutputRefreshCap { output: String },
    #[error("failed to read config {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error(
        "KDL configuration is no longer supported ({path}); migrate to config.toml (see docs/configuration.md)"
    )]
    LegacyKdl { path: PathBuf },
    #[error(transparent)]
    UnknownLayout(#[from] crate::layout::ParseLayoutError),
    #[error(transparent)]
    UnknownGpu(#[from] ParseGpuPreferenceError),
    #[error(transparent)]
    UnknownSystemd(#[from] ParseSystemdModeError),
    #[error(transparent)]
    XWayland(#[from] crate::xwayland::XWaylandConfigError),
    #[error(transparent)]
    Appearance(#[from] AppearanceConfigError),
}

#[cfg(test)]
mod tests;

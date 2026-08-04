#![allow(unexpected_cfgs)] // `tensor-kdl` derive also emits optional downstream DOM impls.

use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use tensor_kdl::{CtxResult, Decode, ErrorCode, ErrorCtx, Flag, Located};
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
mod diagnostic;
mod reload;
mod scalar;
mod watcher;
mod worker;
pub use appearance::AppearanceConfigError;
pub use diagnostic::{
    ConfigDiagnostic, ConfigDiagnosticCategory, ConfigDiagnosticMetadata,
    MAX_DIAGNOSTIC_PATH_BYTES, MAX_DIAGNOSTIC_SUMMARY_BYTES, MAX_VALIDATION_COMMAND_BYTES,
};
pub use reload::{ConfigReloadFailure, ConfigReloadResult, ConfigTransaction};
use scalar::{
    LimitedLayoutGap, ParsedLayoutProportion, ParsedOutputMode, ParsedOutputScale,
    PositiveLayoutFixed, PositiveRefreshCap,
};
pub(crate) use watcher::ConfigWatcher;
pub(crate) use worker::{
    ConfigReloadOutcome, ConfigReloadSubmitError, ConfigReloadSubmitter, ConfigReloadWorker,
    ConfigReloadWorkerError, MAX_PENDING_CONFIG_RELOAD_RESULTS,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub(crate) initial_layout: LayoutKind,
    pub(crate) layout_options: LayoutOptions,
    pub(crate) ipc_socket: PathBuf,
    pub(crate) gpu_preference: GpuPreference,
    pub(crate) render_device: Option<PathBuf>,
    pub(crate) output_rules: BTreeMap<String, OutputRule>,
    pub(crate) appearance: SceneAppearance,
    pub(crate) systemd: SystemdMode,
    pub(crate) xwayland: XWaylandConfig,
    pub(crate) startup_commands: Vec<StartupCommand>,
    pub(crate) environment: EnvironmentConfig,
    pub(crate) cursor: CursorConfig,
    pub(crate) debug: DebugConfig,
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
/// Refresh is stored in millihertz, the same unit as [`tensor_host::PhysicalMode`],
/// which prevents a floating-point comparison at the configuration-to-DRM boundary.
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
        Self::load_or_default(path)?.with_environment_overrides()
    }

    pub(crate) fn load_required_with_environment(path: &Path) -> Result<Self, ConfigError> {
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            return Err(ConfigError::LegacyToml {
                path: path.to_owned(),
            });
        }
        let document = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        Self::from_kdl(path, &document)?.with_environment_overrides()
    }

    fn with_environment_overrides(mut self) -> Result<Self, ConfigError> {
        if let Some(layout) = env::var_os("TENSOR_LAYOUT") {
            let layout = layout.to_str().ok_or(ConfigError::NonUnicodeLayout)?;
            self.initial_layout = LayoutKind::from_str(layout)?;
        }
        if let Some(socket) = env::var_os("TENSOR_IPC_SOCKET") {
            self.ipc_socket = PathBuf::from(socket);
        }
        if let Some(preference) = env::var_os("TENSOR_GPU") {
            let preference = preference.to_str().ok_or(ConfigError::NonUnicodeGpu)?;
            self.gpu_preference = GpuPreference::from_str(preference)?;
        }
        if let Some(path) = env::var_os("TENSOR_RENDER_DEVICE") {
            self.render_device = Some(path.into());
        }
        if let Some(mode) = env::var_os("TENSOR_SYSTEMD") {
            let mode = mode.to_str().ok_or(ConfigError::NonUnicodeSystemd)?;
            self.systemd = SystemdMode::from_str(mode)?;
        }
        if let Some(enabled) = env::var_os("TENSOR_XWAYLAND") {
            self.xwayland = XWaylandConfig::from_environment(enabled.to_str())?;
        }

        Ok(self)
    }

    pub(crate) fn restart_required_change(&self, candidate: &Self) -> Option<&'static str> {
        if self.ipc_socket != candidate.ipc_socket {
            Some("ipc-socket")
        } else if self.gpu_preference != candidate.gpu_preference {
            Some("gpu")
        } else if self.render_device != candidate.render_device {
            Some("render-device")
        } else if self.output_rules != candidate.output_rules {
            Some("output")
        } else if self.systemd != candidate.systemd {
            Some("systemd")
        } else if self.xwayland != candidate.xwayland {
            Some("xwayland")
        } else if self.startup_commands != candidate.startup_commands {
            Some("spawn-at-startup")
        } else if self.environment != candidate.environment {
            Some("environment")
        } else {
            None
        }
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
                Some(config_dir.join("tensor/config.kdl"))
            })
            .unwrap_or_else(|| PathBuf::from("/etc/tensor/config.kdl"))
    }

    pub fn load_or_default(path: &Path) -> Result<Self, ConfigError> {
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            return Err(ConfigError::LegacyToml {
                path: path.to_owned(),
            });
        }
        match fs::read_to_string(path) {
            Ok(document) => Self::from_kdl(path, &document),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if path.extension().is_some_and(|extension| extension == "kdl") {
                    let toml = path.with_extension("toml");
                    if toml.is_file() {
                        return Err(ConfigError::LegacyToml { path: toml });
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

    fn from_kdl(path: &Path, document: &str) -> Result<Self, ConfigError> {
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            return Err(ConfigError::LegacyToml {
                path: path.to_owned(),
            });
        }
        let parsed: FileConfig = tensor_kdl::read(document)
            .map_err(|error| ConfigError::Parse(ConfigDiagnostic::new(path, document, error)))?;
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
            environment: EnvironmentConfig::default(),
            cursor: CursorConfig::default(),
            debug: DebugConfig::default(),
        }
    }
}

#[derive(Debug, Default, Decode)]
struct FileConfig {
    #[kdl(child)]
    layout: Option<LayoutFileConfig>,
    #[kdl(child(name = "ipc-socket"), unwrap(argument))]
    ipc_socket: Option<String>,
    #[kdl(child, unwrap(argument))]
    gpu: Option<String>,
    #[kdl(child(name = "render-device"), unwrap(argument))]
    render_device: Option<String>,
    #[kdl(children(name = "output"))]
    outputs: Vec<OutputFileConfig>,
    #[kdl(child)]
    appearance: Option<AppearanceFileConfig>,
    #[kdl(child, unwrap(argument))]
    systemd: Option<String>,
    #[kdl(child, unwrap(argument))]
    xwayland: Option<bool>,
    #[kdl(children(name = "spawn-at-startup"))]
    spawn_at_startup: Vec<StartupCommandFileConfig>,
    #[kdl(child)]
    environment: Option<EnvironmentFileConfig>,
    #[kdl(child)]
    cursor: Option<CursorFileConfig>,
    #[kdl(child)]
    debug: Option<DebugFileConfig>,
}

#[derive(Debug, Default, Decode)]
struct AppearanceFileConfig {
    #[kdl(child(name = "focus-ring"))]
    focus_ring: Option<FocusRingFileConfig>,
    #[kdl(child(name = "window-shadow"))]
    window_shadow: Option<WindowShadowFileConfig>,
    #[kdl(child(name = "window-corners"))]
    window_corners: Option<WindowCornerFileConfig>,
}

impl AppearanceFileConfig {
    fn resolve(self) -> Result<SceneAppearance, AppearanceConfigError> {
        let focus_ring = match self.focus_ring {
            Some(focus_ring) => appearance::resolve_focus_ring(
                focus_ring.enabled,
                focus_ring.width,
                focus_ring.color,
            ),
            None => Ok(Default::default()),
        }?;
        let window_shadow = match self.window_shadow {
            Some(shadow) => appearance::resolve_window_shadow(
                shadow.enabled,
                shadow.offset_x,
                shadow.offset_y,
                shadow.blur_radius,
                shadow.spread,
                shadow.color,
            ),
            None => Ok(Default::default()),
        }?;
        let window_corners = appearance::resolve_window_corners(
            self.window_corners.and_then(|corners| corners.radius),
        )?;
        Ok(SceneAppearance {
            focus_ring,
            window_shadow,
            window_corners,
        })
    }
}

#[derive(Debug, Default, Decode)]
struct FocusRingFileConfig {
    #[kdl(property)]
    enabled: Option<bool>,
    #[kdl(property)]
    width: Option<u32>,
    #[kdl(property)]
    color: Option<String>,
}

#[derive(Debug, Default, Decode)]
struct WindowShadowFileConfig {
    #[kdl(property)]
    enabled: Option<bool>,
    #[kdl(property(name = "offset-x"))]
    offset_x: Option<i32>,
    #[kdl(property(name = "offset-y"))]
    offset_y: Option<i32>,
    #[kdl(property(name = "blur-radius"))]
    blur_radius: Option<u32>,
    #[kdl(property)]
    spread: Option<u32>,
    #[kdl(property)]
    color: Option<String>,
}

#[derive(Debug, Default, Decode)]
struct WindowCornerFileConfig {
    #[kdl(property)]
    radius: Option<u32>,
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
            .map(|(index, command)| {
                let argv = command.argv;
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
            environment: self
                .environment
                .map(EnvironmentFileConfig::resolve)
                .transpose()?
                .unwrap_or_default(),
            cursor: self
                .cursor
                .map(CursorFileConfig::resolve)
                .unwrap_or_default(),
            debug: self.debug.map(DebugFileConfig::resolve).unwrap_or_default(),
        })
    }
}

#[derive(Debug, Decode)]
struct StartupCommandFileConfig {
    #[kdl(arguments)]
    argv: Vec<String>,
}

#[derive(Debug, Decode)]
struct OutputFileConfig {
    #[kdl(argument)]
    name: String,
    #[kdl(property)]
    scale: Option<ParsedOutputScale>,
    #[kdl(property)]
    mode: Option<ParsedOutputMode>,
    #[kdl(child)]
    position: Option<OutputPositionConfig>,
    #[kdl(property)]
    enabled: Option<bool>,
    #[kdl(property(name = "max-refresh-millihertz"))]
    max_refresh_millihertz: Option<PositiveRefreshCap>,
}

#[derive(Debug, Decode)]
struct OutputPositionConfig {
    #[kdl(property)]
    x: i32,
    #[kdl(property)]
    y: i32,
}

fn resolve_output_rules(
    outputs: Vec<OutputFileConfig>,
) -> Result<BTreeMap<String, OutputRule>, ConfigError> {
    let mut rules = BTreeMap::new();
    for output in outputs {
        let rule = OutputRule {
            scale: output.scale.map(|value| value.0),
            mode: output.mode.map(|value| value.0),
            position: output.position.map(|value| (value.x, value.y)),
            enabled: output.enabled.unwrap_or(true),
            max_refresh_millihertz: output.max_refresh_millihertz.map(|value| value.0),
        };
        if rules.insert(output.name.clone(), rule).is_some() {
            return Err(ConfigError::DuplicateOutput {
                output: output.name,
            });
        }
    }
    Ok(rules)
}

#[derive(Debug, Decode)]
struct LayoutFileConfig {
    #[kdl(argument)]
    kind: String,
    #[kdl(property)]
    gaps: Option<LimitedLayoutGap>,
    #[kdl(child(name = "default-column-width"))]
    default_column_width: Option<LayoutLengthConfig>,
    #[kdl(child(name = "master-width"))]
    master_width: Option<LayoutLengthConfig>,
}

impl LayoutFileConfig {
    fn resolve(self) -> Result<(LayoutKind, LayoutOptions), ConfigError> {
        let kind = LayoutKind::from_str(&self.kind)?;
        let defaults = LayoutOptions::default();
        let gap = self.gaps.map(|value| value.0).unwrap_or(defaults.gap);
        Ok((
            kind,
            LayoutOptions {
                gap,
                scrolling_default_width: resolve_layout_length(
                    self.default_column_width,
                    defaults.scrolling_default_width,
                ),
                master_width: resolve_layout_length(self.master_width, defaults.master_width),
            },
        ))
    }
}

#[derive(Debug, Decode, Default)]
#[kdl(validate = "validate_kdl")]
struct LayoutLengthConfig {
    #[kdl(property)]
    proportion: Option<Located<ParsedLayoutProportion>>,
    #[kdl(property)]
    fixed: Option<Located<PositiveLayoutFixed>>,
}

impl LayoutLengthConfig {
    fn validate_kdl(&self, node_offset: usize) -> CtxResult<()> {
        match (&self.proportion, &self.fixed) {
            (Some(proportion), Some(fixed)) => Err(ErrorCtx::new(
                ErrorCode::DuplicateProperty,
                proportion.offset().max(fixed.offset()),
            )
            .with_message("set either proportion or fixed, not both")),
            (None, None) => Err(ErrorCtx::new(ErrorCode::MissingProperty, node_offset)
                .with_message("layout width requires proportion or fixed")),
            _ => Ok(()),
        }
    }
}

fn resolve_layout_length(
    configured: Option<LayoutLengthConfig>,
    default: LayoutLength,
) -> LayoutLength {
    let Some(configured) = configured else {
        return default;
    };
    match (configured.proportion, configured.fixed) {
        (Some(proportion), None) => proportion.into_inner().0,
        (None, Some(fixed)) => fixed.into_inner().0,
        _ => unreachable!("LayoutLengthConfig is validated during typed KDL decode"),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvironmentConfig {
    pub clear: Vec<String>,
    pub set: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorConfig {
    pub theme: String,
    pub size: u32,
    pub hide_when_typing: bool,
    pub hide_after_inactive_ms: Option<u32>,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            theme: "default".to_owned(),
            size: 24,
            hide_when_typing: false,
            hide_after_inactive_ms: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugConfig {
    pub frame_stats: bool,
    pub force_full_redraw: bool,
}

#[derive(Debug, Default, Decode)]
struct EnvironmentFileConfig {
    #[kdl(children(name = "clear"))]
    clear: Vec<EnvironmentClearFileConfig>,
    #[kdl(children(name = "set"))]
    set: Vec<EnvironmentSetFileConfig>,
}

impl EnvironmentFileConfig {
    fn resolve(self) -> Result<EnvironmentConfig, ConfigError> {
        let mut set = BTreeMap::new();
        for entry in self.set {
            if set.insert(entry.name.clone(), entry.value).is_some() {
                return Err(ConfigError::DuplicateEnvironmentVariable { name: entry.name });
            }
        }
        Ok(EnvironmentConfig {
            clear: self.clear.into_iter().map(|entry| entry.name).collect(),
            set,
        })
    }
}

#[derive(Debug, Decode)]
struct EnvironmentClearFileConfig {
    #[kdl(argument)]
    name: String,
}

#[derive(Debug, Decode)]
struct EnvironmentSetFileConfig {
    #[kdl(argument)]
    name: String,
    #[kdl(argument)]
    value: String,
}

#[derive(Debug, Default, Decode)]
struct CursorFileConfig {
    #[kdl(child(name = "xcursor-theme"), unwrap(argument))]
    theme: Option<String>,
    #[kdl(child(name = "xcursor-size"), unwrap(argument))]
    size: Option<u8>,
    #[kdl(child(name = "hide-when-typing"))]
    hide_when_typing: Option<Flag>,
    #[kdl(child(name = "hide-after-inactive-ms"), unwrap(argument))]
    hide_after_inactive_ms: Option<u32>,
}

impl CursorFileConfig {
    fn resolve(self) -> CursorConfig {
        let defaults = CursorConfig::default();
        CursorConfig {
            theme: self.theme.unwrap_or(defaults.theme),
            size: self.size.map(u32::from).unwrap_or(defaults.size),
            hide_when_typing: self.hide_when_typing.is_some(),
            hide_after_inactive_ms: self.hide_after_inactive_ms,
        }
    }
}

#[derive(Debug, Default, Decode)]
struct DebugFileConfig {
    #[kdl(property(name = "frame-stats"))]
    frame_stats: Option<bool>,
    #[kdl(property(name = "force-full-redraw"))]
    force_full_redraw: Option<bool>,
}

impl DebugFileConfig {
    fn resolve(self) -> DebugConfig {
        DebugConfig {
            frame_stats: self.frame_stats.unwrap_or(false),
            force_full_redraw: self.force_full_redraw.unwrap_or(false),
        }
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
    #[error("spawn-at-startup entry {index} must contain a program")]
    EmptyStartupCommand { index: usize },
    #[error("output {output} has more than one rule")]
    DuplicateOutput { output: String },
    #[error("environment variable {name:?} has more than one set rule")]
    DuplicateEnvironmentVariable { name: String },
    #[error("failed to read config {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse configuration: {0}")]
    Parse(#[source] ConfigDiagnostic),
    #[error(
        "TOML configuration is no longer supported ({path}); migrate to config.kdl (see docs/tensorland/configuration.md)"
    )]
    LegacyToml { path: PathBuf },
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
    #[error("configuration field `{field}` requires a compositor restart")]
    ReloadRequiresRestart { field: &'static str },
}

impl ConfigError {
    /// Rich source diagnostic for parser and typed-decode failures.
    ///
    /// Cross-field semantic errors produced after typed decoding remain
    /// ordinary `ConfigError` variants. Scalar policy errors are emitted from
    /// `DecodeScalar` and arrive here as source-aware [`Self::Parse`] values.
    pub fn diagnostic_report(&self) -> Option<miette::Report> {
        match self {
            Self::Parse(diagnostic) => Some(diagnostic.report()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;

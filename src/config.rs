use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use tensor_util::OutputScale;
use thiserror::Error;

use crate::{
    layout::{LayoutKind, LayoutLength, LayoutOptions},
    render::{GpuPreference, ParseGpuPreferenceError},
    service::{ParseSystemdModeError, SystemdMode},
    xwayland::XWaylandConfig,
};

mod scale;
use scale::ScaleValue;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub initial_layout: LayoutKind,
    pub layout_options: LayoutOptions,
    pub ipc_socket: PathBuf,
    pub gpu_preference: GpuPreference,
    pub render_device: Option<PathBuf>,
    pub output_scales: BTreeMap<String, OutputScale>,
    pub systemd: SystemdMode,
    pub xwayland: XWaylandConfig,
    pub startup_commands: Vec<StartupCommand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupCommand {
    pub argv: Vec<String>,
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
                Some(config_dir.join("tensor/config.kdl"))
            })
            .unwrap_or_else(|| PathBuf::from("/etc/tensor/config.kdl"))
    }

    pub fn load_or_default(path: &Path) -> Result<Self, ConfigError> {
        match fs::read_to_string(path) {
            Ok(document) => Self::from_kdl(path, &document),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    fn from_kdl(path: &Path, document: &str) -> Result<Self, ConfigError> {
        let path_name = path.to_string_lossy();
        let parsed: FileConfig =
            knus::parse(&path_name, document).map_err(|error| ConfigError::Parse {
                path: path.to_owned(),
                message: error.to_string(),
            })?;
        let (initial_layout, layout_options) = parsed
            .layout
            .map(LayoutFileConfig::resolve)
            .transpose()?
            .unwrap_or_else(|| (LayoutKind::default(), LayoutOptions::default()));
        let ipc_socket = parsed
            .ipc_socket
            .map(PathBuf::from)
            .unwrap_or_else(|| Self::default().ipc_socket);
        let gpu_preference = parsed
            .gpu
            .as_deref()
            .map(GpuPreference::from_str)
            .transpose()?
            .unwrap_or_default();
        let render_device = parsed.render_device.map(PathBuf::from);
        let output_scales = resolve_output_scales(parsed.outputs)?;
        let systemd = parsed
            .systemd
            .as_deref()
            .map(SystemdMode::from_str)
            .transpose()?
            .unwrap_or_default();
        let xwayland = parsed
            .xwayland
            .unwrap_or_else(|| XWaylandConfig::default().enabled());
        let startup_commands = parsed
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

        Ok(Self {
            initial_layout,
            layout_options,
            ipc_socket,
            gpu_preference,
            render_device,
            output_scales,
            systemd,
            xwayland: XWaylandConfig::new(xwayland),
            startup_commands,
        })
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
            output_scales: BTreeMap::new(),
            systemd: SystemdMode::default(),
            xwayland: XWaylandConfig::default(),
            startup_commands: Vec::new(),
        }
    }
}

#[derive(Debug, knus::Decode)]
struct FileConfig {
    #[knus(child)]
    layout: Option<LayoutFileConfig>,
    #[knus(child, unwrap(argument))]
    ipc_socket: Option<String>,
    #[knus(child, unwrap(argument))]
    gpu: Option<String>,
    #[knus(child, unwrap(argument))]
    render_device: Option<String>,
    #[knus(children(name = "output"))]
    outputs: Vec<OutputFileConfig>,
    #[knus(child, unwrap(argument))]
    systemd: Option<String>,
    #[knus(child, unwrap(argument))]
    xwayland: Option<bool>,
    #[knus(children(name = "spawn-at-startup"), unwrap(arguments))]
    spawn_at_startup: Vec<Vec<String>>,
}

#[derive(Debug, knus::Decode)]
struct OutputFileConfig {
    #[knus(argument)]
    name: String,
    #[knus(child, unwrap(argument))]
    scale: Option<ScaleValue>,
}

fn resolve_output_scales(
    outputs: Vec<OutputFileConfig>,
) -> Result<BTreeMap<String, OutputScale>, ConfigError> {
    let mut scales = BTreeMap::new();
    for output in outputs {
        let Some(value) = output.scale else {
            continue;
        };
        let scale =
            OutputScale::from_f64(value.get()).ok_or_else(|| ConfigError::InvalidOutputScale {
                output: output.name.clone(),
                message: "must be finite and between 0.1 and 10".to_owned(),
            })?;
        if scales.insert(output.name.clone(), scale).is_some() {
            return Err(ConfigError::DuplicateOutput {
                output: output.name,
            });
        }
    }
    Ok(scales)
}

#[derive(Debug, knus::Decode)]
struct LayoutFileConfig {
    #[knus(argument)]
    kind: String,
    #[knus(child, unwrap(argument))]
    gaps: Option<u32>,
    #[knus(child)]
    default_column_width: Option<LayoutLengthConfig>,
    #[knus(child)]
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
                    "default-column-width",
                    self.default_column_width,
                    defaults.scrolling_default_width,
                )?,
                master_width: resolve_layout_length(
                    "master-width",
                    self.master_width,
                    defaults.master_width,
                )?,
            },
        ))
    }
}

#[derive(Debug, knus::Decode)]
struct LayoutLengthConfig {
    #[knus(property)]
    proportion: Option<f64>,
    #[knus(property)]
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
            message: "requires a proportion or fixed property".to_owned(),
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
    #[error("spawn-at-startup entry {index} must contain a program")]
    EmptyStartupCommand { index: usize },
    #[error("invalid layout option {option}: {message}")]
    InvalidLayoutOption {
        option: &'static str,
        message: String,
    },
    #[error("invalid scale for output {output}: {message}")]
    InvalidOutputScale { output: String, message: String },
    #[error("output {output} has more than one scale rule")]
    DuplicateOutput { output: String },
    #[error("failed to read config {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error(transparent)]
    UnknownLayout(#[from] crate::layout::ParseLayoutError),
    #[error(transparent)]
    UnknownGpu(#[from] ParseGpuPreferenceError),
    #[error(transparent)]
    UnknownSystemd(#[from] ParseSystemdModeError),
    #[error(transparent)]
    XWayland(#[from] crate::xwayland::XWaylandConfigError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kdl_layout_and_ipc_socket() {
        let config = Config::from_kdl(
            Path::new("test.kdl"),
            "layout \"spatial-2d\"\nipc-socket \"/run/user/1000/tensor.sock\"\ngpu \"integrated\"\nrender-device \"/dev/dri/renderD128\"\nsystemd \"disabled\"\nxwayland true\nspawn-at-startup \"waybar\"\nspawn-at-startup \"foot\" \"--server\"",
        )
        .unwrap();

        assert_eq!(config.initial_layout, LayoutKind::Spatial2D);
        assert_eq!(config.layout_options, LayoutOptions::default());
        assert_eq!(
            config.ipc_socket,
            PathBuf::from("/run/user/1000/tensor.sock")
        );
        assert_eq!(config.gpu_preference, GpuPreference::Integrated);
        assert_eq!(
            config.render_device,
            Some(PathBuf::from("/dev/dri/renderD128"))
        );
        assert_eq!(config.systemd, SystemdMode::Disabled);
        assert!(config.xwayland.enabled());
        assert_eq!(
            config.startup_commands,
            vec![
                StartupCommand {
                    argv: vec!["waybar".to_owned()]
                },
                StartupCommand {
                    argv: vec!["foot".to_owned(), "--server".to_owned()]
                }
            ]
        );
    }

    #[test]
    fn parses_nested_layout_policy() {
        let config = Config::from_kdl(
            Path::new("test.kdl"),
            "layout \"scrolling-1d\" {\n  gaps 12\n  default-column-width proportion=0.625\n  master-width fixed=900\n}",
        )
        .unwrap();

        assert_eq!(config.layout_options.gap, 12);
        assert_eq!(
            config.layout_options.scrolling_default_width,
            LayoutLength::proportion(6250, 10_000)
        );
        assert_eq!(config.layout_options.master_width, LayoutLength::fixed(900));
    }

    #[test]
    fn parses_and_quantizes_per_output_fractional_scale() {
        let config =
            Config::from_kdl(Path::new("test.kdl"), "output \"eDP-1\" {\n  scale 1.31\n}").unwrap();

        assert_eq!(
            config.output_scales["eDP-1"],
            OutputScale::from_units(157).unwrap()
        );
    }

    #[test]
    fn accepts_integer_output_scale_literals() {
        let config =
            Config::from_kdl(Path::new("test.kdl"), "output \"eDP-1\" {\n  scale 2\n}").unwrap();
        assert_eq!(
            config.output_scales["eDP-1"],
            OutputScale::from_units(240).unwrap()
        );
    }

    #[test]
    fn rejects_invalid_and_duplicate_output_scales() {
        assert!(matches!(
            Config::from_kdl(Path::new("test.kdl"), "output \"DP-1\" {\n  scale 0\n}"),
            Err(ConfigError::InvalidOutputScale { .. })
        ));
        assert!(matches!(
            Config::from_kdl(
                Path::new("test.kdl"),
                "output \"DP-1\" {\n  scale 1.25\n}\noutput \"DP-1\" {\n  scale 1.5\n}"
            ),
            Err(ConfigError::DuplicateOutput { .. })
        ));
    }

    #[test]
    fn layout_length_requires_one_valid_mode() {
        let both = Config::from_kdl(
            Path::new("test.kdl"),
            "layout \"scrolling-1d\" {\n  default-column-width proportion=0.5 fixed=800\n}",
        )
        .unwrap_err();
        assert!(
            matches!(
                &both,
                ConfigError::InvalidLayoutOption {
                    option: "default-column-width",
                    ..
                }
            ),
            "unexpected error: {both:?}"
        );

        let zero = Config::from_kdl(
            Path::new("test.kdl"),
            "layout \"scrolling-1d\" {\n  master-width fixed=0\n}",
        )
        .unwrap_err();
        assert!(
            matches!(
                &zero,
                ConfigError::InvalidLayoutOption {
                    option: "master-width",
                    ..
                }
            ),
            "unexpected error: {zero:?}"
        );
    }

    #[test]
    fn rejects_unknown_systemd_mode() {
        assert!(matches!(
            Config::from_kdl(Path::new("test.kdl"), "systemd \"launchd\""),
            Err(ConfigError::UnknownSystemd(_))
        ));
    }

    #[test]
    fn rejects_unknown_kdl_nodes() {
        assert!(matches!(
            Config::from_kdl(Path::new("test.kdl"), "compatibility true"),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn rejects_empty_startup_commands() {
        assert!(matches!(
            Config::from_kdl(Path::new("test.kdl"), "spawn-at-startup"),
            Err(ConfigError::EmptyStartupCommand { index: 0 })
        ));
    }
}

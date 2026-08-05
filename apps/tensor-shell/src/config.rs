#![allow(unexpected_cfgs)] // `tensor-kdl` derive emits optional downstream DOM impls.

use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use tensor_kdl::Decode;

use crate::{PanelWidgetKind, ShellLayout, ShellLayoutError};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShellConfig {
    pub layout: ShellLayout,
    pub panel: PanelConfig,
    pub tensorland: TensorlandConfigEndpoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PanelConfig {
    left: Vec<PanelWidgetKind>,
    center: Vec<PanelWidgetKind>,
    right: Vec<PanelWidgetKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TensorlandConfigEndpoint {
    pub config_path: PathBuf,
    pub ipc_socket: PathBuf,
}

impl ShellConfig {
    pub fn resolve_path() -> PathBuf {
        env::var_os("TENSOR_SHELL_CONFIG")
            .map(PathBuf::from)
            .or_else(|| xdg_config_home().map(|path| path.join("tensor/shell.kdl")))
            .unwrap_or_else(|| PathBuf::from("/etc/tensor/shell.kdl"))
    }

    pub fn load_default_path() -> Result<Self, ShellConfigError> {
        Self::load_or_default(&Self::resolve_path())
    }

    pub fn load_or_default(path: &Path) -> Result<Self, ShellConfigError> {
        match fs::read_to_string(path) {
            Ok(document) => Self::from_kdl(path, &document),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ShellConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    fn from_kdl(path: &Path, document: &str) -> Result<Self, ShellConfigError> {
        let parsed: FileConfig =
            tensor_kdl::read(document).map_err(|error| ShellConfigError::Parse {
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

impl PanelConfig {
    pub fn left(&self) -> &[PanelWidgetKind] {
        &self.left
    }

    pub fn center(&self) -> &[PanelWidgetKind] {
        &self.center
    }

    pub fn right(&self) -> &[PanelWidgetKind] {
        &self.right
    }
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            left: vec![
                PanelWidgetKind::Launcher,
                PanelWidgetKind::Workspaces,
                PanelWidgetKind::ActiveWindow,
            ],
            center: vec![PanelWidgetKind::Clock],
            right: vec![
                PanelWidgetKind::Media,
                PanelWidgetKind::SystemStatus,
                PanelWidgetKind::Notifications,
                PanelWidgetKind::ControlCenter,
            ],
        }
    }
}

impl Default for TensorlandConfigEndpoint {
    fn default() -> Self {
        let config_path = env::var_os("TENSOR_CONFIG")
            .map(PathBuf::from)
            .or_else(|| xdg_config_home().map(|path| path.join("tensor/config.kdl")))
            .unwrap_or_else(|| PathBuf::from("/etc/tensor/config.kdl"));
        let ipc_socket = env::var_os("TENSOR_IPC_SOCKET")
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("XDG_RUNTIME_DIR").map(|path| PathBuf::from(path).join("tensor.sock"))
            })
            .unwrap_or_else(|| PathBuf::from("/tmp/tensor.sock"));
        Self {
            config_path,
            ipc_socket,
        }
    }
}

#[derive(Debug, Default, Decode)]
struct FileConfig {
    #[kdl(child)]
    layout: Option<LayoutFileConfig>,
    #[kdl(child)]
    panel: Option<PanelFileConfig>,
    #[kdl(child)]
    tensorland: Option<TensorlandFileConfig>,
}

impl FileConfig {
    fn resolve(self) -> Result<ShellConfig, ShellConfigError> {
        let defaults = ShellConfig::default();
        let layout = self
            .layout
            .map(LayoutFileConfig::resolve)
            .transpose()?
            .unwrap_or(defaults.layout);
        let panel = self
            .panel
            .map(PanelFileConfig::resolve)
            .transpose()?
            .unwrap_or(defaults.panel);
        let tensorland = self
            .tensorland
            .map(TensorlandFileConfig::resolve)
            .unwrap_or(defaults.tensorland);
        Ok(ShellConfig {
            layout,
            panel,
            tensorland,
        })
    }
}

#[derive(Debug, Default, Decode)]
struct LayoutFileConfig {
    #[kdl(child(name = "panel-height"), unwrap(argument))]
    panel_height: Option<u32>,
    #[kdl(child(name = "popover-width"), unwrap(argument))]
    popover_width: Option<u32>,
    #[kdl(child(name = "popover-height"), unwrap(argument))]
    popover_height: Option<u32>,
    #[kdl(child(name = "osd-width"), unwrap(argument))]
    osd_width: Option<u32>,
    #[kdl(child(name = "osd-height"), unwrap(argument))]
    osd_height: Option<u32>,
    #[kdl(child(name = "edge-gap"), unwrap(argument))]
    edge_gap: Option<i32>,
}

impl LayoutFileConfig {
    fn resolve(self) -> Result<ShellLayout, ShellConfigError> {
        let defaults = ShellLayout::default();
        ShellLayout {
            panel_height: self.panel_height.unwrap_or(defaults.panel_height),
            popover_width: self.popover_width.unwrap_or(defaults.popover_width),
            popover_height: self.popover_height.unwrap_or(defaults.popover_height),
            osd_width: self.osd_width.unwrap_or(defaults.osd_width),
            osd_height: self.osd_height.unwrap_or(defaults.osd_height),
            edge_gap: self.edge_gap.unwrap_or(defaults.edge_gap),
        }
        .validate()
        .map_err(ShellConfigError::Layout)
    }
}

#[derive(Debug, Default, Decode)]
struct PanelFileConfig {
    #[kdl(child)]
    left: Option<WidgetList>,
    #[kdl(child)]
    center: Option<WidgetList>,
    #[kdl(child)]
    right: Option<WidgetList>,
}

impl PanelFileConfig {
    fn resolve(self) -> Result<PanelConfig, ShellConfigError> {
        let defaults = PanelConfig::default();
        let panel = PanelConfig {
            left: parse_widgets("left", self.left, defaults.left)?,
            center: parse_widgets("center", self.center, defaults.center)?,
            right: parse_widgets("right", self.right, defaults.right)?,
        };
        let mut unique = BTreeSet::new();
        for widget in panel.left.iter().chain(&panel.center).chain(&panel.right) {
            if !unique.insert(*widget) {
                return Err(ShellConfigError::DuplicatePanelWidget(*widget));
            }
        }
        Ok(panel)
    }
}

#[derive(Debug, Decode)]
struct WidgetList {
    #[kdl(arguments)]
    widgets: Vec<String>,
}

fn parse_widgets(
    section: &'static str,
    configured: Option<WidgetList>,
    defaults: Vec<PanelWidgetKind>,
) -> Result<Vec<PanelWidgetKind>, ShellConfigError> {
    configured
        .map(|list| {
            list.widgets
                .into_iter()
                .map(|name| {
                    PanelWidgetKind::from_str(&name)
                        .map_err(|()| ShellConfigError::UnknownPanelWidget { section, name })
                })
                .collect()
        })
        .unwrap_or(Ok(defaults))
}

#[derive(Debug, Default, Decode)]
struct TensorlandFileConfig {
    #[kdl(child(name = "config-path"), unwrap(argument))]
    config_path: Option<String>,
    #[kdl(child(name = "ipc-socket"), unwrap(argument))]
    ipc_socket: Option<String>,
}

impl TensorlandFileConfig {
    fn resolve(self) -> TensorlandConfigEndpoint {
        let defaults = TensorlandConfigEndpoint::default();
        TensorlandConfigEndpoint {
            config_path: self
                .config_path
                .map(PathBuf::from)
                .unwrap_or(defaults.config_path),
            ipc_socket: self
                .ipc_socket
                .map(PathBuf::from)
                .unwrap_or(defaults.ipc_socket),
        }
    }
}

fn xdg_config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

#[derive(Debug, thiserror::Error)]
pub enum ShellConfigError {
    #[error("failed to read Tensor Shell configuration {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse Tensor Shell configuration {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error(transparent)]
    Layout(#[from] ShellLayoutError),
    #[error("unknown panel widget `{name}` in `{section}` section")]
    UnknownPanelWidget { section: &'static str, name: String },
    #[error("panel widget `{0}` is configured more than once")]
    DuplicatePanelWidget(PanelWidgetKind),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(document: &str) -> Result<ShellConfig, ShellConfigError> {
        ShellConfig::from_kdl(Path::new("shell.kdl"), document)
    }

    #[test]
    fn empty_document_uses_complete_defaults() {
        assert_eq!(parse("").unwrap(), ShellConfig::default());
    }

    #[test]
    fn kdl_controls_layout_widget_order_and_tensorland_endpoint() {
        let config = parse(
            r#"
                layout {
                    panel-height 48
                    edge-gap 16
                }
                panel {
                    left "launcher" "active-window"
                    center "workspaces" "clock"
                    right "notifications" "control-center"
                }
                tensorland {
                    config-path "/tmp/tensor.kdl"
                    ipc-socket "/tmp/tensor.sock"
                }
            "#,
        )
        .unwrap();
        assert_eq!(config.layout.panel_height, 48);
        assert_eq!(
            config.panel.center(),
            [PanelWidgetKind::Workspaces, PanelWidgetKind::Clock]
        );
        assert_eq!(config.tensorland.config_path, Path::new("/tmp/tensor.kdl"));
    }

    #[test]
    fn duplicate_and_unknown_widgets_are_rejected() {
        assert!(matches!(
            parse("panel { left \"launcher\"; right \"launcher\"; }").unwrap_err(),
            ShellConfigError::DuplicatePanelWidget(PanelWidgetKind::Launcher)
        ));
        assert!(matches!(
            parse("panel { left \"unknown\"; }").unwrap_err(),
            ShellConfigError::UnknownPanelWidget { .. }
        ));
    }
}

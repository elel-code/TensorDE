//! Desktop state snapshots supplied by compositor adapters.

pub mod adapters;
pub mod power;
pub mod session;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesktopSnapshot {
    #[serde(default)]
    pub compositor: Option<CompositorKind>,
    #[serde(default)]
    pub outputs: Vec<DesktopOutput>,
    #[serde(default)]
    pub power: PowerState,
    #[serde(default = "default_true")]
    pub session_active: bool,
}

impl Default for DesktopSnapshot {
    fn default() -> Self {
        Self {
            compositor: None,
            outputs: Vec::new(),
            power: PowerState::Unknown,
            session_active: true,
        }
    }
}

impl DesktopSnapshot {
    pub fn placeholder() -> Self {
        Self::default()
    }

    pub fn output(&self, name: &str) -> Option<&DesktopOutput> {
        self.outputs.iter().find(|output| output.name == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompositorKind {
    GenericWayland,
    Hyprland,
    Niri,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesktopOutput {
    pub name: String,
    #[serde(default)]
    pub make: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default)]
    pub focused: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub has_fullscreen: bool,
    #[serde(default)]
    pub active_workspace: Option<String>,
}

impl DesktopOutput {
    pub fn virtual_output(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            make: None,
            model: None,
            width: None,
            height: None,
            scale: 1.0,
            focused: true,
            visible: true,
            has_fullscreen: false,
            active_workspace: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PowerState {
    #[default]
    Unknown,
    Ac,
    Battery,
}

impl PowerState {
    pub fn is_battery(self) -> bool {
        matches!(self, Self::Battery)
    }
}

fn default_true() -> bool {
    true
}

fn default_scale() -> f32 {
    1.0
}

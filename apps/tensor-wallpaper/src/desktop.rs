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
    #[serde(default)]
    pub session_locked: bool,
}

impl Default for DesktopSnapshot {
    fn default() -> Self {
        Self {
            compositor: None,
            outputs: Vec::new(),
            power: PowerState::Unknown,
            session_active: true,
            session_locked: false,
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
    pub logical_x: Option<i32>,
    #[serde(default)]
    pub logical_y: Option<i32>,
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
    #[serde(default)]
    pub cursor_parallax: Option<DesktopCursorParallax>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DesktopCursorParallax {
    pub x: f64,
    pub y: f64,
}

impl DesktopCursorParallax {
    pub fn parse_override(value: &str) -> Option<(Option<String>, Self)> {
        let value = value.trim();
        if value.is_empty() || matches!(value.to_ascii_lowercase().as_str(), "auto" | "compositor")
        {
            return None;
        }
        let (output_name, coords) = match value.split_once(':') {
            Some((output_name, coords)) => (Some(output_name.trim()), coords.trim()),
            None => (None, value),
        };
        let output_name = output_name
            .filter(|output_name| !output_name.is_empty())
            .map(str::to_owned);
        let (x, y) = coords.split_once(',')?;
        Some((
            output_name,
            Self {
                x: x.trim().parse::<f64>().ok()?.clamp(-1.0, 1.0),
                y: y.trim().parse::<f64>().ok()?.clamp(-1.0, 1.0),
            },
        ))
    }
}

impl DesktopOutput {
    pub fn virtual_output(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            logical_x: None,
            logical_y: None,
            make: None,
            model: None,
            width: None,
            height: None,
            scale: 1.0,
            focused: true,
            visible: true,
            has_fullscreen: false,
            active_workspace: None,
            cursor_parallax: None,
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

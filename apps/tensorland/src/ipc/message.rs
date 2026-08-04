use serde::{Deserialize, Serialize};

use crate::config::ConfigDiagnosticMetadata;
use crate::layout::LayoutKind;

#[cfg(test)]
mod tests;

pub const IPC_PROTOCOL_VERSION: u16 = 5;
pub const MAX_OVERVIEW_VIEWS: usize = 2_048;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Request {
    pub version: u16,
    pub request_id: u64,
    pub command: Command,
}

impl Request {
    pub fn new(request_id: u64, command: Command) -> Self {
        Self {
            version: IPC_PROTOCOL_VERSION,
            request_id,
            command,
        }
    }
}

impl Response {
    pub fn new(request_id: u64, result: ResultBody) -> Self {
        Self {
            version: IPC_PROTOCOL_VERSION,
            request_id,
            result,
        }
    }

    pub fn error(request_id: u64, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            request_id,
            ResultBody::Error(IpcErrorBody {
                code: code.into(),
                message: message.into(),
            }),
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Command {
    Ping,
    GetState,
    GetOutputs,
    /// List regular and named hidden workspaces with occupancy.
    GetWorkspaces,
    /// Return the bounded window/workspace inventory used by overview chrome.
    GetOverview,
    /// Return the active configuration generation and last bounded failure.
    GetConfigStatus,
    /// Queue an off-thread load of the configured KDL path.
    ReloadConfig,
    SetLayout {
        layout: LayoutKind,
    },
    /// Queue an application launch on the compositor's async worker.
    ///
    /// `argv` is one program followed by zero or more arguments. Tensor never
    /// invokes a shell; empty argv is rejected with a structured error.
    Spawn {
        argv: Vec<String>,
    },
    /// Activate virtual desktop by zero-based index (`0..workspace_count`).
    SetWorkspace {
        index: u32,
    },
    /// Move the focused view to another workspace.
    MoveFocusedToWorkspace {
        index: u32,
        /// When true, also activate the destination workspace.
        #[serde(default)]
        follow: bool,
    },
    /// Activate a stable view from the current overview inventory.
    ActivateView {
        view: u64,
    },
    /// Move a stable view's complete attachment family to a regular workspace.
    MoveViewToWorkspace {
        view: u64,
        index: u32,
        #[serde(default)]
        follow: bool,
    },
    /// Move the focused view family into the configured hidden minimize target.
    MinimizeFocused,
    /// Restore a minimized view to its retained regular workspace.
    RestoreMinimized {
        view: u64,
        #[serde(default = "default_true")]
        follow: bool,
    },
    /// Set logical origin of a connector (`eDP-1`, `HDMI-A-1`, …).
    SetOutputPosition {
        name: String,
        x: i32,
        y: i32,
    },
    /// Enable or disable a connector for scanout.
    SetOutputEnabled {
        name: String,
        enabled: bool,
    },
    /// Set fractional scale for a connector (e.g. `1.25`).
    SetOutputScale {
        name: String,
        /// Scale as percent of 100 (125 → 1.25) to stay integer on the wire.
        scale_percent: u32,
    },
    Quit,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Response {
    pub version: u16,
    pub request_id: u64,
    pub result: ResultBody,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResultBody {
    Pong,
    State(StateSnapshot),
    Outputs(Vec<OutputSnapshot>),
    Workspaces(Vec<WorkspaceSnapshot>),
    Overview(OverviewSnapshot),
    ConfigStatus(ConfigStatusSnapshot),
    Accepted,
    Error(IpcErrorBody),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ConfigStatusSnapshot {
    pub generation: u64,
    pub last_failure: Option<ConfigDiagnosticMetadata>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StateSnapshot {
    pub layout: LayoutKind,
    pub view_count: usize,
    pub output_count: usize,
    pub focused_view: Option<u64>,
    /// Active virtual desktop (zero-based).
    pub workspace: u32,
    /// Configured regular workspace pool size.
    pub workspace_count: u32,
    pub hidden_workspace_count: usize,
    /// Minimized root families; attached dialog views do not inflate this.
    pub minimized_count: usize,
}

/// One regular or named hidden workspace.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkspaceSnapshot {
    /// Zero-based index.
    pub index: u32,
    /// Human-facing regular index or configured hidden-workspace name.
    pub name: String,
    pub active: bool,
    pub hidden: bool,
    pub show_in_overview: bool,
    pub minimize_target: bool,
    pub view_count: usize,
    pub focused_view: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OverviewSnapshot {
    pub active_workspace: u32,
    /// Primary output work area used by the compositor-owned plan.
    pub area: Option<OverviewGeometrySnapshot>,
    /// True when the stable prefix reached [`MAX_OVERVIEW_VIEWS`].
    pub truncated: bool,
    pub workspaces: Vec<OverviewWorkspaceSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OverviewWorkspaceSnapshot {
    pub index: u32,
    pub name: String,
    pub hidden: bool,
    pub minimize_target: bool,
    /// Workspace card in overview logical coordinates.
    pub geometry: Option<OverviewGeometrySnapshot>,
    /// Total inventory size before the global bounded-prefix limit.
    pub view_count: usize,
    pub views: Vec<OverviewViewSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OverviewViewSnapshot {
    pub id: u64,
    pub root: u64,
    /// Joins this record to `ext-foreign-toplevel-list-v1` metadata.
    pub foreign_toplevel_identifier: Option<String>,
    /// Current or last valid arranged geometry before overview scaling.
    pub source_geometry: Option<OverviewGeometrySnapshot>,
    /// Transformed geometry in the overview plan.
    pub geometry: Option<OverviewGeometrySnapshot>,
    /// Visible input/render region after clipping to the workspace card.
    pub clip: Option<OverviewGeometrySnapshot>,
    pub focused: bool,
    pub kind: OverviewViewKindSnapshot,
    /// Back-to-front order within the compositor scene.
    pub stacking_order: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OverviewGeometrySnapshot {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverviewViewKindSnapshot {
    Tiled,
    Floating,
    Attached,
}

/// Value-only output topology for control clients.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OutputSnapshot {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    pub mode_width: i32,
    pub mode_height: i32,
    pub refresh_millihertz: i32,
    pub primary: bool,
    /// False when the connector is policy-disabled (still may appear in DRM).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IpcErrorBody {
    pub code: String,
    pub message: String,
}

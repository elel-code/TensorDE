use serde::{Deserialize, Serialize};

use crate::layout::LayoutKind;

pub const IPC_PROTOCOL_VERSION: u16 = 1;

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
    /// List virtual desktops (fixed pool) with occupancy.
    GetWorkspaces,
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
    Accepted,
    Error(IpcErrorBody),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StateSnapshot {
    pub layout: LayoutKind,
    pub view_count: usize,
    pub output_count: usize,
    pub focused_view: Option<u64>,
    /// Active virtual desktop (zero-based).
    pub workspace: u32,
    /// Fixed workspace pool size.
    pub workspace_count: u32,
}

/// One virtual desktop in the fixed pool.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkspaceSnapshot {
    /// Zero-based index.
    pub index: u32,
    /// Human-facing name (`"1"` … `"9"`).
    pub name: String,
    pub active: bool,
    pub view_count: usize,
    pub focused_view: Option<u64>,
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

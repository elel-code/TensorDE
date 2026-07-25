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
    Accepted,
    Error(IpcErrorBody),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StateSnapshot {
    pub layout: LayoutKind,
    pub view_count: usize,
    pub output_count: usize,
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IpcErrorBody {
    pub code: String,
    pub message: String,
}

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
    SetLayout { layout: LayoutKind },
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
    Accepted,
    Error(IpcErrorBody),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StateSnapshot {
    pub layout: LayoutKind,
    pub view_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IpcErrorBody {
    pub code: String,
    pub message: String,
}

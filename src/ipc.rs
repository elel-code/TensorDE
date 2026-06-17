//! Shared helpers for the local Gilder IPC protocol.

pub mod command;
mod protocol;
pub mod socket;

pub use command::{help_text, parse_client_args, ClientCommand};
pub use protocol::{
    error_response, event_notification, parse_request, success_response, IpcRequest, RequestMethod,
    RpcError, PROTOCOL_VERSION,
};
pub use socket::{runtime_socket_path, SOCKET_DIR_NAME, SOCKET_FILE_NAME};

//! Shared helpers for the local Gilder IPC protocol.

pub mod command;
mod protocol;
pub mod socket;

pub use command::{ClientCommand, help_text, parse_client_args};
pub use protocol::{
    IpcRequest, PROTOCOL_VERSION, RequestMethod, RpcError, error_response, event_notification,
    parse_request, success_response,
};
pub use socket::{SOCKET_DIR_NAME, SOCKET_FILE_NAME, runtime_socket_path};

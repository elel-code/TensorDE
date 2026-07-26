mod codec;
mod message;
mod server;

pub use codec::{CodecError, FrameDecoder, MAX_FRAME_SIZE, encode};
pub use message::{
    Command, IPC_PROTOCOL_VERSION, IpcErrorBody, OutputSnapshot, Request, Response, ResultBody,
    StateSnapshot, WorkspaceSnapshot,
};
pub(crate) use server::IpcReply;
pub use server::{IpcError, IpcServer};

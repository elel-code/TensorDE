mod codec;
mod message;
mod server;

pub use codec::{CodecError, FrameDecoder, MAX_FRAME_SIZE, encode};
pub use message::{
    Command, ConfigStatusSnapshot, IPC_PROTOCOL_VERSION, IpcErrorBody, MAX_OVERVIEW_VIEWS,
    OutputSnapshot, OverviewGeometrySnapshot, OverviewSnapshot, OverviewViewKindSnapshot,
    OverviewViewSnapshot, OverviewWorkspaceSnapshot, Request, Response, ResultBody, StateSnapshot,
    WorkspaceSnapshot,
};
pub(crate) use server::{
    IpcControlEvent, IpcEvent, IpcReply, IpcRuntime, MAX_PENDING_IPC_CONTROL_EVENTS,
    MAX_PENDING_IPC_REQUESTS,
};
pub use server::{IpcError, IpcServer};

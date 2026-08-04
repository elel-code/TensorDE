mod codec;
mod message;
mod server;
mod subscription;

pub use codec::{CodecError, FrameDecoder, MAX_FRAME_SIZE, encode};
pub use message::{
    Command, ConfigReloadEvent, ConfigReloadEventResult, ConfigStatusSnapshot, EventMessage,
    EventTopic, IPC_PROTOCOL_VERSION, IpcErrorBody, MAX_OVERVIEW_VIEWS, MAX_SUBSCRIPTION_TOPICS,
    OutputSnapshot, OverviewGeometrySnapshot, OverviewSnapshot, OverviewViewKindSnapshot,
    OverviewViewSnapshot, OverviewWorkspaceSnapshot, Request, Response, ResultBody, ServerEvent,
    ServerMessage, StateSnapshot, WorkspaceSnapshot,
};
pub(crate) use server::{
    IpcControlEvent, IpcEvent, IpcReply, IpcRuntime, MAX_PENDING_IPC_CONTROL_EVENTS,
    MAX_PENDING_IPC_REQUESTS,
};
pub use server::{IpcError, IpcServer};
pub(crate) use subscription::{IpcSubscriptionSink, IpcSubscriptions, subscription_channel};

mod client;
mod codec;
mod message;
mod values;

pub use client::{ClientError, CompioClient, default_socket_path};
pub use codec::{CodecError, FrameDecoder, MAX_FRAME_SIZE, encode, encode_into};
pub use message::{
    Command, ConfigReloadEvent, ConfigReloadEventResult, ConfigStatusSnapshot, EventMessage,
    EventTopic, IPC_PROTOCOL_VERSION, IpcErrorBody, MAX_OVERVIEW_VIEWS, MAX_SUBSCRIPTION_TOPICS,
    OutputSnapshot, OverviewGeometrySnapshot, OverviewSnapshot, OverviewViewKindSnapshot,
    OverviewViewSnapshot, OverviewWorkspaceSnapshot, Request, Response, ResultBody, ServerEvent,
    ServerMessage, StateSnapshot, WorkspaceSnapshot,
};
pub use values::{
    ConfigDiagnosticCategory, ConfigDiagnosticMetadata, LayoutKind, ParseLayoutError,
};

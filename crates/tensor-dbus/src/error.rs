use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("D-Bus address is unavailable: {0}")]
    AddressUnavailable(&'static str),
    #[error("invalid D-Bus address: {0}")]
    InvalidAddress(String),
    #[error("invalid D-Bus match rule: {0}")]
    InvalidMatchRule(String),
    #[error("unsupported D-Bus transport: {0}")]
    UnsupportedTransport(String),
    #[error("invalid D-Bus GUID: {0}")]
    InvalidGuid(String),
    #[error("invalid D-Bus machine ID: {0}")]
    InvalidMachineId(String),
    #[error("D-Bus I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("D-Bus connection is unusable after a terminal transport or protocol error")]
    ConnectionUnusable,
    #[error("D-Bus operation requires a message-bus connection")]
    BusOperationOnPeer,
    #[error("D-Bus authentication failed: {0}")]
    Authentication(String),
    #[error("invalid D-Bus message: {0}")]
    InvalidMessage(String),
    #[error("D-Bus message exceeds the {limit}-byte limit")]
    MessageTooLarge { limit: usize },
    #[error("D-Bus message serial space is exhausted")]
    SerialExhausted,
    #[error("NO_REPLY_EXPECTED cannot be used with a reply-producing D-Bus call")]
    InvalidCallFlags,
    #[error("D-Bus pending-message queue reached its {limit}-message limit")]
    PendingQueueFull { limit: usize },
    #[error("D-Bus pending-message queue reached its {limit}-byte limit")]
    PendingBytesFull { limit: usize },
    #[error("D-Bus abandoned-reply registry reached its {limit}-reply limit")]
    AbandonedReplyQueueFull { limit: usize },
    #[error("D-Bus Unix file descriptor passing was not negotiated with the peer")]
    UnixFdUnsupported,
    #[error("D-Bus message carries {count} Unix file descriptors; the limit is {limit}")]
    UnixFdLimit { count: usize, limit: usize },
    #[error("D-Bus ancillary data was truncated")]
    AncillaryTruncated,
    #[error("invalid D-Bus ancillary data: {0}")]
    InvalidAncillary(String),
    #[error("invalid D-Bus name `{value}`: {kind}")]
    InvalidName { kind: &'static str, value: String },
    #[error("D-Bus interface `{0}` is reserved by the object server")]
    ReservedInterface(String),
    #[error("duplicate D-Bus method `{interface}.{member}` at `{path}`")]
    DuplicateMethod {
        path: String,
        interface: String,
        member: String,
    },
    #[error("duplicate D-Bus property `{interface}.{property}` at `{path}`")]
    DuplicateProperty {
        path: String,
        interface: String,
        property: String,
    },
    #[error("duplicate D-Bus signal `{interface}.{signal}` at `{path}`")]
    DuplicateSignal {
        path: String,
        interface: String,
        signal: String,
    },
    #[error("D-Bus body codec failed: {0}")]
    Body(#[from] zvariant::Error),
    #[error("D-Bus peer returned {name}: {message}")]
    Method { name: String, message: String },
    #[error("D-Bus service method failed locally: {0}")]
    Service(#[source] crate::MethodError),
}

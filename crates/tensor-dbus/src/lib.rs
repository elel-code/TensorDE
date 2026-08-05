//! Caller-driven asynchronous D-Bus clients and services built on Compio.
//!
//! This crate never creates a runtime, executor, or worker thread. Callers enter
//! a Compio runtime and await every connection, method, signal, and service
//! operation. Connections remain affine to the runtime thread that owns them.
//! [`PendingReply`] separates method submission from reply routing so one
//! caller-owned connection can keep multiple requests in flight without tasks.
//! Callers can explicitly abandon a token to discard its eventual reply; this
//! local routing operation does not cancel the remote method.
//! In-flight socket operations and frame buffers remain inside [`Connection`]
//! when an outer future is dropped, so caller-owned timeout/select logic can
//! resume reads without losing stream bytes or Unix file descriptors.

mod address;
mod auth;
mod connection;
mod dynamic_body;
mod error;
mod flags;
pub mod freedesktop;
mod guid;
mod machine_id;
mod name;
mod object_server;
mod peer_listener;
mod pending;
mod proxy;
mod server;
mod signal;
mod unix_fd;
mod wire;

pub use address::{BusAddress, BusKind};
pub use connection::{Connection, ConnectionMode, PeerCredentials};
pub use dynamic_body::DynamicBody;
pub use error::{Error, Result};
pub use flags::MethodCallFlags;
pub use guid::Guid;
pub use machine_id::MachineId;
pub use name::{ReleaseNameReply, RequestNameFlags, RequestNameReply};
pub use object_server::{MethodContext, ObjectServer, PropertyChangeMode};
pub use peer_listener::{AcceptedPeer, PeerListener};
pub use pending::PendingReply;
pub use proxy::{IntoProxyTarget, Proxy};
pub use server::{
    MethodCall, MethodError, MethodResult, reply_method, reply_method_error, reply_method_result,
};
pub use signal::{MatchRule, SignalStream};
pub use wire::{Message, MessageKind};

pub use zvariant;

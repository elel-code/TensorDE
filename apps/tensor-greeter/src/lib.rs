//! Security-conscious value model for TensorDE's standalone greetd frontend.
//!
//! The crate owns no compositor, DRM, Vulkan, PAM, or logind objects. PAM stays
//! behind greetd; authentication responses pass directly from UI input into a
//! zeroed protocol frame and are never retained by [`GreeterModel`].

mod accounts;
mod client;
mod config;
mod model;
mod protocol;
mod surface;
mod transaction;

pub use accounts::{UserDiscoveryError, discover_users};
pub use client::{GreetdClient, GreetdClientError};
pub use config::{
    GreeterConfig, GreeterConfigError, MAX_AUTH_MESSAGE_BYTES, MAX_SESSIONS, MAX_USERS,
    SessionDefinition,
};
pub use model::{
    AuthAttemptId, AuthPhase, AuthPrompt, AuthPromptKind, AuthStart, GreeterModel,
    GreeterModelError, SessionStart, UserAccount,
};
pub use protocol::{
    AuthMessageType, ErrorType, FrameDecoder, GreetdProtocolError, Request, Response,
    SensitiveFrame, encode_request,
};
pub use surface::{GreeterSurface, GreeterSurfaceError};
pub use transaction::{AuthUpdate, GreeterTransaction, GreeterTransactionError};

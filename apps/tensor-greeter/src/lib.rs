//! Security-conscious value model for TensorDE's standalone greetd frontend.
//!
//! The crate owns no compositor, DRM, Vulkan, PAM, or logind objects. PAM stays
//! behind greetd; authentication responses pass directly from UI input into a
//! zeroed protocol frame and are never retained by [`GreeterModel`].

mod config;
mod model;
mod protocol;

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

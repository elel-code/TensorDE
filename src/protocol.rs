#[cfg(feature = "tty")]
mod cursor;
mod extensions;
mod focus;
mod globals;
mod handlers;
#[cfg(feature = "tty")]
mod input;
mod runtime;
mod state;

#[cfg(test)]
pub(crate) use runtime::test_runtime_state;
pub use runtime::{ProtocolError, WaylandRuntime};
pub(crate) use state::RuntimeState;
pub(crate) use tensor_protocol::{PROTOCOL_CATALOG, ProtocolTier};

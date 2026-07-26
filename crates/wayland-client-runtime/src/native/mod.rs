//! Native Wayland backend (no SCTK).
//!
//! Phase 2 skeleton: own the connection, registry snapshot, and a Compio-driven
//! read/dispatch pump using `wayland-client` wire bindings only. Shell/seat
//! Production Fika still uses SCTK [`crate::Runtime`] until native coverage is
//! complete; [`NativeShell`] is already usable for toplevel + input + scale.
//!
//! Protocol implementations are split by upstream class under [`protocols`]
//! (core / stable / staging / unstable / ext / community), following
//! wayland-protocols and Smithay-style organization.

mod connection;
pub mod protocols;
mod pump;
mod registry;
mod shell;

pub use connection::{NativeConnection, NativeError};
pub use protocols::{ProtocolClass, ProtocolSpec, FIKA_PROTOCOL_MATRIX, specs_in_class};
pub use pump::{NativePump, PumpStep};
pub use registry::{list_env_globals, GlobalAdvertisement, NativeRegistry};
pub use shell::{
    NativeCapabilities, NativeShell, NativeShellEvent, NativeSurfaceHandle, NativeSurfaceId,
};

/// Shared helper for modules that already hold a `Connection`.
pub(crate) fn display_readiness_from_conn(
    connection: &wayland_client::Connection,
) -> Result<crate::DisplayReadiness, NativeError> {
    crate::DisplayReadiness::from_as_fd(connection).map_err(NativeError::from)
}

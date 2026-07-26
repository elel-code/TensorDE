//! Native Wayland backend (no SCTK).
//!
//! Phase 2 skeleton: own the connection, registry snapshot, and a Compio-driven
//! read/dispatch pump using `wayland-client` wire bindings only. Shell/seat
//! objects are not implemented here yet; the production [`crate::Runtime`] still
//! uses SCTK until those land.
//!
//! Protocol implementations are split by upstream class under [`protocols`]
//! (core / stable / staging / unstable / ext / community), following
//! wayland-protocols and Smithay-style organization.

mod connection;
pub mod protocols;
mod pump;
mod registry;

pub use connection::{NativeConnection, NativeError};
pub use protocols::{ProtocolClass, ProtocolSpec, FIKA_PROTOCOL_MATRIX, specs_in_class};
pub use pump::{NativePump, PumpStep};
pub use registry::{list_env_globals, GlobalAdvertisement, NativeRegistry};

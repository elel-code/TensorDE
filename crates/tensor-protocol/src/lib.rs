//! Smithay-free protocol state and values for Tensor.
//!
//! Wire adapters translate live Wayland resources into opaque keys and commit
//! tokens. This crate owns stable identities, renderable surface values, and
//! buffer attachment lifetime. It never owns protocol objects, file
//! descriptors, renderer handles, or compositor ECS entities.

mod catalog;
mod content;
mod ids;
mod output;
mod registry;
mod security_context;
mod sync;

pub use catalog::{
    PROTOCOL_CATALOG, ProtocolCapabilityRef, ProtocolTier, catalog_count_at_most, catalog_entry,
};
pub use content::{
    ContentRevision, SurfaceContent, SurfaceLayer, SurfaceSampleTransform, SurfaceSourceRect,
    SurfaceTransform, SurfaceUvTransform,
};
pub use ids::{SurfaceBufferId, SurfaceId};
pub use output::{OutputHeadSnapshot, OutputHeadUpdate, configuration_keeps_head_enabled};
pub use registry::{
    SurfaceBufferRegistry, SurfaceCommit, SurfaceTreeRemoval, SurfaceTreeUpdate, SurfaceUpdate,
};
pub use security_context::SecurityContextMetadata;
pub use sync::{SurfaceSync, SurfaceSyncRegistry};

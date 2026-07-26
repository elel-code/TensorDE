//! Smithay key/token binding for the Tensor-owned surface registry.

use smithay::{
    backend::renderer::utils::CommitCounter, reexports::wayland_server::backend::ObjectId,
};

pub(super) type SurfaceBufferRegistry =
    tensor_protocol::SurfaceBufferRegistry<ObjectId, CommitCounter>;
pub(super) type SurfaceCommit<K = ObjectId> = tensor_protocol::SurfaceCommit<K, CommitCounter>;

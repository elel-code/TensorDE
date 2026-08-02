//! Backend-neutral scene metadata for dynamic-rendering local read.
//!
//! Gilder proves authored producer/consumer topology and shader interfaces.
//! `vulkan-renderer` owns device-limit validation, Vulkan attachment mapping,
//! barriers, command recording, and synchronization.

mod pipeline;
mod scope_plan;

pub(super) use pipeline::{
    SceneLocalReadPipelineMetadata, validate_scene_local_read_shader_variant,
};
pub(super) use scope_plan::{
    SceneLocalReadScopePassRole, SceneLocalReadScopePlan, scene_local_read_scope_plans,
};

//! Vulkanalia scene runtime entrypoint.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/exe/blend-and-render.md`
//! - `reverse-engineered/gilder/docs/exe/global-uniforms.md`
//! - `references/gilder/godot/servers/rendering/rendering_device_graph.*`
//! - `references/gilder/godot/drivers/vulkan/rendering_device_driver_vulkan.*`

mod runtime;

pub use runtime::{
    NativeVulkanSceneOwnedUniformArenaPlanSnapshot, NativeVulkanSceneOwnedUniformSliceSnapshot,
    native_vulkan_scene_owned_uniform_arena_plan,
};
pub(in crate::renderer::native_vulkan) use runtime::{
    NativeVulkanVulkanaliaScenePresentOptions, NativeVulkanVulkanaliaScenePresentSnapshot,
    run_native_vulkan_vulkanalia_scene_present,
};

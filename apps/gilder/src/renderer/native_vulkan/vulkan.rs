//! Shared-renderer scene runtime implementation.
//!
//! Vulkan object ownership stays inside `vulkan-renderer`; this module only
//! carries Gilder's typed scene presentation policy and command plan.

mod scene;

pub use scene::{
    NativeVulkanSceneOwnedUniformArenaPlanSnapshot, NativeVulkanSceneOwnedUniformSliceSnapshot,
    NativeVulkanScenePresentSnapshot, native_vulkan_scene_owned_uniform_arena_plan,
};
pub(in crate::renderer::native_vulkan) use scene::{
    NativeVulkanScenePresentOptions, run_native_vulkan_scene_present,
};

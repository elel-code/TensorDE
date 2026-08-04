//! Shared-renderer scene runtime implementation.
//!
//! Vulkan object ownership stays inside `vulkan-renderer`; this module only
//! carries Tensor Wallpaper's typed scene presentation policy and command plan.

mod scene;

pub use scene::{
    RenderingDeviceSceneOwnedUniformArenaPlanSnapshot, RenderingDeviceSceneOwnedUniformSliceSnapshot,
    RenderingDeviceScenePresentSnapshot, rendering_device_scene_owned_uniform_arena_plan,
};
pub(in crate::renderer::rendering_device) use scene::{
    RenderingDeviceScenePresentOptions, run_rendering_device_scene_present,
};

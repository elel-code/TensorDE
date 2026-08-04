//! Shared-renderer scene runtime entrypoint.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/exe/blend-and-render.md`
//! - `reverse-engineered/tensor-wallpaper/docs/exe/global-uniforms.md`
//! - `references/tensor-wallpaper/godot/servers/rendering/rendering_device_graph.*`
//! - `references/tensor-wallpaper/godot/drivers/vulkan/rendering_device_driver_vulkan.*`

mod runtime;

pub use runtime::{
    RenderingDeviceSceneOwnedUniformArenaPlanSnapshot, RenderingDeviceSceneOwnedUniformSliceSnapshot,
    RenderingDeviceScenePresentSnapshot, rendering_device_scene_owned_uniform_arena_plan,
};
pub(in crate::renderer::rendering_device) use runtime::{
    RenderingDeviceScenePresentOptions, run_rendering_device_scene_present,
};

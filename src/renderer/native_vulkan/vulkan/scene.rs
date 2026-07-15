//! Vulkanalia scene runtime entrypoint.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.*`

mod runtime;

pub use runtime::{
    NativeVulkanSceneFrameCaptureSnapshot, NativeVulkanSceneFrameTemporalAnalysisSnapshot,
};
pub(in crate::renderer::native_vulkan) use runtime::{
    NativeVulkanVulkanaliaScenePresentOptions, NativeVulkanVulkanaliaScenePresentSnapshot,
    run_native_vulkan_vulkanalia_scene_present,
};

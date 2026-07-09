//! Native Vulkan scene backend contracts.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `references/godot/servers/rendering/renderer_scene_render.*`
//! - `references/godot/servers/rendering/rendering_device.*`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.*`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

mod backend_plan;

pub use backend_plan::{
    NativeVulkanSceneBackendPlan, NativeVulkanSceneDescriptorHeapPlan,
    NativeVulkanSceneMeshUploadPlan, native_vulkan_scene_backend_plan,
};

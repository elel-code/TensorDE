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
mod pipeline_cache;
mod render_graph_executor;
mod resource_storage;
mod runtime;
mod shader_catalog;

pub use backend_plan::{
    NativeVulkanSceneBackendPlan, NativeVulkanSceneDescriptorHeapPlan,
    NativeVulkanSceneMeshUploadPlan, native_vulkan_scene_backend_plan,
    native_vulkan_scene_backend_plan_from_render_item,
    native_vulkan_scene_backend_plan_from_semantic_frame,
};
pub use pipeline_cache::{
    NativeVulkanScenePipelineCacheEntry, NativeVulkanScenePipelineCachePlan,
    native_vulkan_scene_pipeline_cache_plan,
};
pub use render_graph_executor::{
    NativeVulkanSceneRenderGraphCommand, NativeVulkanSceneRenderGraphCommandKind,
    NativeVulkanSceneRenderGraphExecutorPlan, native_vulkan_scene_render_graph_executor_plan,
};
pub use resource_storage::{
    NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES, NativeVulkanSceneHeapStoragePlan,
    NativeVulkanSceneMeshBufferPlan, NativeVulkanSceneResourceStoragePlan,
    NativeVulkanSceneShaderHeapSlice, native_vulkan_scene_resource_storage_plan,
};
pub use runtime::{
    NativeVulkanSceneRunOptions, NativeVulkanSceneRuntimeSnapshot, run_scene,
    run_scene_with_options,
};
pub(crate) use runtime::validate_scene_runtime_plan;
pub use shader_catalog::{
    BuiltinSceneLocalReadShader, BuiltinSceneParameterLayout, BuiltinSceneShader,
    native_vulkan_particle_compute_shader, native_vulkan_scene_shader_catalog,
    native_vulkan_scene_shader_for_key, native_vulkan_scene_vertex_spirv_for_primitive,
};

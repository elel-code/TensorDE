//! Typed scene execution plans and renderer resource requirements.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/exe/blend-and-render.md`
//! - `reverse-engineered/tensor-wallpaper/docs/exe/global-uniforms.md`
//! - `references/tensor-wallpaper/godot/servers/rendering/renderer_scene_render.*`
//! - `references/tensor-wallpaper/godot/servers/rendering/rendering_device.*`
//! - `references/tensor-wallpaper/godot/drivers/vulkan/rendering_device_driver_vulkan.*`
//! - `crates/vulkan-renderer/src/descriptor_heap.rs`

mod execution_plan;
mod pipeline_cache;
mod render_graph_executor;
mod resource_storage;
mod runtime;
mod shader_catalog;

pub use execution_plan::{
    SceneExecutionPlan, RenderingDeviceSceneDescriptorHeapPlan,
    RenderingDeviceSceneMeshUploadPlan, scene_execution_plan,
    scene_execution_plan_from_render_item,
    scene_execution_plan_from_semantic_frame,
};
pub use pipeline_cache::{
    RenderingDeviceScenePipelineCacheEntry, RenderingDeviceScenePipelineCachePlan,
    RenderingDeviceSceneShaderProgramSet, RenderingDeviceSceneSpirvProgram,
    RenderingDeviceSceneVertexProgram, rendering_device_scene_pipeline_cache_plan,
};
pub use render_graph_executor::{
    RenderingDeviceSceneRenderGraphCommand, RenderingDeviceSceneRenderGraphCommandKind,
    RenderingDeviceSceneRenderGraphExecutorPlan, rendering_device_scene_render_graph_executor_plan,
};
pub use resource_storage::{
    RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES, RenderingDeviceSceneHeapStoragePlan,
    RenderingDeviceSceneMeshBufferPlan, RenderingDeviceSceneResourceStoragePlan,
    RenderingDeviceSceneShaderHeapSlice, rendering_device_scene_resource_storage_plan,
};
pub use runtime::{
    RenderingDeviceSceneRunOptions, RenderingDeviceSceneRuntimeSnapshot, RenderingDeviceSceneVideoSource,
    run_scene, run_scene_with_options,
};
pub(crate) use runtime::validate_scene_runtime_plan;
#[allow(unused_imports)]
pub use shader_catalog::{
    BuiltinSceneDescriptorBinding, BuiltinSceneDescriptorBindingKind,
    BuiltinSceneInputAttachment, BuiltinSceneLocalReadShader, BuiltinSceneParameterLayout,
    BuiltinSceneShader, BuiltinSceneVertexShader,
    rendering_device_particle_compute_shader, rendering_device_scene_shader_catalog,
    rendering_device_scene_shader_for_key, rendering_device_scene_vertex_spirv_for_primitive,
    rendering_device_scene_vertex_shader_for_primitive,
};

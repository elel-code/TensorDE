//! Godot-style scene engine boundary for Wallpaper Engine scene semantics.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_server_default.h`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device.h`

pub mod frame;
pub mod graph;
pub mod graph_execution;
pub mod ingest;
pub mod material;
pub mod object;
pub mod plan;
pub mod renderer_scene_render;
pub mod rendering_device;
pub mod rendering_server;
pub mod resource;
pub mod resource_gpu_layout;
pub mod resource_residency;
pub mod shader_uniforms;
pub mod timeline;
pub mod we;

pub use frame::{SceneFrameContext, SceneFramePlan};
pub use graph::{
    SCENE_WE_MAX_SHADER_TEXTURE_SLOTS, SCENE_WE_PASS_INPUT_TEXTURE_SLOT, SceneGraph,
    SceneGraphDraw, SceneGraphPass, SceneGraphPipelineClass, SceneGraphResourceBinding,
    SceneGraphResourceRole, SceneGraphTarget,
};
pub use graph_execution::{
    SceneGraphExecutionPass, SceneGraphExecutionPlan, SceneGraphTargetBarrier,
    SceneGraphTargetBarrierReason, SceneGraphTargetLifetime, SceneGraphTargetUsage,
};
pub use material::{
    SceneBlendContract, SceneCullMode, SceneDepthTest, SceneMaterialContract, SceneMaterialKey,
    SceneMaterialRenderState,
};
pub use object::{SceneObject, SceneObjectGeometry, SceneObjectId};
pub use plan::SceneEnginePlan;
pub use renderer_scene_render::RendererSceneRender;
pub use rendering_device::{RenderingDevice, RenderingDeviceCommand};
pub use rendering_server::RenderingServer;
pub use resource::{
    SceneGeometryId, ScenePuppetId, SceneResource, SceneResourceId, SceneTextureFormat,
};
pub use resource_gpu_layout::{
    SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES, SCENE_GPU_MESH_INDEX_BYTES,
    SCENE_GPU_MESH_VERTEX_BYTES, SCENE_GPU_PARENT_NONE, SCENE_GPU_PUPPET_BONE_BYTES,
    SCENE_GPU_PUPPET_CLIP_FRAME_BYTES, SCENE_GPU_PUPPET_SKIN_VERTEX_BYTES,
    SCENE_GPU_PUPPET_TRANSFORM_BYTES, scene_gpu_record_bytes,
};
pub use resource_residency::{
    SceneBufferResidency, SceneMeshResidency, ScenePuppetRigResidency, SceneResidentResource,
    SceneResourceResidencyPlan, SceneTextureResidency,
};
pub use shader_uniforms::{SceneGenericImage4MaterialUniformRecord, SceneShaderUniformFramePlan};
pub use timeline::{SceneSampleClock, SceneTimelineSample};
pub use we::{WE_VEC4_BYTES, WE_VEC4_LANES, WeShaderInterface, WeVec4};

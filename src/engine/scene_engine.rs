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

pub mod draw_family;
pub mod effect;
pub mod effect_graph;
pub mod effect_pass_graph;
pub mod effect_uniforms;
pub mod final_compositor;
pub mod frame;
pub mod graph;
pub mod graph_execution;
pub mod image_layer_targets;
pub mod ingest;
pub mod layer_aux;
pub mod layer_compositor;
pub mod material;
pub mod object;
pub mod plan;
pub mod puppet_clipping;
pub mod renderer_scene_render;
pub mod rendering_device;
pub mod rendering_server;
pub mod resource;
pub mod resource_gpu_layout;
pub mod resource_residency;
pub mod shader_uniforms;
pub mod timeline;
pub mod we;

pub use draw_family::{
    SceneGraphDrawFamily, SceneGraphDrawFamilyEntry, SceneGraphDrawFamilyPlan,
    SceneGraphPassDrawFamilyPlan,
};
pub use effect::{
    SceneEffectCommand, SceneEffectConstantValue, SceneEffectCopyCommand, SceneEffectFboBinding,
    SceneEffectFboFormat, SceneEffectImageRef, SceneEffectMaterialPass, SceneEffectPassBlend,
    SceneEffectProgram, SceneEffectSwapCommand, SceneEffectTextureResourceBinding,
    SceneObjectEffectProgram,
};
pub use effect_graph::{
    SceneEffectGraphPlan, SceneEffectInput, SceneEffectPassPlan, SceneEffectShaderFamily,
    SceneEffectTargetBinding,
};
pub use effect_pass_graph::{
    SceneEffectPassGraphCopy, SceneEffectPassGraphInputBinding, SceneEffectPassGraphInputSource,
    SceneEffectPassGraphMaterialPass, SceneEffectPassGraphOutput, SceneEffectPassGraphPlan,
    SceneEffectPassGraphSwap, SceneEffectPassGraphTarget,
};
pub use effect_uniforms::{SceneEffectUniformFramePlan, SceneIrisEffectUniformRecord};
pub use final_compositor::{SceneFinalCompositorObjectInput, SceneFinalCompositorPlan};
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
pub use image_layer_targets::{
    SceneImageLayerPassTarget, SceneImageLayerTargetPlan, image_layer_pass_target,
    image_layer_prefill_target,
};
pub use layer_aux::{
    SceneLayerAuxCompositeTargets, WE_AUX_CLEAR_PREP_VMA, WE_AUX_CLEAR_TARGET_CREATE_VMA,
    WE_AUX_CLEAR_TARGET_RELEASE_ZERO_VMA, WE_AUX_CLEAR_TARGET_STORE_VMA,
    WE_LAYER_AUX_CLEAR_MATERIAL_OFFSET, WE_LAYER_AUX_CLEAR_TARGET_OFFSET,
    WE_LAYER_AUX_EFFECT_TARGET_OFFSET, WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET,
    WE_LAYER_AUX_MATERIAL_TARGET_OFFSET,
};
pub use layer_compositor::{
    SceneLayerCompositorBlendKey, SceneLayerCompositorCommand, SceneLayerCompositorCondition,
    SceneLayerCompositorEntry, SceneLayerCompositorLayer, SceneLayerCompositorOperation,
    SceneLayerCompositorPlan, SceneLayerCompositorRoute, SceneLayerCompositorTarget,
};
pub use material::{
    SceneAlphaWriteMode, SceneBlendContract, SceneCullMode, SceneDepthTest, SceneMaterialContract,
    SceneMaterialKey, SceneMaterialRenderState,
};
pub use object::{SceneObject, SceneObjectGeometry, SceneObjectId};
pub use plan::SceneEnginePlan;
pub use puppet_clipping::{
    ScenePuppetClippingActiveSource, ScenePuppetClippingProgram, ScenePuppetClippingRecord,
    scene_stable_name_hash,
};
pub use renderer_scene_render::RendererSceneRender;
pub use rendering_device::{RenderingDevice, RenderingDeviceCommand};
pub use rendering_server::RenderingServer;
pub use resource::{
    SceneGeometryId, SceneLayerAlphaMaskRtMethod8MdlvGeometry,
    SceneLayerAlphaMaskRtMethod8MdlvSourceRecord, SceneLayerAlphaMaskRtMethod8MdlvSubdraw,
    ScenePuppetId, SceneResource, SceneResourceId, SceneTextureFormat,
};
pub use resource_gpu_layout::{
    SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES, SCENE_GPU_IRIS_EFFECT_FRAGMENT_UNIFORM_BYTES,
    SCENE_GPU_IRIS_EFFECT_UNIFORM_BYTES, SCENE_GPU_IRIS_EFFECT_VERTEX_UNIFORM_BYTES,
    SCENE_GPU_LAYER_ALPHA_MASK_RT_METHOD8_MDLV_INDEX_BYTES, SCENE_GPU_MESH_INDEX_BYTES,
    SCENE_GPU_MESH_VERTEX_BYTES, SCENE_GPU_PARENT_NONE, SCENE_GPU_PUPPET_ACTIVE_SOURCE_BYTES,
    SCENE_GPU_PUPPET_BONE_BYTES, SCENE_GPU_PUPPET_CLIP_FRAME_BYTES,
    SCENE_GPU_PUPPET_CLIPPING_BONE_INDEX_BYTES, SCENE_GPU_PUPPET_CLIPPING_FRAME_KEY_BYTES,
    SCENE_GPU_PUPPET_CLIPPING_RECORD_BYTES, SCENE_GPU_PUPPET_SKIN_VERTEX_BYTES,
    SCENE_GPU_PUPPET_TRANSFORM_BYTES, scene_gpu_record_bytes,
};
pub use resource_residency::{
    SceneBufferResidency, SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency,
    SceneLayerAuxCompositeTargetsResidency, SceneMeshResidency, ScenePuppetRigResidency,
    SceneResidentResource, SceneResourceResidencyPlan, SceneTextureResidency,
};
pub use shader_uniforms::{SceneGenericImage4MaterialUniformRecord, SceneShaderUniformFramePlan};
pub use timeline::{SceneSampleClock, SceneTimelineSample};
pub use we::{WE_VEC4_BYTES, WE_VEC4_LANES, WeShaderInterface, WeVec4};

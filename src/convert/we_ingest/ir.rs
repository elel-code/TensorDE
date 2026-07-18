//! Wallpaper Engine cold-path ingest IR.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/project-format.md`
//! - `reverse-engineered/docs/scene-pkg-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/mdl-format.md`

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::engine::render_graph::RenderGraph;
use crate::engine::scene::abi::{
    SceneAudioBandMaterialTarget, SceneCullMode, SceneDepthTest,
    SceneObjectKind as SceneAbiObjectKind, ScenePipelineBlend, SceneResourceKind,
    SceneTextProviderKind, SceneTextureFormat, SceneVec3,
};

mod particle;

pub use particle::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeSceneIr {
    pub project_root: PathBuf,
    pub project: WeProjectIr,
    pub scene: WeSceneRootIr,
    pub resources: Vec<WeIrResource>,
    pub textures: Vec<WeIrTexture>,
    pub objects: Vec<WeIrObject>,
    pub object_effects: Vec<WeIrObjectEffect>,
    pub object_animation_layers: Vec<WeIrObjectAnimationLayer>,
    pub object_transform_tracks: Vec<WeIrObjectTransformTrack>,
    pub object_transform_channels: Vec<WeIrObjectTransformChannel>,
    pub object_transform_keyframes: Vec<WeIrObjectTransformKeyframe>,
    pub audio_band_material_bindings: Vec<WeIrAudioBandMaterialBinding>,
    pub text_providers: Vec<WeIrTextProvider>,
    pub puppet_animation_clips: Vec<WeIrPuppetAnimationClip>,
    pub puppet_animation_tracks: Vec<WeIrPuppetAnimationTrack>,
    pub puppet_animation_transform_samples: Vec<WeIrPuppetAnimationTransformSample>,
    pub puppet_animation_opacity_samples: Vec<f32>,
    pub materials: Vec<WeIrMaterial>,
    pub material_passes: Vec<WeIrMaterialPass>,
    pub material_textures: Vec<WeIrMaterialTexture>,
    pub material_constants: Vec<WeIrMaterialConstant>,
    pub meshes: Vec<WeIrMesh>,
    pub mesh_vertices: Vec<WeIrMeshVertex>,
    pub mesh_indices: Vec<u32>,
    pub mesh_source_records: Vec<WeIrMeshSourceRecord>,
    pub mesh_clipping_subdraws: Vec<WeIrMeshClippingSubdraw>,
    pub mesh_clipping_source_ordinals: Vec<u32>,
    pub mesh_clipping_slices: Vec<WeIrMeshClippingSlice>,
    pub puppets: Vec<WeIrPuppet>,
    pub puppet_bones: Vec<WeIrPuppetBone>,
    pub puppet_attachments: Vec<WeIrPuppetAttachment>,
    pub particles: Vec<WeIrParticleSystem>,
    pub effects: Vec<WeIrEffect>,
    pub effect_passes: Vec<WeIrEffectPass>,
    pub effect_bindings: Vec<WeIrEffectBinding>,
    pub effect_combos: Vec<WeIrEffectCombo>,
    pub shader_combo_definitions: Vec<WeIrShaderComboDefinition>,
    pub effect_fbos: Vec<WeIrEffectFbo>,
    pub render_graphs: Vec<RenderGraph>,
    pub image_targets: Vec<WeIrImageTarget>,
    pub shader_contracts: Vec<WeIrShaderContract>,
    pub unsupported: Vec<WeIrUnsupported>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrTextProvider {
    pub object: u32,
    pub kind: SceneTextProviderKind,
    pub initial_text: String,
    pub source_data: String,
    pub update_interval_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeProjectIr {
    pub title: String,
    pub wallpaper_type: String,
    pub scene_file: String,
    pub preview: String,
    pub properties_json: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeSceneRootIr {
    pub logical_width: u32,
    pub logical_height: u32,
    pub clear_color: [f32; 4],
    pub ambient_color: [f32; 4],
    pub skylight_color: [f32; 4],
    pub camera_eye: SceneVec3,
    pub camera_center: SceneVec3,
    pub camera_up: SceneVec3,
    pub camera_parallax_enabled: bool,
    pub camera_parallax_amount: f32,
    pub camera_parallax_delay: f32,
    pub camera_parallax_mouse_influence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrResource {
    pub handle: u32,
    pub kind: SceneResourceKind,
    pub path: String,
    pub source: WeIrResourceSource,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeIrResourceSource {
    LooseFile,
    ScenePackage,
    Builtin,
    Missing,
}

impl WeIrResourceSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LooseFile => "loose-file",
            Self::ScenePackage => "scene.pkg",
            Self::Builtin => "builtin",
            Self::Missing => "missing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrTexture {
    pub resource: u32,
    pub format: SceneTextureFormat,
    pub source_runtime_format: u32,
    pub payload_format: u32,
    pub sampler_flags: u32,
    pub width: u32,
    pub height: u32,
    pub storage_width: u32,
    pub storage_height: u32,
    pub texv_tag: String,
    pub texb_tag: String,
    pub mips: Vec<WeIrTextureMip>,
    pub upload_payload: Vec<u8>,
    pub alpha_coverage_rows: [u32; crate::engine::scene::SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrTextureMip {
    pub width: u32,
    pub height: u32,
    pub payload_offset: u64,
    pub payload_len: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrObject {
    pub handle: u32,
    pub we_id: u32,
    pub name: String,
    pub kind: SceneAbiObjectKind,
    pub resource: Option<u32>,
    pub material: Option<u32>,
    pub parent_we_id: Option<u32>,
    pub attachment: String,
    pub origin: SceneVec3,
    pub angles: SceneVec3,
    pub scale: SceneVec3,
    pub color: SceneVec3,
    pub alpha: f32,
    pub visible: bool,
    pub color_blend_mode: i32,
    pub sort_order: i32,
    pub parallax_depth: [f32; 2],
    pub utility_layer: Option<WeIrUtilityLayerKind>,
    pub render_graph: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeIrUtilityLayerKind {
    SolidColor,
    FramebufferComposite,
    FullscreenPostprocess,
}

impl WeIrUtilityLayerKind {
    pub const fn samples_scene_color(self) -> bool {
        matches!(
            self,
            Self::FramebufferComposite | Self::FullscreenPostprocess
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrObjectEffect {
    pub object: u32,
    pub effect: u32,
    pub instance_id: u32,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeIrAudioBandMaterialBinding {
    pub object: u32,
    pub target: SceneAudioBandMaterialTarget,
    pub spectrum_resolution: u32,
    pub band_index: u32,
    pub smoothing: f32,
    pub minimum_multiplier: f32,
    pub maximum_multiplier: f32,
    pub initial_value: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrObjectAnimationLayer {
    pub object: u32,
    pub animation_id: u32,
    pub layer_index: u32,
    pub additive: bool,
    pub autosort: bool,
    pub visible: bool,
    pub playback_rate: f32,
    pub blend_weight: f32,
    pub initial_progress: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeIrObjectTransformProperty {
    Origin,
    Angles,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeIrObjectTransformChannelKind {
    Keyframed,
    Sine,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrObjectTransformTrack {
    pub object: u32,
    pub property: WeIrObjectTransformProperty,
    pub relative: bool,
    pub wrap_loop: bool,
    pub playback: String,
    pub fps: f32,
    pub frame_count: u32,
    pub channel_start: u32,
    pub channel_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrObjectTransformChannel {
    pub track: u32,
    pub component: u32,
    pub kind: WeIrObjectTransformChannelKind,
    pub offset: f32,
    pub amplitude: f32,
    pub frequency: f32,
    pub phase: f32,
    pub keyframe_start: u32,
    pub keyframe_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrObjectTransformKeyframe {
    pub frame: f32,
    pub value: f32,
    pub back: [f32; 2],
    pub front: [f32; 2],
    pub back_enabled: bool,
    pub front_enabled: bool,
    pub back_magic: bool,
    pub front_magic: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrPuppetAnimationClip {
    pub puppet: u32,
    pub clip_id: u32,
    pub flags: u32,
    pub name: String,
    pub playback: String,
    pub fps: f32,
    pub frame_count: u32,
    pub frame_metadata: u32,
    pub track_start: u32,
    pub track_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrPuppetAnimationTrack {
    pub clip: u32,
    pub bone_index: u32,
    pub track_flags: u32,
    pub sample_start: u32,
    pub sample_count: u32,
    pub opacity_flags: u32,
    pub opacity_sample_start: u32,
    pub opacity_sample_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrPuppetAnimationTransformSample {
    pub translation: SceneVec3,
    pub rotation: SceneVec3,
    pub scale: SceneVec3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrMaterial {
    pub handle: u32,
    pub resource: u32,
    pub pass_start: u32,
    pub pass_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrMaterialPass {
    pub material: u32,
    pub shader_key: String,
    pub target: String,
    pub texture_start: u32,
    pub texture_count: u32,
    pub constant_start: u32,
    pub constant_count: u32,
    pub pipeline_blend: ScenePipelineBlend,
    pub depth_test: SceneDepthTest,
    pub depth_write: bool,
    pub cull_mode: SceneCullMode,
    pub alpha_writing: String,
    pub clear_target: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrMaterialTexture {
    pub slot: u32,
    pub resource: Option<u32>,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrMaterialConstant {
    pub name: String,
    pub value_json: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrMesh {
    pub object: u32,
    pub material: Option<u32>,
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
    pub width: f32,
    pub height: f32,
    pub bounds_min: SceneVec3,
    pub bounds_max: SceneVec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrMeshVertex {
    pub position: SceneVec3,
    pub uv: [f32; 2],
    pub blend_indices: [u32; 4],
    pub blend_weights: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrMeshSourceRecord {
    pub mesh: u32,
    pub source_index: u32,
    pub local_index_offset: u32,
    pub index_start: u32,
    pub index_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrMeshClippingSubdraw {
    pub mesh: u32,
    pub source_qword: u64,
    pub mask: String,
    pub mask_resource: Option<u32>,
    pub raw_flags: u32,
    pub target_source_start: u32,
    pub target_source_count: u32,
    pub mask_source_start: u32,
    pub mask_source_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeIrMeshClippingSliceRole {
    VisiblePrefix,
    MaskProducer,
    ClippedTarget,
    VisibleRemainder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrMeshClippingSlice {
    pub mesh: u32,
    pub subdraw: u32,
    pub role: WeIrMeshClippingSliceRole,
    pub index_start: u32,
    pub index_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrPuppet {
    pub object: u32,
    pub resource: u32,
    pub mesh_start: u32,
    pub mesh_count: u32,
    pub bone_start: u32,
    pub bone_count: u32,
    pub attachment_start: u32,
    pub attachment_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrPuppetBone {
    pub puppet: u32,
    pub bone_index: u32,
    pub name: String,
    pub simulation_type: i32,
    pub parent_index: i32,
    pub local_bind_matrix: [f32; 16],
    pub simulation_json: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrPuppetAttachment {
    pub puppet: u32,
    pub bone_index: u32,
    pub name: String,
    pub local_matrix: [f32; 16],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrEffect {
    pub handle: u32,
    pub resource: u32,
    pub replacement_key: String,
    pub pass_start: u32,
    pub pass_count: u32,
    pub fbo_start: u32,
    pub fbo_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrEffectPass {
    pub effect: u32,
    pub pass_index: u32,
    pub material: Option<u32>,
    pub command: String,
    pub source: String,
    pub target: String,
    pub binding_start: u32,
    pub binding_count: u32,
    pub combo_start: u32,
    pub combo_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrEffectBinding {
    pub slot: u32,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrEffectCombo {
    pub name: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrShaderComboDefinition {
    pub shader_key: String,
    pub name: String,
    pub default_value: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeIrEffectFbo {
    pub name: String,
    pub format: String,
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrImageTarget {
    pub name: String,
    pub format: String,
    pub role: WeIrImageTargetRole,
    pub width_divisor_milli: u32,
    pub height_divisor_milli: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeIrImageTargetRole {
    NamedFbo,
    FirstClassEffectTarget,
    Temporary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrShaderContract {
    pub shader_key: String,
    pub pipeline_key: String,
    pub texture_slot_mask: u32,
    pub constants: Vec<String>,
    pub resource_heap_count: u32,
    pub sampler_heap_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeIrUnsupported {
    pub object: Option<u32>,
    pub pass_index: Option<u32>,
    pub feature: String,
    pub expected_subsystem: String,
    pub containment: String,
}

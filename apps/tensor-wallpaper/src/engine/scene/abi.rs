//! New scene engine ABI records.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/project-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/scene-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/material-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/effect-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/tex-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/mdl-format.md`
//! - `references/tensor-wallpaper/godot/servers/rendering/rendering_server_default.*`

use serde::{Deserialize, Serialize};

mod binary_contract;
mod dynamic_text_contract;
mod event_contract;
mod particle_contract;
mod render_contract;
mod render_state;
mod script_contract;
mod user_property_contract;
pub use {
    binary_contract::*, dynamic_text_contract::*, event_contract::*, particle_contract::*,
    render_contract::*, render_state::*, script_contract::*, user_property_contract::*,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SceneStringId(pub u32);

impl SceneStringId {
    pub const NONE: Self = Self(INVALID_STRING_ID);

    pub const fn is_some(self) -> bool {
        self.0 != INVALID_STRING_ID
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SceneResourceId(pub u32);

impl SceneResourceId {
    pub const NONE: Self = Self(INVALID_RESOURCE_ID);

    pub const fn is_some(self) -> bool {
        self.0 != INVALID_RESOURCE_ID
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SceneObjectHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SceneMaterialHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SceneEffectHandle(pub u32);

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneVec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl SceneVec3 {
    pub const ONE: Self = Self {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneResourceKind {
    ProjectJson,
    SceneJson,
    ScenePackageEntry,
    ModelJson,
    TextureTex,
    MaterialJson,
    EffectJson,
    Mdl,
    Audio,
    Font,
    Script,
    Raw,
    BuiltinShader,
    ParticleDefinition,
}

impl SceneResourceKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::ProjectJson => 1,
            Self::SceneJson => 2,
            Self::ScenePackageEntry => 3,
            Self::ModelJson => 4,
            Self::TextureTex => 5,
            Self::MaterialJson => 6,
            Self::EffectJson => 7,
            Self::Mdl => 8,
            Self::Audio => 9,
            Self::Font => 10,
            Self::Script => 11,
            Self::Raw => 12,
            Self::BuiltinShader => 13,
            Self::ParticleDefinition => 14,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::ProjectJson),
            2 => Some(Self::SceneJson),
            3 => Some(Self::ScenePackageEntry),
            4 => Some(Self::ModelJson),
            5 => Some(Self::TextureTex),
            6 => Some(Self::MaterialJson),
            7 => Some(Self::EffectJson),
            8 => Some(Self::Mdl),
            9 => Some(Self::Audio),
            10 => Some(Self::Font),
            11 => Some(Self::Script),
            12 => Some(Self::Raw),
            13 => Some(Self::BuiltinShader),
            14 => Some(Self::ParticleDefinition),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneObjectKind {
    Image,
    Model,
    Puppet,
    ParticleEmitter,
    Text,
    Clear,
    Camera,
    Unsupported,
}

impl SceneObjectKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Image => 1,
            Self::Model => 2,
            Self::Puppet => 3,
            Self::ParticleEmitter => 4,
            Self::Text => 5,
            Self::Clear => 6,
            Self::Camera => 7,
            Self::Unsupported => 0xffff,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Image),
            2 => Some(Self::Model),
            3 => Some(Self::Puppet),
            4 => Some(Self::ParticleEmitter),
            5 => Some(Self::Text),
            6 => Some(Self::Clear),
            7 => Some(Self::Camera),
            0xffff => Some(Self::Unsupported),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderTargetKind {
    SceneColor,
    Swapchain,
    ImageLocalMain,
    ImageLocalSub,
    NamedFbo,
    FirstClassEffectTarget,
    VideoExternalImage,
    Temporary,
}

impl SceneRenderTargetKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::SceneColor => 1,
            Self::Swapchain => 2,
            Self::ImageLocalMain => 3,
            Self::ImageLocalSub => 4,
            Self::NamedFbo => 5,
            Self::FirstClassEffectTarget => 6,
            Self::VideoExternalImage => 7,
            Self::Temporary => 8,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::SceneColor),
            2 => Some(Self::Swapchain),
            3 => Some(Self::ImageLocalMain),
            4 => Some(Self::ImageLocalSub),
            5 => Some(Self::NamedFbo),
            6 => Some(Self::FirstClassEffectTarget),
            7 => Some(Self::VideoExternalImage),
            8 => Some(Self::Temporary),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderPassKind {
    Clear,
    BaseMaterial,
    ObjectLocalSource,
    EffectMaterial,
    ColorBlendPassthrough,
    CopyTarget,
    SwapTargetReferences,
    VideoSample,
    Particle,
    TextPath,
    SceneComposite,
    MeshVisiblePrefix,
    MeshClippingMask,
    MeshClippedTarget,
    MeshVisibleRemainder,
    DebugEvidence,
    Unsupported,
}

impl SceneRenderPassKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Clear => 1,
            Self::BaseMaterial => 2,
            Self::ObjectLocalSource => 16,
            Self::EffectMaterial => 3,
            Self::ColorBlendPassthrough => 4,
            Self::CopyTarget => 5,
            Self::SwapTargetReferences => 6,
            Self::VideoSample => 7,
            Self::Particle => 8,
            Self::TextPath => 9,
            Self::SceneComposite => 10,
            Self::MeshVisiblePrefix => 12,
            Self::MeshClippingMask => 13,
            Self::MeshClippedTarget => 14,
            Self::MeshVisibleRemainder => 15,
            Self::DebugEvidence => 11,
            Self::Unsupported => 0xffff,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Clear),
            2 => Some(Self::BaseMaterial),
            16 => Some(Self::ObjectLocalSource),
            3 => Some(Self::EffectMaterial),
            4 => Some(Self::ColorBlendPassthrough),
            5 => Some(Self::CopyTarget),
            6 => Some(Self::SwapTargetReferences),
            7 => Some(Self::VideoSample),
            8 => Some(Self::Particle),
            9 => Some(Self::TextPath),
            10 => Some(Self::SceneComposite),
            12 => Some(Self::MeshVisiblePrefix),
            13 => Some(Self::MeshClippingMask),
            14 => Some(Self::MeshClippedTarget),
            15 => Some(Self::MeshVisibleRemainder),
            11 => Some(Self::DebugEvidence),
            0xffff => Some(Self::Unsupported),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderPassDrawPrimitive {
    None,
    ObjectMesh,
    ObjectCompositeMesh,
    FramebufferCompositeMesh,
    FullscreenTriangle,
    ObjectUvSupportQuad,
    ParticleBillboard,
}

impl SceneRenderPassDrawPrimitive {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::None => 0,
            Self::ObjectMesh => 1,
            Self::ObjectCompositeMesh => 5,
            Self::FramebufferCompositeMesh => 6,
            Self::FullscreenTriangle => 2,
            Self::ObjectUvSupportQuad => 3,
            Self::ParticleBillboard => 4,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::ObjectMesh),
            5 => Some(Self::ObjectCompositeMesh),
            6 => Some(Self::FramebufferCompositeMesh),
            2 => Some(Self::FullscreenTriangle),
            3 => Some(Self::ObjectUvSupportQuad),
            4 => Some(Self::ParticleBillboard),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderGraphActivationPolicy {
    Always,
    AnyEffectVisible,
}

impl SceneRenderGraphActivationPolicy {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Always => 0,
            Self::AnyEffectVisible => 1,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Always),
            1 => Some(Self::AnyEffectVisible),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderEffectVisibilityPolicy {
    None,
    Passthrough,
    WaterWavesStages,
    FlatRoundedMask,
    MaterialStages,
    AnyVisible,
    NoneVisible,
}

impl SceneRenderEffectVisibilityPolicy {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Passthrough => 1,
            Self::WaterWavesStages => 2,
            Self::FlatRoundedMask => 3,
            Self::MaterialStages => 4,
            Self::AnyVisible => 5,
            Self::NoneVisible => 6,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Passthrough),
            2 => Some(Self::WaterWavesStages),
            3 => Some(Self::FlatRoundedMask),
            4 => Some(Self::MaterialStages),
            5 => Some(Self::AnyVisible),
            6 => Some(Self::NoneVisible),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderBindingKind {
    SourceTexture,
    TextureSlot,
    AlphaTextureSlot,
    PreviousGraphTarget,
    GraphTarget,
    NamedFboBind,
    EffectTarget,
    VideoFrame,
    AudioUniform,
    SystemUniform,
    PassConstant,
}

impl SceneRenderBindingKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::SourceTexture => 1,
            Self::TextureSlot => 2,
            Self::AlphaTextureSlot => 3,
            Self::PreviousGraphTarget => 4,
            Self::GraphTarget => 5,
            Self::NamedFboBind => 6,
            Self::EffectTarget => 7,
            Self::VideoFrame => 8,
            Self::AudioUniform => 9,
            Self::SystemUniform => 10,
            Self::PassConstant => 11,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::SourceTexture),
            2 => Some(Self::TextureSlot),
            3 => Some(Self::AlphaTextureSlot),
            4 => Some(Self::PreviousGraphTarget),
            5 => Some(Self::GraphTarget),
            6 => Some(Self::NamedFboBind),
            7 => Some(Self::EffectTarget),
            8 => Some(Self::VideoFrame),
            9 => Some(Self::AudioUniform),
            10 => Some(Self::SystemUniform),
            11 => Some(Self::PassConstant),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneProjectRecord {
    pub title: SceneStringId,
    pub wallpaper_type: SceneStringId,
    pub scene_file: SceneStringId,
    pub preview: SceneStringId,
    pub properties_json: SceneStringId,
    pub logical_width: u32,
    pub logical_height: u32,
    pub clear_color: [f32; 4],
    pub ambient_color: [f32; 4],
    pub skylight_color: [f32; 4],
    pub camera_eye: SceneVec3,
    pub camera_center: SceneVec3,
    pub camera_up: SceneVec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneResourceRecord {
    pub id: SceneResourceId,
    pub kind: SceneResourceKind,
    pub path: SceneStringId,
    pub source: SceneStringId,
    pub payload_offset: u64,
    pub payload_len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneTextureRecord {
    pub resource: SceneResourceId,
    pub format: SceneTextureFormat,
    pub source_runtime_format: u32,
    pub payload_format: u32,
    pub sampler_filter: SceneTextureSamplerFilter,
    pub sampler_address_mode: SceneTextureSamplerAddressMode,
    pub width: u32,
    pub height: u32,
    pub storage_width: u32,
    pub storage_height: u32,
    pub mip_start: u32,
    pub mip_count: u32,
    pub texv_tag: SceneStringId,
    pub texb_tag: SceneStringId,
    pub sequence_tag: SceneStringId,
    pub sequence_cell_width: u32,
    pub sequence_cell_height: u32,
    pub sequence_frame_start: u32,
    pub sequence_frame_count: u32,
    pub payload_offset: u64,
    pub payload_len: u64,
    pub alpha_coverage_rows: [u32; SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneTextureSequenceFrameRecord {
    pub resource_index: u32,
    pub duration: f32,
    pub origin: [f32; 2],
    pub axis_x: [f32; 2],
    pub axis_y: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneTextureMipRecord {
    pub width: u32,
    pub height: u32,
    pub payload_offset: u64,
    pub payload_len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneTextureFormat {
    Rgba8Unorm,
    Rg8Unorm,
    R8Unorm,
    Bc1RgbaUnormBlock,
    Bc2UnormBlock,
    Bc3UnormBlock,
    Bc4UnormBlock,
    Bc5UnormBlock,
    Bc7UnormBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneTextureSamplerFilter {
    Point,
    Linear,
    Anisotropic8,
}

impl SceneTextureSamplerFilter {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Point => 0,
            Self::Linear => 1,
            Self::Anisotropic8 => 2,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Point),
            1 => Some(Self::Linear),
            2 => Some(Self::Anisotropic8),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneTextureSamplerAddressMode {
    Repeat,
    ClampToEdge,
    ClampToTransparentBlackBorder,
}

impl SceneTextureSamplerAddressMode {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Repeat => 0,
            Self::ClampToEdge => 1,
            Self::ClampToTransparentBlackBorder => 2,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Repeat),
            1 => Some(Self::ClampToEdge),
            2 => Some(Self::ClampToTransparentBlackBorder),
            _ => None,
        }
    }
}

impl SceneTextureFormat {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Rgba8Unorm => 0,
            Self::Rg8Unorm => 1,
            Self::R8Unorm => 2,
            Self::Bc1RgbaUnormBlock => 3,
            Self::Bc2UnormBlock => 4,
            Self::Bc3UnormBlock => 5,
            Self::Bc4UnormBlock => 6,
            Self::Bc5UnormBlock => 7,
            Self::Bc7UnormBlock => 8,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Rgba8Unorm),
            1 => Some(Self::Rg8Unorm),
            2 => Some(Self::R8Unorm),
            3 => Some(Self::Bc1RgbaUnormBlock),
            4 => Some(Self::Bc2UnormBlock),
            5 => Some(Self::Bc3UnormBlock),
            6 => Some(Self::Bc4UnormBlock),
            7 => Some(Self::Bc5UnormBlock),
            8 => Some(Self::Bc7UnormBlock),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneObjectRecord {
    pub id: SceneObjectHandle,
    pub we_id: u32,
    pub name: SceneStringId,
    pub kind: SceneObjectKind,
    pub resource: SceneResourceId,
    pub material: SceneMaterialHandle,
    pub parent_we_id: u32,
    pub attachment: SceneStringId,
    pub origin: SceneVec3,
    pub angles: SceneVec3,
    pub scale: SceneVec3,
    pub camera_zoom: f32,
    pub color: SceneVec3,
    pub alpha: f32,
    pub visible: bool,
    pub color_blend_mode: i32,
    pub sort_order: i32,
    pub effect_start: u32,
    pub effect_count: u32,
    pub render_graph: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneObjectEffectRecord {
    pub object: SceneObjectHandle,
    pub effect: SceneEffectHandle,
    pub name: SceneStringId,
    pub instance_id: u32,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneObjectAnimationLayerRecord {
    pub object: SceneObjectHandle,
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
pub enum SceneObjectTransformProperty {
    Origin,
    Angles,
    Scale,
    CameraZoom,
}

impl SceneObjectTransformProperty {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Origin => 1,
            Self::Angles => 2,
            Self::Scale => 3,
            Self::CameraZoom => 4,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Origin),
            2 => Some(Self::Angles),
            3 => Some(Self::Scale),
            4 => Some(Self::CameraZoom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneObjectTransformChannelKind {
    Keyframed,
    Sine,
}

impl SceneObjectTransformChannelKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Keyframed => 1,
            Self::Sine => 2,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Keyframed),
            2 => Some(Self::Sine),
            _ => None,
        }
    }
}

pub const SCENE_OBJECT_TRANSFORM_TRACK_RELATIVE: u32 = 1 << 0;
pub const SCENE_OBJECT_TRANSFORM_TRACK_WRAP_LOOP: u32 = 1 << 1;
pub const SCENE_OBJECT_TRANSFORM_KEYFRAME_BACK_ENABLED: u32 = 1 << 0;
pub const SCENE_OBJECT_TRANSFORM_KEYFRAME_FRONT_ENABLED: u32 = 1 << 1;
pub const SCENE_OBJECT_TRANSFORM_KEYFRAME_BACK_MAGIC: u32 = 1 << 2;
pub const SCENE_OBJECT_TRANSFORM_KEYFRAME_FRONT_MAGIC: u32 = 1 << 3;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneObjectTransformTrackRecord {
    pub object: SceneObjectHandle,
    pub property: SceneObjectTransformProperty,
    pub flags: u32,
    pub playback: SceneStringId,
    pub fps: f32,
    pub frame_count: u32,
    pub channel_start: u32,
    pub channel_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneObjectTransformChannelRecord {
    pub track: u32,
    pub component: u32,
    pub kind: SceneObjectTransformChannelKind,
    pub offset: f32,
    pub amplitude: f32,
    pub frequency: f32,
    pub phase: f32,
    pub keyframe_start: u32,
    pub keyframe_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneObjectTransformKeyframeRecord {
    pub frame: f32,
    pub value: f32,
    pub back: [f32; 2],
    pub front: [f32; 2],
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetAnimationClipRecord {
    pub puppet: u32,
    pub clip_id: u32,
    pub flags: u32,
    pub name: SceneStringId,
    pub playback: SceneStringId,
    pub fps: f32,
    pub frame_count: u32,
    pub frame_metadata: u32,
    pub track_start: u32,
    pub track_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenePuppetAnimationTrackRecord {
    pub clip: u32,
    pub bone_index: u32,
    pub track_flags: u32,
    pub sample_start: u32,
    pub sample_count: u32,
    pub opacity_flags: u32,
    pub opacity_sample_start: u32,
    pub opacity_sample_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetAnimationTransformSampleRecord {
    pub translation: SceneVec3,
    pub rotation: SceneVec3,
    pub scale: SceneVec3,
}

include!("abi/material_mesh_effect_records.rs");

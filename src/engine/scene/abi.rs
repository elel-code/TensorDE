//! New scene engine ABI records.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/project-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `references/godot/servers/rendering/rendering_server_default.*`
//! - `references/godot/servers/rendering/storage/*`

use serde::{Deserialize, Serialize};

pub const SCENE_BINARY_MAGIC: [u8; 8] = *b"GSCNENG1";
pub const SCENE_BINARY_VERSION: u32 = 1;
pub const SCENE_BINARY_ENDIANNESS_LITTLE: u8 = 1;

pub const SCENE_FEATURE_DESCRIPTOR_HEAP: u64 = 1 << 0;
pub const SCENE_FEATURE_RENDER_GRAPH: u64 = 1 << 1;
pub const SCENE_FEATURE_EMBEDDED_PAYLOADS: u64 = 1 << 2;
pub const SCENE_FEATURE_WE_SEMANTICS: u64 = 1 << 3;
pub const SCENE_DEFAULT_FEATURE_FLAGS: u64 = SCENE_FEATURE_DESCRIPTOR_HEAP
    | SCENE_FEATURE_RENDER_GRAPH
    | SCENE_FEATURE_EMBEDDED_PAYLOADS
    | SCENE_FEATURE_WE_SEMANTICS;

pub const CHUNK_STRING_TABLE: u32 = u32::from_le_bytes(*b"STRS");
pub const CHUNK_PROJECT: u32 = u32::from_le_bytes(*b"PROJ");
pub const CHUNK_SCENE_OBJECT: u32 = u32::from_le_bytes(*b"OBJT");
pub const CHUNK_RESOURCE: u32 = u32::from_le_bytes(*b"RSRC");
pub const CHUNK_RESOURCE_PAYLOAD: u32 = u32::from_le_bytes(*b"PAYL");
pub const CHUNK_TEXTURE: u32 = u32::from_le_bytes(*b"TEXR");
pub const CHUNK_MATERIAL: u32 = u32::from_le_bytes(*b"MTRL");
pub const CHUNK_EFFECT: u32 = u32::from_le_bytes(*b"EFFT");
pub const CHUNK_TIMELINE: u32 = u32::from_le_bytes(*b"TMLN");
pub const CHUNK_MESH: u32 = u32::from_le_bytes(*b"MESH");
pub const CHUNK_PUPPET: u32 = u32::from_le_bytes(*b"PUPP");
pub const CHUNK_PARTICLE: u32 = u32::from_le_bytes(*b"PART");
pub const CHUNK_AUDIO: u32 = u32::from_le_bytes(*b"AUDO");
pub const CHUNK_SCRIPT_BINDING: u32 = u32::from_le_bytes(*b"SCRP");
pub const CHUNK_RENDER_GRAPH: u32 = u32::from_le_bytes(*b"RGRF");
pub const CHUNK_IMAGE_TARGET: u32 = u32::from_le_bytes(*b"IMGT");
pub const CHUNK_SHADER_CONTRACT: u32 = u32::from_le_bytes(*b"SHDR");

pub const REQUIRED_SCENE_CHUNKS: &[u32] = &[
    CHUNK_STRING_TABLE,
    CHUNK_PROJECT,
    CHUNK_SCENE_OBJECT,
    CHUNK_RESOURCE,
    CHUNK_RESOURCE_PAYLOAD,
    CHUNK_TEXTURE,
    CHUNK_MATERIAL,
    CHUNK_EFFECT,
    CHUNK_TIMELINE,
    CHUNK_MESH,
    CHUNK_PUPPET,
    CHUNK_PARTICLE,
    CHUNK_AUDIO,
    CHUNK_SCRIPT_BINDING,
    CHUNK_RENDER_GRAPH,
    CHUNK_IMAGE_TARGET,
    CHUNK_SHADER_CONTRACT,
];

pub const INVALID_STRING_ID: u32 = u32::MAX;
pub const INVALID_RESOURCE_ID: u32 = u32::MAX;
pub const INVALID_OBJECT_ID: u32 = u32::MAX;
pub const INVALID_MATERIAL_ID: u32 = u32::MAX;
pub const INVALID_EFFECT_ID: u32 = u32::MAX;

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
            0xffff => Some(Self::Unsupported),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenePipelineBlend {
    Normal,
    Translucent,
    Additive,
    Disabled,
    AlphaToCoverage,
}

impl ScenePipelineBlend {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::Translucent => 1,
            Self::Additive => 2,
            Self::Disabled => 3,
            Self::AlphaToCoverage => 4,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Normal),
            1 => Some(Self::Translucent),
            2 => Some(Self::Additive),
            3 => Some(Self::Disabled),
            4 => Some(Self::AlphaToCoverage),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneDepthTest {
    Disabled,
    Enabled,
}

impl SceneDepthTest {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Disabled => 0,
            Self::Enabled => 1,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Disabled),
            1 => Some(Self::Enabled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneCullMode {
    None,
    Normal,
}

impl SceneCullMode {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Normal => 1,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Normal),
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
    EffectMaterial,
    ColorBlendPassthrough,
    CopyTarget,
    SwapTargetReferences,
    VideoSample,
    Particle,
    TextPath,
    SceneComposite,
    DebugEvidence,
    Unsupported,
}

impl SceneRenderPassKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Clear => 1,
            Self::BaseMaterial => 2,
            Self::EffectMaterial => 3,
            Self::ColorBlendPassthrough => 4,
            Self::CopyTarget => 5,
            Self::SwapTargetReferences => 6,
            Self::VideoSample => 7,
            Self::Particle => 8,
            Self::TextPath => 9,
            Self::SceneComposite => 10,
            Self::DebugEvidence => 11,
            Self::Unsupported => 0xffff,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Clear),
            2 => Some(Self::BaseMaterial),
            3 => Some(Self::EffectMaterial),
            4 => Some(Self::ColorBlendPassthrough),
            5 => Some(Self::CopyTarget),
            6 => Some(Self::SwapTargetReferences),
            7 => Some(Self::VideoSample),
            8 => Some(Self::Particle),
            9 => Some(Self::TextPath),
            10 => Some(Self::SceneComposite),
            11 => Some(Self::DebugEvidence),
            0xffff => Some(Self::Unsupported),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneTextureRecord {
    pub resource: SceneResourceId,
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub storage_width: u32,
    pub storage_height: u32,
    pub mip_count: u32,
    pub texv_tag: SceneStringId,
    pub texb_tag: SceneStringId,
    pub payload_offset: u64,
    pub payload_len: u64,
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
    pub instance_id: u32,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneObjectAnimationLayerRecord {
    pub object: SceneObjectHandle,
    pub animation_id: u32,
    pub layer_index: u32,
    pub additive: bool,
    pub autosort: bool,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetAnimationTransformSampleRecord {
    pub translation: SceneVec3,
    pub rotation: SceneVec3,
    pub scale: SceneVec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMaterialRecord {
    pub id: SceneMaterialHandle,
    pub resource: SceneResourceId,
    pub pass_start: u32,
    pub pass_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMaterialPassRecord {
    pub material: SceneMaterialHandle,
    pub shader_key: SceneStringId,
    pub target: SceneStringId,
    pub texture_start: u32,
    pub texture_count: u32,
    pub constant_start: u32,
    pub constant_count: u32,
    pub pipeline_blend: ScenePipelineBlend,
    pub depth_test: SceneDepthTest,
    pub depth_write: bool,
    pub cull_mode: SceneCullMode,
    pub alpha_writing: SceneStringId,
    pub clear_target: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMaterialTextureRecord {
    pub slot: u32,
    pub resource: SceneResourceId,
    pub path: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMaterialConstantRecord {
    pub name: SceneStringId,
    pub value_json: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshRecord {
    pub object: SceneObjectHandle,
    pub material: SceneMaterialHandle,
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
    pub width: f32,
    pub height: f32,
    pub bounds_min: SceneVec3,
    pub bounds_max: SceneVec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshVertexRecord {
    pub position: SceneVec3,
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenePuppetRecord {
    pub object: SceneObjectHandle,
    pub resource: SceneResourceId,
    pub mesh_start: u32,
    pub mesh_count: u32,
    pub bone_start: u32,
    pub bone_count: u32,
    pub attachment_start: u32,
    pub attachment_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetBoneRecord {
    pub puppet: u32,
    pub bone_index: u32,
    pub flags: u32,
    pub parent_index: i32,
    pub local_matrix: [f32; 16],
    pub info: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetAttachmentRecord {
    pub puppet: u32,
    pub bone_index: u32,
    pub name: SceneStringId,
    pub local_matrix: [f32; 16],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEffectRecord {
    pub id: SceneEffectHandle,
    pub resource: SceneResourceId,
    pub replacement_key: SceneStringId,
    pub pass_start: u32,
    pub pass_count: u32,
    pub fbo_start: u32,
    pub fbo_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEffectPassRecord {
    pub effect: SceneEffectHandle,
    pub pass_index: u32,
    pub material: SceneMaterialHandle,
    pub command: SceneStringId,
    pub source: SceneStringId,
    pub target: SceneStringId,
    pub binding_start: u32,
    pub binding_count: u32,
    pub combo_start: u32,
    pub combo_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEffectBindingRecord {
    pub slot: u32,
    pub target: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEffectComboRecord {
    pub name: SceneStringId,
    pub value: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectFboRecord {
    pub name: SceneStringId,
    pub format: SceneStringId,
    pub scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderGraphRecord {
    pub object: SceneObjectHandle,
    pub pass_start: u32,
    pub pass_count: u32,
    pub unsupported_start: u32,
    pub unsupported_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderPassRecord {
    pub id: u32,
    pub role: SceneRenderPassKind,
    pub object: SceneObjectHandle,
    pub material: SceneMaterialHandle,
    pub pass_index: u32,
    pub shader_key: SceneStringId,
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
    pub binding_start: u32,
    pub binding_count: u32,
    pub pipeline_blend: ScenePipelineBlend,
    pub depth_test: SceneDepthTest,
    pub depth_write: bool,
    pub cull_mode: SceneCullMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderBindingRecord {
    pub kind: SceneRenderBindingKind,
    pub slot: u32,
    pub target: SceneRenderTargetKind,
    pub name: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneUnsupportedRecord {
    pub object: SceneObjectHandle,
    pub pass_index: u32,
    pub feature: SceneStringId,
    pub expected_subsystem: SceneStringId,
    pub containment: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneImageTargetRecord {
    pub name: SceneStringId,
    pub role: SceneRenderTargetKind,
    pub format: SceneStringId,
    pub scale_x_milli: u32,
    pub scale_y_milli: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneShaderContractRecord {
    pub shader_key: SceneStringId,
    pub pipeline_key: SceneStringId,
    pub texture_slot_mask: u32,
    pub constant_start: u32,
    pub constant_count: u32,
    pub resource_heap_count: u32,
    pub sampler_heap_count: u32,
}

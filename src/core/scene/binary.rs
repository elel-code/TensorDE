use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use super::{
    SceneAlphaTextureMode, SceneAnimatedProperty, SceneBlendMode, SceneCurve, SceneDocument,
    SceneEffect, SceneEffectPass, SceneEffectUvExtent, SceneEffectUvTransform, SceneKeyframe,
    SceneNode, SceneNodeKind, ScenePuppetTransform, SceneResource, SceneResourceKind,
    SceneTimelineChannel,
};
use crate::core::FitMode;

mod effect_uv;
mod flutter;
mod geometry;
mod puppet;

pub(crate) use self::effect_uv::decode_effect_uv_transform_record;
pub use self::effect_uv::{
    SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION,
    SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE, SceneBinaryEffectUvTransformRecord,
};
use self::effect_uv::{effect_uv_transform_flags, effect_uv_transform_mapping_code};
pub(crate) use self::flutter::decode_flutter_state_record;
pub use self::flutter::{
    SCENE_BINARY_FLUTTER_STATE_RECORD_SIZE, SCENE_BINARY_MOTION_FAMILY_DRIFT,
    SCENE_BINARY_MOTION_FAMILY_FLUTTER, SCENE_BINARY_MOTION_FAMILY_SHAKE,
    SCENE_BINARY_MOTION_FAMILY_SWAY, SceneBinaryFlutterStateRecord,
};
use self::flutter::{effect_is_motion_family, motion_dirty_range_count, motion_family_mask};
pub use self::geometry::{
    SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE, SCENE_BINARY_GEOMETRY_PRIMITIVE_AUDIO_RESPONSE,
    SCENE_BINARY_GEOMETRY_PRIMITIVE_ELLIPSE, SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH,
    SCENE_BINARY_GEOMETRY_PRIMITIVE_PARTICLES, SCENE_BINARY_GEOMETRY_PRIMITIVE_PATH,
    SCENE_BINARY_GEOMETRY_PRIMITIVE_QUAD, SCENE_BINARY_GEOMETRY_PRIMITIVE_TEXT,
    SCENE_BINARY_GEOMETRY_QUAD_INDEX_COUNT, SCENE_BINARY_GEOMETRY_QUAD_VERTEX_COUNT,
    SCENE_BINARY_GEOMETRY_RECORD_SIZE, SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_GENERATED,
    SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY,
    SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE, SceneBinaryGeometryIndexRecord,
    SceneBinaryGeometryRecord, SceneBinaryGeometryVertexRecord,
};
pub(crate) use self::geometry::{
    decode_geometry_index_record, decode_geometry_record, decode_geometry_vertex_record,
};
use self::geometry::{
    geometry_flags, geometry_has_uv, geometry_ranges, geometry_stream_shape, node_has_geometry,
};
pub use self::puppet::{
    SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE, SCENE_BINARY_PUPPET_CLIP_FLAG_LOOPING,
    SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE, SCENE_BINARY_PUPPET_FLAG_ANIMATION_LAYERS,
    SCENE_BINARY_PUPPET_FLAG_ATTACHMENTS, SCENE_BINARY_PUPPET_FLAG_CLIPS,
    SCENE_BINARY_PUPPET_FLAG_MESH, SCENE_BINARY_PUPPET_FLAG_SKIN,
    SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE, SCENE_BINARY_PUPPET_LAYER_FLAG_ADDITIVE,
    SCENE_BINARY_PUPPET_LAYER_FLAG_LOCK_TRANSFORMS, SCENE_BINARY_PUPPET_LAYER_FLAG_VISIBLE,
    SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE, SCENE_BINARY_PUPPET_RECORD_SIZE,
    SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE, SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE,
    SceneBinaryPuppetAttachmentRecord, SceneBinaryPuppetClipRecord, SceneBinaryPuppetFrameRecord,
    SceneBinaryPuppetLayerRecord, SceneBinaryPuppetRecord, SceneBinaryPuppetSkinBoneRecord,
    SceneBinaryPuppetSkinVertexRecord,
};
pub(crate) use self::puppet::{
    decode_puppet_attachment_record, decode_puppet_clip_record, decode_puppet_frame_record,
    decode_puppet_layer_record, decode_puppet_record, decode_puppet_skin_bone_record,
    decode_puppet_skin_vertex_record,
};
use self::puppet::{puppet_clip_flags, puppet_first_record, puppet_flags, puppet_layer_flags};

pub const SCENE_BINARY_MAGIC: [u8; 4] = *b"GSCN";
pub const SCENE_BINARY_VERSION: u16 = 10;
pub const SCENE_BINARY_ENDIAN_LITTLE: u8 = 1;
pub const SCENE_BINARY_ALIGNMENT: u8 = 8;
pub const SCENE_BINARY_HEADER_SIZE: usize = 24;
pub const SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE: usize = 24;
pub const SCENE_BINARY_RESOURCE_RECORD_SIZE: usize = 32;
pub const SCENE_BINARY_NODE_RECORD_SIZE: usize = 96;
pub const SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE: usize = 80;
pub const SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE: usize = 16;
pub const SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE: usize = 32;
pub const SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE: usize = 56;
pub const SCENE_BINARY_EFFECT_PASS_RECORD_SIZE: usize = 56;
pub const SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE: usize = 48;
pub const SCENE_BINARY_RENDER_STATE_RECORD_SIZE: usize = 32;
pub const SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE: usize = 24;
pub const SCENE_BINARY_DEBUG_NAME_RECORD_SIZE: usize = 16;

pub const SCENE_BINARY_NONE_ID: u32 = u32::MAX;
const SCENE_BINARY_DEFAULT_TRANSFORM_PROPERTY: u16 = 0;
pub const SCENE_BINARY_RETAINED_RESOURCE: u16 = 1;
pub const SCENE_BINARY_RETAINED_TEXTURE_SLOT: u16 = 2;
pub const SCENE_BINARY_RETAINED_MATERIAL_PASS: u16 = 3;
pub const SCENE_BINARY_RETAINED_EFFECT_PASS: u16 = 4;
pub const SCENE_BINARY_RETAINED_EFFECT_PARAMETER: u16 = 5;
pub const SCENE_BINARY_RETAINED_GEOMETRY: u16 = 6;
pub const SCENE_BINARY_RETAINED_EFFECT_UV_TRANSFORM: u16 = 7;
pub const SCENE_BINARY_RETAINED_PUPPET: u16 = 8;

const SCENE_BINARY_PARAMETER_VALUE_BOOL: u16 = 1;
const SCENE_BINARY_PARAMETER_VALUE_FLOAT: u16 = 2;
const SCENE_BINARY_PARAMETER_VALUE_INTEGER: u16 = 3;
const SCENE_BINARY_PARAMETER_VALUE_STRING: u16 = 4;
const SCENE_BINARY_PARAMETER_VALUE_VEC2: u16 = 5;
const SCENE_BINARY_PARAMETER_VALUE_VEC3: u16 = 6;
const SCENE_BINARY_PARAMETER_VALUE_VEC4: u16 = 7;

pub const SCENE_BINARY_PARAMETER_ROLE_EFFECT_PROPERTY: u16 = 1;
pub const SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT: u16 = 2;
pub const SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO: u16 = 4;

const SCENE_BINARY_TEXTURE_ROLE_BASE_COLOR: u16 = 1;
const SCENE_BINARY_TEXTURE_ROLE_EFFECT_INPUT: u16 = 2;
const SCENE_BINARY_TEXTURE_ROLE_ALPHA_MASK: u16 = 4;
const SCENE_BINARY_TEXTURE_ROLE_FIRST_CLASS_TARGET: u16 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SceneBinaryChunkKind {
    ResourceTable,
    NodeTable,
    TransformTimeline,
    TransformKeyframes,
    Geometry,
    GeometryVertices,
    GeometryIndices,
    TextureSlots,
    MaterialPass,
    EffectPass,
    EffectUvTransform,
    EffectParameter,
    FlutterState,
    Puppet,
    PuppetSkinBones,
    PuppetSkinVertices,
    PuppetAttachments,
    PuppetClips,
    PuppetFrames,
    PuppetLayers,
    RenderState,
    RetainedGpuState,
    DebugNames,
}

impl SceneBinaryChunkKind {
    pub const REQUIRED_ORDER: [Self; 23] = [
        Self::ResourceTable,
        Self::NodeTable,
        Self::TransformTimeline,
        Self::TransformKeyframes,
        Self::Geometry,
        Self::GeometryVertices,
        Self::GeometryIndices,
        Self::TextureSlots,
        Self::MaterialPass,
        Self::EffectPass,
        Self::EffectUvTransform,
        Self::EffectParameter,
        Self::FlutterState,
        Self::Puppet,
        Self::PuppetSkinBones,
        Self::PuppetSkinVertices,
        Self::PuppetAttachments,
        Self::PuppetClips,
        Self::PuppetFrames,
        Self::PuppetLayers,
        Self::RenderState,
        Self::RetainedGpuState,
        Self::DebugNames,
    ];

    pub fn code(self) -> u32 {
        match self {
            Self::ResourceTable => u32::from_le_bytes(*b"REST"),
            Self::NodeTable => u32::from_le_bytes(*b"NODE"),
            Self::TransformTimeline => u32::from_le_bytes(*b"XFRM"),
            Self::TransformKeyframes => u32::from_le_bytes(*b"XKEY"),
            Self::Geometry => u32::from_le_bytes(*b"GEOM"),
            Self::GeometryVertices => u32::from_le_bytes(*b"GVTX"),
            Self::GeometryIndices => u32::from_le_bytes(*b"GIDX"),
            Self::TextureSlots => u32::from_le_bytes(*b"TEXS"),
            Self::MaterialPass => u32::from_le_bytes(*b"MATP"),
            Self::EffectPass => u32::from_le_bytes(*b"EFTP"),
            Self::EffectUvTransform => u32::from_le_bytes(*b"EUVT"),
            Self::EffectParameter => u32::from_le_bytes(*b"EPRM"),
            Self::FlutterState => u32::from_le_bytes(*b"FLUT"),
            Self::Puppet => u32::from_le_bytes(*b"PUPT"),
            Self::PuppetSkinBones => u32::from_le_bytes(*b"PSKB"),
            Self::PuppetSkinVertices => u32::from_le_bytes(*b"PSKV"),
            Self::PuppetAttachments => u32::from_le_bytes(*b"PATT"),
            Self::PuppetClips => u32::from_le_bytes(*b"PCLP"),
            Self::PuppetFrames => u32::from_le_bytes(*b"PFRM"),
            Self::PuppetLayers => u32::from_le_bytes(*b"PLYR"),
            Self::RenderState => u32::from_le_bytes(*b"RNDS"),
            Self::RetainedGpuState => u32::from_le_bytes(*b"RGPU"),
            Self::DebugNames => u32::from_le_bytes(*b"NAME"),
        }
    }

    pub fn from_code(code: u32) -> Option<Self> {
        Self::REQUIRED_ORDER
            .iter()
            .copied()
            .find(|kind| kind.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ResourceTable => "resource_table",
            Self::NodeTable => "node_table",
            Self::TransformTimeline => "transform_timeline",
            Self::TransformKeyframes => "transform_keyframes",
            Self::Geometry => "geometry",
            Self::GeometryVertices => "geometry_vertices",
            Self::GeometryIndices => "geometry_indices",
            Self::TextureSlots => "texture_slots",
            Self::MaterialPass => "material_pass",
            Self::EffectPass => "effect_pass",
            Self::EffectUvTransform => "effect_uv_transform",
            Self::EffectParameter => "effect_parameter",
            Self::FlutterState => "flutter_state",
            Self::Puppet => "puppet",
            Self::PuppetSkinBones => "puppet_skin_bones",
            Self::PuppetSkinVertices => "puppet_skin_vertices",
            Self::PuppetAttachments => "puppet_attachments",
            Self::PuppetClips => "puppet_clips",
            Self::PuppetFrames => "puppet_frames",
            Self::PuppetLayers => "puppet_layers",
            Self::RenderState => "render_state",
            Self::RetainedGpuState => "retained_gpu_state",
            Self::DebugNames => "debug_names",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneBinaryChunkDescriptor {
    pub kind: SceneBinaryChunkKind,
    pub record_count: u32,
    pub offset: u64,
    pub length: u64,
}

impl SceneBinaryChunkDescriptor {
    pub fn payload<'a>(&self, container: &'a [u8]) -> Result<&'a [u8], SceneBinaryError> {
        let start =
            usize::try_from(self.offset).map_err(|_| SceneBinaryError::ChunkOutOfBounds {
                kind: self.kind,
                offset: self.offset,
                length: self.length,
                container_len: container.len(),
            })?;
        let length =
            usize::try_from(self.length).map_err(|_| SceneBinaryError::ChunkOutOfBounds {
                kind: self.kind,
                offset: self.offset,
                length: self.length,
                container_len: container.len(),
            })?;
        let end = start
            .checked_add(length)
            .ok_or(SceneBinaryError::ChunkOutOfBounds {
                kind: self.kind,
                offset: self.offset,
                length: self.length,
                container_len: container.len(),
            })?;
        container
            .get(start..end)
            .ok_or(SceneBinaryError::ChunkOutOfBounds {
                kind: self.kind,
                offset: self.offset,
                length: self.length,
                container_len: container.len(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneBinaryChunkPayload<'a> {
    pub kind: SceneBinaryChunkKind,
    pub record_count: u32,
    pub bytes: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneBinaryOwnedChunkPayload {
    pub kind: SceneBinaryChunkKind,
    pub record_count: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneBinaryDocumentPayloads {
    pub shape: SceneBinaryDocumentShape,
    pub chunks: Vec<SceneBinaryOwnedChunkPayload>,
}

impl SceneBinaryDocumentPayloads {
    pub fn chunk(&self, kind: SceneBinaryChunkKind) -> Option<&SceneBinaryOwnedChunkPayload> {
        self.chunks.iter().find(|chunk| chunk.kind == kind)
    }

    pub fn encode_container(&self, feature_flags: u32) -> Result<Vec<u8>, SceneBinaryError> {
        let payloads = self
            .chunks
            .iter()
            .map(|chunk| SceneBinaryChunkPayload {
                kind: chunk.kind,
                record_count: chunk.record_count,
                bytes: &chunk.bytes,
            })
            .collect::<Vec<_>>();
        encode_scene_binary_container(feature_flags, &payloads)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneBinaryLayoutPlan {
    pub feature_flags: u32,
    pub chunks: Vec<SceneBinaryChunkDescriptor>,
}

impl SceneBinaryLayoutPlan {
    pub fn chunk(&self, kind: SceneBinaryChunkKind) -> Option<&SceneBinaryChunkDescriptor> {
        self.chunks.iter().find(|chunk| chunk.kind == kind)
    }

    pub fn resource_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryResourceRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::ResourceTable,
            SCENE_BINARY_RESOURCE_RECORD_SIZE,
            decode_resource_record,
        )
    }

    pub fn node_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryNodeRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::NodeTable,
            SCENE_BINARY_NODE_RECORD_SIZE,
            decode_node_record,
        )
    }

    pub fn node_record_at(
        &self,
        container: &[u8],
        record_index: u32,
    ) -> Result<SceneBinaryNodeRecord, SceneBinaryError> {
        self.record_at(
            container,
            SceneBinaryChunkKind::NodeTable,
            SCENE_BINARY_NODE_RECORD_SIZE,
            record_index,
            decode_node_record,
        )
    }

    pub fn transform_timeline_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTransformTimelineRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::TransformTimeline,
            SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
            decode_transform_timeline_record,
        )
    }

    pub fn node_transform_records<'a>(
        &self,
        container: &'a [u8],
        node: SceneBinaryNodeRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTransformTimelineRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::TransformTimeline,
            SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
            node.first_transform,
            node.transform_count,
            decode_transform_timeline_record,
        )
    }

    pub fn transform_keyframe_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTransformKeyframeRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::TransformKeyframes,
            SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE,
            decode_transform_keyframe_record,
        )
    }

    pub fn transform_keyframe_record_range<'a>(
        &self,
        container: &'a [u8],
        transform: SceneBinaryTransformTimelineRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTransformKeyframeRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            if transform.first_keyframe == SCENE_BINARY_NONE_ID && transform.keyframe_count == 0 {
                (0, 0)
            } else {
                (transform.first_keyframe, transform.keyframe_count)
            };
        self.records_range(
            container,
            SceneBinaryChunkKind::TransformKeyframes,
            SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE,
            first_record,
            record_count,
            decode_transform_keyframe_record,
        )
    }

    pub fn geometry_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryGeometryRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::Geometry,
            SCENE_BINARY_GEOMETRY_RECORD_SIZE,
            decode_geometry_record,
        )
    }

    pub fn geometry_record_at(
        &self,
        container: &[u8],
        record_index: u32,
    ) -> Result<SceneBinaryGeometryRecord, SceneBinaryError> {
        self.record_at(
            container,
            SceneBinaryChunkKind::Geometry,
            SCENE_BINARY_GEOMETRY_RECORD_SIZE,
            record_index,
            decode_geometry_record,
        )
    }

    pub fn geometry_vertex_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryGeometryVertexRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::GeometryVertices,
            SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
            decode_geometry_vertex_record,
        )
    }

    pub fn geometry_index_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryGeometryIndexRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::GeometryIndices,
            SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
            decode_geometry_index_record,
        )
    }

    pub fn geometry_vertex_record_range<'a>(
        &self,
        container: &'a [u8],
        geometry: SceneBinaryGeometryRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryGeometryVertexRecord>, SceneBinaryError> {
        let (first_record, record_count) = if geometry.first_vertex == SCENE_BINARY_NONE_ID {
            (0, 0)
        } else {
            (geometry.first_vertex, geometry.vertex_count)
        };
        self.records_range(
            container,
            SceneBinaryChunkKind::GeometryVertices,
            SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
            first_record,
            record_count,
            decode_geometry_vertex_record,
        )
    }

    pub fn geometry_index_record_range<'a>(
        &self,
        container: &'a [u8],
        geometry: SceneBinaryGeometryRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryGeometryIndexRecord>, SceneBinaryError> {
        let (first_record, record_count) = if geometry.first_index == SCENE_BINARY_NONE_ID {
            (0, 0)
        } else {
            (geometry.first_index, geometry.index_count)
        };
        self.records_range(
            container,
            SceneBinaryChunkKind::GeometryIndices,
            SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
            first_record,
            record_count,
            decode_geometry_index_record,
        )
    }

    pub fn texture_slot_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTextureSlotRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::TextureSlots,
            SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
            decode_texture_slot_record,
        )
    }

    pub fn material_texture_slot_records<'a>(
        &self,
        container: &'a [u8],
        material: SceneBinaryMaterialPassRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTextureSlotRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::TextureSlots,
            SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
            material.first_texture_slot,
            material.texture_slot_count,
            decode_texture_slot_record,
        )
    }

    pub fn material_effect_pass_records<'a>(
        &self,
        container: &'a [u8],
        material: SceneBinaryMaterialPassRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectPassRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::EffectPass,
            SCENE_BINARY_EFFECT_PASS_RECORD_SIZE,
            material.first_effect_pass,
            material.effect_pass_count,
            decode_effect_pass_record,
        )
    }

    pub fn material_pass_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryMaterialPassRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::MaterialPass,
            SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE,
            decode_material_pass_record,
        )
    }

    pub fn material_pass_record_at(
        &self,
        container: &[u8],
        record_index: u32,
    ) -> Result<SceneBinaryMaterialPassRecord, SceneBinaryError> {
        self.record_at(
            container,
            SceneBinaryChunkKind::MaterialPass,
            SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE,
            record_index,
            decode_material_pass_record,
        )
    }

    pub fn effect_pass_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectPassRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::EffectPass,
            SCENE_BINARY_EFFECT_PASS_RECORD_SIZE,
            decode_effect_pass_record,
        )
    }

    pub fn effect_uv_transform_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectUvTransformRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::EffectUvTransform,
            SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE,
            decode_effect_uv_transform_record,
        )
    }

    pub fn effect_parameter_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectParameterRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::EffectParameter,
            SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
            decode_effect_parameter_record,
        )
    }

    pub fn effect_texture_slot_records<'a>(
        &self,
        container: &'a [u8],
        effect_pass: SceneBinaryEffectPassRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTextureSlotRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::TextureSlots,
            SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
            effect_pass.first_texture_slot,
            effect_pass.texture_slot_count,
            decode_texture_slot_record,
        )
    }

    pub fn effect_parameter_record_range<'a>(
        &self,
        container: &'a [u8],
        effect_pass: SceneBinaryEffectPassRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectParameterRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::EffectParameter,
            SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
            effect_pass.first_parameter,
            effect_pass.parameter_count,
            decode_effect_parameter_record,
        )
    }

    pub fn effect_uv_transform_record_range<'a>(
        &self,
        container: &'a [u8],
        effect_pass: SceneBinaryEffectPassRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectUvTransformRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::EffectUvTransform,
            SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE,
            effect_pass.first_effect_uv_transform,
            effect_pass.effect_uv_transform_count,
            decode_effect_uv_transform_record,
        )
    }

    pub fn flutter_parameter_records<'a>(
        &self,
        container: &'a [u8],
        flutter: SceneBinaryFlutterStateRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectParameterRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::EffectParameter,
            SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
            flutter.first_parameter,
            flutter.parameter_count,
            decode_effect_parameter_record,
        )
    }

    pub fn flutter_state_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryFlutterStateRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::FlutterState,
            SCENE_BINARY_FLUTTER_STATE_RECORD_SIZE,
            decode_flutter_state_record,
        )
    }

    pub fn puppet_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::Puppet,
            SCENE_BINARY_PUPPET_RECORD_SIZE,
            decode_puppet_record,
        )
    }

    pub fn puppet_record_at(
        &self,
        container: &[u8],
        record_index: u32,
    ) -> Result<SceneBinaryPuppetRecord, SceneBinaryError> {
        self.record_at(
            container,
            SceneBinaryChunkKind::Puppet,
            SCENE_BINARY_PUPPET_RECORD_SIZE,
            record_index,
            decode_puppet_record,
        )
    }

    pub fn puppet_skin_bone_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetSkinBoneRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::PuppetSkinBones,
            SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE,
            decode_puppet_skin_bone_record,
        )
    }

    pub fn puppet_skin_bone_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetSkinBoneRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_bone, puppet.bone_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetSkinBones,
            SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_skin_bone_record,
        )
    }

    pub fn puppet_skin_vertex_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetSkinVertexRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_skin_vertex, puppet.skin_vertex_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetSkinVertices,
            SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_skin_vertex_record,
        )
    }

    pub fn puppet_attachment_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetAttachmentRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_attachment, puppet.attachment_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetAttachments,
            SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_attachment_record,
        )
    }

    pub fn puppet_clip_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetClipRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_clip, puppet.clip_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetClips,
            SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_clip_record,
        )
    }

    pub fn puppet_frame_record_range<'a>(
        &self,
        container: &'a [u8],
        clip: SceneBinaryPuppetClipRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetFrameRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(clip.first_frame, clip.frame_record_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetFrames,
            SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_frame_record,
        )
    }

    pub fn puppet_layer_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetLayerRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_layer, puppet.animation_layer_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetLayers,
            SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_layer_record,
        )
    }

    pub fn render_state_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryRenderStateRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::RenderState,
            SCENE_BINARY_RENDER_STATE_RECORD_SIZE,
            decode_render_state_record,
        )
    }

    pub fn retained_gpu_state_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryRetainedGpuStateRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::RetainedGpuState,
            SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE,
            decode_retained_gpu_state_record,
        )
    }

    pub fn debug_names<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryDebugNames<'a>, SceneBinaryError> {
        let descriptor =
            self.chunk(SceneBinaryChunkKind::DebugNames)
                .ok_or(SceneBinaryError::MissingChunk {
                    kind: SceneBinaryChunkKind::DebugNames,
                })?;
        let payload = descriptor.payload(container)?;
        SceneBinaryDebugNames::new(descriptor.record_count, payload)
    }

    fn records<'a, T>(
        &self,
        container: &'a [u8],
        kind: SceneBinaryChunkKind,
        record_size: usize,
        decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
    ) -> Result<SceneBinaryRecords<'a, T>, SceneBinaryError> {
        let descriptor = self
            .chunk(kind)
            .ok_or(SceneBinaryError::MissingChunk { kind })?;
        let payload = descriptor.payload(container)?;
        let expected = usize::try_from(descriptor.record_count)
            .ok()
            .and_then(|count| count.checked_mul(record_size))
            .ok_or(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            })?;
        if payload.len() != expected {
            return Err(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            });
        }
        Ok(SceneBinaryRecords {
            bytes: payload,
            record_size,
            index: 0,
            record_count: descriptor.record_count as usize,
            decode,
        })
    }

    fn record_at<T>(
        &self,
        container: &[u8],
        kind: SceneBinaryChunkKind,
        record_size: usize,
        record_index: u32,
        decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
    ) -> Result<T, SceneBinaryError> {
        let descriptor = self
            .chunk(kind)
            .ok_or(SceneBinaryError::MissingChunk { kind })?;
        let payload = descriptor.payload(container)?;
        let expected = usize::try_from(descriptor.record_count)
            .ok()
            .and_then(|count| count.checked_mul(record_size))
            .ok_or(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            })?;
        if payload.len() != expected {
            return Err(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            });
        }
        if record_index >= descriptor.record_count {
            return Err(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record: record_index,
                record_count: 1,
                chunk_record_count: descriptor.record_count,
            });
        }
        let start = usize::try_from(record_index)
            .ok()
            .and_then(|index| index.checked_mul(record_size))
            .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record: record_index,
                record_count: 1,
                chunk_record_count: descriptor.record_count,
            })?;
        let end =
            start
                .checked_add(record_size)
                .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                    kind,
                    first_record: record_index,
                    record_count: 1,
                    chunk_record_count: descriptor.record_count,
                })?;
        decode(&payload[start..end])
    }

    fn records_range<'a, T>(
        &self,
        container: &'a [u8],
        kind: SceneBinaryChunkKind,
        record_size: usize,
        first_record: u32,
        record_count: u32,
        decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
    ) -> Result<SceneBinaryRecords<'a, T>, SceneBinaryError> {
        let descriptor = self
            .chunk(kind)
            .ok_or(SceneBinaryError::MissingChunk { kind })?;
        let payload = descriptor.payload(container)?;
        let expected = usize::try_from(descriptor.record_count)
            .ok()
            .and_then(|count| count.checked_mul(record_size))
            .ok_or(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            })?;
        if payload.len() != expected {
            return Err(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            });
        }
        if record_count == 0 {
            return Ok(SceneBinaryRecords {
                bytes: &payload[0..0],
                record_size,
                index: 0,
                record_count: 0,
                decode,
            });
        }
        let first = usize::try_from(first_record).map_err(|_| {
            SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            }
        })?;
        let count = usize::try_from(record_count).map_err(|_| {
            SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            }
        })?;
        let end_record =
            first
                .checked_add(count)
                .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                    kind,
                    first_record,
                    record_count,
                    chunk_record_count: descriptor.record_count,
                })?;
        if end_record > descriptor.record_count as usize {
            return Err(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            });
        }
        let start =
            first
                .checked_mul(record_size)
                .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                    kind,
                    first_record,
                    record_count,
                    chunk_record_count: descriptor.record_count,
                })?;
        let byte_len =
            count
                .checked_mul(record_size)
                .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                    kind,
                    first_record,
                    record_count,
                    chunk_record_count: descriptor.record_count,
                })?;
        let end = start
            .checked_add(byte_len)
            .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            })?;
        Ok(SceneBinaryRecords {
            bytes: &payload[start..end],
            record_size,
            index: 0,
            record_count: count,
            decode,
        })
    }
}

pub struct SceneBinaryRecords<'a, T> {
    bytes: &'a [u8],
    record_size: usize,
    index: usize,
    record_count: usize,
    decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
}

impl<'a, T> SceneBinaryRecords<'a, T> {
    pub fn len(&self) -> usize {
        self.record_count.saturating_sub(self.index)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Iterator for SceneBinaryRecords<'_, T> {
    type Item = Result<T, SceneBinaryError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.record_count {
            return None;
        }
        let start = self.index.checked_mul(self.record_size)?;
        let end = start.checked_add(self.record_size)?;
        self.index += 1;
        Some((self.decode)(&self.bytes[start..end]))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<T> ExactSizeIterator for SceneBinaryRecords<'_, T> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryResourceRecord {
    pub id_name: u32,
    pub source_name: u32,
    pub original_source_name: u32,
    pub role_name: u32,
    pub kind: u16,
    pub flags: u16,
    pub width: u32,
    pub height: u32,
    pub upload_hints: u32,
}

impl SceneBinaryResourceRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.id_name);
        write_u32(out, self.source_name);
        write_u32(out, self.original_source_name);
        write_u32(out, self.role_name);
        write_u16(out, self.kind);
        write_u16(out, self.flags);
        write_u32(out, self.width);
        write_u32(out, self.height);
        write_u32(out, self.upload_hints);
        debug_assert_eq!(SCENE_BINARY_RESOURCE_RECORD_SIZE, 32);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryNodeRecord {
    pub id_name: u32,
    pub display_name: u32,
    pub parent_index: u32,
    pub resource_name: u32,
    pub kind: u16,
    pub flags: u16,
    pub draw_order: u32,
    pub child_count: u32,
    pub first_child_index: u32,
    pub subtree_node_count: u32,
    pub effect_count: u32,
    pub audio_count: u32,
    pub property_count: u32,
    pub material_index: u32,
    pub geometry_index: u32,
    pub first_transform: u32,
    pub transform_count: u32,
    pub puppet_index: u32,
    pub opacity: f32,
    pub color_rgba: u32,
    pub stroke_color_rgba: u32,
    pub stroke_width: f32,
    pub corner_radius: f32,
    pub fit: u16,
}

impl SceneBinaryNodeRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.id_name);
        write_u32(out, self.display_name);
        write_u32(out, self.parent_index);
        write_u32(out, self.resource_name);
        write_u16(out, self.kind);
        write_u16(out, self.flags);
        write_u32(out, self.draw_order);
        write_u32(out, self.child_count);
        write_u32(out, self.first_child_index);
        write_u32(out, self.subtree_node_count);
        write_u32(out, self.effect_count);
        write_u32(out, self.audio_count);
        write_u32(out, self.property_count);
        write_u32(out, self.material_index);
        write_u32(out, self.geometry_index);
        write_u32(out, self.first_transform);
        write_u32(out, self.transform_count);
        write_u32(out, self.puppet_index);
        write_f32(out, self.opacity);
        write_u32(out, self.color_rgba);
        write_u32(out, self.stroke_color_rgba);
        write_f32(out, self.stroke_width);
        write_f32(out, self.corner_radius);
        write_u16(out, self.fit);
        write_u16(out, 0);
        write_u32(out, 0);
        debug_assert_eq!(SCENE_BINARY_NODE_RECORD_SIZE, 96);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryTransformTimelineRecord {
    pub owner_name: u32,
    pub timeline_name: u32,
    pub property: u16,
    pub flags: u16,
    pub keyframe_count: u32,
    pub first_keyframe: u32,
    pub time_offset_ms: u64,
    pub first_time_ms: u64,
    pub last_time_ms: u64,
    pub value0: f32,
    pub value1: f32,
    pub value2: f32,
    pub value3: f32,
    pub value4: f32,
    pub value5: f32,
    pub value6: f32,
}

impl SceneBinaryTransformTimelineRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.timeline_name);
        write_u16(out, self.property);
        write_u16(out, self.flags);
        write_u32(out, self.keyframe_count);
        write_u32(out, self.first_keyframe);
        write_u32(out, 0);
        write_u64(out, self.time_offset_ms);
        write_u64(out, self.first_time_ms);
        write_u64(out, self.last_time_ms);
        write_f32(out, self.value0);
        write_f32(out, self.value1);
        write_f32(out, self.value2);
        write_f32(out, self.value3);
        write_f32(out, self.value4);
        write_f32(out, self.value5);
        write_f32(out, self.value6);
        write_u32(out, 0);
        debug_assert_eq!(SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE, 80);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryTransformKeyframeRecord {
    pub time_ms: u64,
    pub value: f32,
    pub curve: u16,
    pub flags: u16,
}

impl SceneBinaryTransformKeyframeRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u64(out, self.time_ms);
        write_f32(out, self.value);
        write_u16(out, self.curve);
        write_u16(out, self.flags);
        debug_assert_eq!(SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE, 16);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryTextureSlotRecord {
    pub owner_name: u32,
    pub pass_name: u32,
    pub texture_name: u32,
    pub resource_index: u32,
    pub slot: u32,
    pub width: u32,
    pub height: u32,
    pub role_flags: u16,
    pub sampler_flags: u16,
}

impl SceneBinaryTextureSlotRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.pass_name);
        write_u32(out, self.texture_name);
        write_u32(out, self.resource_index);
        write_u32(out, self.slot);
        write_u32(out, self.width);
        write_u32(out, self.height);
        write_u16(out, self.role_flags);
        write_u16(out, self.sampler_flags);
        debug_assert_eq!(SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE, 32);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryMaterialPassRecord {
    pub owner_name: u32,
    pub shader_name: u32,
    pub blending_name: u32,
    pub first_texture_slot: u32,
    pub alpha_texture_slot: u32,
    pub first_effect_pass: u32,
    pub pipeline_key: u32,
    pub texture_slot_count: u32,
    pub effect_pass_count: u32,
    pub effect_kind_flags: u32,
    pub material_kind: u16,
    pub descriptor_layout: u16,
    pub blend_mode: u16,
    pub alpha_texture_mode: u16,
    pub depth_test: u16,
    pub depth_write: u16,
    pub cull_mode: u16,
    pub flags: u16,
}

impl SceneBinaryMaterialPassRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.shader_name);
        write_u32(out, self.blending_name);
        write_u32(out, self.first_texture_slot);
        write_u32(out, self.alpha_texture_slot);
        write_u32(out, self.first_effect_pass);
        write_u32(out, self.pipeline_key);
        write_u32(out, self.texture_slot_count);
        write_u32(out, self.effect_pass_count);
        write_u32(out, self.effect_kind_flags);
        write_u16(out, self.material_kind);
        write_u16(out, self.descriptor_layout);
        write_u16(out, self.blend_mode);
        write_u16(out, self.alpha_texture_mode);
        write_u16(out, self.depth_test);
        write_u16(out, self.depth_write);
        write_u16(out, self.cull_mode);
        write_u16(out, self.flags);
        debug_assert_eq!(SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE, 56);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryEffectPassRecord {
    pub owner_name: u32,
    pub effect_name: u32,
    pub shader_name: u32,
    pub blending_name: u32,
    pub pass_index: u32,
    pub first_texture_slot: u32,
    pub texture_slot_count: u32,
    pub first_effect_uv_transform: u32,
    pub effect_uv_transform_count: u32,
    pub first_parameter: u32,
    pub parameter_count: u32,
    pub kind: u16,
    pub evaluation_boundary: u16,
    pub depth_test: u16,
    pub depth_write: u16,
    pub cull_mode: u16,
    pub flags: u16,
}

impl SceneBinaryEffectPassRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.effect_name);
        write_u32(out, self.shader_name);
        write_u32(out, self.blending_name);
        write_u32(out, self.pass_index);
        write_u32(out, self.first_texture_slot);
        write_u32(out, self.texture_slot_count);
        write_u32(out, self.first_effect_uv_transform);
        write_u32(out, self.effect_uv_transform_count);
        write_u32(out, self.first_parameter);
        write_u32(out, self.parameter_count);
        write_u16(out, self.kind);
        write_u16(out, self.evaluation_boundary);
        write_u16(out, self.depth_test);
        write_u16(out, self.depth_write);
        write_u16(out, self.cull_mode);
        write_u16(out, self.flags);
        debug_assert_eq!(SCENE_BINARY_EFFECT_PASS_RECORD_SIZE, 56);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryEffectParameterRecord {
    pub owner_name: u32,
    pub effect_name: u32,
    pub parameter_name: u32,
    pub value_name: u32,
    pub pass_index: u32,
    pub value_kind: u16,
    pub role_flags: u16,
    pub integer_value: i64,
    pub value0: f32,
    pub value1: f32,
    pub value2: f32,
    pub value3: f32,
}

impl SceneBinaryEffectParameterRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.effect_name);
        write_u32(out, self.parameter_name);
        write_u32(out, self.value_name);
        write_u32(out, self.pass_index);
        write_u16(out, self.value_kind);
        write_u16(out, self.role_flags);
        write_i64(out, self.integer_value);
        write_f32(out, self.value0);
        write_f32(out, self.value1);
        write_f32(out, self.value2);
        write_f32(out, self.value3);
        debug_assert_eq!(SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE, 48);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryRenderStateRecord {
    pub width: u32,
    pub height: u32,
    pub resource_count: u32,
    pub node_count: u32,
    pub material_count: u32,
    pub effect_count: u32,
    pub flags: u32,
    pub texture_slot_count: u32,
}

impl SceneBinaryRenderStateRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.width);
        write_u32(out, self.height);
        write_u32(out, self.resource_count);
        write_u32(out, self.node_count);
        write_u32(out, self.material_count);
        write_u32(out, self.effect_count);
        write_u32(out, self.flags);
        write_u32(out, self.texture_slot_count);
        debug_assert_eq!(SCENE_BINARY_RENDER_STATE_RECORD_SIZE, 32);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryRetainedGpuStateRecord {
    pub owner_kind: u16,
    pub flags: u16,
    pub owner_name: u32,
    pub stable_id: u64,
    pub record_index: u32,
    pub dirty_range_count: u32,
}

impl SceneBinaryRetainedGpuStateRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u16(out, self.owner_kind);
        write_u16(out, self.flags);
        write_u32(out, self.owner_name);
        write_u64(out, self.stable_id);
        write_u32(out, self.record_index);
        write_u32(out, self.dirty_range_count);
        debug_assert_eq!(SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE, 24);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryDebugNameRecord {
    pub id: u32,
    pub kind: u32,
    pub offset: u32,
    pub length: u32,
}

pub struct SceneBinaryDebugNames<'a> {
    records: &'a [u8],
    strings: &'a [u8],
    record_count: usize,
}

impl<'a> SceneBinaryDebugNames<'a> {
    fn new(record_count: u32, payload: &'a [u8]) -> Result<Self, SceneBinaryError> {
        let record_bytes = usize::try_from(record_count)
            .ok()
            .and_then(|count| count.checked_mul(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE))
            .ok_or(SceneBinaryError::InvalidRecordPayload {
                kind: SceneBinaryChunkKind::DebugNames,
                record_size: SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
                record_count,
                length: payload.len(),
            })?;
        if payload.len() < record_bytes {
            return Err(SceneBinaryError::InvalidRecordPayload {
                kind: SceneBinaryChunkKind::DebugNames,
                record_size: SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
                record_count,
                length: payload.len(),
            });
        }
        let (records, strings) = payload.split_at(record_bytes);
        Ok(Self {
            records,
            strings,
            record_count: record_count as usize,
        })
    }

    pub fn len(&self) -> usize {
        self.record_count
    }

    pub fn is_empty(&self) -> bool {
        self.record_count == 0
    }

    pub fn record(&self, id: u32) -> Result<Option<SceneBinaryDebugNameRecord>, SceneBinaryError> {
        let Some(start) = usize::try_from(id)
            .ok()
            .and_then(|index| index.checked_mul(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE))
        else {
            return Ok(None);
        };
        let Some(end) = start.checked_add(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE) else {
            return Ok(None);
        };
        let Some(bytes) = self.records.get(start..end) else {
            return Ok(None);
        };
        let record = decode_debug_name_record(bytes)?;
        Ok(Some(record))
    }

    pub fn name(&self, id: u32) -> Result<Option<&'a str>, SceneBinaryError> {
        let Some(record) = self.record(id)? else {
            return Ok(None);
        };
        let start =
            usize::try_from(record.offset).map_err(|_| SceneBinaryError::NameOutOfBounds {
                id,
                offset: record.offset,
                length: record.length,
                string_table_len: self.strings.len(),
            })?;
        let length =
            usize::try_from(record.length).map_err(|_| SceneBinaryError::NameOutOfBounds {
                id,
                offset: record.offset,
                length: record.length,
                string_table_len: self.strings.len(),
            })?;
        let end = start
            .checked_add(length)
            .ok_or(SceneBinaryError::NameOutOfBounds {
                id,
                offset: record.offset,
                length: record.length,
                string_table_len: self.strings.len(),
            })?;
        let Some(bytes) = self.strings.get(start..end) else {
            return Err(SceneBinaryError::NameOutOfBounds {
                id,
                offset: record.offset,
                length: record.length,
                string_table_len: self.strings.len(),
            });
        };
        std::str::from_utf8(bytes)
            .map(Some)
            .map_err(|_| SceneBinaryError::InvalidNameUtf8 { id })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneBinaryDocumentShape {
    pub resource_table_records: u32,
    pub node_table_records: u32,
    pub transform_timeline_records: u32,
    pub transform_keyframe_records: u32,
    pub geometry_records: u32,
    pub geometry_vertex_records: u32,
    pub geometry_index_records: u32,
    pub texture_slot_records: u32,
    pub material_pass_records: u32,
    pub effect_pass_records: u32,
    pub effect_uv_transform_records: u32,
    pub effect_parameter_records: u32,
    pub flutter_state_records: u32,
    pub puppet_records: u32,
    pub puppet_skin_bone_records: u32,
    pub puppet_skin_vertex_records: u32,
    pub puppet_attachment_records: u32,
    pub puppet_clip_records: u32,
    pub puppet_frame_records: u32,
    pub puppet_layer_records: u32,
    pub render_state_records: u32,
    pub retained_gpu_state_records: u32,
    pub debug_name_records: u32,
}

impl SceneBinaryDocumentShape {
    pub fn from_document(document: &SceneDocument) -> Self {
        let mut shape = Self {
            resource_table_records: saturating_u32(document.resources.len()),
            transform_timeline_records: saturating_u32(
                document
                    .timelines
                    .iter()
                    .map(|timeline| timeline.channels.len())
                    .sum::<usize>(),
            ),
            transform_keyframe_records: saturating_u32(
                document
                    .timelines
                    .iter()
                    .flat_map(|timeline| timeline.channels.iter())
                    .map(|channel| channel.keyframes.len())
                    .sum::<usize>(),
            ),
            render_state_records: 1,
            debug_name_records: saturating_u32(document.resources.len()),
            ..Default::default()
        };
        for node in &document.nodes {
            shape.include_node(node);
        }
        shape.retained_gpu_state_records = shape
            .resource_table_records
            .saturating_add(shape.geometry_records)
            .saturating_add(shape.texture_slot_records)
            .saturating_add(shape.material_pass_records)
            .saturating_add(shape.effect_pass_records)
            .saturating_add(shape.effect_uv_transform_records)
            .saturating_add(shape.effect_parameter_records)
            .saturating_add(shape.puppet_records);
        shape
    }

    pub fn record_count(self, kind: SceneBinaryChunkKind) -> u32 {
        match kind {
            SceneBinaryChunkKind::ResourceTable => self.resource_table_records,
            SceneBinaryChunkKind::NodeTable => self.node_table_records,
            SceneBinaryChunkKind::TransformTimeline => self.transform_timeline_records,
            SceneBinaryChunkKind::TransformKeyframes => self.transform_keyframe_records,
            SceneBinaryChunkKind::Geometry => self.geometry_records,
            SceneBinaryChunkKind::GeometryVertices => self.geometry_vertex_records,
            SceneBinaryChunkKind::GeometryIndices => self.geometry_index_records,
            SceneBinaryChunkKind::TextureSlots => self.texture_slot_records,
            SceneBinaryChunkKind::MaterialPass => self.material_pass_records,
            SceneBinaryChunkKind::EffectPass => self.effect_pass_records,
            SceneBinaryChunkKind::EffectUvTransform => self.effect_uv_transform_records,
            SceneBinaryChunkKind::EffectParameter => self.effect_parameter_records,
            SceneBinaryChunkKind::FlutterState => self.flutter_state_records,
            SceneBinaryChunkKind::Puppet => self.puppet_records,
            SceneBinaryChunkKind::PuppetSkinBones => self.puppet_skin_bone_records,
            SceneBinaryChunkKind::PuppetSkinVertices => self.puppet_skin_vertex_records,
            SceneBinaryChunkKind::PuppetAttachments => self.puppet_attachment_records,
            SceneBinaryChunkKind::PuppetClips => self.puppet_clip_records,
            SceneBinaryChunkKind::PuppetFrames => self.puppet_frame_records,
            SceneBinaryChunkKind::PuppetLayers => self.puppet_layer_records,
            SceneBinaryChunkKind::RenderState => self.render_state_records,
            SceneBinaryChunkKind::RetainedGpuState => self.retained_gpu_state_records,
            SceneBinaryChunkKind::DebugNames => self.debug_name_records,
        }
    }

    fn include_node(&mut self, node: &SceneNode) {
        self.node_table_records = self.node_table_records.saturating_add(1);
        self.transform_timeline_records = self.transform_timeline_records.saturating_add(1);
        self.debug_name_records = self
            .debug_name_records
            .saturating_add(1 + u32::from(node.name.is_some()));
        if node.resource.is_some() {
            self.texture_slot_records = self.texture_slot_records.saturating_add(1);
        }
        if node_has_geometry(node) {
            self.geometry_records = self.geometry_records.saturating_add(1);
            if let Some(mesh) = node.mesh.as_ref() {
                self.geometry_vertex_records = self
                    .geometry_vertex_records
                    .saturating_add(saturating_u32(mesh.vertices.len()));
                self.geometry_index_records = self
                    .geometry_index_records
                    .saturating_add(saturating_u32(mesh.indices.len()));
            }
        }
        if node_has_material(node) {
            self.material_pass_records = self.material_pass_records.saturating_add(1);
        }
        if node.mesh.is_some() || !node.puppet_animation_layers.is_empty() {
            self.puppet_records = self.puppet_records.saturating_add(1);
            self.include_puppet_payload(node);
        }
        for effect in &node.effects {
            self.include_effect(effect);
        }
        if node_first_effect_pass_reuses_base_resource(node) {
            self.texture_slot_records = self.texture_slot_records.saturating_sub(1);
        }
        for child in &node.children {
            self.include_node(child);
        }
    }

    fn include_effect(&mut self, effect: &SceneEffect) {
        self.debug_name_records = self.debug_name_records.saturating_add(
            1 + u32::from(effect.name.is_some()) + u32::from(effect.resource.is_some()),
        );
        self.effect_pass_records = self
            .effect_pass_records
            .saturating_add(saturating_u32(effect.passes.len().max(1)));
        self.effect_parameter_records = self
            .effect_parameter_records
            .saturating_add(effect_parameter_record_count(effect));
        self.effect_uv_transform_records = self
            .effect_uv_transform_records
            .saturating_add(effect_uv_transform_record_count(effect));
        if effect_is_motion_family(effect) {
            self.flutter_state_records = self.flutter_state_records.saturating_add(1);
        }
        for pass in &effect.passes {
            self.texture_slot_records = self
                .texture_slot_records
                .saturating_add(effect_pass_texture_slot_count(pass));
        }
    }

    fn include_puppet_payload(&mut self, node: &SceneNode) {
        self.puppet_layer_records = self
            .puppet_layer_records
            .saturating_add(saturating_u32(node.puppet_animation_layers.len()));
        self.debug_name_records = self.debug_name_records.saturating_add(
            node.puppet_animation_layers
                .iter()
                .filter(|layer| layer.name.is_some())
                .count()
                .min(u32::MAX as usize) as u32,
        );
        let Some(mesh) = node.mesh.as_ref() else {
            return;
        };
        if let Some(skin) = mesh.skin.as_ref() {
            self.puppet_skin_bone_records = self
                .puppet_skin_bone_records
                .saturating_add(saturating_u32(skin.bones.len()));
            self.puppet_skin_vertex_records = self
                .puppet_skin_vertex_records
                .saturating_add(saturating_u32(skin.vertices.len()));
            self.puppet_attachment_records = self
                .puppet_attachment_records
                .saturating_add(saturating_u32(skin.attachments.len()));
            self.debug_name_records = self.debug_name_records.saturating_add(
                skin.attachments
                    .iter()
                    .filter(|attachment| !attachment.name.is_empty())
                    .count()
                    .min(u32::MAX as usize) as u32,
            );
        }
        self.puppet_clip_records = self
            .puppet_clip_records
            .saturating_add(saturating_u32(mesh.puppet_clips.len()));
        self.debug_name_records = self.debug_name_records.saturating_add(
            mesh.puppet_clips
                .iter()
                .filter(|clip| clip.name.is_some())
                .count()
                .min(u32::MAX as usize) as u32,
        );
        self.puppet_frame_records = self.puppet_frame_records.saturating_add(
            mesh.puppet_clips
                .iter()
                .flat_map(|clip| clip.bones.iter())
                .map(|bone| saturating_u32(bone.frames.len()))
                .fold(0u32, u32::saturating_add),
        );
    }
}

pub fn scene_binary_payloads_from_document(
    document: &SceneDocument,
) -> SceneBinaryDocumentPayloads {
    let mut builder = SceneBinaryPayloadBuilder::new();
    builder.include_document(document);
    builder.finish()
}

pub fn encode_scene_binary_document(
    feature_flags: u32,
    document: &SceneDocument,
) -> Result<Vec<u8>, SceneBinaryError> {
    scene_binary_payloads_from_document(document).encode_container(feature_flags)
}

#[derive(Debug, Default)]
struct SceneBinaryChunkWriter {
    bytes: Vec<u8>,
    record_count: u32,
}

impl SceneBinaryChunkWriter {
    fn push_record<F>(&mut self, write: F) -> u32
    where
        F: FnOnce(&mut Vec<u8>),
    {
        let index = self.record_count;
        write(&mut self.bytes);
        self.record_count = self.record_count.saturating_add(1);
        index
    }

    fn into_payload(self, kind: SceneBinaryChunkKind) -> SceneBinaryOwnedChunkPayload {
        SceneBinaryOwnedChunkPayload {
            kind,
            record_count: self.record_count,
            bytes: self.bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneBinaryNameKind {
    ResourceId,
    ResourcePath,
    NodeId,
    DisplayName,
    EffectFile,
    Shader,
    Material,
    Timeline,
    Property,
    EffectParameter,
    ParameterValue,
    PuppetClip,
    PuppetLayer,
    PuppetAttachment,
}

impl SceneBinaryNameKind {
    fn code(self) -> u32 {
        match self {
            Self::ResourceId => 1,
            Self::ResourcePath => 2,
            Self::NodeId => 3,
            Self::DisplayName => 4,
            Self::EffectFile => 5,
            Self::Shader => 6,
            Self::Material => 7,
            Self::Timeline => 8,
            Self::Property => 9,
            Self::EffectParameter => 10,
            Self::ParameterValue => 11,
            Self::PuppetClip => 12,
            Self::PuppetLayer => 13,
            Self::PuppetAttachment => 14,
        }
    }
}

#[derive(Debug, Default)]
struct SceneBinaryNameTable {
    ids: BTreeMap<String, u32>,
    records: Vec<(u32, SceneBinaryNameKind, u32, u32)>,
    bytes: Vec<u8>,
}

impl SceneBinaryNameTable {
    fn intern(&mut self, kind: SceneBinaryNameKind, value: &str) -> u32 {
        if value.is_empty() {
            return SCENE_BINARY_NONE_ID;
        }
        if let Some(id) = self.ids.get(value) {
            return *id;
        }
        let id = self.records.len().min(u32::MAX as usize) as u32;
        let offset = self.bytes.len().min(u32::MAX as usize) as u32;
        let bytes = value.as_bytes();
        let length = bytes.len().min(u32::MAX as usize) as u32;
        self.bytes.extend_from_slice(bytes);
        self.records.push((id, kind, offset, length));
        self.ids.insert(value.to_owned(), id);
        id
    }

    fn intern_optional(&mut self, kind: SceneBinaryNameKind, value: Option<&str>) -> u32 {
        value.map_or(SCENE_BINARY_NONE_ID, |value| self.intern(kind, value))
    }

    fn record_count(&self) -> u32 {
        self.records.len().min(u32::MAX as usize) as u32
    }

    fn encode(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            self.records.len() * SCENE_BINARY_DEBUG_NAME_RECORD_SIZE + self.bytes.len(),
        );
        for (id, kind, offset, length) in self.records {
            write_u32(&mut out, id);
            write_u32(&mut out, kind.code());
            write_u32(&mut out, offset);
            write_u32(&mut out, length);
        }
        out.extend_from_slice(&self.bytes);
        out
    }
}

#[derive(Debug, Default)]
struct SceneBinaryPayloadBuilder {
    names: SceneBinaryNameTable,
    resource_table: SceneBinaryChunkWriter,
    node_table: SceneBinaryChunkWriter,
    transform_timeline: SceneBinaryChunkWriter,
    transform_keyframes: SceneBinaryChunkWriter,
    geometry: SceneBinaryChunkWriter,
    geometry_vertices: SceneBinaryChunkWriter,
    geometry_indices: SceneBinaryChunkWriter,
    texture_slots: SceneBinaryChunkWriter,
    material_pass: SceneBinaryChunkWriter,
    effect_pass: SceneBinaryChunkWriter,
    effect_uv_transform: SceneBinaryChunkWriter,
    effect_parameter: SceneBinaryChunkWriter,
    flutter_state: SceneBinaryChunkWriter,
    puppet: SceneBinaryChunkWriter,
    puppet_skin_bones: SceneBinaryChunkWriter,
    puppet_skin_vertices: SceneBinaryChunkWriter,
    puppet_attachments: SceneBinaryChunkWriter,
    puppet_clips: SceneBinaryChunkWriter,
    puppet_frames: SceneBinaryChunkWriter,
    puppet_layers: SceneBinaryChunkWriter,
    render_state: SceneBinaryChunkWriter,
    retained_gpu_state: SceneBinaryChunkWriter,
}

#[derive(Debug, Clone, Copy)]
struct SceneBinaryResourceBinding<'a> {
    index: u32,
    resource: &'a SceneResource,
}

#[derive(Debug, Clone, Copy)]
struct SceneBinaryBaseTextureSlot {
    record_index: u32,
    resource_index: u32,
}

#[derive(Debug, Clone, Copy)]
struct SceneBinaryTextureSlotRange {
    first_record: u32,
    record_count: u32,
}

#[derive(Debug)]
struct SceneBinaryResourceIndex<'a> {
    bindings: BTreeMap<&'a str, SceneBinaryResourceBinding<'a>>,
}

impl<'a> SceneBinaryResourceIndex<'a> {
    fn from_document(document: &'a SceneDocument) -> Self {
        let bindings = document
            .resources
            .iter()
            .enumerate()
            .map(|(index, resource)| {
                (
                    resource.id.as_str(),
                    SceneBinaryResourceBinding {
                        index: index.min(u32::MAX as usize) as u32,
                        resource,
                    },
                )
            })
            .collect();
        Self { bindings }
    }

    fn binding(&self, resource_id: &str) -> Option<SceneBinaryResourceBinding<'a>> {
        self.bindings.get(resource_id).copied()
    }
}

#[derive(Debug, Clone, Copy)]
struct SceneBinaryTimelineChannelBinding<'a> {
    timeline_id: &'a str,
    channel: &'a SceneTimelineChannel,
}

#[derive(Debug)]
struct SceneBinaryTimelineIndex<'a> {
    by_target: BTreeMap<&'a str, Vec<SceneBinaryTimelineChannelBinding<'a>>>,
    untargeted: Vec<SceneBinaryTimelineChannelBinding<'a>>,
}

impl<'a> SceneBinaryTimelineIndex<'a> {
    fn from_document(document: &'a SceneDocument) -> Self {
        let mut by_target: BTreeMap<&'a str, Vec<SceneBinaryTimelineChannelBinding<'a>>> =
            BTreeMap::new();
        let mut untargeted = Vec::new();
        for timeline in &document.timelines {
            for channel in &timeline.channels {
                let binding = SceneBinaryTimelineChannelBinding {
                    timeline_id: &timeline.id,
                    channel,
                };
                if let Some(target_node) = timeline.target_node.as_deref() {
                    by_target.entry(target_node).or_default().push(binding);
                } else {
                    untargeted.push(binding);
                }
            }
        }
        Self {
            by_target,
            untargeted,
        }
    }

    fn channels_for_node(&self, node_id: &str) -> &[SceneBinaryTimelineChannelBinding<'a>] {
        self.by_target
            .get(node_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

#[derive(Debug, Clone, Copy)]
struct SceneBinaryMaterialState<'a> {
    shader: Option<&'a str>,
    blending: Option<&'a str>,
    blend_mode: SceneBlendMode,
    alpha_texture_slot: Option<u32>,
    alpha_texture_mode: SceneAlphaTextureMode,
    texture_slot_count: u32,
    effect_pass_count: u32,
    effect_kind_flags: u32,
    material_kind: u16,
    descriptor_layout: u16,
    depth_test: u16,
    depth_write: u16,
    cull_mode: u16,
    flags: u16,
}

impl<'a> SceneBinaryMaterialState<'a> {
    fn from_node(
        node: &'a SceneNode,
        base_resource: Option<SceneBinaryResourceBinding<'_>>,
        resource_index: &SceneBinaryResourceIndex<'_>,
    ) -> Self {
        let first_pass = node
            .effects
            .iter()
            .flat_map(|effect| effect.passes.iter())
            .next();
        let effect_pass_count = node_effect_pass_count(&node.effects);
        let effect_texture_slot_count =
            node_effect_texture_slot_count(&node.effects, base_resource, resource_index);
        let texture_slot_count =
            u32::from(base_resource.is_some()).saturating_add(effect_texture_slot_count);
        let (alpha_texture_slot, alpha_texture_mode) =
            node_alpha_texture_state(&node.effects, resource_index);
        let effect_kind_flags = effect_kind_flags(&node.effects);
        let material_kind = material_kind_code(node, effect_pass_count);
        let descriptor_layout = descriptor_layout_code(
            base_resource.is_some(),
            texture_slot_count,
            alpha_texture_slot.is_some(),
            effect_pass_count,
        );
        let property_blend_mode = super::scene_blend_mode_from_properties(&node.properties);
        let blend_mode = match property_blend_mode {
            SceneBlendMode::Alpha => first_pass
                .and_then(|pass| pass.blending.as_deref())
                .and_then(super::scene_blend_mode_from_material_blending)
                .unwrap_or(property_blend_mode),
            _ => property_blend_mode,
        };
        Self {
            shader: first_pass.and_then(|pass| pass.shader.as_deref()),
            blending: first_pass.and_then(|pass| pass.blending.as_deref()),
            blend_mode,
            alpha_texture_slot,
            alpha_texture_mode,
            texture_slot_count,
            effect_pass_count,
            effect_kind_flags,
            material_kind,
            descriptor_layout,
            depth_test: material_flag_code(first_pass.and_then(|pass| pass.depthtest.as_deref())),
            depth_write: material_flag_code(first_pass.and_then(|pass| pass.depthwrite.as_deref())),
            cull_mode: cull_mode_code(first_pass.and_then(|pass| pass.cullmode.as_deref())),
            flags: material_flags(node, base_resource, alpha_texture_slot, effect_pass_count),
        }
    }

    fn pipeline_key(self) -> u32 {
        u32::from(self.material_kind & 0x0f)
            | (u32::from(self.descriptor_layout & 0x0f) << 4)
            | (u32::from(blend_mode_code(self.blend_mode) & 0x0f) << 8)
            | (u32::from(alpha_texture_mode_code(self.alpha_texture_mode) & 0x0f) << 12)
            | (u32::from(self.depth_test & 0x03) << 16)
            | (u32::from(self.depth_write & 0x03) << 18)
            | (u32::from(self.cull_mode & 0x0f) << 20)
            | ((self.effect_kind_flags & 0xff) << 24)
    }
}

impl SceneBinaryPayloadBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn include_document(&mut self, document: &SceneDocument) {
        let resource_index = SceneBinaryResourceIndex::from_document(document);
        let timeline_index = SceneBinaryTimelineIndex::from_document(document);
        for resource in &document.resources {
            self.include_resource(resource_id_fields(resource));
        }
        let mut draw_order = 0;
        for node in &document.nodes {
            self.include_node(
                node,
                None,
                &mut draw_order,
                &resource_index,
                &timeline_index,
            );
        }
        for channel in &timeline_index.untargeted {
            self.push_timeline_channel(SCENE_BINARY_NONE_ID, *channel);
        }
        let (width, height) = document
            .size
            .map_or((0, 0), |size| (size.width, size.height));
        self.render_state.push_record(|out| {
            SceneBinaryRenderStateRecord {
                width,
                height,
                resource_count: self.resource_table.record_count,
                node_count: self.node_table.record_count,
                material_count: self.material_pass.record_count,
                effect_count: self.effect_pass.record_count,
                flags: render_state_flags(document),
                texture_slot_count: self.texture_slots.record_count,
            }
            .encode(out)
        });
    }

    fn include_resource(&mut self, resource: SceneBinaryResourceFields<'_>) {
        let id_name = self
            .names
            .intern(SceneBinaryNameKind::ResourceId, resource.id);
        let source_name = self
            .names
            .intern(SceneBinaryNameKind::ResourcePath, resource.source);
        let original_source_name = self
            .names
            .intern_optional(SceneBinaryNameKind::ResourcePath, resource.original_source);
        let role_name = self
            .names
            .intern_optional(SceneBinaryNameKind::Material, resource.role);
        let flags = u16::from(resource.width.is_some())
            | (u16::from(resource.height.is_some()) << 1)
            | (u16::from(resource.original_source.is_some()) << 2)
            | (u16::from(resource.role.is_some()) << 3);
        let record_index = self.resource_table.push_record(|out| {
            SceneBinaryResourceRecord {
                id_name,
                source_name,
                original_source_name,
                role_name,
                kind: resource_kind_code(resource.kind),
                flags,
                width: resource.width.unwrap_or(0),
                height: resource.height.unwrap_or(0),
                upload_hints: 0,
            }
            .encode(out)
        });
        self.push_retained(SCENE_BINARY_RETAINED_RESOURCE, id_name, record_index);
    }

    fn include_node(
        &mut self,
        node: &SceneNode,
        parent_index: Option<u32>,
        draw_order: &mut u32,
        resource_index: &SceneBinaryResourceIndex<'_>,
        timeline_index: &SceneBinaryTimelineIndex<'_>,
    ) {
        let node_index = self.node_table.record_count;
        let id_name = self.names.intern(SceneBinaryNameKind::NodeId, &node.id);
        let display_name = self
            .names
            .intern_optional(SceneBinaryNameKind::DisplayName, node.name.as_deref());
        let resource_name = self
            .names
            .intern_optional(SceneBinaryNameKind::ResourceId, node.resource.as_deref());
        let base_resource = node
            .resource
            .as_deref()
            .and_then(|resource| resource_index.binding(resource));
        let material_state =
            SceneBinaryMaterialState::from_node(node, base_resource, resource_index);
        let texture_start = if material_state.texture_slot_count > 0 {
            self.texture_slots.record_count
        } else {
            SCENE_BINARY_NONE_ID
        };
        let base_texture_slot = base_resource.map(|resource| SceneBinaryBaseTextureSlot {
            record_index: texture_start,
            resource_index: resource.index,
        });
        let base_role_flags = SCENE_BINARY_TEXTURE_ROLE_BASE_COLOR
            | if node_first_effect_pass_reuses_base_resource(node) {
                SCENE_BINARY_TEXTURE_ROLE_EFFECT_INPUT
            } else {
                0
            };
        if let Some(base_resource) = base_resource {
            self.push_texture_slot(SceneBinaryTextureSlotRecord {
                owner_name: id_name,
                pass_name: SCENE_BINARY_NONE_ID,
                texture_name: SCENE_BINARY_NONE_ID,
                resource_index: base_resource.index,
                slot: 0,
                width: base_resource.resource.width.unwrap_or(0),
                height: base_resource.resource.height.unwrap_or(0),
                role_flags: base_role_flags,
                sampler_flags: 0,
            });
        };
        let geometry_index = if node_has_geometry(node) {
            self.push_geometry(id_name, node)
        } else {
            SCENE_BINARY_NONE_ID
        };
        let material_index = if node_has_material(node) {
            let index = self.material_pass.record_count;
            let shader_name = self
                .names
                .intern_optional(SceneBinaryNameKind::Shader, material_state.shader);
            let blending_name = self
                .names
                .intern_optional(SceneBinaryNameKind::Material, material_state.blending);
            let first_effect_pass = if material_state.effect_pass_count > 0 {
                self.effect_pass.record_count
            } else {
                SCENE_BINARY_NONE_ID
            };
            self.material_pass.push_record(|out| {
                SceneBinaryMaterialPassRecord {
                    owner_name: id_name,
                    shader_name,
                    blending_name,
                    first_texture_slot: texture_start,
                    alpha_texture_slot: material_state
                        .alpha_texture_slot
                        .unwrap_or(SCENE_BINARY_NONE_ID),
                    first_effect_pass,
                    pipeline_key: material_state.pipeline_key(),
                    texture_slot_count: material_state.texture_slot_count,
                    effect_pass_count: material_state.effect_pass_count,
                    effect_kind_flags: material_state.effect_kind_flags,
                    material_kind: material_state.material_kind,
                    descriptor_layout: material_state.descriptor_layout,
                    blend_mode: blend_mode_code(material_state.blend_mode),
                    alpha_texture_mode: alpha_texture_mode_code(material_state.alpha_texture_mode),
                    depth_test: material_state.depth_test,
                    depth_write: material_state.depth_write,
                    cull_mode: material_state.cull_mode,
                    flags: material_state.flags,
                }
                .encode(out)
            });
            self.push_retained(SCENE_BINARY_RETAINED_MATERIAL_PASS, id_name, index);
            index
        } else {
            SCENE_BINARY_NONE_ID
        };
        let first_transform = self.transform_timeline.record_count;
        self.push_default_transform(id_name, node);
        for channel in timeline_index.channels_for_node(&node.id) {
            self.push_timeline_channel(id_name, *channel);
        }
        let transform_count = self
            .transform_timeline
            .record_count
            .saturating_sub(first_transform);
        let puppet_index = if node.mesh.is_some() || !node.puppet_animation_layers.is_empty() {
            self.push_puppet(id_name, node)
        } else {
            SCENE_BINARY_NONE_ID
        };
        self.node_table.push_record(|out| {
            SceneBinaryNodeRecord {
                id_name,
                display_name,
                parent_index: parent_index.unwrap_or(SCENE_BINARY_NONE_ID),
                resource_name,
                kind: node_kind_code(node.kind),
                flags: node_flags(node),
                draw_order: *draw_order,
                child_count: saturating_u32(node.children.len()),
                first_child_index: if node.children.is_empty() {
                    SCENE_BINARY_NONE_ID
                } else {
                    node_index.saturating_add(1)
                },
                subtree_node_count: node_subtree_count(node),
                effect_count: saturating_u32(node.effects.len()),
                audio_count: saturating_u32(node.audio.len()),
                property_count: saturating_u32(node.properties.len()),
                material_index,
                geometry_index,
                first_transform,
                transform_count,
                puppet_index,
                opacity: node.opacity as f32,
                color_rgba: scene_binary_color_rgba(node.color.as_deref()),
                stroke_color_rgba: scene_binary_color_rgba(node.stroke_color.as_deref()),
                stroke_width: node.stroke_width.unwrap_or(0.0) as f32,
                corner_radius: node.corner_radius.unwrap_or(0.0) as f32,
                fit: fit_code(node.fit),
            }
            .encode(out)
        });
        *draw_order = draw_order.saturating_add(1);
        let mut base_texture_reuse_available = base_texture_slot.is_some();
        for effect in &node.effects {
            self.include_effect(
                id_name,
                effect,
                resource_index,
                base_texture_slot,
                &mut base_texture_reuse_available,
            );
        }
        for child in &node.children {
            self.include_node(
                child,
                Some(node_index),
                draw_order,
                resource_index,
                timeline_index,
            );
        }
    }

    fn include_effect(
        &mut self,
        owner_name: u32,
        effect: &SceneEffect,
        resource_index: &SceneBinaryResourceIndex<'_>,
        base_texture_slot: Option<SceneBinaryBaseTextureSlot>,
        base_texture_reuse_available: &mut bool,
    ) {
        let effect_name = self
            .names
            .intern(SceneBinaryNameKind::EffectFile, &effect.file);
        let effect_parameter_start = self.effect_parameter.record_count;
        let effect_property_count = self.push_effect_parameters(owner_name, effect_name, effect);
        if effect.passes.is_empty() {
            self.push_effect_record(
                owner_name,
                effect_name,
                effect,
                None,
                0,
                SCENE_BINARY_NONE_ID,
                0,
                SCENE_BINARY_NONE_ID,
                0,
                effect_parameter_start,
                effect_property_count,
            );
        } else {
            for (pass_index, pass) in effect.passes.iter().enumerate() {
                let reusable_base_texture_slot = if *base_texture_reuse_available {
                    base_texture_slot
                } else {
                    None
                };
                let texture_slot_range = self.push_effect_texture_slots(
                    owner_name,
                    effect_name,
                    effect,
                    pass,
                    resource_index,
                    reusable_base_texture_slot,
                );
                if *base_texture_reuse_available {
                    *base_texture_reuse_available = false;
                }
                let effect_uv_transform_start = self.effect_uv_transform.record_count;
                let effect_uv_transform_count =
                    self.push_effect_uv_transform(owner_name, effect_name, pass_index, pass);
                let first_parameter = self.effect_parameter.record_count;
                let parameter_count =
                    self.push_effect_pass_parameters(owner_name, effect_name, pass_index, pass);
                self.push_effect_record(
                    owner_name,
                    effect_name,
                    effect,
                    Some(pass),
                    pass_index,
                    texture_slot_range.first_record,
                    texture_slot_range.record_count,
                    if effect_uv_transform_count == 0 {
                        SCENE_BINARY_NONE_ID
                    } else {
                        effect_uv_transform_start
                    },
                    effect_uv_transform_count,
                    first_parameter,
                    parameter_count,
                );
            }
        }
        if effect_is_motion_family(effect) {
            let parameter_count = self
                .effect_parameter
                .record_count
                .saturating_sub(effect_parameter_start);
            self.flutter_state.push_record(|out| {
                SceneBinaryFlutterStateRecord {
                    owner_name,
                    effect_name,
                    first_parameter: effect_parameter_start,
                    parameter_count,
                    pass_count: saturating_u32(effect.passes.len().max(1)),
                    motion_family_mask: motion_family_mask(effect),
                    anchor_name: owner_name,
                    dirty_range_count: motion_dirty_range_count(effect, parameter_count),
                }
                .encode(out)
            });
        }
    }

    fn push_effect_record(
        &mut self,
        owner_name: u32,
        effect_name: u32,
        effect: &SceneEffect,
        pass: Option<&SceneEffectPass>,
        pass_index: usize,
        first_texture_slot: u32,
        texture_slot_count: u32,
        first_effect_uv_transform: u32,
        effect_uv_transform_count: u32,
        first_parameter: u32,
        parameter_count: u32,
    ) {
        let shader_name = pass
            .and_then(|pass| pass.shader.as_deref())
            .map_or(SCENE_BINARY_NONE_ID, |shader| {
                self.names.intern(SceneBinaryNameKind::Shader, shader)
            });
        let blending_name = pass
            .and_then(|pass| pass.blending.as_deref())
            .map_or(SCENE_BINARY_NONE_ID, |blending| {
                self.names.intern(SceneBinaryNameKind::Material, blending)
            });
        let record_index = self.effect_pass.record_count;
        self.effect_pass.push_record(|out| {
            SceneBinaryEffectPassRecord {
                owner_name,
                effect_name,
                shader_name,
                blending_name,
                pass_index: pass_index.min(u32::MAX as usize) as u32,
                first_texture_slot,
                texture_slot_count,
                first_effect_uv_transform,
                effect_uv_transform_count,
                first_parameter,
                parameter_count,
                kind: effect_kind_code(effect),
                evaluation_boundary: effect_evaluation_boundary_code(effect),
                depth_test: material_flag_code(pass.and_then(|pass| pass.depthtest.as_deref())),
                depth_write: material_flag_code(pass.and_then(|pass| pass.depthwrite.as_deref())),
                cull_mode: cull_mode_code(pass.and_then(|pass| pass.cullmode.as_deref())),
                flags: effect_flags(effect, pass),
            }
            .encode(out)
        });
        self.push_retained(SCENE_BINARY_RETAINED_EFFECT_PASS, effect_name, record_index);
    }

    fn push_effect_uv_transform(
        &mut self,
        owner_name: u32,
        effect_name: u32,
        pass_index: usize,
        pass: &SceneEffectPass,
    ) -> u32 {
        let Some(transform) = pass.effect_uv_transform else {
            return 0;
        };
        let pass_index = pass_index.min(u32::MAX as usize) as u32;
        let record_index = self.effect_uv_transform.record_count;
        self.effect_uv_transform.push_record(|out| {
            let (input_width, input_height) = scene_binary_effect_uv_extent(transform.input_extent);
            let (mask_width, mask_height) = scene_binary_effect_uv_extent(transform.mask_extent);
            let (backing_width, backing_height) =
                scene_binary_effect_uv_extent(transform.mask_backing_extent);
            SceneBinaryEffectUvTransformRecord {
                owner_name,
                effect_name,
                pass_index,
                source_slot: transform.source_slot,
                mask_slot: transform.mask_slot,
                input_width,
                input_height,
                mask_width,
                mask_height,
                backing_width,
                backing_height,
                scale_u: transform.scale[0] as f32,
                scale_v: transform.scale[1] as f32,
                offset_u: transform.offset[0] as f32,
                offset_v: transform.offset[1] as f32,
                mapping: effect_uv_transform_mapping_code(transform),
                flags: effect_uv_transform_flags(transform),
            }
            .encode(out)
        });
        self.push_retained(
            SCENE_BINARY_RETAINED_EFFECT_UV_TRANSFORM,
            effect_name,
            record_index,
        );
        1
    }

    fn push_effect_parameters(
        &mut self,
        owner_name: u32,
        effect_name: u32,
        effect: &SceneEffect,
    ) -> u32 {
        let before = self.effect_parameter.record_count;
        for (name, value) in &effect.properties {
            self.push_effect_parameter(
                owner_name,
                effect_name,
                SCENE_BINARY_NONE_ID,
                SCENE_BINARY_PARAMETER_ROLE_EFFECT_PROPERTY,
                name,
                value,
            );
        }
        self.effect_parameter.record_count.saturating_sub(before)
    }

    fn push_effect_pass_parameters(
        &mut self,
        owner_name: u32,
        effect_name: u32,
        pass_index: usize,
        pass: &SceneEffectPass,
    ) -> u32 {
        let before = self.effect_parameter.record_count;
        let pass_index = pass_index.min(u32::MAX as usize) as u32;
        for (name, value) in &pass.constant_shader_values {
            self.push_effect_parameter(
                owner_name,
                effect_name,
                pass_index,
                SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT,
                name,
                value,
            );
        }
        for (name, value) in &pass.combos {
            self.push_effect_combo(owner_name, effect_name, pass_index, name, *value);
        }
        self.effect_parameter.record_count.saturating_sub(before)
    }

    fn push_effect_parameter(
        &mut self,
        owner_name: u32,
        effect_name: u32,
        pass_index: u32,
        role_flags: u16,
        name: &str,
        value: &serde_json::Value,
    ) {
        let Some(value) = scene_binary_parameter_value(value, &mut self.names) else {
            return;
        };
        let parameter_name = self
            .names
            .intern(SceneBinaryNameKind::EffectParameter, name);
        let record_index = self.effect_parameter.record_count;
        self.effect_parameter.push_record(|out| {
            SceneBinaryEffectParameterRecord {
                owner_name,
                effect_name,
                parameter_name,
                value_name: value.value_name,
                pass_index,
                value_kind: value.kind,
                role_flags,
                integer_value: value.integer,
                value0: value.values[0],
                value1: value.values[1],
                value2: value.values[2],
                value3: value.values[3],
            }
            .encode(out)
        });
        self.push_retained(
            SCENE_BINARY_RETAINED_EFFECT_PARAMETER,
            parameter_name,
            record_index,
        );
    }

    fn push_effect_combo(
        &mut self,
        owner_name: u32,
        effect_name: u32,
        pass_index: u32,
        name: &str,
        value: i64,
    ) {
        let parameter_name = self
            .names
            .intern(SceneBinaryNameKind::EffectParameter, name);
        let record_index = self.effect_parameter.record_count;
        self.effect_parameter.push_record(|out| {
            SceneBinaryEffectParameterRecord {
                owner_name,
                effect_name,
                parameter_name,
                value_name: SCENE_BINARY_NONE_ID,
                pass_index,
                value_kind: SCENE_BINARY_PARAMETER_VALUE_INTEGER,
                role_flags: SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO,
                integer_value: value,
                value0: value as f32,
                value1: 0.0,
                value2: 0.0,
                value3: 0.0,
            }
            .encode(out)
        });
        self.push_retained(
            SCENE_BINARY_RETAINED_EFFECT_PARAMETER,
            parameter_name,
            record_index,
        );
    }

    fn push_effect_texture_slots(
        &mut self,
        owner_name: u32,
        effect_name: u32,
        effect: &SceneEffect,
        pass: &SceneEffectPass,
        resource_index: &SceneBinaryResourceIndex<'_>,
        reusable_base_texture_slot: Option<SceneBinaryBaseTextureSlot>,
    ) -> SceneBinaryTextureSlotRange {
        let before = self.texture_slots.record_count;
        let reused_base_texture_slot = reusable_base_texture_slot
            .filter(|base| pass_reuses_base_texture_slot(pass, *base, resource_index));
        let first_record = reused_base_texture_slot.map_or(before, |base| base.record_index);
        let slot_count = pass.textures.len().max(pass.texture_resources.len());
        let alpha_texture_mode = super::scene_effect_alpha_texture_mode(effect);
        let first_class_target = effect_is_first_class_target(effect);
        for slot in 0..slot_count {
            if reused_base_texture_slot.is_some() && slot == 0 {
                continue;
            }
            let texture_name = pass
                .textures
                .get(slot)
                .and_then(|value| value.as_deref())
                .map_or(SCENE_BINARY_NONE_ID, |texture| {
                    self.names
                        .intern(SceneBinaryNameKind::ResourcePath, texture)
                });
            let resource = pass
                .texture_resources
                .get(slot)
                .and_then(|value| value.as_deref())
                .and_then(|resource| resource_index.binding(resource));
            if texture_name == SCENE_BINARY_NONE_ID && resource.is_none() {
                continue;
            }
            let role_flags = SCENE_BINARY_TEXTURE_ROLE_EFFECT_INPUT
                | if alpha_texture_mode.is_some() && slot > 0 {
                    SCENE_BINARY_TEXTURE_ROLE_ALPHA_MASK
                } else {
                    0
                }
                | if first_class_target && slot > 0 {
                    SCENE_BINARY_TEXTURE_ROLE_FIRST_CLASS_TARGET
                } else {
                    0
                };
            self.push_texture_slot(SceneBinaryTextureSlotRecord {
                owner_name,
                pass_name: effect_name,
                texture_name,
                resource_index: resource.map_or(SCENE_BINARY_NONE_ID, |resource| resource.index),
                slot: slot.min(u32::MAX as usize) as u32,
                width: resource
                    .and_then(|resource| resource.resource.width)
                    .unwrap_or(0),
                height: resource
                    .and_then(|resource| resource.resource.height)
                    .unwrap_or(0),
                role_flags,
                sampler_flags: 0,
            });
        }
        let pushed_count = self.texture_slots.record_count.saturating_sub(before);
        let record_count =
            pushed_count.saturating_add(u32::from(reused_base_texture_slot.is_some()));
        SceneBinaryTextureSlotRange {
            first_record: if record_count == 0 {
                SCENE_BINARY_NONE_ID
            } else {
                first_record
            },
            record_count,
        }
    }

    fn push_texture_slot(&mut self, record: SceneBinaryTextureSlotRecord) {
        let owner_name = record.owner_name;
        let record_index = self.texture_slots.push_record(|out| record.encode(out));
        self.push_retained(SCENE_BINARY_RETAINED_TEXTURE_SLOT, owner_name, record_index);
    }

    fn push_geometry(&mut self, owner_name: u32, node: &SceneNode) -> u32 {
        let (first_mesh_vertex, mesh_vertex_count, first_mesh_index, mesh_index_count) = node
            .mesh
            .as_ref()
            .map_or((SCENE_BINARY_NONE_ID, 0, SCENE_BINARY_NONE_ID, 0), |mesh| {
                (
                    self.geometry_vertices.record_count,
                    saturating_u32(mesh.vertices.len()),
                    self.geometry_indices.record_count,
                    saturating_u32(mesh.indices.len()),
                )
            });
        let stream_shape = geometry_stream_shape(
            node,
            first_mesh_vertex,
            mesh_vertex_count,
            first_mesh_index,
            mesh_index_count,
        );
        let record_index = self.geometry.push_record(|out| {
            let ranges = geometry_ranges(node);
            SceneBinaryGeometryRecord {
                owner_name,
                kind: node_kind_code(node.kind),
                flags: geometry_flags(node),
                width: node.width.unwrap_or(0.0) as f32,
                height: node.height.unwrap_or(0.0) as f32,
                first_vertex: stream_shape.first_vertex,
                vertex_count: stream_shape.vertex_count,
                first_index: stream_shape.first_index,
                index_count: stream_shape.index_count,
                material_uv_count: u32::from(geometry_has_uv(node)),
                primitive_kind: stream_shape.primitive_kind,
                vertex_layout: stream_shape.vertex_layout,
                bounds_min_x: ranges.bounds_min_x,
                bounds_min_y: ranges.bounds_min_y,
                bounds_max_x: ranges.bounds_max_x,
                bounds_max_y: ranges.bounds_max_y,
                uv_min_u: ranges.uv_min_u,
                uv_min_v: ranges.uv_min_v,
                uv_max_u: ranges.uv_max_u,
                uv_max_v: ranges.uv_max_v,
            }
            .encode(out)
        });
        if let Some(mesh) = node.mesh.as_ref() {
            self.push_geometry_streams(mesh);
        }
        self.push_retained(SCENE_BINARY_RETAINED_GEOMETRY, owner_name, record_index);
        record_index
    }

    fn push_geometry_streams(&mut self, mesh: &super::SceneMesh) {
        for vertex in &mesh.vertices {
            self.geometry_vertices.push_record(|out| {
                SceneBinaryGeometryVertexRecord {
                    x: vertex.x as f32,
                    y: vertex.y as f32,
                    u: vertex.u as f32,
                    v: vertex.v as f32,
                    opacity: vertex.opacity as f32,
                }
                .encode(out)
            });
        }
        for &index in &mesh.indices {
            self.geometry_indices
                .push_record(|out| SceneBinaryGeometryIndexRecord { index }.encode(out));
        }
    }

    fn push_default_transform(&mut self, owner_name: u32, node: &SceneNode) {
        self.transform_timeline.push_record(|out| {
            SceneBinaryTransformTimelineRecord {
                owner_name,
                timeline_name: SCENE_BINARY_NONE_ID,
                property: SCENE_BINARY_DEFAULT_TRANSFORM_PROPERTY,
                flags: 0,
                keyframe_count: 0,
                first_keyframe: SCENE_BINARY_NONE_ID,
                time_offset_ms: 0,
                first_time_ms: 0,
                last_time_ms: 0,
                value0: node.transform.x as f32,
                value1: node.transform.y as f32,
                value2: node.transform.scale_x as f32,
                value3: node.transform.scale_y as f32,
                value4: node.transform.rotation_deg as f32,
                value5: node.transform.anchor_x as f32,
                value6: node.transform.anchor_y as f32,
            }
            .encode(out)
        });
    }

    fn push_timeline_channel(
        &mut self,
        owner_name: u32,
        binding: SceneBinaryTimelineChannelBinding<'_>,
    ) {
        let timeline_name = self
            .names
            .intern(SceneBinaryNameKind::Timeline, binding.timeline_id);
        let channel = binding.channel;
        let first_keyframe = if channel.keyframes.is_empty() {
            SCENE_BINARY_NONE_ID
        } else {
            self.transform_keyframes.record_count
        };
        for keyframe in &channel.keyframes {
            self.push_transform_keyframe(keyframe);
        }
        let (first_time_ms, last_time_ms, first_value, last_value) =
            timeline_channel_bounds(channel);
        let property_name = self.names.intern(
            SceneBinaryNameKind::Property,
            animated_property_label(channel.property),
        );
        self.transform_timeline.push_record(|out| {
            SceneBinaryTransformTimelineRecord {
                owner_name,
                timeline_name,
                property: animated_property_code(channel.property),
                flags: u16::from(channel.loop_playback),
                keyframe_count: saturating_u32(channel.keyframes.len()),
                first_keyframe,
                time_offset_ms: channel.time_offset_ms,
                first_time_ms,
                last_time_ms,
                value0: first_value,
                value1: last_value,
                value2: property_name as f32,
                value3: 0.0,
                value4: 0.0,
                value5: 0.0,
                value6: 0.0,
            }
            .encode(out)
        });
    }

    fn push_transform_keyframe(&mut self, keyframe: &SceneKeyframe) {
        self.transform_keyframes.push_record(|out| {
            SceneBinaryTransformKeyframeRecord {
                time_ms: keyframe.time_ms,
                value: keyframe.value as f32,
                curve: curve_code(keyframe.curve),
                flags: 0,
            }
            .encode(out)
        });
    }

    fn push_puppet(&mut self, owner_name: u32, node: &SceneNode) -> u32 {
        let record_index = self.puppet.record_count;
        let mesh = node.mesh.as_deref();
        let (vertex_count, index_count) = mesh.map_or((0, 0), |mesh| {
            (
                saturating_u32(mesh.vertices.len()),
                saturating_u32(mesh.indices.len()),
            )
        });

        let first_bone = self.puppet_skin_bones.record_count;
        let first_skin_vertex = self.puppet_skin_vertices.record_count;
        let first_attachment = self.puppet_attachments.record_count;
        let mut bone_count = 0;
        let mut skin_vertex_count = 0;
        let mut attachment_count = 0;
        if let Some(skin) = mesh.and_then(|mesh| mesh.skin.as_ref()) {
            bone_count = saturating_u32(skin.bones.len());
            for bone in &skin.bones {
                self.puppet_skin_bones.push_record(|out| {
                    SceneBinaryPuppetSkinBoneRecord {
                        owner_name,
                        parent_index: bone.parent.map_or(SCENE_BINARY_NONE_ID, saturating_u32),
                        transform: bone.bind,
                    }
                    .encode(out)
                });
            }
            skin_vertex_count = saturating_u32(skin.vertices.len());
            for vertex in &skin.vertices {
                let mut bone_indices = [0; 4];
                for (slot, index) in vertex.bone_indices.iter().enumerate() {
                    bone_indices[slot] = saturating_u32(*index);
                }
                self.puppet_skin_vertices.push_record(|out| {
                    SceneBinaryPuppetSkinVertexRecord {
                        owner_name,
                        bone_indices,
                        weights: [
                            vertex.weights[0] as f32,
                            vertex.weights[1] as f32,
                            vertex.weights[2] as f32,
                            vertex.weights[3] as f32,
                        ],
                        weight_count: saturating_u32(
                            vertex
                                .weights
                                .iter()
                                .filter(|weight| weight.is_finite() && **weight > f64::EPSILON)
                                .count(),
                        ),
                    }
                    .encode(out)
                });
            }
            attachment_count = saturating_u32(skin.attachments.len());
            for attachment in &skin.attachments {
                let name = self
                    .names
                    .intern(SceneBinaryNameKind::PuppetAttachment, &attachment.name);
                self.puppet_attachments.push_record(|out| {
                    SceneBinaryPuppetAttachmentRecord {
                        owner_name,
                        name,
                        bone_index: saturating_u32(attachment.bone_index),
                        local_position: [
                            attachment.local_position[0] as f32,
                            attachment.local_position[1] as f32,
                            attachment.local_position[2] as f32,
                        ],
                        bind_position: [
                            attachment.bind_position[0] as f32,
                            attachment.bind_position[1] as f32,
                            attachment.bind_position[2] as f32,
                        ],
                        flags: 0,
                    }
                    .encode(out)
                });
            }
        }

        let first_clip = self.puppet_clips.record_count;
        let first_clip_frame = self.puppet_frames.record_count;
        if let Some(mesh) = mesh {
            for clip in &mesh.puppet_clips {
                let clip_name = self
                    .names
                    .intern_optional(SceneBinaryNameKind::PuppetClip, clip.name.as_deref());
                let first_frame = self.puppet_frames.record_count;
                let mut frame_record_count = 0u32;
                for (bone_index, bone) in clip.bones.iter().enumerate() {
                    for (frame_index, transform) in bone.frames.iter().enumerate() {
                        self.puppet_frames.push_record(|out| {
                            SceneBinaryPuppetFrameRecord {
                                owner_name,
                                clip_id: clip.id,
                                bone_index: saturating_u32(bone_index),
                                frame_index: saturating_u32(frame_index),
                                transform: *transform,
                            }
                            .encode(out)
                        });
                        frame_record_count = frame_record_count.saturating_add(1);
                    }
                }
                self.puppet_clips.push_record(|out| {
                    SceneBinaryPuppetClipRecord {
                        owner_name,
                        clip_name,
                        clip_id: clip.id,
                        first_frame: puppet_first_record(first_frame, frame_record_count),
                        bone_count: saturating_u32(clip.bones.len()),
                        frame_count: clip.frame_count,
                        frame_record_count,
                        fps: clip.fps as f32,
                        flags: puppet_clip_flags(clip.looping),
                        dirty_range_count: u32::from(frame_record_count > 0),
                    }
                    .encode(out)
                });
            }
        }
        let clip_count = self.puppet_clips.record_count.saturating_sub(first_clip);
        let clip_frame_count = self
            .puppet_frames
            .record_count
            .saturating_sub(first_clip_frame);

        let first_layer = self.puppet_layers.record_count;
        for (layer_index, layer) in node.puppet_animation_layers.iter().enumerate() {
            let layer_name = self
                .names
                .intern_optional(SceneBinaryNameKind::PuppetLayer, layer.name.as_deref());
            self.puppet_layers.push_record(|out| {
                SceneBinaryPuppetLayerRecord {
                    owner_name,
                    layer_name,
                    clip_id: layer.clip_id,
                    layer_index: saturating_u32(layer_index),
                    flags: puppet_layer_flags(layer.additive, layer.lock_transforms, layer.visible),
                    blend: layer.blend as f32,
                    rate: layer.rate as f32,
                    initial_phase: layer.initial_phase as f32,
                }
                .encode(out)
            });
        }
        let animation_layer_count = self.puppet_layers.record_count.saturating_sub(first_layer);

        let flags = puppet_flags(
            mesh.is_some(),
            animation_layer_count > 0,
            bone_count > 0 && skin_vertex_count > 0,
            clip_count > 0,
            attachment_count > 0,
        );
        let dirty_range_count = 1
            + u32::from(bone_count > 0)
            + u32::from(skin_vertex_count > 0)
            + u32::from(attachment_count > 0)
            + u32::from(clip_count > 0)
            + u32::from(clip_frame_count > 0)
            + u32::from(animation_layer_count > 0);
        self.puppet.push_record(|out| {
            SceneBinaryPuppetRecord {
                owner_name,
                vertex_count,
                index_count,
                first_bone: puppet_first_record(first_bone, bone_count),
                bone_count,
                first_skin_vertex: puppet_first_record(first_skin_vertex, skin_vertex_count),
                skin_vertex_count,
                first_attachment: puppet_first_record(first_attachment, attachment_count),
                attachment_count,
                first_clip: puppet_first_record(first_clip, clip_count),
                clip_count,
                first_clip_frame: puppet_first_record(first_clip_frame, clip_frame_count),
                clip_frame_count,
                first_layer: puppet_first_record(first_layer, animation_layer_count),
                animation_layer_count,
                flags,
                dirty_range_count,
            }
            .encode(out)
        });
        self.push_retained(SCENE_BINARY_RETAINED_PUPPET, owner_name, record_index);
        record_index
    }

    fn push_retained(&mut self, owner_kind: u16, owner_name: u32, record_index: u32) {
        self.retained_gpu_state.push_record(|out| {
            SceneBinaryRetainedGpuStateRecord {
                owner_kind,
                flags: 0,
                owner_name,
                stable_id: retained_stable_id(owner_kind, owner_name, record_index),
                record_index,
                dirty_range_count: 1,
            }
            .encode(out)
        });
    }

    fn finish(self) -> SceneBinaryDocumentPayloads {
        let debug_name_records = self.names.record_count();
        let debug_names = SceneBinaryOwnedChunkPayload {
            kind: SceneBinaryChunkKind::DebugNames,
            record_count: debug_name_records,
            bytes: self.names.encode(),
        };
        let shape = SceneBinaryDocumentShape {
            resource_table_records: self.resource_table.record_count,
            node_table_records: self.node_table.record_count,
            transform_timeline_records: self.transform_timeline.record_count,
            transform_keyframe_records: self.transform_keyframes.record_count,
            geometry_records: self.geometry.record_count,
            geometry_vertex_records: self.geometry_vertices.record_count,
            geometry_index_records: self.geometry_indices.record_count,
            texture_slot_records: self.texture_slots.record_count,
            material_pass_records: self.material_pass.record_count,
            effect_pass_records: self.effect_pass.record_count,
            effect_uv_transform_records: self.effect_uv_transform.record_count,
            effect_parameter_records: self.effect_parameter.record_count,
            flutter_state_records: self.flutter_state.record_count,
            puppet_records: self.puppet.record_count,
            puppet_skin_bone_records: self.puppet_skin_bones.record_count,
            puppet_skin_vertex_records: self.puppet_skin_vertices.record_count,
            puppet_attachment_records: self.puppet_attachments.record_count,
            puppet_clip_records: self.puppet_clips.record_count,
            puppet_frame_records: self.puppet_frames.record_count,
            puppet_layer_records: self.puppet_layers.record_count,
            render_state_records: self.render_state.record_count,
            retained_gpu_state_records: self.retained_gpu_state.record_count,
            debug_name_records,
        };
        SceneBinaryDocumentPayloads {
            shape,
            chunks: vec![
                self.resource_table
                    .into_payload(SceneBinaryChunkKind::ResourceTable),
                self.node_table
                    .into_payload(SceneBinaryChunkKind::NodeTable),
                self.transform_timeline
                    .into_payload(SceneBinaryChunkKind::TransformTimeline),
                self.transform_keyframes
                    .into_payload(SceneBinaryChunkKind::TransformKeyframes),
                self.geometry.into_payload(SceneBinaryChunkKind::Geometry),
                self.geometry_vertices
                    .into_payload(SceneBinaryChunkKind::GeometryVertices),
                self.geometry_indices
                    .into_payload(SceneBinaryChunkKind::GeometryIndices),
                self.texture_slots
                    .into_payload(SceneBinaryChunkKind::TextureSlots),
                self.material_pass
                    .into_payload(SceneBinaryChunkKind::MaterialPass),
                self.effect_pass
                    .into_payload(SceneBinaryChunkKind::EffectPass),
                self.effect_uv_transform
                    .into_payload(SceneBinaryChunkKind::EffectUvTransform),
                self.effect_parameter
                    .into_payload(SceneBinaryChunkKind::EffectParameter),
                self.flutter_state
                    .into_payload(SceneBinaryChunkKind::FlutterState),
                self.puppet.into_payload(SceneBinaryChunkKind::Puppet),
                self.puppet_skin_bones
                    .into_payload(SceneBinaryChunkKind::PuppetSkinBones),
                self.puppet_skin_vertices
                    .into_payload(SceneBinaryChunkKind::PuppetSkinVertices),
                self.puppet_attachments
                    .into_payload(SceneBinaryChunkKind::PuppetAttachments),
                self.puppet_clips
                    .into_payload(SceneBinaryChunkKind::PuppetClips),
                self.puppet_frames
                    .into_payload(SceneBinaryChunkKind::PuppetFrames),
                self.puppet_layers
                    .into_payload(SceneBinaryChunkKind::PuppetLayers),
                self.render_state
                    .into_payload(SceneBinaryChunkKind::RenderState),
                self.retained_gpu_state
                    .into_payload(SceneBinaryChunkKind::RetainedGpuState),
                debug_names,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SceneBinaryResourceFields<'a> {
    id: &'a str,
    kind: SceneResourceKind,
    source: &'a str,
    width: Option<u32>,
    height: Option<u32>,
    original_source: Option<&'a str>,
    role: Option<&'a str>,
}

fn resource_id_fields(resource: &super::SceneResource) -> SceneBinaryResourceFields<'_> {
    SceneBinaryResourceFields {
        id: &resource.id,
        kind: resource.kind,
        source: resource.source.as_str(),
        width: resource.width,
        height: resource.height,
        original_source: resource.original_source.as_deref(),
        role: resource.role.as_deref(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneBinaryError {
    BufferTooSmall {
        needed: usize,
        actual: usize,
    },
    BadMagic {
        actual: [u8; 4],
    },
    UnsupportedVersion {
        version: u16,
    },
    UnsupportedEndian {
        endian: u8,
    },
    InvalidAlignment {
        alignment: u8,
    },
    InvalidChunkOrder {
        index: usize,
        expected: SceneBinaryChunkKind,
        actual: SceneBinaryChunkKind,
    },
    RequiredChunkCount {
        expected: usize,
        actual: usize,
    },
    DuplicateChunk {
        kind: SceneBinaryChunkKind,
    },
    MissingChunk {
        kind: SceneBinaryChunkKind,
    },
    UnknownChunk {
        code: u32,
    },
    UnknownRetainedOwnerKind {
        owner_kind: u16,
    },
    InvalidRecordPayload {
        kind: SceneBinaryChunkKind,
        record_size: usize,
        record_count: u32,
        length: usize,
    },
    RecordRangeOutOfBounds {
        kind: SceneBinaryChunkKind,
        first_record: u32,
        record_count: u32,
        chunk_record_count: u32,
    },
    NameOutOfBounds {
        id: u32,
        offset: u32,
        length: u32,
        string_table_len: usize,
    },
    InvalidNameUtf8 {
        id: u32,
    },
    ChunkTableOutOfBounds {
        offset: u64,
        count: u32,
        container_len: usize,
    },
    MisalignedChunk {
        kind: SceneBinaryChunkKind,
        offset: u64,
        alignment: u8,
    },
    ChunkOutOfBounds {
        kind: SceneBinaryChunkKind,
        offset: u64,
        length: u64,
        container_len: usize,
    },
    ChunkOverlap {
        previous: SceneBinaryChunkKind,
        current: SceneBinaryChunkKind,
    },
    StreamIo {
        operation: &'static str,
        message: String,
    },
}

impl fmt::Display for SceneBinaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooSmall { needed, actual } => {
                write!(f, "scene binary buffer is {actual} bytes; needs {needed}")
            }
            Self::BadMagic { actual } => write!(f, "invalid scene binary magic {actual:?}"),
            Self::UnsupportedVersion { version } => {
                write!(f, "unsupported scene binary version {version}")
            }
            Self::UnsupportedEndian { endian } => {
                write!(f, "unsupported scene binary endian policy {endian}")
            }
            Self::InvalidAlignment { alignment } => {
                write!(f, "invalid scene binary alignment {alignment}")
            }
            Self::InvalidChunkOrder {
                index,
                expected,
                actual,
            } => write!(
                f,
                "scene binary chunk {index} is {}; expected {}",
                actual.label(),
                expected.label()
            ),
            Self::RequiredChunkCount { expected, actual } => write!(
                f,
                "scene binary has {actual} required chunk families; expected {expected}"
            ),
            Self::DuplicateChunk { kind } => {
                write!(f, "duplicate scene binary chunk {}", kind.label())
            }
            Self::MissingChunk { kind } => {
                write!(f, "missing scene binary chunk {}", kind.label())
            }
            Self::UnknownChunk { code } => write!(f, "unknown scene binary chunk code {code:#x}"),
            Self::UnknownRetainedOwnerKind { owner_kind } => {
                write!(f, "unknown scene binary retained owner kind {owner_kind}")
            }
            Self::InvalidRecordPayload {
                kind,
                record_size,
                record_count,
                length,
            } => write!(
                f,
                "scene binary chunk {} has {length} payload bytes; expected {} records of {record_size} bytes",
                kind.label(),
                record_count
            ),
            Self::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count,
            } => write!(
                f,
                "scene binary chunk {} record range {}..{} exceeds {} records",
                kind.label(),
                first_record,
                first_record.saturating_add(*record_count),
                chunk_record_count
            ),
            Self::NameOutOfBounds {
                id,
                offset,
                length,
                string_table_len,
            } => write!(
                f,
                "scene binary debug name {id} offset {offset} length {length} exceeds {string_table_len} string bytes"
            ),
            Self::InvalidNameUtf8 { id } => {
                write!(f, "scene binary debug name {id} is not valid UTF-8")
            }
            Self::ChunkTableOutOfBounds {
                offset,
                count,
                container_len,
            } => write!(
                f,
                "scene binary chunk table offset {offset} count {count} exceeds {container_len} bytes"
            ),
            Self::MisalignedChunk {
                kind,
                offset,
                alignment,
            } => write!(
                f,
                "scene binary chunk {} offset {offset} is not aligned to {alignment}",
                kind.label()
            ),
            Self::ChunkOutOfBounds {
                kind,
                offset,
                length,
                container_len,
            } => write!(
                f,
                "scene binary chunk {} offset {offset} length {length} exceeds {container_len} bytes",
                kind.label()
            ),
            Self::ChunkOverlap { previous, current } => write!(
                f,
                "scene binary chunk {} overlaps {}",
                current.label(),
                previous.label()
            ),
            Self::StreamIo { operation, message } => {
                write!(f, "scene binary {operation} failed: {message}")
            }
        }
    }
}

impl Error for SceneBinaryError {}

pub fn encode_scene_binary_container(
    feature_flags: u32,
    payloads: &[SceneBinaryChunkPayload<'_>],
) -> Result<Vec<u8>, SceneBinaryError> {
    validate_required_payload_order(payloads)?;
    let table_size = payloads
        .len()
        .checked_mul(SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE)
        .and_then(|size| size.checked_add(SCENE_BINARY_HEADER_SIZE))
        .expect("scene binary table size overflow");
    let mut next_payload_offset = align_usize(table_size, usize::from(SCENE_BINARY_ALIGNMENT));
    let mut descriptors = Vec::with_capacity(payloads.len());
    for payload in payloads {
        let offset = next_payload_offset;
        let length = payload.bytes.len();
        descriptors.push(SceneBinaryChunkDescriptor {
            kind: payload.kind,
            record_count: payload.record_count,
            offset: offset.min(u64::MAX as usize) as u64,
            length: length.min(u64::MAX as usize) as u64,
        });
        next_payload_offset = align_usize(
            offset
                .checked_add(length)
                .expect("scene binary payload size overflow"),
            usize::from(SCENE_BINARY_ALIGNMENT),
        );
    }

    let mut bytes = Vec::with_capacity(next_payload_offset);
    write_header(
        &mut bytes,
        feature_flags,
        payloads.len().min(u32::MAX as usize) as u32,
    );
    for descriptor in &descriptors {
        write_chunk_descriptor(&mut bytes, descriptor);
    }
    bytes.resize(
        descriptors
            .first()
            .map_or(table_size, |chunk| chunk.offset as usize),
        0,
    );
    for (descriptor, payload) in descriptors.iter().zip(payloads) {
        bytes.resize(descriptor.offset as usize, 0);
        bytes.extend_from_slice(payload.bytes);
        let aligned = align_usize(bytes.len(), usize::from(SCENE_BINARY_ALIGNMENT));
        bytes.resize(aligned, 0);
    }
    Ok(bytes)
}

pub fn decode_scene_binary_container(
    bytes: &[u8],
) -> Result<SceneBinaryLayoutPlan, SceneBinaryError> {
    decode_scene_binary_header_table(bytes, bytes.len())
}

pub fn decode_scene_binary_header_table(
    bytes: &[u8],
    container_len: usize,
) -> Result<SceneBinaryLayoutPlan, SceneBinaryError> {
    if bytes.len() < SCENE_BINARY_HEADER_SIZE {
        return Err(SceneBinaryError::BufferTooSmall {
            needed: SCENE_BINARY_HEADER_SIZE,
            actual: bytes.len(),
        });
    }
    let magic = read_array_4(bytes, 0)?;
    if magic != SCENE_BINARY_MAGIC {
        return Err(SceneBinaryError::BadMagic { actual: magic });
    }
    let version = read_u16(bytes, 4)?;
    if version != SCENE_BINARY_VERSION {
        return Err(SceneBinaryError::UnsupportedVersion { version });
    }
    let endian = bytes[6];
    if endian != SCENE_BINARY_ENDIAN_LITTLE {
        return Err(SceneBinaryError::UnsupportedEndian { endian });
    }
    let alignment = bytes[7];
    if alignment != SCENE_BINARY_ALIGNMENT {
        return Err(SceneBinaryError::InvalidAlignment { alignment });
    }
    let feature_flags = read_u32(bytes, 8)?;
    let chunk_count = read_u32(bytes, 12)?;
    let chunk_table_offset = read_u64(bytes, 16)?;
    let table_start = usize::try_from(chunk_table_offset).map_err(|_| {
        SceneBinaryError::ChunkTableOutOfBounds {
            offset: chunk_table_offset,
            count: chunk_count,
            container_len,
        }
    })?;
    let table_size = usize::try_from(chunk_count)
        .ok()
        .and_then(|count| count.checked_mul(SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE))
        .ok_or(SceneBinaryError::ChunkTableOutOfBounds {
            offset: chunk_table_offset,
            count: chunk_count,
            container_len,
        })?;
    let table_end =
        table_start
            .checked_add(table_size)
            .ok_or(SceneBinaryError::ChunkTableOutOfBounds {
                offset: chunk_table_offset,
                count: chunk_count,
                container_len,
            })?;
    if table_end > bytes.len() || table_end > container_len {
        return Err(SceneBinaryError::ChunkTableOutOfBounds {
            offset: chunk_table_offset,
            count: chunk_count,
            container_len,
        });
    }

    let mut seen = BTreeSet::new();
    let mut chunks = Vec::with_capacity(chunk_count as usize);
    for index in 0..chunk_count as usize {
        let descriptor_offset = table_start + index * SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE;
        let chunk = read_chunk_descriptor(bytes, descriptor_offset)?;
        let expected = SceneBinaryChunkKind::REQUIRED_ORDER
            .get(index)
            .copied()
            .ok_or(SceneBinaryError::InvalidChunkOrder {
                index,
                expected: SceneBinaryChunkKind::DebugNames,
                actual: chunk.kind,
            })?;
        if chunk.kind != expected {
            return Err(SceneBinaryError::InvalidChunkOrder {
                index,
                expected,
                actual: chunk.kind,
            });
        }
        if !seen.insert(chunk.kind) {
            return Err(SceneBinaryError::DuplicateChunk { kind: chunk.kind });
        }
        validate_chunk_bounds(container_len, alignment, table_end, chunks.last(), &chunk)?;
        chunks.push(chunk);
    }
    if chunks.len() != SceneBinaryChunkKind::REQUIRED_ORDER.len() {
        return Err(SceneBinaryError::RequiredChunkCount {
            expected: SceneBinaryChunkKind::REQUIRED_ORDER.len(),
            actual: chunks.len(),
        });
    }
    Ok(SceneBinaryLayoutPlan {
        feature_flags,
        chunks,
    })
}

pub fn scene_binary_empty_payloads_for_shape(
    shape: SceneBinaryDocumentShape,
) -> Vec<SceneBinaryChunkPayload<'static>> {
    SceneBinaryChunkKind::REQUIRED_ORDER
        .into_iter()
        .map(|kind| SceneBinaryChunkPayload {
            kind,
            record_count: shape.record_count(kind),
            bytes: &[],
        })
        .collect()
}

fn resource_kind_code(kind: SceneResourceKind) -> u16 {
    match kind {
        SceneResourceKind::Image => 1,
        SceneResourceKind::Video => 2,
        SceneResourceKind::Audio => 3,
        SceneResourceKind::Texture => 4,
        SceneResourceKind::Model => 5,
        SceneResourceKind::Material => 6,
        SceneResourceKind::Effect => 7,
        SceneResourceKind::Particle => 8,
        SceneResourceKind::Font => 9,
        SceneResourceKind::Shader => 10,
        SceneResourceKind::Script => 11,
        SceneResourceKind::Json => 12,
        SceneResourceKind::Other => 13,
    }
}

fn node_kind_code(kind: SceneNodeKind) -> u16 {
    match kind {
        SceneNodeKind::Image => 1,
        SceneNodeKind::Video => 2,
        SceneNodeKind::Color => 3,
        SceneNodeKind::Rectangle => 4,
        SceneNodeKind::Ellipse => 5,
        SceneNodeKind::Text => 6,
        SceneNodeKind::Path => 7,
        SceneNodeKind::Group => 8,
        SceneNodeKind::Shader => 9,
        SceneNodeKind::ParticleEmitter => 10,
        SceneNodeKind::AudioResponse => 11,
        SceneNodeKind::Audio => 12,
        SceneNodeKind::Script => 13,
        SceneNodeKind::Unknown => 14,
    }
}

fn blend_mode_code(mode: SceneBlendMode) -> u16 {
    match mode {
        SceneBlendMode::Alpha => 1,
        SceneBlendMode::Additive => 2,
        SceneBlendMode::Multiply => 3,
        SceneBlendMode::Screen => 4,
        SceneBlendMode::Max => 5,
        SceneBlendMode::Normal => 6,
    }
}

fn alpha_texture_mode_code(mode: SceneAlphaTextureMode) -> u16 {
    match mode {
        SceneAlphaTextureMode::Multiply => 1,
        SceneAlphaTextureMode::Inverse => 2,
        SceneAlphaTextureMode::Iris => 3,
        SceneAlphaTextureMode::Coverage => 4,
    }
}

fn animated_property_code(property: SceneAnimatedProperty) -> u16 {
    match property {
        SceneAnimatedProperty::X => 1,
        SceneAnimatedProperty::Y => 2,
        SceneAnimatedProperty::ScaleX => 3,
        SceneAnimatedProperty::ScaleY => 4,
        SceneAnimatedProperty::Opacity => 5,
        SceneAnimatedProperty::RotationDeg => 6,
        SceneAnimatedProperty::Width => 7,
        SceneAnimatedProperty::Height => 8,
        SceneAnimatedProperty::CornerRadius => 9,
        SceneAnimatedProperty::Custom => 10,
    }
}

fn animated_property_label(property: SceneAnimatedProperty) -> &'static str {
    match property {
        SceneAnimatedProperty::X => "x",
        SceneAnimatedProperty::Y => "y",
        SceneAnimatedProperty::ScaleX => "scale_x",
        SceneAnimatedProperty::ScaleY => "scale_y",
        SceneAnimatedProperty::Opacity => "opacity",
        SceneAnimatedProperty::RotationDeg => "rotation_deg",
        SceneAnimatedProperty::Width => "width",
        SceneAnimatedProperty::Height => "height",
        SceneAnimatedProperty::CornerRadius => "corner_radius",
        SceneAnimatedProperty::Custom => "custom",
    }
}

fn curve_code(curve: SceneCurve) -> u16 {
    match curve {
        SceneCurve::Linear => 1,
        SceneCurve::Step => 2,
        SceneCurve::EaseIn => 3,
        SceneCurve::EaseOut => 4,
        SceneCurve::EaseInOut => 5,
    }
}

fn node_flags(node: &SceneNode) -> u16 {
    u16::from(node.visible)
        | (u16::from(node.resource.is_some()) << 1)
        | (u16::from(!node.effects.is_empty()) << 2)
        | (u16::from(!node.children.is_empty()) << 3)
        | (u16::from(node.mesh.is_some()) << 4)
        | (u16::from(!node.puppet_animation_layers.is_empty()) << 5)
        | (u16::from(!node.audio.is_empty()) << 6)
        | (u16::from(node.color.is_some()) << 7)
        | (u16::from(node.stroke_color.is_some()) << 8)
        | (u16::from(node.stroke_width.is_some()) << 9)
        | (u16::from(node.corner_radius.is_some()) << 10)
        | (u16::from(node.fit != FitMode::Cover) << 11)
}

fn node_subtree_count(node: &SceneNode) -> u32 {
    node.children.iter().fold(1u32, |count, child| {
        count.saturating_add(node_subtree_count(child))
    })
}

fn fit_code(fit: FitMode) -> u16 {
    match fit {
        FitMode::Cover => 1,
        FitMode::Contain => 2,
        FitMode::Stretch => 3,
        FitMode::Tile => 4,
        FitMode::Center => 5,
    }
}

fn scene_binary_color_rgba(color: Option<&str>) -> u32 {
    let Some(color) = color.and_then(scene_binary_hex_color_rgb) else {
        return 0;
    };
    (u32::from(color[0]) << 24) | (u32::from(color[1]) << 16) | (u32::from(color[2]) << 8) | 0xff
}

fn scene_binary_hex_color_rgb(color: &str) -> Option<[u8; 3]> {
    let hex = color.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ])
}

fn material_flags(
    node: &SceneNode,
    base_resource: Option<SceneBinaryResourceBinding<'_>>,
    alpha_texture_slot: Option<u32>,
    effect_pass_count: u32,
) -> u16 {
    u16::from(node.visible)
        | (u16::from(base_resource.is_some()) << 1)
        | (u16::from(effect_pass_count > 0) << 2)
        | (u16::from(alpha_texture_slot.is_some()) << 3)
        | (u16::from(node.mesh.is_some()) << 4)
        | (u16::from(!node.puppet_animation_layers.is_empty()) << 5)
        | (u16::from(!node.properties.is_empty()) << 6)
}

fn effect_flags(effect: &SceneEffect, pass: Option<&SceneEffectPass>) -> u16 {
    u16::from(effect.resource.is_some())
        | (u16::from(effect.runtime.is_some()) << 1)
        | (u16::from(effect.visible.is_some()) << 2)
        | (u16::from(pass.and_then(|pass| pass.shader.as_ref()).is_some()) << 3)
        | (u16::from(pass.and_then(|pass| pass.blending.as_ref()).is_some()) << 4)
}

fn render_state_flags(document: &SceneDocument) -> u32 {
    u32::from(document.size.is_some())
        | (u32::from(document.render.clear_color.is_some()) << 1)
        | (u32::from(document.render.clear_enabled.unwrap_or(false)) << 2)
        | (u32::from(document.render.hdr.unwrap_or(false)) << 3)
}

fn material_kind_code(node: &SceneNode, effect_pass_count: u32) -> u16 {
    if node.mesh.is_some() || !node.puppet_animation_layers.is_empty() {
        4
    } else if matches!(node.kind, SceneNodeKind::Image | SceneNodeKind::Video)
        && effect_pass_count > 0
    {
        3
    } else if matches!(node.kind, SceneNodeKind::Image | SceneNodeKind::Video) {
        2
    } else if node_has_geometry(node) {
        1
    } else {
        5
    }
}

fn descriptor_layout_code(
    has_base_resource: bool,
    texture_slot_count: u32,
    has_alpha_texture: bool,
    effect_pass_count: u32,
) -> u16 {
    if texture_slot_count == 0 {
        1
    } else if has_alpha_texture {
        3
    } else if effect_pass_count > 0 && has_base_resource {
        4
    } else if effect_pass_count > 0 {
        5
    } else {
        2
    }
}

fn effect_kind_code(effect: &SceneEffect) -> u16 {
    let file = effect.file.to_ascii_lowercase();
    let runtime = effect.runtime.as_deref().unwrap_or_default();
    if runtime == "native-opacity-mask" || file.contains("opacity") {
        1
    } else if runtime == "native-iris-mask" || file.contains("iris") {
        2
    } else if runtime == "native-water-caustics"
        || file.contains("watercaustics")
        || file.contains("water_caustics")
    {
        6
    } else if file.contains("waterripple") || file.contains("water_ripple") {
        3
    } else if file.contains("waterwaves") || file.contains("water_waves") {
        4
    } else if file.contains("waterflow") || file.contains("water_flow") {
        5
    } else if file.contains("sway") || file.contains("shake") {
        7
    } else if file.contains("flutter") {
        8
    } else if file.contains("drift") {
        9
    } else if file.contains("blur") {
        10
    } else if file.contains("composelayer") || file.contains("fullscreenlayer") {
        11
    } else if file.contains("newproperty5")
        || file.contains("newproperty6")
        || file.contains("userbinding")
        || file.contains("user_binding")
    {
        12
    } else {
        13
    }
}

fn effect_kind_flags(effects: &[SceneEffect]) -> u32 {
    let mut flags = 0u32;
    for effect in effects {
        let kind = effect_kind_code(effect);
        if (1..=32).contains(&kind) {
            flags |= 1u32 << u32::from(kind - 1);
        }
    }
    flags
}

fn effect_evaluation_boundary_code(effect: &SceneEffect) -> u16 {
    match effect_kind_code(effect) {
        2 => 2,
        7 => 3,
        8 | 9 => 4,
        10 | 11 => 5,
        _ => 1,
    }
}

fn material_flag_code(value: Option<&str>) -> u16 {
    match value.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if matches!(value.as_str(), "1" | "true" | "enabled" | "enable" | "on") => 1,
        Some(value)
            if matches!(
                value.as_str(),
                "0" | "false" | "disabled" | "disable" | "off"
            ) =>
        {
            2
        }
        Some(_) | None => 0,
    }
}

fn cull_mode_code(value: Option<&str>) -> u16 {
    match value.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if matches!(value.as_str(), "none" | "off" | "disabled" | "disable") => 1,
        Some(value) if value == "back" => 2,
        Some(value) if value == "front" => 3,
        Some(value) if matches!(value.as_str(), "frontandback" | "front-and-back") => 4,
        Some(value) if value.is_empty() => 0,
        Some(_) => 5,
        None => 0,
    }
}

fn effect_is_first_class_target(effect: &SceneEffect) -> bool {
    let file = effect.file.replace('\\', "/").to_ascii_lowercase();
    effect.runtime.as_deref() == Some("native-iris-mask")
        || file == "effects/iris/effect.json"
        || file.ends_with("/effects/iris/effect.json")
}

#[derive(Debug, Clone, Copy)]
struct SceneBinaryParameterValue {
    kind: u16,
    value_name: u32,
    integer: i64,
    values: [f32; 4],
}

fn scene_binary_parameter_value(
    value: &serde_json::Value,
    names: &mut SceneBinaryNameTable,
) -> Option<SceneBinaryParameterValue> {
    match value {
        serde_json::Value::Bool(value) => Some(SceneBinaryParameterValue {
            kind: SCENE_BINARY_PARAMETER_VALUE_BOOL,
            value_name: SCENE_BINARY_NONE_ID,
            integer: i64::from(*value),
            values: [if *value { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        }),
        serde_json::Value::Number(value) => value.as_f64().map(|value| {
            let integer = value as i64;
            SceneBinaryParameterValue {
                kind: SCENE_BINARY_PARAMETER_VALUE_FLOAT,
                value_name: SCENE_BINARY_NONE_ID,
                integer,
                values: [value as f32, 0.0, 0.0, 0.0],
            }
        }),
        serde_json::Value::String(value) => Some(SceneBinaryParameterValue {
            kind: SCENE_BINARY_PARAMETER_VALUE_STRING,
            value_name: names.intern(SceneBinaryNameKind::ParameterValue, value),
            integer: 0,
            values: [0.0, 0.0, 0.0, 0.0],
        }),
        serde_json::Value::Array(values) => scene_binary_vector_parameter_value(values),
        serde_json::Value::Null | serde_json::Value::Object(_) => None,
    }
}

fn scene_binary_vector_parameter_value(
    values: &[serde_json::Value],
) -> Option<SceneBinaryParameterValue> {
    if values.is_empty() || values.len() > 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (index, value) in values.iter().enumerate() {
        out[index] = value.as_f64()? as f32;
    }
    let kind = match values.len() {
        1 => SCENE_BINARY_PARAMETER_VALUE_FLOAT,
        2 => SCENE_BINARY_PARAMETER_VALUE_VEC2,
        3 => SCENE_BINARY_PARAMETER_VALUE_VEC3,
        4 => SCENE_BINARY_PARAMETER_VALUE_VEC4,
        _ => return None,
    };
    Some(SceneBinaryParameterValue {
        kind,
        value_name: SCENE_BINARY_NONE_ID,
        integer: out[0] as i64,
        values: out,
    })
}

fn effect_parameter_record_count(effect: &SceneEffect) -> u32 {
    let property_count = effect
        .properties
        .values()
        .filter(|value| scene_binary_parameter_value_supported(value))
        .count();
    let pass_parameter_count = effect
        .passes
        .iter()
        .map(|pass| {
            pass.constant_shader_values
                .values()
                .filter(|value| scene_binary_parameter_value_supported(value))
                .count()
                .saturating_add(pass.combos.len())
        })
        .sum::<usize>();
    saturating_u32(property_count.saturating_add(pass_parameter_count))
}

fn effect_uv_transform_record_count(effect: &SceneEffect) -> u32 {
    saturating_u32(
        effect
            .passes
            .iter()
            .filter(|pass| pass.effect_uv_transform.is_some())
            .count(),
    )
}

fn scene_binary_effect_uv_extent(extent: Option<SceneEffectUvExtent>) -> (u32, u32) {
    extent.map_or((0, 0), |extent| (extent.width, extent.height))
}

fn scene_binary_parameter_value_supported(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => true,
        serde_json::Value::Array(values) => {
            !values.is_empty()
                && values.len() <= 4
                && values.iter().all(|value| value.as_f64().is_some())
        }
        serde_json::Value::Null | serde_json::Value::Object(_) => false,
    }
}

fn node_effect_pass_count(effects: &[SceneEffect]) -> u32 {
    saturating_u32(
        effects
            .iter()
            .map(|effect| effect.passes.len().max(1))
            .sum::<usize>(),
    )
}

fn node_effect_texture_slot_count(
    effects: &[SceneEffect],
    base_resource: Option<SceneBinaryResourceBinding<'_>>,
    resource_index: &SceneBinaryResourceIndex<'_>,
) -> u32 {
    let total = effects
        .iter()
        .flat_map(|effect| effect.passes.iter())
        .map(|pass| scene_binary_effect_pass_texture_slot_count(pass, resource_index))
        .fold(0u32, u32::saturating_add);
    let Some(base_resource) = base_resource else {
        return total;
    };
    let Some(first_pass) = effects
        .iter()
        .flat_map(|effect| effect.passes.iter())
        .next()
    else {
        return total;
    };
    total.saturating_sub(u32::from(pass_reuses_base_texture_slot(
        first_pass,
        SceneBinaryBaseTextureSlot {
            record_index: 0,
            resource_index: base_resource.index,
        },
        resource_index,
    )))
}

fn scene_binary_effect_pass_texture_slot_count(
    pass: &SceneEffectPass,
    resource_index: &SceneBinaryResourceIndex<'_>,
) -> u32 {
    let slot_count = pass.textures.len().max(pass.texture_resources.len());
    let mut count = 0u32;
    for slot in 0..slot_count {
        let has_texture_name = pass
            .textures
            .get(slot)
            .and_then(|value| value.as_ref())
            .is_some();
        let has_resource = pass
            .texture_resources
            .get(slot)
            .and_then(|value| value.as_deref())
            .is_some_and(|resource| resource_index.binding(resource).is_some());
        if has_texture_name || has_resource {
            count = count.saturating_add(1);
        }
    }
    count
}

fn pass_reuses_base_texture_slot(
    pass: &SceneEffectPass,
    base_texture_slot: SceneBinaryBaseTextureSlot,
    resource_index: &SceneBinaryResourceIndex<'_>,
) -> bool {
    pass.texture_resources
        .first()
        .and_then(|value| value.as_deref())
        .and_then(|resource| resource_index.binding(resource))
        .is_some_and(|resource| resource.index == base_texture_slot.resource_index)
}

fn node_alpha_texture_state(
    effects: &[SceneEffect],
    resource_index: &SceneBinaryResourceIndex<'_>,
) -> (Option<u32>, SceneAlphaTextureMode) {
    for effect in effects {
        let Some(effect_mode) = super::scene_effect_alpha_texture_mode(effect) else {
            continue;
        };
        for pass in &effect.passes {
            for (slot, resource_id) in pass.texture_resources.iter().enumerate().skip(1) {
                let Some(resource_id) = resource_id.as_deref() else {
                    continue;
                };
                if resource_index.binding(resource_id).is_none() {
                    continue;
                }
                let Ok(slot) = u32::try_from(slot) else {
                    continue;
                };
                return (Some(slot), effect_mode);
            }
        }
    }
    (None, SceneAlphaTextureMode::Multiply)
}

fn effect_pass_texture_slot_count(pass: &SceneEffectPass) -> u32 {
    let slot_count = pass.textures.len().max(pass.texture_resources.len());
    let mut count = 0u32;
    for slot in 0..slot_count {
        if pass
            .textures
            .get(slot)
            .and_then(|value| value.as_ref())
            .is_some()
            || pass
                .texture_resources
                .get(slot)
                .and_then(|value| value.as_ref())
                .is_some()
        {
            count = count.saturating_add(1);
        }
    }
    count
}

fn timeline_channel_bounds(channel: &SceneTimelineChannel) -> (u64, u64, f32, f32) {
    let first = channel.keyframes.first();
    let last = channel.keyframes.last().or(first);
    (
        first.map_or(0, |keyframe| keyframe.time_ms),
        last.map_or(0, |keyframe| keyframe.time_ms),
        first.map_or(0.0, |keyframe| keyframe.value as f32),
        last.map_or(0.0, |keyframe| keyframe.value as f32),
    )
}

fn retained_stable_id(owner_kind: u16, owner_name: u32, record_index: u32) -> u64 {
    (u64::from(owner_kind) << 48) | (u64::from(owner_name) << 16) | u64::from(record_index)
}

pub(crate) fn decode_resource_record(
    bytes: &[u8],
) -> Result<SceneBinaryResourceRecord, SceneBinaryError> {
    Ok(SceneBinaryResourceRecord {
        id_name: read_u32(bytes, 0)?,
        source_name: read_u32(bytes, 4)?,
        original_source_name: read_u32(bytes, 8)?,
        role_name: read_u32(bytes, 12)?,
        kind: read_u16(bytes, 16)?,
        flags: read_u16(bytes, 18)?,
        width: read_u32(bytes, 20)?,
        height: read_u32(bytes, 24)?,
        upload_hints: read_u32(bytes, 28)?,
    })
}

pub(crate) fn decode_node_record(bytes: &[u8]) -> Result<SceneBinaryNodeRecord, SceneBinaryError> {
    Ok(SceneBinaryNodeRecord {
        id_name: read_u32(bytes, 0)?,
        display_name: read_u32(bytes, 4)?,
        parent_index: read_u32(bytes, 8)?,
        resource_name: read_u32(bytes, 12)?,
        kind: read_u16(bytes, 16)?,
        flags: read_u16(bytes, 18)?,
        draw_order: read_u32(bytes, 20)?,
        child_count: read_u32(bytes, 24)?,
        first_child_index: read_u32(bytes, 28)?,
        subtree_node_count: read_u32(bytes, 32)?,
        effect_count: read_u32(bytes, 36)?,
        audio_count: read_u32(bytes, 40)?,
        property_count: read_u32(bytes, 44)?,
        material_index: read_u32(bytes, 48)?,
        geometry_index: read_u32(bytes, 52)?,
        first_transform: read_u32(bytes, 56)?,
        transform_count: read_u32(bytes, 60)?,
        puppet_index: read_u32(bytes, 64)?,
        opacity: read_f32(bytes, 68)?,
        color_rgba: read_u32(bytes, 72)?,
        stroke_color_rgba: read_u32(bytes, 76)?,
        stroke_width: read_f32(bytes, 80)?,
        corner_radius: read_f32(bytes, 84)?,
        fit: read_u16(bytes, 88)?,
    })
}

pub(crate) fn decode_transform_timeline_record(
    bytes: &[u8],
) -> Result<SceneBinaryTransformTimelineRecord, SceneBinaryError> {
    Ok(SceneBinaryTransformTimelineRecord {
        owner_name: read_u32(bytes, 0)?,
        timeline_name: read_u32(bytes, 4)?,
        property: read_u16(bytes, 8)?,
        flags: read_u16(bytes, 10)?,
        keyframe_count: read_u32(bytes, 12)?,
        first_keyframe: read_u32(bytes, 16)?,
        time_offset_ms: read_u64(bytes, 24)?,
        first_time_ms: read_u64(bytes, 32)?,
        last_time_ms: read_u64(bytes, 40)?,
        value0: read_f32(bytes, 48)?,
        value1: read_f32(bytes, 52)?,
        value2: read_f32(bytes, 56)?,
        value3: read_f32(bytes, 60)?,
        value4: read_f32(bytes, 64)?,
        value5: read_f32(bytes, 68)?,
        value6: read_f32(bytes, 72)?,
    })
}

pub(crate) fn decode_transform_keyframe_record(
    bytes: &[u8],
) -> Result<SceneBinaryTransformKeyframeRecord, SceneBinaryError> {
    Ok(SceneBinaryTransformKeyframeRecord {
        time_ms: read_u64(bytes, 0)?,
        value: read_f32(bytes, 8)?,
        curve: read_u16(bytes, 12)?,
        flags: read_u16(bytes, 14)?,
    })
}

pub(crate) fn decode_texture_slot_record(
    bytes: &[u8],
) -> Result<SceneBinaryTextureSlotRecord, SceneBinaryError> {
    Ok(SceneBinaryTextureSlotRecord {
        owner_name: read_u32(bytes, 0)?,
        pass_name: read_u32(bytes, 4)?,
        texture_name: read_u32(bytes, 8)?,
        resource_index: read_u32(bytes, 12)?,
        slot: read_u32(bytes, 16)?,
        width: read_u32(bytes, 20)?,
        height: read_u32(bytes, 24)?,
        role_flags: read_u16(bytes, 28)?,
        sampler_flags: read_u16(bytes, 30)?,
    })
}

pub(crate) fn decode_material_pass_record(
    bytes: &[u8],
) -> Result<SceneBinaryMaterialPassRecord, SceneBinaryError> {
    Ok(SceneBinaryMaterialPassRecord {
        owner_name: read_u32(bytes, 0)?,
        shader_name: read_u32(bytes, 4)?,
        blending_name: read_u32(bytes, 8)?,
        first_texture_slot: read_u32(bytes, 12)?,
        alpha_texture_slot: read_u32(bytes, 16)?,
        first_effect_pass: read_u32(bytes, 20)?,
        pipeline_key: read_u32(bytes, 24)?,
        texture_slot_count: read_u32(bytes, 28)?,
        effect_pass_count: read_u32(bytes, 32)?,
        effect_kind_flags: read_u32(bytes, 36)?,
        material_kind: read_u16(bytes, 40)?,
        descriptor_layout: read_u16(bytes, 42)?,
        blend_mode: read_u16(bytes, 44)?,
        alpha_texture_mode: read_u16(bytes, 46)?,
        depth_test: read_u16(bytes, 48)?,
        depth_write: read_u16(bytes, 50)?,
        cull_mode: read_u16(bytes, 52)?,
        flags: read_u16(bytes, 54)?,
    })
}

pub(crate) fn decode_effect_pass_record(
    bytes: &[u8],
) -> Result<SceneBinaryEffectPassRecord, SceneBinaryError> {
    Ok(SceneBinaryEffectPassRecord {
        owner_name: read_u32(bytes, 0)?,
        effect_name: read_u32(bytes, 4)?,
        shader_name: read_u32(bytes, 8)?,
        blending_name: read_u32(bytes, 12)?,
        pass_index: read_u32(bytes, 16)?,
        first_texture_slot: read_u32(bytes, 20)?,
        texture_slot_count: read_u32(bytes, 24)?,
        first_effect_uv_transform: read_u32(bytes, 28)?,
        effect_uv_transform_count: read_u32(bytes, 32)?,
        first_parameter: read_u32(bytes, 36)?,
        parameter_count: read_u32(bytes, 40)?,
        kind: read_u16(bytes, 44)?,
        evaluation_boundary: read_u16(bytes, 46)?,
        depth_test: read_u16(bytes, 48)?,
        depth_write: read_u16(bytes, 50)?,
        cull_mode: read_u16(bytes, 52)?,
        flags: read_u16(bytes, 54)?,
    })
}

pub(crate) fn decode_effect_parameter_record(
    bytes: &[u8],
) -> Result<SceneBinaryEffectParameterRecord, SceneBinaryError> {
    Ok(SceneBinaryEffectParameterRecord {
        owner_name: read_u32(bytes, 0)?,
        effect_name: read_u32(bytes, 4)?,
        parameter_name: read_u32(bytes, 8)?,
        value_name: read_u32(bytes, 12)?,
        pass_index: read_u32(bytes, 16)?,
        value_kind: read_u16(bytes, 20)?,
        role_flags: read_u16(bytes, 22)?,
        integer_value: read_i64(bytes, 24)?,
        value0: read_f32(bytes, 32)?,
        value1: read_f32(bytes, 36)?,
        value2: read_f32(bytes, 40)?,
        value3: read_f32(bytes, 44)?,
    })
}

pub(crate) fn decode_render_state_record(
    bytes: &[u8],
) -> Result<SceneBinaryRenderStateRecord, SceneBinaryError> {
    Ok(SceneBinaryRenderStateRecord {
        width: read_u32(bytes, 0)?,
        height: read_u32(bytes, 4)?,
        resource_count: read_u32(bytes, 8)?,
        node_count: read_u32(bytes, 12)?,
        material_count: read_u32(bytes, 16)?,
        effect_count: read_u32(bytes, 20)?,
        flags: read_u32(bytes, 24)?,
        texture_slot_count: read_u32(bytes, 28)?,
    })
}

pub(crate) fn decode_retained_gpu_state_record(
    bytes: &[u8],
) -> Result<SceneBinaryRetainedGpuStateRecord, SceneBinaryError> {
    Ok(SceneBinaryRetainedGpuStateRecord {
        owner_kind: read_u16(bytes, 0)?,
        flags: read_u16(bytes, 2)?,
        owner_name: read_u32(bytes, 4)?,
        stable_id: read_u64(bytes, 8)?,
        record_index: read_u32(bytes, 16)?,
        dirty_range_count: read_u32(bytes, 20)?,
    })
}

pub(crate) fn decode_debug_name_record(
    bytes: &[u8],
) -> Result<SceneBinaryDebugNameRecord, SceneBinaryError> {
    Ok(SceneBinaryDebugNameRecord {
        id: read_u32(bytes, 0)?,
        kind: read_u32(bytes, 4)?,
        offset: read_u32(bytes, 8)?,
        length: read_u32(bytes, 12)?,
    })
}

fn validate_required_payload_order(
    payloads: &[SceneBinaryChunkPayload<'_>],
) -> Result<(), SceneBinaryError> {
    if payloads.len() != SceneBinaryChunkKind::REQUIRED_ORDER.len() {
        return Err(SceneBinaryError::RequiredChunkCount {
            expected: SceneBinaryChunkKind::REQUIRED_ORDER.len(),
            actual: payloads.len(),
        });
    }
    let mut seen = BTreeSet::new();
    for (index, payload) in payloads.iter().enumerate() {
        let expected = SceneBinaryChunkKind::REQUIRED_ORDER[index];
        if payload.kind != expected {
            return Err(SceneBinaryError::InvalidChunkOrder {
                index,
                expected,
                actual: payload.kind,
            });
        }
        if !seen.insert(payload.kind) {
            return Err(SceneBinaryError::DuplicateChunk { kind: payload.kind });
        }
    }
    Ok(())
}

fn write_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_i64(bytes: &mut Vec<u8>, value: i64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_f32(bytes: &mut Vec<u8>, value: f32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_header(bytes: &mut Vec<u8>, feature_flags: u32, chunk_count: u32) {
    bytes.extend_from_slice(&SCENE_BINARY_MAGIC);
    bytes.extend_from_slice(&SCENE_BINARY_VERSION.to_le_bytes());
    bytes.push(SCENE_BINARY_ENDIAN_LITTLE);
    bytes.push(SCENE_BINARY_ALIGNMENT);
    bytes.extend_from_slice(&feature_flags.to_le_bytes());
    bytes.extend_from_slice(&chunk_count.to_le_bytes());
    bytes.extend_from_slice(&(SCENE_BINARY_HEADER_SIZE as u64).to_le_bytes());
}

fn write_chunk_descriptor(bytes: &mut Vec<u8>, descriptor: &SceneBinaryChunkDescriptor) {
    bytes.extend_from_slice(&descriptor.kind.code().to_le_bytes());
    bytes.extend_from_slice(&descriptor.record_count.to_le_bytes());
    bytes.extend_from_slice(&descriptor.offset.to_le_bytes());
    bytes.extend_from_slice(&descriptor.length.to_le_bytes());
}

fn read_chunk_descriptor(
    bytes: &[u8],
    offset: usize,
) -> Result<SceneBinaryChunkDescriptor, SceneBinaryError> {
    let code = read_u32(bytes, offset)?;
    let kind =
        SceneBinaryChunkKind::from_code(code).ok_or(SceneBinaryError::UnknownChunk { code })?;
    Ok(SceneBinaryChunkDescriptor {
        kind,
        record_count: read_u32(bytes, offset + 4)?,
        offset: read_u64(bytes, offset + 8)?,
        length: read_u64(bytes, offset + 16)?,
    })
}

fn validate_chunk_bounds(
    container_len: usize,
    alignment: u8,
    payload_min_offset: usize,
    previous: Option<&SceneBinaryChunkDescriptor>,
    chunk: &SceneBinaryChunkDescriptor,
) -> Result<(), SceneBinaryError> {
    if chunk.offset % u64::from(alignment) != 0 {
        return Err(SceneBinaryError::MisalignedChunk {
            kind: chunk.kind,
            offset: chunk.offset,
            alignment,
        });
    }
    let start = usize::try_from(chunk.offset).map_err(|_| SceneBinaryError::ChunkOutOfBounds {
        kind: chunk.kind,
        offset: chunk.offset,
        length: chunk.length,
        container_len,
    })?;
    let length = usize::try_from(chunk.length).map_err(|_| SceneBinaryError::ChunkOutOfBounds {
        kind: chunk.kind,
        offset: chunk.offset,
        length: chunk.length,
        container_len,
    })?;
    let end = start
        .checked_add(length)
        .ok_or(SceneBinaryError::ChunkOutOfBounds {
            kind: chunk.kind,
            offset: chunk.offset,
            length: chunk.length,
            container_len,
        })?;
    if start < payload_min_offset || end > container_len {
        return Err(SceneBinaryError::ChunkOutOfBounds {
            kind: chunk.kind,
            offset: chunk.offset,
            length: chunk.length,
            container_len,
        });
    }
    if let Some(previous) = previous {
        let previous_end = usize::try_from(previous.offset)
            .ok()
            .and_then(|offset| offset.checked_add(usize::try_from(previous.length).ok()?))
            .ok_or(SceneBinaryError::ChunkOutOfBounds {
                kind: previous.kind,
                offset: previous.offset,
                length: previous.length,
                container_len,
            })?;
        if start < previous_end {
            return Err(SceneBinaryError::ChunkOverlap {
                previous: previous.kind,
                current: chunk.kind,
            });
        }
    }
    Ok(())
}

fn read_array_4(bytes: &[u8], offset: usize) -> Result<[u8; 4], SceneBinaryError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or(SceneBinaryError::BufferTooSmall {
            needed: offset + 4,
            actual: bytes.len(),
        })?;
    Ok([slice[0], slice[1], slice[2], slice[3]])
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, SceneBinaryError> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or(SceneBinaryError::BufferTooSmall {
            needed: offset + 2,
            actual: bytes.len(),
        })?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, SceneBinaryError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or(SceneBinaryError::BufferTooSmall {
            needed: offset + 4,
            actual: bytes.len(),
        })?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, SceneBinaryError> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or(SceneBinaryError::BufferTooSmall {
            needed: offset + 8,
            actual: bytes.len(),
        })?;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

fn read_i64(bytes: &[u8], offset: usize) -> Result<i64, SceneBinaryError> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or(SceneBinaryError::BufferTooSmall {
            needed: offset + 8,
            actual: bytes.len(),
        })?;
    Ok(i64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, SceneBinaryError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or(SceneBinaryError::BufferTooSmall {
            needed: offset + 4,
            actual: bytes.len(),
        })?;
    Ok(f32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn align_usize(value: usize, alignment: usize) -> usize {
    debug_assert!(alignment.is_power_of_two());
    (value + alignment - 1) & !(alignment - 1)
}

fn node_has_material(node: &SceneNode) -> bool {
    node_has_geometry(node) || node.resource.is_some() || !node.effects.is_empty()
}

fn node_first_effect_pass_reuses_base_resource(node: &SceneNode) -> bool {
    let Some(base_resource) = node.resource.as_deref() else {
        return false;
    };
    node.effects
        .iter()
        .flat_map(|effect| effect.passes.iter())
        .next()
        .and_then(|pass| pass.texture_resources.first())
        .and_then(|value| value.as_deref())
        == Some(base_resource)
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

fn binary_range_start_count(first_record: u32, record_count: u32) -> (u32, u32) {
    if first_record == SCENE_BINARY_NONE_ID && record_count == 0 {
        (0, 0)
    } else {
        (first_record, record_count)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;
    use crate::core::path::PackagePath;
    use crate::core::scene::{
        SceneAudioCue, SceneCamera, SceneEffectPass, SceneImportMetadata, SceneNativeLowering,
        SceneNodeProvenance, ScenePathFillRule, SceneProfile, ScenePropertyBinding,
        SceneRenderSettings, SceneResource, SceneResourceKind, SceneSourceMetadata, SceneSystems,
        SceneTimeline, SceneTimelineChannel, SceneTransform,
    };
    use crate::core::{FitMode, SceneAnimatedProperty, SceneKeyframe};

    #[test]
    fn binary_container_round_trips_required_typed_chunks() {
        let payloads = SceneBinaryChunkKind::REQUIRED_ORDER
            .into_iter()
            .enumerate()
            .map(|(index, kind)| SceneBinaryChunkPayload {
                kind,
                record_count: index as u32,
                bytes: if kind == SceneBinaryChunkKind::ResourceTable {
                    &[1, 2, 3][..]
                } else {
                    &[][..]
                },
            })
            .collect::<Vec<_>>();

        let bytes = encode_scene_binary_container(0x10, &payloads).expect("encode");
        let layout = decode_scene_binary_container(&bytes).expect("decode");

        assert_eq!(&bytes[0..4], &SCENE_BINARY_MAGIC);
        assert_eq!(layout.feature_flags, 0x10);
        assert_eq!(
            layout.chunks.len(),
            SceneBinaryChunkKind::REQUIRED_ORDER.len()
        );
        let resource = layout
            .chunk(SceneBinaryChunkKind::ResourceTable)
            .expect("resource table chunk");
        assert_eq!(resource.record_count, 0);
        assert_eq!(
            resource.payload(&bytes).expect("resource payload"),
            &[1, 2, 3]
        );
        for chunk in &layout.chunks {
            assert_eq!(chunk.offset % u64::from(SCENE_BINARY_ALIGNMENT), 0);
        }
    }

    #[test]
    fn binary_container_rejects_missing_required_chunk_family() {
        let payloads = SceneBinaryChunkKind::REQUIRED_ORDER
            .into_iter()
            .take(SceneBinaryChunkKind::REQUIRED_ORDER.len() - 1)
            .map(|kind| SceneBinaryChunkPayload {
                kind,
                record_count: 0,
                bytes: &[],
            })
            .collect::<Vec<_>>();

        assert!(matches!(
            encode_scene_binary_container(0, &payloads),
            Err(SceneBinaryError::RequiredChunkCount { .. })
        ));
    }

    #[test]
    fn document_shape_counts_binary_chunks_without_json_payload_copy() {
        let document = SceneDocument {
            version: SCENE_BINARY_VERSION as u32,
            profile: SceneProfile::NativeVulkanFullScene,
            source: SceneSourceMetadata::default(),
            size: None,
            render: SceneRenderSettings::default(),
            camera: SceneCamera::default(),
            import: SceneImportMetadata::default(),
            properties: BTreeMap::new(),
            resources: vec![
                SceneResource {
                    id: "image".to_owned(),
                    kind: SceneResourceKind::Image,
                    source: PackagePath::new("assets/image.gtex").unwrap(),
                    width: Some(64),
                    height: Some(64),
                    original_source: None,
                    role: None,
                },
                SceneResource {
                    id: "effect".to_owned(),
                    kind: SceneResourceKind::Effect,
                    source: PackagePath::new("effects/flutter/effect.json").unwrap(),
                    width: None,
                    height: None,
                    original_source: None,
                    role: None,
                },
            ],
            nodes: vec![SceneNode {
                id: "hair".to_owned(),
                kind: SceneNodeKind::Image,
                name: Some("Hair".to_owned()),
                visible: true,
                opacity: 1.0,
                transform: SceneTransform::default(),
                provenance: Option::<SceneNodeProvenance>::None,
                resource: Some("image".to_owned()),
                effects: vec![SceneEffect {
                    file: "effects/flutter/effect.json".to_owned(),
                    resource: Some("effect".to_owned()),
                    properties: BTreeMap::from([("phase".to_owned(), json!(0.25))]),
                    passes: vec![SceneEffectPass {
                        shader: Some("effects/flutter".to_owned()),
                        blending: Some("additive".to_owned()),
                        depthtest: Some("false".to_owned()),
                        depthwrite: Some("false".to_owned()),
                        cullmode: Some("none".to_owned()),
                        textures: vec![Some("g_Texture0".to_owned())],
                        texture_resources: vec![Some("image".to_owned())],
                        combos: BTreeMap::from([("WIND_MODE".to_owned(), 2)]),
                        constant_shader_values: BTreeMap::from([
                            ("speed".to_owned(), json!(1.0)),
                            ("wind".to_owned(), json!([1.0, 0.0])),
                        ]),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                audio: Vec::<SceneAudioCue>::new(),
                color: None,
                stroke_color: None,
                stroke_width: None,
                corner_radius: None,
                width: Some(64.0),
                height: Some(64.0),
                mesh: None,
                puppet_animation_layers: Vec::new(),
                puppet_attachment: None,
                parallax_depth: None,
                text: None,
                font_size: None,
                font_family: None,
                font_resource: None,
                font_weight: None,
                text_align: None,
                path_data: None,
                path_fill_rule: ScenePathFillRule::default(),
                fit: FitMode::Cover,
                properties: BTreeMap::from([(
                    "wallpaper_engine_blend".to_owned(),
                    json!({ "colorBlendMode": 7 }),
                )]),
                children: Vec::new(),
            }],
            timelines: vec![SceneTimeline {
                id: "hair-x".to_owned(),
                target_node: Some("hair".to_owned()),
                channels: vec![SceneTimelineChannel {
                    property: SceneAnimatedProperty::X,
                    loop_playback: true,
                    time_offset_ms: 0,
                    keyframes: vec![SceneKeyframe {
                        time_ms: 0,
                        value: 0.0,
                        curve: Default::default(),
                    }],
                }],
            }],
            property_bindings: Vec::<ScenePropertyBinding>::new(),
            systems: SceneSystems::default(),
            native_lowering: SceneNativeLowering::default(),
            unsupported_features: Vec::new(),
        };

        let payloads = scene_binary_payloads_from_document(&document);
        let shape = payloads.shape;
        assert_eq!(shape.resource_table_records, 2);
        assert_eq!(shape.node_table_records, 1);
        assert_eq!(shape.transform_timeline_records, 2);
        assert_eq!(shape.transform_keyframe_records, 1);
        assert_eq!(shape.geometry_records, 1);
        assert_eq!(shape.geometry_vertex_records, 0);
        assert_eq!(shape.geometry_index_records, 0);
        assert_eq!(shape.texture_slot_records, 1);
        assert_eq!(shape.material_pass_records, 1);
        assert_eq!(shape.effect_pass_records, 1);
        assert_eq!(shape.effect_parameter_records, 4);
        assert_eq!(shape.flutter_state_records, 1);
        assert_eq!(shape.render_state_records, 1);
        assert_eq!(
            payloads
                .chunk(SceneBinaryChunkKind::ResourceTable)
                .expect("resource payload")
                .bytes
                .len(),
            2 * SCENE_BINARY_RESOURCE_RECORD_SIZE
        );
        assert_eq!(
            payloads
                .chunk(SceneBinaryChunkKind::TextureSlots)
                .expect("texture slot payload")
                .bytes
                .len(),
            SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE
        );
        assert!(
            payloads
                .chunk(SceneBinaryChunkKind::DebugNames)
                .expect("debug names")
                .bytes
                .len()
                > shape.debug_name_records as usize * SCENE_BINARY_DEBUG_NAME_RECORD_SIZE
        );

        let bytes = payloads
            .encode_container(0)
            .expect("encode document chunks");
        assert!(
            !bytes
                .windows("constant_shader_values".len())
                .any(|window| window == b"constant_shader_values")
        );
        let layout = decode_scene_binary_container(&bytes).expect("decode document chunks");
        assert_eq!(
            layout
                .chunk(SceneBinaryChunkKind::TextureSlots)
                .expect("texture slot chunk")
                .record_count,
            1
        );
        let debug_names = layout.debug_names(&bytes).expect("debug names");
        let resources = layout
            .resource_records(&bytes)
            .expect("resource records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded resource records");
        assert_eq!(resources.len(), 2);
        assert_eq!(
            resources[0].kind,
            resource_kind_code(SceneResourceKind::Image)
        );
        assert_eq!(resources[0].width, 64);
        assert_eq!(resources[0].height, 64);
        assert_eq!(
            debug_names.name(resources[0].id_name).expect("image id"),
            Some("image")
        );
        assert_eq!(
            debug_names
                .name(resources[1].source_name)
                .expect("effect source"),
            Some("effects/flutter/effect.json")
        );

        let nodes = layout
            .node_records(&bytes)
            .expect("node records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded node records");
        assert_eq!(nodes.len(), 1);
        assert_eq!(debug_names.name(nodes[0].id_name).unwrap(), Some("hair"));
        assert_eq!(nodes[0].child_count, 0);
        assert_eq!(nodes[0].first_child_index, SCENE_BINARY_NONE_ID);
        assert_eq!(nodes[0].subtree_node_count, 1);
        assert_eq!(nodes[0].first_transform, 0);
        assert_eq!(nodes[0].transform_count, 2);
        assert_eq!(nodes[0].effect_count, 1);
        assert_ne!(nodes[0].material_index, SCENE_BINARY_NONE_ID);
        assert_ne!(nodes[0].geometry_index, SCENE_BINARY_NONE_ID);
        assert_eq!(nodes[0].puppet_index, SCENE_BINARY_NONE_ID);

        let transforms = layout
            .node_transform_records(&bytes, nodes[0])
            .expect("node transform range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded node transforms");
        assert_eq!(transforms.len(), 2);
        assert_eq!(
            transforms[0].property,
            SCENE_BINARY_DEFAULT_TRANSFORM_PROPERTY
        );
        assert_eq!(transforms[0].first_keyframe, SCENE_BINARY_NONE_ID);
        assert_eq!(
            transforms[1].property,
            animated_property_code(SceneAnimatedProperty::X)
        );
        assert_eq!(transforms[1].first_keyframe, 0);
        assert_eq!(transforms[1].keyframe_count, 1);
        let keyframes = layout
            .transform_keyframe_record_range(&bytes, transforms[1])
            .expect("transform keyframe range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded transform keyframes");
        assert_eq!(keyframes.len(), 1);
        assert_eq!(keyframes[0].time_ms, 0);
        assert_eq!(keyframes[0].value, 0.0);
        assert_eq!(keyframes[0].curve, curve_code(Default::default()));

        let geometry = layout
            .geometry_records(&bytes)
            .expect("geometry records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded geometry records");
        assert_eq!(geometry.len(), 1);
        assert_eq!(geometry[0].first_vertex, SCENE_BINARY_NONE_ID);
        assert_eq!(
            geometry[0].vertex_count,
            SCENE_BINARY_GEOMETRY_QUAD_VERTEX_COUNT
        );
        assert_eq!(geometry[0].first_index, SCENE_BINARY_NONE_ID);
        assert_eq!(
            geometry[0].index_count,
            SCENE_BINARY_GEOMETRY_QUAD_INDEX_COUNT
        );
        assert_eq!(geometry[0].material_uv_count, 1);
        assert_eq!(
            geometry[0].primitive_kind,
            SCENE_BINARY_GEOMETRY_PRIMITIVE_QUAD
        );
        assert_eq!(
            geometry[0].vertex_layout,
            SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_GENERATED
        );
        assert_eq!(geometry[0].bounds_min_x, 0.0);
        assert_eq!(geometry[0].bounds_min_y, 0.0);
        assert_eq!(geometry[0].bounds_max_x, 64.0);
        assert_eq!(geometry[0].bounds_max_y, 64.0);
        assert_eq!(geometry[0].uv_min_u, 0.0);
        assert_eq!(geometry[0].uv_min_v, 0.0);
        assert_eq!(geometry[0].uv_max_u, 1.0);
        assert_eq!(geometry[0].uv_max_v, 1.0);
        assert_eq!(
            layout
                .geometry_vertex_record_range(&bytes, geometry[0])
                .expect("empty geometry vertex range")
                .len(),
            0
        );
        assert_eq!(
            layout
                .geometry_index_record_range(&bytes, geometry[0])
                .expect("empty geometry index range")
                .len(),
            0
        );

        let materials = layout
            .material_pass_records(&bytes)
            .expect("material records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded material records");
        assert_eq!(materials.len(), 1);
        let material_texture_slots = layout
            .material_texture_slot_records(&bytes, materials[0])
            .expect("material texture slot range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded material texture slot range");
        assert_eq!(material_texture_slots.len(), 1);
        assert_eq!(material_texture_slots[0].resource_index, 0);
        assert_eq!(
            debug_names
                .name(resources[material_texture_slots[0].resource_index as usize].id_name)
                .expect("material texture resource"),
            Some("image")
        );
        assert_eq!(
            debug_names
                .name(materials[0].shader_name)
                .expect("material shader"),
            Some("effects/flutter")
        );
        assert_eq!(
            debug_names
                .name(materials[0].blending_name)
                .expect("material blending"),
            Some("additive")
        );
        assert_eq!(materials[0].texture_slot_count, 1);
        assert_eq!(materials[0].effect_pass_count, 1);
        assert_eq!(materials[0].first_effect_pass, 0);
        assert_eq!(
            materials[0].blend_mode,
            blend_mode_code(SceneBlendMode::Max)
        );
        assert_eq!(materials[0].depth_test, material_flag_code(Some("false")));
        assert_eq!(materials[0].depth_write, material_flag_code(Some("false")));
        assert_eq!(materials[0].cull_mode, cull_mode_code(Some("none")));
        assert_eq!(materials[0].effect_kind_flags, 1 << (8 - 1));
        assert_ne!(materials[0].pipeline_key, 0);
        let material_effect_passes = layout
            .material_effect_pass_records(&bytes, materials[0])
            .expect("material effect pass range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded material effect pass range");
        assert_eq!(material_effect_passes.len(), 1);

        let transforms = layout
            .transform_timeline_records(&bytes)
            .expect("transform records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded transform records");
        assert_eq!(transforms.len(), 2);
        assert!(
            transforms
                .iter()
                .any(|record| record.property == SCENE_BINARY_DEFAULT_TRANSFORM_PROPERTY)
        );
        assert!(
            transforms
                .iter()
                .any(|record| record.property == animated_property_code(SceneAnimatedProperty::X))
        );

        let texture_slots = layout
            .texture_slot_records(&bytes)
            .expect("texture slot records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded texture slot records");
        assert_eq!(texture_slots.len(), 1);
        assert_eq!(texture_slots[0].slot, 0);
        assert_eq!(texture_slots[0].resource_index, 0);

        let effect_passes = layout
            .effect_pass_records(&bytes)
            .expect("effect pass records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded effect pass records");
        assert_eq!(effect_passes.len(), 1);
        assert_eq!(material_effect_passes[0], effect_passes[0]);
        assert_eq!(effect_passes[0].texture_slot_count, 1);
        assert_eq!(effect_passes[0].first_texture_slot, 0);
        assert_eq!(effect_passes[0].first_parameter, 1);
        assert_eq!(effect_passes[0].parameter_count, 3);
        assert_eq!(
            effect_passes[0].kind,
            effect_kind_code(&document.nodes[0].effects[0])
        );
        assert_eq!(effect_passes[0].evaluation_boundary, 4);
        assert_eq!(
            effect_passes[0].depth_test,
            material_flag_code(Some("false"))
        );
        assert_eq!(
            effect_passes[0].depth_write,
            material_flag_code(Some("false"))
        );
        assert_eq!(effect_passes[0].cull_mode, cull_mode_code(Some("none")));
        let effect_texture_slots = layout
            .effect_texture_slot_records(&bytes, effect_passes[0])
            .expect("effect texture slot range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded effect texture slot range");
        assert_eq!(effect_texture_slots.len(), 1);
        assert_eq!(effect_texture_slots[0].resource_index, 0);

        let parameters = layout
            .effect_parameter_records(&bytes)
            .expect("effect parameter records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded effect parameter records");
        assert_eq!(parameters.len(), 4);
        assert_eq!(
            debug_names
                .name(parameters[0].parameter_name)
                .expect("effect property name"),
            Some("phase")
        );
        assert_eq!(
            parameters[0].role_flags,
            SCENE_BINARY_PARAMETER_ROLE_EFFECT_PROPERTY
        );
        assert!((parameters[0].value0 - 0.25).abs() < f32::EPSILON);
        assert_eq!(
            debug_names
                .name(parameters[2].parameter_name)
                .expect("wind parameter name"),
            Some("wind")
        );
        assert_eq!(parameters[2].value_kind, SCENE_BINARY_PARAMETER_VALUE_VEC2);
        assert_eq!(parameters[2].value0, 1.0);
        assert_eq!(parameters[2].value1, 0.0);
        assert_eq!(
            debug_names
                .name(parameters[3].parameter_name)
                .expect("combo parameter name"),
            Some("WIND_MODE")
        );
        assert_eq!(
            parameters[3].role_flags,
            SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO
        );
        assert_eq!(parameters[3].integer_value, 2);
        let pass_parameters = layout
            .effect_parameter_record_range(&bytes, effect_passes[0])
            .expect("effect pass parameter range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded effect pass parameter range");
        assert_eq!(pass_parameters.len(), 3);
        assert_eq!(
            debug_names
                .name(pass_parameters[0].parameter_name)
                .expect("first pass parameter"),
            Some("speed")
        );
        let mut bad_effect_pass = effect_passes[0];
        bad_effect_pass.first_parameter = shape.effect_parameter_records;
        bad_effect_pass.parameter_count = 1;
        assert!(matches!(
            layout.effect_parameter_record_range(&bytes, bad_effect_pass),
            Err(SceneBinaryError::RecordRangeOutOfBounds { .. })
        ));

        let flutter = layout
            .flutter_state_records(&bytes)
            .expect("flutter records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded flutter records");
        assert_eq!(flutter.len(), 1);
        assert_eq!(flutter[0].pass_count, 1);
        assert_eq!(flutter[0].first_parameter, 0);
        assert_eq!(flutter[0].parameter_count, 4);
        assert_eq!(
            flutter[0].motion_family_mask,
            SCENE_BINARY_MOTION_FAMILY_FLUTTER
        );
        assert_eq!(flutter[0].anchor_name, nodes[0].id_name);
        assert_eq!(flutter[0].dirty_range_count, 3);
        let flutter_parameters = layout
            .flutter_parameter_records(&bytes, flutter[0])
            .expect("flutter parameter range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded flutter parameter range");
        assert_eq!(flutter_parameters.len(), 4);
        assert_eq!(flutter_parameters[0].role_flags, parameters[0].role_flags);

        let render_state = layout
            .render_state_records(&bytes)
            .expect("render state records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded render records");
        assert_eq!(render_state.len(), 1);
        assert_eq!(render_state[0].resource_count, 2);
        assert_eq!(render_state[0].node_count, 1);
        assert_eq!(render_state[0].effect_count, 1);
        assert_eq!(render_state[0].texture_slot_count, 1);

        let retained = layout
            .retained_gpu_state_records(&bytes)
            .expect("retained records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded retained records");
        assert_eq!(retained.len() as u32, shape.retained_gpu_state_records);
        assert!(
            retained
                .iter()
                .any(|record| record.owner_kind == SCENE_BINARY_RETAINED_EFFECT_PASS)
        );
        assert!(
            retained
                .iter()
                .any(|record| record.owner_kind == SCENE_BINARY_RETAINED_GEOMETRY)
        );
    }

    #[test]
    fn binary_node_table_carries_subtree_and_runtime_record_indices() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "root",
                    "type": "group",
                    "children": [
                        {
                            "id": "mesh-child",
                            "type": "image",
                            "opacity": 0.5,
                            "color": "#112233",
                            "stroke_color": "#445566",
                            "stroke_width": 2.5,
                            "corner_radius": 3.5,
                            "fit": "contain",
                            "mesh": {
                                "vertices": [
                                    { "x": 0.0, "y": 0.0, "u": 0.0, "v": 0.0 },
                                    { "x": 1.0, "y": 0.0, "u": 1.0, "v": 0.0 },
                                    { "x": 0.0, "y": 1.0, "u": 0.0, "v": 1.0 }
                                ],
                                "indices": [0, 1, 2]
                            },
                            "children": [
                                { "id": "grandchild", "type": "rectangle", "width": 4.0, "height": 8.0 }
                            ]
                        },
                        { "id": "sibling", "type": "rectangle", "width": 2.0, "height": 2.0 }
                    ]
                }
            ]
        }))
        .expect("scene document");

        let bytes = scene_binary_payloads_from_document(&document)
            .encode_container(0)
            .expect("encode");
        let layout = decode_scene_binary_container(&bytes).expect("decode");
        let nodes = layout
            .node_records(&bytes)
            .expect("node records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded node records");

        assert_eq!(nodes.len(), 4);
        assert_eq!(nodes[0].parent_index, SCENE_BINARY_NONE_ID);
        assert_eq!(nodes[0].child_count, 2);
        assert_eq!(nodes[0].first_child_index, 1);
        assert_eq!(nodes[0].subtree_node_count, 4);
        assert_eq!(nodes[1].parent_index, 0);
        assert_eq!(nodes[1].child_count, 1);
        assert_eq!(nodes[1].first_child_index, 2);
        assert_eq!(nodes[1].subtree_node_count, 2);
        assert_ne!(nodes[1].geometry_index, SCENE_BINARY_NONE_ID);
        assert_ne!(nodes[1].material_index, SCENE_BINARY_NONE_ID);
        assert_ne!(nodes[1].puppet_index, SCENE_BINARY_NONE_ID);
        assert_eq!(nodes[1].opacity, 0.5);
        assert_eq!(nodes[1].color_rgba, 0x112233ff);
        assert_eq!(nodes[1].stroke_color_rgba, 0x445566ff);
        assert_eq!(nodes[1].stroke_width, 2.5);
        assert_eq!(nodes[1].corner_radius, 3.5);
        assert_eq!(nodes[1].fit, fit_code(FitMode::Contain));
        assert_eq!(nodes[2].parent_index, 1);
        assert_eq!(nodes[3].parent_index, 0);
        for node in &nodes {
            assert_ne!(node.first_transform, SCENE_BINARY_NONE_ID);
            assert_eq!(node.transform_count, 1);
            assert_eq!(
                layout
                    .node_transform_records(&bytes, *node)
                    .expect("node transform range")
                    .len(),
                1
            );
        }
        assert_eq!(
            layout
                .puppet_record_at(&bytes, nodes[1].puppet_index)
                .expect("puppet record")
                .vertex_count,
            3
        );
    }

    #[test]
    fn binary_geometry_streams_carry_mesh_vertices_and_indices() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "mesh-node",
                    "type": "image",
                    "mesh": {
                        "vertices": [
                            { "x": -2.0, "y": 1.0, "u": 0.25, "v": 0.75, "opacity": 0.5 },
                            { "x": 4.0, "y": -3.0, "u": 1.0, "v": 0.0 },
                            { "x": 2.0, "y": 5.0, "u": 0.0, "v": 1.0 }
                        ],
                        "indices": [2, 1, 0]
                    }
                }
            ]
        }))
        .expect("scene document");

        let payloads = scene_binary_payloads_from_document(&document);
        assert_eq!(payloads.shape.geometry_records, 1);
        assert_eq!(payloads.shape.geometry_vertex_records, 3);
        assert_eq!(payloads.shape.geometry_index_records, 3);
        assert_eq!(
            payloads
                .chunk(SceneBinaryChunkKind::GeometryVertices)
                .expect("geometry vertex payload")
                .bytes
                .len(),
            3 * SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE
        );
        assert_eq!(
            payloads
                .chunk(SceneBinaryChunkKind::GeometryIndices)
                .expect("geometry index payload")
                .bytes
                .len(),
            3 * SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE
        );

        let bytes = payloads.encode_container(0).expect("encode");
        let layout = decode_scene_binary_container(&bytes).expect("decode");
        let geometry = layout
            .geometry_records(&bytes)
            .expect("geometry records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded geometry");
        assert_eq!(geometry.len(), 1);
        assert_eq!(geometry[0].first_vertex, 0);
        assert_eq!(geometry[0].vertex_count, 3);
        assert_eq!(geometry[0].first_index, 0);
        assert_eq!(geometry[0].index_count, 3);
        assert_eq!(
            geometry[0].primitive_kind,
            SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH
        );
        assert_eq!(
            geometry[0].vertex_layout,
            SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY
        );
        assert_eq!(geometry[0].bounds_min_x, -2.0);
        assert_eq!(geometry[0].bounds_min_y, -3.0);
        assert_eq!(geometry[0].bounds_max_x, 4.0);
        assert_eq!(geometry[0].bounds_max_y, 5.0);
        assert_eq!(geometry[0].uv_min_u, 0.0);
        assert_eq!(geometry[0].uv_min_v, 0.0);
        assert_eq!(geometry[0].uv_max_u, 1.0);
        assert_eq!(geometry[0].uv_max_v, 1.0);

        let vertices = layout
            .geometry_vertex_record_range(&bytes, geometry[0])
            .expect("geometry vertex range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded geometry vertices");
        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[0].x, -2.0);
        assert_eq!(vertices[0].y, 1.0);
        assert_eq!(vertices[0].u, 0.25);
        assert_eq!(vertices[0].v, 0.75);
        assert_eq!(vertices[0].opacity, 0.5);
        assert_eq!(vertices[1].opacity, 1.0);

        let indices = layout
            .geometry_index_record_range(&bytes, geometry[0])
            .expect("geometry index range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded geometry indices");
        assert_eq!(
            indices
                .iter()
                .map(|record| record.index)
                .collect::<Vec<_>>(),
            vec![2, 1, 0]
        );
    }

    #[test]
    fn binary_puppet_payload_carries_skin_clips_and_layers() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "eye",
                    "type": "image",
                    "mesh": {
                        "vertices": [
                            { "x": 0.0, "y": 0.0, "u": 0.0, "v": 0.0 },
                            { "x": 2.0, "y": 0.0, "u": 1.0, "v": 0.0 },
                            { "x": 0.0, "y": 2.0, "u": 0.0, "v": 1.0 }
                        ],
                        "indices": [0, 1, 2],
                        "skin": {
                            "bones": [
                                { "bind": { "translation": [0.0, 0.0, 0.0] } },
                                { "parent": 0, "bind": { "translation": [1.0, 0.0, 0.0] } }
                            ],
                            "vertices": [
                                { "bone_indices": [0, 1, 0, 0], "weights": [0.25, 0.75, 0.0, 0.0] },
                                { "bone_indices": [1, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] },
                                { "bone_indices": [0, 0, 0, 0], "weights": [1.0, 0.0, 0.0, 0.0] }
                            ],
                            "attachments": [
                                {
                                    "name": "socket",
                                    "bone_index": 1,
                                    "local_position": [1.0, 2.0, 0.0],
                                    "bind_position": [2.0, 2.0, 0.0]
                                }
                            ]
                        },
                        "puppet_clips": [
                            {
                                "id": 7,
                                "name": "blink",
                                "fps": 30.0,
                                "frame_count": 2,
                                "looping": true,
                                "bones": [
                                    {
                                        "frames": [
                                            { "translation": [0.0, 0.0, 0.0] },
                                            { "translation": [0.0, 1.0, 0.0] }
                                        ]
                                    },
                                    {
                                        "frames": [
                                            { "translation": [1.0, 0.0, 0.0], "opacity": 1.0 },
                                            { "translation": [1.0, 1.0, 0.0], "opacity": 0.25 }
                                        ]
                                    }
                                ]
                            }
                        ]
                    },
                    "puppet_animation_layers": [
                        {
                            "clip_id": 7,
                            "name": "blink-layer",
                            "blend": 0.75,
                            "rate": 1.25,
                            "initial_phase": 0.5,
                            "additive": true,
                            "lock_transforms": true
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");

        let payloads = scene_binary_payloads_from_document(&document);
        assert_eq!(payloads.shape.puppet_records, 1);
        assert_eq!(payloads.shape.puppet_skin_bone_records, 2);
        assert_eq!(payloads.shape.puppet_skin_vertex_records, 3);
        assert_eq!(payloads.shape.puppet_attachment_records, 1);
        assert_eq!(payloads.shape.puppet_clip_records, 1);
        assert_eq!(payloads.shape.puppet_frame_records, 4);
        assert_eq!(payloads.shape.puppet_layer_records, 1);
        assert_eq!(
            payloads
                .chunk(SceneBinaryChunkKind::Puppet)
                .expect("puppet payload")
                .bytes
                .len(),
            SCENE_BINARY_PUPPET_RECORD_SIZE
        );
        assert_eq!(
            payloads
                .chunk(SceneBinaryChunkKind::PuppetFrames)
                .expect("puppet frames")
                .bytes
                .len(),
            4 * SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE
        );

        let bytes = payloads.encode_container(0).expect("encode");
        assert!(
            !bytes
                .windows("lock_transforms".len())
                .any(|window| window == b"lock_transforms")
        );
        let layout = decode_scene_binary_container(&bytes).expect("decode");
        let nodes = layout
            .node_records(&bytes)
            .expect("nodes")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded nodes");
        let puppet = layout
            .puppet_record_at(&bytes, nodes[0].puppet_index)
            .expect("puppet record");
        assert_eq!(puppet.vertex_count, 3);
        assert_eq!(puppet.index_count, 3);
        assert_eq!(puppet.bone_count, 2);
        assert_eq!(puppet.skin_vertex_count, 3);
        assert_eq!(puppet.attachment_count, 1);
        assert_eq!(puppet.clip_count, 1);
        assert_eq!(puppet.clip_frame_count, 4);
        assert_eq!(puppet.animation_layer_count, 1);
        assert!(puppet.flags & SCENE_BINARY_PUPPET_FLAG_MESH != 0);
        assert!(puppet.flags & SCENE_BINARY_PUPPET_FLAG_SKIN != 0);
        assert!(puppet.flags & SCENE_BINARY_PUPPET_FLAG_CLIPS != 0);
        assert!(puppet.flags & SCENE_BINARY_PUPPET_FLAG_ATTACHMENTS != 0);
        assert!(puppet.flags & SCENE_BINARY_PUPPET_FLAG_ANIMATION_LAYERS != 0);

        let bones = layout
            .puppet_skin_bone_record_range(&bytes, puppet)
            .expect("bones")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded bones");
        assert_eq!(bones[0].parent_index, SCENE_BINARY_NONE_ID);
        assert_eq!(bones[1].parent_index, 0);
        assert_eq!(bones[1].transform.translation[0], 1.0);
        let skin_vertices = layout
            .puppet_skin_vertex_record_range(&bytes, puppet)
            .expect("skin vertices")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded skin vertices");
        assert_eq!(skin_vertices[0].bone_indices, [0, 1, 0, 0]);
        assert_eq!(skin_vertices[0].weight_count, 2);
        assert!((skin_vertices[0].weights[1] - 0.75).abs() < f32::EPSILON);

        let attachments = layout
            .puppet_attachment_record_range(&bytes, puppet)
            .expect("attachments")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded attachments");
        assert_eq!(attachments[0].bone_index, 1);
        assert_eq!(attachments[0].local_position, [1.0, 2.0, 0.0]);

        let clips = layout
            .puppet_clip_record_range(&bytes, puppet)
            .expect("clips")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded clips");
        assert_eq!(clips[0].clip_id, 7);
        assert_eq!(clips[0].bone_count, 2);
        assert_eq!(clips[0].frame_count, 2);
        assert_eq!(clips[0].frame_record_count, 4);
        assert!(clips[0].flags & SCENE_BINARY_PUPPET_CLIP_FLAG_LOOPING != 0);
        let frames = layout
            .puppet_frame_record_range(&bytes, clips[0])
            .expect("frames")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded frames");
        assert_eq!(frames[3].bone_index, 1);
        assert_eq!(frames[3].frame_index, 1);
        assert_eq!(frames[3].transform.opacity, 0.25);

        let layers = layout
            .puppet_layer_record_range(&bytes, puppet)
            .expect("layers")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded layers");
        assert_eq!(layers[0].clip_id, 7);
        assert!(layers[0].flags & SCENE_BINARY_PUPPET_LAYER_FLAG_ADDITIVE != 0);
        assert!(layers[0].flags & SCENE_BINARY_PUPPET_LAYER_FLAG_LOCK_TRANSFORMS != 0);
        assert!(layers[0].flags & SCENE_BINARY_PUPPET_LAYER_FLAG_VISIBLE != 0);
        assert!((layers[0].blend - 0.75).abs() < f32::EPSILON);

        let retained = layout
            .retained_gpu_state_records(&bytes)
            .expect("retained")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded retained");
        assert!(
            retained
                .iter()
                .any(|record| record.owner_kind == SCENE_BINARY_RETAINED_PUPPET)
        );
    }

    #[test]
    fn binary_material_pass_carries_alpha_mask_render_state_and_resource_indices() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "base", "type": "image", "source": "assets/base.gtex", "width": 128, "height": 64 },
                { "id": "mask", "type": "image", "source": "assets/mask.gtex", "width": 128, "height": 64 }
            ],
            "nodes": [
                {
                    "id": "panel",
                    "type": "image",
                    "resource": "base",
                    "properties": { "wallpaper_engine_blend": { "colorBlendMode": 2 } },
                    "effects": [
                        {
                            "file": "effects/opacity/effect.json",
                            "passes": [
                                {
                                    "shader": "effects/opacity",
                                    "blending": "normal",
                                    "depthtest": "false",
                                    "depthwrite": "false",
                                    "cullmode": "none",
                                    "texture_resources": ["base", "mask"]
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");

        let payloads = scene_binary_payloads_from_document(&document);
        assert_eq!(payloads.shape.texture_slot_records, 2);
        let bytes = payloads.encode_container(0).expect("encode");
        let layout = decode_scene_binary_container(&bytes).expect("decode");

        let materials = layout
            .material_pass_records(&bytes)
            .expect("material records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded material records");
        assert_eq!(materials.len(), 1);
        assert_eq!(materials[0].texture_slot_count, 2);
        assert_eq!(materials[0].alpha_texture_slot, 1);
        assert_eq!(
            materials[0].alpha_texture_mode,
            alpha_texture_mode_code(SceneAlphaTextureMode::Multiply)
        );
        assert_eq!(
            materials[0].blend_mode,
            blend_mode_code(SceneBlendMode::Multiply)
        );
        assert_eq!(materials[0].depth_test, material_flag_code(Some("false")));
        assert_eq!(materials[0].depth_write, material_flag_code(Some("false")));
        assert_eq!(materials[0].cull_mode, cull_mode_code(Some("none")));
        assert_eq!(materials[0].descriptor_layout, 3);

        let texture_slots = layout
            .material_texture_slot_records(&bytes, materials[0])
            .expect("material texture slots")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded material texture slots");
        assert_eq!(texture_slots.len(), 2);
        assert_eq!(texture_slots[0].resource_index, 0);
        assert_eq!(texture_slots[1].resource_index, 1);
        assert_eq!(
            texture_slots[0].role_flags,
            SCENE_BINARY_TEXTURE_ROLE_BASE_COLOR | SCENE_BINARY_TEXTURE_ROLE_EFFECT_INPUT
        );
        assert_eq!(
            texture_slots[1].role_flags,
            SCENE_BINARY_TEXTURE_ROLE_EFFECT_INPUT | SCENE_BINARY_TEXTURE_ROLE_ALPHA_MASK
        );

        let effect_passes = layout
            .material_effect_pass_records(&bytes, materials[0])
            .expect("material effect range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded material effect range");
        assert_eq!(effect_passes.len(), 1);
        assert_eq!(effect_passes[0].first_texture_slot, 0);
        assert_eq!(effect_passes[0].texture_slot_count, 2);
        assert_eq!(effect_passes[0].evaluation_boundary, 1);
    }

    #[test]
    fn binary_effect_pass_carries_effect_uv_transform_records() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "base", "type": "image", "source": "assets/eye.gtex", "width": 663, "height": 230 },
                { "id": "mask", "type": "image", "source": "assets/iris-mask.gtex", "width": 331, "height": 115 }
            ],
            "nodes": [
                {
                    "id": "eye",
                    "type": "image",
                    "resource": "base",
                    "width": 663,
                    "height": 230,
                    "effects": [
                        {
                            "file": "effects/iris/effect.json",
                            "runtime": "native-iris-mask",
                            "passes": [
                                {
                                    "shader": "effects/iris",
                                    "blending": "normal",
                                    "texture_resources": ["base", "mask"],
                                    "effect_uv_transform": {
                                        "mapping": "texture-resolution",
                                        "source_slot": 0,
                                        "mask_slot": 1,
                                        "scale": [1.0, 1.0],
                                        "offset": [0.25, 0.0],
                                        "input_extent": { "width": 663, "height": 230 },
                                        "mask_extent": { "width": 331, "height": 115 },
                                        "mask_backing_extent": { "width": 331, "height": 115 }
                                    }
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");

        let payloads = scene_binary_payloads_from_document(&document);
        assert_eq!(payloads.shape.effect_uv_transform_records, 1);
        let bytes = payloads
            .encode_container(0)
            .expect("encode document chunks");
        let layout = decode_scene_binary_container(&bytes).expect("decode document chunks");
        let effect_passes = layout
            .effect_pass_records(&bytes)
            .expect("effect passes")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded effect passes");
        assert_eq!(effect_passes.len(), 1);
        assert_eq!(effect_passes[0].first_effect_uv_transform, 0);
        assert_eq!(effect_passes[0].effect_uv_transform_count, 1);
        let transforms = layout
            .effect_uv_transform_record_range(&bytes, effect_passes[0])
            .expect("effect uv transform range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded effect uv transforms");
        assert_eq!(transforms.len(), 1);
        assert_eq!(transforms[0].source_slot, 0);
        assert_eq!(transforms[0].mask_slot, 1);
        assert_eq!(transforms[0].input_width, 663);
        assert_eq!(transforms[0].mask_width, 331);
        assert_eq!(transforms[0].backing_height, 115);
        assert_eq!(transforms[0].scale_u, 1.0);
        assert_eq!(transforms[0].offset_u, 0.25);
        let retained = layout
            .retained_gpu_state_records(&bytes)
            .expect("retained")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded retained records");
        assert!(
            retained
                .iter()
                .any(|record| record.owner_kind == SCENE_BINARY_RETAINED_EFFECT_UV_TRANSFORM)
        );
    }

    #[test]
    fn binary_material_pass_maps_effect_normal_blend_to_overwrite_mode() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "eye", "type": "image", "source": "assets/eye.gtex", "width": 100, "height": 50 },
                { "id": "mask", "type": "image", "source": "assets/iris-mask.gtex", "width": 50, "height": 25 }
            ],
            "nodes": [
                {
                    "id": "eye-node",
                    "type": "image",
                    "resource": "eye",
                    "effects": [
                        {
                            "file": "effects/iris/effect.json",
                            "runtime": "wallpaper-engine-effect",
                            "passes": [
                                {
                                    "shader": "effects/iris",
                                    "blending": "normal",
                                    "texture_resources": ["eye", "mask"]
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");

        let payloads = scene_binary_payloads_from_document(&document);
        let bytes = payloads.encode_container(0).expect("encode");
        let layout = decode_scene_binary_container(&bytes).expect("decode");

        let materials = layout
            .material_pass_records(&bytes)
            .expect("material records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded material records");

        assert_eq!(materials.len(), 1);
        assert_eq!(
            materials[0].blend_mode,
            blend_mode_code(SceneBlendMode::Normal)
        );
    }
}

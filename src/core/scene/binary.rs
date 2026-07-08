use std::collections::BTreeMap;

use serde_json::Value;

use super::{
    SceneAlphaTextureMode, SceneAnimatedProperty, SceneBlendMode, SceneCurve, SceneDocument,
    SceneEffect, SceneEffectFbo, SceneEffectPass, SceneEffectUvExtent, SceneEffectUvTransform,
    SceneKeyframe, SceneMeshPuppetClippingActiveSource, SceneMeshPuppetClippingRecord, SceneNode,
    SceneNodeKind, SceneParticleEmitterSettings, ScenePuppetTransform, SceneResource,
    SceneResourceKind, SceneTextAlign, SceneTimelineChannel,
};
use crate::core::FitMode;

mod chunk;
mod constants;
mod container;
mod debug_names;
mod effect_uv;
mod error;
mod flutter;
mod geometry;
mod io;
mod layout;
mod material;
mod node;
mod particle;
mod puppet;
mod resource;
mod transform;

pub use self::chunk::{
    SceneBinaryChunkDescriptor, SceneBinaryChunkKind, SceneBinaryChunkPayload,
    SceneBinaryDocumentPayloads, SceneBinaryOwnedChunkPayload,
};
pub use self::constants::{
    SCENE_BINARY_ALIGNMENT, SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE,
    SCENE_BINARY_DEBUG_NAME_RECORD_SIZE, SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
    SCENE_BINARY_EFFECT_PASS_RECORD_SIZE, SCENE_BINARY_EFFECT_PASS_RECORD_SIZE_V12,
    SCENE_BINARY_ENDIAN_LITTLE, SCENE_BINARY_HEADER_SIZE, SCENE_BINARY_MAGIC,
    SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE, SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE_V12,
    SCENE_BINARY_NONE_ID, SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO,
    SCENE_BINARY_PARAMETER_ROLE_EFFECT_PROPERTY, SCENE_BINARY_PARAMETER_ROLE_PASS_BIND,
    SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO, SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT,
    SCENE_BINARY_PARAMETER_VALUE_BOOL, SCENE_BINARY_PARAMETER_VALUE_FLOAT,
    SCENE_BINARY_PARAMETER_VALUE_INTEGER, SCENE_BINARY_PARAMETER_VALUE_STRING,
    SCENE_BINARY_PARAMETER_VALUE_VEC2, SCENE_BINARY_PARAMETER_VALUE_VEC3,
    SCENE_BINARY_PARAMETER_VALUE_VEC4, SCENE_BINARY_RENDER_STATE_RECORD_SIZE,
    SCENE_BINARY_RETAINED_EFFECT_PARAMETER, SCENE_BINARY_RETAINED_EFFECT_PASS,
    SCENE_BINARY_RETAINED_EFFECT_UV_TRANSFORM, SCENE_BINARY_RETAINED_GEOMETRY,
    SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE, SCENE_BINARY_RETAINED_MATERIAL_PASS,
    SCENE_BINARY_RETAINED_PARTICLE_EMITTER, SCENE_BINARY_RETAINED_PUPPET,
    SCENE_BINARY_RETAINED_RESOURCE, SCENE_BINARY_RETAINED_TEXTURE_SLOT,
    SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE, SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE,
    SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE, SCENE_BINARY_VERSION, SCENE_BINARY_VERSION_V12,
};
use self::constants::{
    SCENE_BINARY_DEFAULT_TRANSFORM_PROPERTY, SCENE_BINARY_TEXTURE_ROLE_ALPHA_MASK,
    SCENE_BINARY_TEXTURE_ROLE_BASE_COLOR, SCENE_BINARY_TEXTURE_ROLE_EFFECT_INPUT,
    SCENE_BINARY_TEXTURE_ROLE_FIRST_CLASS_TARGET,
};
#[cfg(test)]
use self::container::write_chunk_descriptor;
pub use self::container::{
    decode_scene_binary_container, decode_scene_binary_header_table, encode_scene_binary_container,
    scene_binary_empty_payloads_for_shape,
};
pub(crate) use self::debug_names::decode_debug_name_record;
pub use self::debug_names::{SceneBinaryDebugNameRecord, SceneBinaryDebugNames};
pub(crate) use self::effect_uv::decode_effect_uv_transform_record;
pub use self::effect_uv::{
    SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION,
    SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE, SceneBinaryEffectUvTransformRecord,
};
use self::effect_uv::{effect_uv_transform_flags, effect_uv_transform_mapping_code};
pub use self::error::SceneBinaryError;
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
pub(in crate::core::scene::binary) use self::io::{
    read_f32, read_i64, read_u16, read_u16_or, read_u32, read_u64, write_f32, write_i64, write_u16,
    write_u32, write_u64,
};
pub use self::layout::{SceneBinaryLayoutPlan, SceneBinaryRecords};
pub use self::material::{
    SceneBinaryEffectParameterRecord, SceneBinaryEffectPassRecord, SceneBinaryMaterialPassRecord,
    SceneBinaryRenderStateRecord, SceneBinaryRetainedGpuStateRecord, SceneBinaryTextureSlotRecord,
};
pub(crate) use self::material::{
    decode_effect_parameter_record, decode_effect_pass_record, decode_material_pass_record,
    decode_render_state_record, decode_retained_gpu_state_record, decode_texture_slot_record,
};
pub(crate) use self::node::decode_node_record;
pub use self::node::{
    SCENE_BINARY_NODE_RECORD_SIZE, SCENE_BINARY_NODE_RECORD_SIZE_V12, SceneBinaryNodeRecord,
};
pub(crate) use self::particle::decode_particle_emitter_record;
use self::particle::particle_emitter_record_from_node;
pub use self::particle::{
    SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE, SCENE_BINARY_PARTICLE_FLAG_FADE,
    SCENE_BINARY_PARTICLE_FLAG_LOOP, SCENE_BINARY_PARTICLE_SHAPE_ELLIPSE,
    SCENE_BINARY_PARTICLE_SHAPE_RECTANGLE, SceneBinaryParticleEmitterRecord,
    scene_binary_particle_shape_kind, scene_binary_particle_transform,
};
pub use self::puppet::{
    SCENE_BINARY_PUPPET_ACTIVE_SOURCE_RECORD_SIZE, SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE,
    SCENE_BINARY_PUPPET_CLIP_FLAG_LOOPING, SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE,
    SCENE_BINARY_PUPPET_CLIPPING_BONE_RECORD_SIZE,
    SCENE_BINARY_PUPPET_CLIPPING_FRAME_KEY_RECORD_SIZE, SCENE_BINARY_PUPPET_CLIPPING_RECORD_SIZE,
    SCENE_BINARY_PUPPET_FLAG_ANIMATION_LAYERS, SCENE_BINARY_PUPPET_FLAG_ATTACHMENTS,
    SCENE_BINARY_PUPPET_FLAG_CLIPPING, SCENE_BINARY_PUPPET_FLAG_CLIPS,
    SCENE_BINARY_PUPPET_FLAG_MESH, SCENE_BINARY_PUPPET_FLAG_SKIN,
    SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE, SCENE_BINARY_PUPPET_LAYER_FLAG_ADDITIVE,
    SCENE_BINARY_PUPPET_LAYER_FLAG_LOCK_TRANSFORMS, SCENE_BINARY_PUPPET_LAYER_FLAG_VISIBLE,
    SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE, SCENE_BINARY_PUPPET_RECORD_SIZE,
    SCENE_BINARY_PUPPET_RECORD_SIZE_V12, SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE,
    SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE, SceneBinaryPuppetActiveSourceRecord,
    SceneBinaryPuppetAttachmentRecord, SceneBinaryPuppetClipRecord,
    SceneBinaryPuppetClippingBoneRecord, SceneBinaryPuppetClippingFrameKeyRecord,
    SceneBinaryPuppetClippingRecord, SceneBinaryPuppetFrameRecord, SceneBinaryPuppetLayerRecord,
    SceneBinaryPuppetRecord, SceneBinaryPuppetSkinBoneRecord, SceneBinaryPuppetSkinVertexRecord,
};
pub(crate) use self::puppet::{
    decode_puppet_active_source_record, decode_puppet_attachment_record, decode_puppet_clip_record,
    decode_puppet_clipping_bone_record, decode_puppet_clipping_frame_key_record,
    decode_puppet_clipping_record, decode_puppet_frame_record, decode_puppet_layer_record,
    decode_puppet_record, decode_puppet_skin_bone_record, decode_puppet_skin_vertex_record,
};
use self::puppet::{puppet_clip_flags, puppet_first_record, puppet_flags, puppet_layer_flags};
pub(crate) use self::resource::decode_resource_record;
pub use self::resource::{SCENE_BINARY_RESOURCE_RECORD_SIZE, SceneBinaryResourceRecord};
pub use self::transform::{SceneBinaryTransformKeyframeRecord, SceneBinaryTransformTimelineRecord};
pub(crate) use self::transform::{
    decode_transform_keyframe_record, decode_transform_timeline_record,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneBinaryDocumentShape {
    pub resource_table_records: u32,
    pub node_table_records: u32,
    pub transform_timeline_records: u32,
    pub transform_keyframe_records: u32,
    pub geometry_records: u32,
    pub geometry_vertex_records: u32,
    pub geometry_index_records: u32,
    pub particle_emitter_records: u32,
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
    pub puppet_clipping_records: u32,
    pub puppet_clipping_bone_records: u32,
    pub puppet_clipping_frame_key_records: u32,
    pub puppet_active_source_records: u32,
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
            .saturating_add(shape.particle_emitter_records)
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
            SceneBinaryChunkKind::ParticleEmitter => self.particle_emitter_records,
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
            SceneBinaryChunkKind::PuppetClipping => self.puppet_clipping_records,
            SceneBinaryChunkKind::PuppetClippingBones => self.puppet_clipping_bone_records,
            SceneBinaryChunkKind::PuppetClippingFrameKeys => self.puppet_clipping_frame_key_records,
            SceneBinaryChunkKind::PuppetActiveSources => self.puppet_active_source_records,
            SceneBinaryChunkKind::RenderState => self.render_state_records,
            SceneBinaryChunkKind::RetainedGpuState => self.retained_gpu_state_records,
            SceneBinaryChunkKind::DebugNames => self.debug_name_records,
        }
    }

    fn include_node(&mut self, node: &SceneNode) {
        self.node_table_records = self.node_table_records.saturating_add(1);
        self.transform_timeline_records = self.transform_timeline_records.saturating_add(1);
        self.debug_name_records = self.debug_name_records.saturating_add(
            1 + u32::from(node.name.is_some())
                + u32::from(node.text.is_some())
                + u32::from(node.font_family.is_some())
                + u32::from(node.font_resource.is_some())
                + u32::from(node.font_weight.is_some()),
        );
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
        if node.kind == SceneNodeKind::ParticleEmitter
            && SceneParticleEmitterSettings::from_node(node).is_some()
        {
            self.particle_emitter_records = self.particle_emitter_records.saturating_add(1);
        }
        if node_has_material(node) {
            self.material_pass_records = self.material_pass_records.saturating_add(1);
        }
        if node.mesh.is_some() || !node.puppet_animation_layers.is_empty() {
            self.puppet_records = self.puppet_records.saturating_add(1);
            self.include_puppet_payload(node);
        }
        for effect in node
            .effects
            .iter()
            .filter(|effect| scene_binary_effect_is_visible(effect))
        {
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
        self.puppet_clipping_records = self
            .puppet_clipping_records
            .saturating_add(saturating_u32(mesh.puppet_clipping_records.len()));
        self.puppet_clipping_bone_records = self.puppet_clipping_bone_records.saturating_add(
            mesh.puppet_clipping_records
                .iter()
                .map(|record| saturating_u32(record.bones.len()))
                .fold(0u32, u32::saturating_add),
        );
        self.puppet_clipping_frame_key_records =
            self.puppet_clipping_frame_key_records.saturating_add(
                mesh.puppet_clipping_records
                    .iter()
                    .map(|record| saturating_u32(record.frame_keys.len()))
                    .fold(0u32, u32::saturating_add),
            );
        self.debug_name_records = self.debug_name_records.saturating_add(
            mesh.puppet_clipping_records
                .iter()
                .filter(|record| !record.mask.is_empty())
                .count()
                .min(u32::MAX as usize) as u32,
        );
        self.puppet_active_source_records = self
            .puppet_active_source_records
            .saturating_add(saturating_u32(mesh.puppet_clipping_active_sources.len()));
        self.debug_name_records = self.debug_name_records.saturating_add(
            mesh.puppet_clipping_active_sources
                .iter()
                .filter(|source| !source.source_name.is_empty())
                .count()
                .min(u32::MAX as usize) as u32,
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
    Text,
    Font,
    EffectCommand,
    EffectSource,
    EffectTarget,
    EffectBind,
    PuppetClippingMask,
    PuppetClippingSource,
    PuppetActiveSource,
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
            Self::Text => 15,
            Self::Font => 16,
            Self::EffectCommand => 17,
            Self::EffectSource => 18,
            Self::EffectTarget => 19,
            Self::EffectBind => 20,
            Self::PuppetClippingMask => 21,
            Self::PuppetClippingSource => 22,
            Self::PuppetActiveSource => 23,
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
    particle_emitter: SceneBinaryChunkWriter,
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
    puppet_clipping: SceneBinaryChunkWriter,
    puppet_clipping_bones: SceneBinaryChunkWriter,
    puppet_clipping_frame_keys: SceneBinaryChunkWriter,
    puppet_active_sources: SceneBinaryChunkWriter,
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
    alpha_write: u16,
}

#[derive(Debug, Clone, Copy)]
struct SceneBinaryMaterialPassSource<'a> {
    shader: Option<&'a str>,
    blending: Option<&'a str>,
    depthtest: Option<&'a str>,
    depthwrite: Option<&'a str>,
    cullmode: Option<&'a str>,
    alphawriting: Option<&'a str>,
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
            .filter(|effect| scene_binary_effect_is_visible(effect))
            .flat_map(|effect| effect.passes.iter())
            .next();
        let material_source = scene_binary_base_material_pass_source(node)
            .or_else(|| first_pass.map(scene_binary_effect_material_pass_source));
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
        let blend_mode = super::scene_blend_mode_from_properties(&node.properties);
        Self {
            shader: material_source.and_then(|source| source.shader),
            blending: material_source.and_then(|source| source.blending),
            blend_mode,
            alpha_texture_slot,
            alpha_texture_mode,
            texture_slot_count,
            effect_pass_count,
            effect_kind_flags,
            material_kind,
            descriptor_layout,
            depth_test: material_flag_code(material_source.and_then(|source| source.depthtest)),
            depth_write: material_flag_code(material_source.and_then(|source| source.depthwrite)),
            cull_mode: cull_mode_code(material_source.and_then(|source| source.cullmode)),
            alpha_write: material_flag_code(material_source.and_then(|source| source.alphawriting)),
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

fn scene_binary_base_material_pass_source(
    node: &SceneNode,
) -> Option<SceneBinaryMaterialPassSource<'_>> {
    let pass = node
        .properties
        .get("material")?
        .as_object()?
        .get("passes")?
        .as_array()?
        .iter()
        .find_map(serde_json::Value::as_object)?;
    Some(SceneBinaryMaterialPassSource {
        shader: pass.get("shader").and_then(serde_json::Value::as_str),
        blending: pass.get("blending").and_then(serde_json::Value::as_str),
        depthtest: pass.get("depthtest").and_then(serde_json::Value::as_str),
        depthwrite: pass.get("depthwrite").and_then(serde_json::Value::as_str),
        cullmode: pass.get("cullmode").and_then(serde_json::Value::as_str),
        alphawriting: pass.get("alphawriting").and_then(serde_json::Value::as_str),
    })
}

fn scene_binary_effect_material_pass_source(
    pass: &SceneEffectPass,
) -> SceneBinaryMaterialPassSource<'_> {
    SceneBinaryMaterialPassSource {
        shader: pass.shader.as_deref(),
        blending: pass.blending.as_deref(),
        depthtest: pass.depthtest.as_deref(),
        depthwrite: pass.depthwrite.as_deref(),
        cullmode: pass.cullmode.as_deref(),
        alphawriting: pass.alphawriting.as_deref(),
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
                document,
                true,
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
        document: &SceneDocument,
        parent_visible: bool,
    ) {
        let node_index = self.node_table.record_count;
        let id_name = self.names.intern(SceneBinaryNameKind::NodeId, &node.id);
        let display_name = self
            .names
            .intern_optional(SceneBinaryNameKind::DisplayName, node.name.as_deref());
        let resource_name = self
            .names
            .intern_optional(SceneBinaryNameKind::ResourceId, node.resource.as_deref());
        let text_name = self
            .names
            .intern_optional(SceneBinaryNameKind::Text, node.text.as_deref());
        let font_family_name = self
            .names
            .intern_optional(SceneBinaryNameKind::Font, node.font_family.as_deref());
        let font_resource_name = self.names.intern_optional(
            SceneBinaryNameKind::ResourceId,
            node.font_resource.as_deref(),
        );
        let font_weight_name = self
            .names
            .intern_optional(SceneBinaryNameKind::Font, node.font_weight.as_deref());
        let puppet_attachment_name = self.names.intern_optional(
            SceneBinaryNameKind::PuppetAttachment,
            node.puppet_attachment.as_deref(),
        );
        let puppet_source_name = self.names.intern_optional(
            SceneBinaryNameKind::ResourcePath,
            node.provenance
                .as_ref()
                .and_then(|provenance| provenance.model.as_ref())
                .and_then(|model| model.puppet.as_deref()),
        );
        let base_resource = node
            .resource
            .as_deref()
            .and_then(|resource| resource_index.binding(resource));
        let material_state =
            SceneBinaryMaterialState::from_node(node, base_resource, resource_index);
        let effective_visible = parent_visible && node_binary_default_visible(node, document);
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
                    alpha_write: material_state.alpha_write,
                    flags: material_flags(
                        node,
                        effective_visible,
                        base_resource,
                        material_state.alpha_texture_slot,
                        material_state.effect_pass_count,
                    ),
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
        let particle_index = particle_emitter_record_from_node(id_name, node)
            .map(|record| self.push_particle_emitter(record))
            .unwrap_or(SCENE_BINARY_NONE_ID);
        self.node_table.push_record(|out| {
            SceneBinaryNodeRecord {
                id_name,
                display_name,
                parent_index: parent_index.unwrap_or(SCENE_BINARY_NONE_ID),
                resource_name,
                kind: node_kind_code(node.kind),
                flags: node_flags(node, effective_visible),
                draw_order: *draw_order,
                child_count: saturating_u32(node.children.len()),
                first_child_index: if node.children.is_empty() {
                    SCENE_BINARY_NONE_ID
                } else {
                    node_index.saturating_add(1)
                },
                subtree_node_count: node_subtree_count(node),
                effect_count: scene_binary_visible_effect_count(&node.effects),
                audio_count: saturating_u32(node.audio.len()),
                property_count: saturating_u32(node.properties.len()),
                material_index,
                geometry_index,
                first_transform,
                transform_count,
                puppet_index,
                particle_index,
                puppet_attachment_name,
                opacity: node.opacity as f32,
                color_rgba: scene_binary_color_rgba(node.color.as_deref()),
                stroke_color_rgba: scene_binary_color_rgba(node.stroke_color.as_deref()),
                stroke_width: node.stroke_width.unwrap_or(0.0) as f32,
                corner_radius: node.corner_radius.unwrap_or(0.0) as f32,
                font_size: node.font_size.unwrap_or(0.0) as f32,
                text_name,
                font_family_name,
                font_resource_name,
                font_weight_name,
                fit: fit_code(node.fit),
                text_align: text_align_code(node.text_align),
                puppet_source_name,
            }
            .encode(out)
        });
        *draw_order = draw_order.saturating_add(1);
        let mut base_texture_reuse_available = base_texture_slot.is_some();
        for effect in node
            .effects
            .iter()
            .filter(|effect| scene_binary_effect_is_visible(effect))
        {
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
                document,
                effective_visible,
            );
        }
    }

    fn push_particle_emitter(&mut self, record: SceneBinaryParticleEmitterRecord) -> u32 {
        let owner_name = record.owner_name;
        let record_index = self.particle_emitter.push_record(|out| record.encode(out));
        self.push_retained(
            SCENE_BINARY_RETAINED_PARTICLE_EMITTER,
            owner_name,
            record_index,
        );
        record_index
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
                let parameter_count = self.push_effect_pass_parameters(
                    owner_name,
                    effect_name,
                    pass_index,
                    pass,
                    &effect.fbos,
                );
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
        let command_name =
            pass.and_then(|pass| pass.command.as_deref())
                .map_or(SCENE_BINARY_NONE_ID, |command| {
                    self.names
                        .intern(SceneBinaryNameKind::EffectCommand, command)
                });
        let source_name = pass
            .and_then(|pass| pass.source.as_deref())
            .map_or(SCENE_BINARY_NONE_ID, |source| {
                self.names.intern(SceneBinaryNameKind::EffectSource, source)
            });
        let target_name = pass
            .and_then(|pass| pass.target.as_deref())
            .map_or(SCENE_BINARY_NONE_ID, |target| {
                self.names.intern(SceneBinaryNameKind::EffectTarget, target)
            });
        let record_index = self.effect_pass.record_count;
        self.effect_pass.push_record(|out| {
            SceneBinaryEffectPassRecord {
                owner_name,
                effect_name,
                shader_name,
                blending_name,
                command_name,
                source_name,
                target_name,
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
                alpha_write: material_flag_code(pass.and_then(|pass| pass.alphawriting.as_deref())),
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
        fbos: &[SceneEffectFbo],
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
        for (slot, name) in &pass.binds {
            self.push_effect_bind(owner_name, effect_name, pass_index, *slot, name);
        }
        for fbo in fbos {
            self.push_effect_fbo(owner_name, effect_name, pass_index, fbo);
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

    fn push_effect_bind(
        &mut self,
        owner_name: u32,
        effect_name: u32,
        pass_index: u32,
        slot: u32,
        name: &str,
    ) {
        let slot_name = slot.to_string();
        let parameter_name = self
            .names
            .intern(SceneBinaryNameKind::EffectBind, &slot_name);
        let value_name = self.names.intern(SceneBinaryNameKind::EffectBind, name);
        let record_index = self.effect_parameter.record_count;
        self.effect_parameter.push_record(|out| {
            SceneBinaryEffectParameterRecord {
                owner_name,
                effect_name,
                parameter_name,
                value_name,
                pass_index,
                value_kind: SCENE_BINARY_PARAMETER_VALUE_STRING,
                role_flags: SCENE_BINARY_PARAMETER_ROLE_PASS_BIND,
                integer_value: i64::from(slot),
                value0: slot as f32,
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

    fn push_effect_fbo(
        &mut self,
        owner_name: u32,
        effect_name: u32,
        pass_index: u32,
        fbo: &SceneEffectFbo,
    ) {
        let parameter_name = self
            .names
            .intern(SceneBinaryNameKind::EffectBind, &fbo.name);
        let value_name = fbo
            .format
            .as_deref()
            .map_or(SCENE_BINARY_NONE_ID, |format| {
                self.names.intern(SceneBinaryNameKind::EffectBind, format)
            });
        let record_index = self.effect_parameter.record_count;
        self.effect_parameter.push_record(|out| {
            SceneBinaryEffectParameterRecord {
                owner_name,
                effect_name,
                parameter_name,
                value_name,
                pass_index,
                value_kind: SCENE_BINARY_PARAMETER_VALUE_STRING,
                role_flags: SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO,
                integer_value: i64::from(fbo.unique),
                value0: fbo.scale as f32,
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

        let first_clipping_record = self.puppet_clipping.record_count;
        let first_clipping_bone = self.puppet_clipping_bones.record_count;
        let first_clipping_frame_key = self.puppet_clipping_frame_keys.record_count;
        let first_active_source = self.puppet_active_sources.record_count;
        if let Some(mesh) = mesh {
            for record in &mesh.puppet_clipping_records {
                self.push_puppet_clipping_record(owner_name, record);
            }
            for source in &mesh.puppet_clipping_active_sources {
                self.push_puppet_active_source(source);
            }
        }
        let clipping_record_count = self
            .puppet_clipping
            .record_count
            .saturating_sub(first_clipping_record);
        let clipping_bone_count = self
            .puppet_clipping_bones
            .record_count
            .saturating_sub(first_clipping_bone);
        let clipping_frame_key_count = self
            .puppet_clipping_frame_keys
            .record_count
            .saturating_sub(first_clipping_frame_key);
        let active_source_count = self
            .puppet_active_sources
            .record_count
            .saturating_sub(first_active_source);

        let flags = puppet_flags(
            mesh.is_some(),
            animation_layer_count > 0,
            bone_count > 0 && skin_vertex_count > 0,
            clip_count > 0,
            attachment_count > 0,
            clipping_record_count > 0 || active_source_count > 0,
        );
        let dirty_range_count = 1
            + u32::from(bone_count > 0)
            + u32::from(skin_vertex_count > 0)
            + u32::from(attachment_count > 0)
            + u32::from(clip_count > 0)
            + u32::from(clip_frame_count > 0)
            + u32::from(animation_layer_count > 0)
            + u32::from(clipping_record_count > 0)
            + u32::from(clipping_bone_count > 0)
            + u32::from(clipping_frame_key_count > 0)
            + u32::from(active_source_count > 0);
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
                first_clipping_record: puppet_first_record(
                    first_clipping_record,
                    clipping_record_count,
                ),
                clipping_record_count,
                first_clipping_bone: puppet_first_record(first_clipping_bone, clipping_bone_count),
                clipping_bone_count,
                first_clipping_frame_key: puppet_first_record(
                    first_clipping_frame_key,
                    clipping_frame_key_count,
                ),
                clipping_frame_key_count,
                first_active_source: puppet_first_record(first_active_source, active_source_count),
                active_source_count,
                flags,
                dirty_range_count,
            }
            .encode(out)
        });
        self.push_retained(SCENE_BINARY_RETAINED_PUPPET, owner_name, record_index);
        record_index
    }

    fn push_puppet_clipping_record(
        &mut self,
        owner_name: u32,
        record: &SceneMeshPuppetClippingRecord,
    ) {
        let record_owner_name = record
            .source_name
            .as_ref()
            .map(|source_name| {
                self.names
                    .intern(SceneBinaryNameKind::PuppetClippingSource, source_name)
            })
            .unwrap_or(owner_name);
        let mask_name = self.names.intern(
            SceneBinaryNameKind::PuppetClippingMask,
            record.mask_resource.as_deref().unwrap_or(&record.mask),
        );
        let first_bone = self.puppet_clipping_bones.record_count;
        for bone in &record.bones {
            self.puppet_clipping_bones.push_record(|out| {
                SceneBinaryPuppetClippingBoneRecord {
                    owner_name: record_owner_name,
                    bone_index: saturating_u32(*bone),
                }
                .encode(out)
            });
        }
        let bone_count = self
            .puppet_clipping_bones
            .record_count
            .saturating_sub(first_bone);
        let first_frame_key = self.puppet_clipping_frame_keys.record_count;
        for frame_key in &record.frame_keys {
            self.puppet_clipping_frame_keys.push_record(|out| {
                SceneBinaryPuppetClippingFrameKeyRecord {
                    owner_name: record_owner_name,
                    frame_key: *frame_key,
                }
                .encode(out)
            });
        }
        let frame_key_count = self
            .puppet_clipping_frame_keys
            .record_count
            .saturating_sub(first_frame_key);
        self.puppet_clipping.push_record(|out| {
            SceneBinaryPuppetClippingRecord {
                owner_name: record_owner_name,
                mask_name,
                duration_frames: record.duration_frames,
                flags: record.flags,
                first_bone: puppet_first_record(first_bone, bone_count),
                bone_count,
                first_frame_key: puppet_first_record(first_frame_key, frame_key_count),
                frame_key_count,
                dirty_range_count: 1 + u32::from(bone_count > 0) + u32::from(frame_key_count > 0),
                reserved: 0,
            }
            .encode(out)
        });
    }

    fn push_puppet_active_source(&mut self, source: &SceneMeshPuppetClippingActiveSource) {
        let source_name = self
            .names
            .intern(SceneBinaryNameKind::PuppetActiveSource, &source.source_name);
        self.puppet_active_sources.push_record(|out| {
            SceneBinaryPuppetActiveSourceRecord {
                source_name,
                source_id: source.source_id,
                scalar_bits: source.scalar_bits,
                source_scale: source.source_scale,
                flags: source.flags,
                transform_index: source.transform_index,
                parameter0: source.parameter0,
                parameter1: source.parameter1,
                reserved: 0,
            }
            .encode(out)
        });
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
            particle_emitter_records: self.particle_emitter.record_count,
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
            puppet_clipping_records: self.puppet_clipping.record_count,
            puppet_clipping_bone_records: self.puppet_clipping_bones.record_count,
            puppet_clipping_frame_key_records: self.puppet_clipping_frame_keys.record_count,
            puppet_active_source_records: self.puppet_active_sources.record_count,
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
                self.particle_emitter
                    .into_payload(SceneBinaryChunkKind::ParticleEmitter),
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
                self.puppet_clipping
                    .into_payload(SceneBinaryChunkKind::PuppetClipping),
                self.puppet_clipping_bones
                    .into_payload(SceneBinaryChunkKind::PuppetClippingBones),
                self.puppet_clipping_frame_keys
                    .into_payload(SceneBinaryChunkKind::PuppetClippingFrameKeys),
                self.puppet_active_sources
                    .into_payload(SceneBinaryChunkKind::PuppetActiveSources),
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
        SceneBlendMode::Modulate => 7,
        SceneBlendMode::HslColor => 8,
        SceneBlendMode::AlphaToCoverage => 9,
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

fn node_flags(node: &SceneNode, effective_visible: bool) -> u16 {
    u16::from(effective_visible)
        | (u16::from(node.resource.is_some()) << 1)
        | (u16::from(scene_binary_node_has_visible_effects(node)) << 2)
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

fn node_binary_default_visible(node: &SceneNode, document: &SceneDocument) -> bool {
    if !node.visible {
        return false;
    }
    let Some(condition) = node
        .properties
        .get("visibility_condition")
        .and_then(serde_json::Value::as_object)
    else {
        return true;
    };
    if condition
        .get("runtime")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|runtime| runtime != "wallpaper-engine-user-condition")
    {
        return true;
    }
    let authored_visible = condition
        .get("authored_value")
        .and_then(super::scene_runtime_visibility_value_bool)
        .unwrap_or(true);
    let Some(property) = condition
        .get("property")
        .and_then(super::scene_runtime_visibility_value_string)
    else {
        return condition
            .get("default_visible")
            .and_then(super::scene_runtime_visibility_value_bool)
            .unwrap_or(true);
    };
    let Some(expected) = condition.get("condition") else {
        return condition
            .get("default_visible")
            .and_then(super::scene_runtime_visibility_value_bool)
            .unwrap_or(authored_visible);
    };
    let actual = document
        .properties
        .get(&property)
        .and_then(|property| property.get("default"));
    let actual_number = actual.and_then(super::scene_runtime_visibility_value_number);
    let actual_text = actual.and_then(super::scene_runtime_visibility_value_string);
    if actual_number.is_none() && actual_text.is_none() {
        return condition
            .get("default_visible")
            .and_then(super::scene_runtime_visibility_value_bool)
            .unwrap_or(authored_visible);
    }
    super::scene_runtime_visibility_condition_matches(
        expected,
        actual_number,
        actual_text.as_deref(),
    )
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

fn text_align_code(align: Option<SceneTextAlign>) -> u16 {
    match align.unwrap_or_default() {
        SceneTextAlign::Start => 1,
        SceneTextAlign::Middle => 2,
        SceneTextAlign::End => 3,
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
    effective_visible: bool,
    base_resource: Option<SceneBinaryResourceBinding<'_>>,
    alpha_texture_slot: Option<u32>,
    effect_pass_count: u32,
) -> u16 {
    u16::from(effective_visible)
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
        | (u16::from(pass.and_then(|pass| pass.alphawriting.as_ref()).is_some()) << 5)
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
        if !scene_binary_effect_is_visible(effect) {
            continue;
        }
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
                .saturating_add(pass.binds.len())
                .saturating_add(effect.fbos.len())
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
            .filter(|effect| scene_binary_effect_is_visible(effect))
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
        .filter(|effect| scene_binary_effect_is_visible(effect))
        .flat_map(|effect| effect.passes.iter())
        .map(|pass| scene_binary_effect_pass_texture_slot_count(pass, resource_index))
        .fold(0u32, u32::saturating_add);
    let Some(base_resource) = base_resource else {
        return total;
    };
    let Some(first_pass) = effects
        .iter()
        .filter(|effect| scene_binary_effect_is_visible(effect))
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
        if !scene_binary_effect_is_visible(effect) {
            continue;
        }
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

fn align_usize(value: usize, alignment: usize) -> usize {
    debug_assert!(alignment.is_power_of_two());
    (value + alignment - 1) & !(alignment - 1)
}

fn node_has_material(node: &SceneNode) -> bool {
    node_has_geometry(node)
        || node.resource.is_some()
        || scene_binary_node_has_visible_effects(node)
}

fn node_first_effect_pass_reuses_base_resource(node: &SceneNode) -> bool {
    let Some(base_resource) = node.resource.as_deref() else {
        return false;
    };
    node.effects
        .iter()
        .filter(|effect| scene_binary_effect_is_visible(effect))
        .flat_map(|effect| effect.passes.iter())
        .next()
        .and_then(|pass| pass.texture_resources.first())
        .and_then(|value| value.as_deref())
        == Some(base_resource)
}

fn scene_binary_node_has_visible_effects(node: &SceneNode) -> bool {
    node.effects.iter().any(scene_binary_effect_is_visible)
}

fn scene_binary_visible_effect_count(effects: &[SceneEffect]) -> u32 {
    saturating_u32(
        effects
            .iter()
            .filter(|effect| scene_binary_effect_is_visible(effect))
            .count(),
    )
}

fn scene_binary_effect_is_visible(effect: &SceneEffect) -> bool {
    effect
        .visible
        .as_ref()
        .and_then(scene_binary_visibility_value_bool)
        .unwrap_or(true)
}

fn scene_binary_visibility_value_bool(value: &Value) -> Option<bool> {
    match value.get("value").unwrap_or(value) {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_i64().map(|value| value != 0),
        Value::String(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "1" | "true" | "yes" | "on" | "enabled" => Some(true),
                "0" | "false" | "no" | "off" | "disabled" => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
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
    fn binary_container_decodes_version12_schema() {
        let bytes = version12_empty_container();
        let layout = decode_scene_binary_container(&bytes).expect("decode v12");

        assert_eq!(layout.version, SCENE_BINARY_VERSION_V12);
        assert_eq!(
            layout.chunks.len(),
            SceneBinaryChunkKind::REQUIRED_ORDER_V12.len()
        );
        assert!(layout.chunk(SceneBinaryChunkKind::PuppetClipping).is_none());
        assert_eq!(
            layout
                .required_record_size(SceneBinaryChunkKind::NodeTable)
                .unwrap(),
            SCENE_BINARY_NODE_RECORD_SIZE_V12
        );
        assert_eq!(
            layout
                .required_record_size(SceneBinaryChunkKind::MaterialPass)
                .unwrap(),
            SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE_V12
        );
        assert_eq!(
            layout
                .required_record_size(SceneBinaryChunkKind::EffectPass)
                .unwrap(),
            SCENE_BINARY_EFFECT_PASS_RECORD_SIZE_V12
        );
        assert_eq!(
            layout
                .required_record_size(SceneBinaryChunkKind::Puppet)
                .unwrap(),
            SCENE_BINARY_PUPPET_RECORD_SIZE_V12
        );
    }

    #[test]
    fn version12_tail_decoders_default_removed_fields() {
        let node =
            decode_node_record(&vec![0; SCENE_BINARY_NODE_RECORD_SIZE_V12]).expect("v12 node");
        assert_eq!(node.fit, 1);
        assert_eq!(node.text_align, 0);

        let mut effect_bytes = vec![0; SCENE_BINARY_EFFECT_PASS_RECORD_SIZE_V12];
        write_u32_at(&mut effect_bytes, 16, 9);
        write_u32_at(&mut effect_bytes, 20, 3);
        write_u32_at(&mut effect_bytes, 24, 2);
        write_u32_at(&mut effect_bytes, 28, SCENE_BINARY_NONE_ID);
        write_u32_at(&mut effect_bytes, 32, 1);
        write_u32_at(&mut effect_bytes, 36, 11);
        write_u32_at(&mut effect_bytes, 40, 5);
        write_u16_at(&mut effect_bytes, 44, 0x10);
        write_u16_at(&mut effect_bytes, 46, 0x11);
        write_u16_at(&mut effect_bytes, 48, 0x12);
        write_u16_at(&mut effect_bytes, 50, 0x13);
        write_u16_at(&mut effect_bytes, 52, 0x14);
        write_u16_at(&mut effect_bytes, 54, 0x15);
        let effect = decode_effect_pass_record(&effect_bytes).expect("v12 effect pass");
        assert_eq!(effect.command_name, SCENE_BINARY_NONE_ID);
        assert_eq!(effect.source_name, SCENE_BINARY_NONE_ID);
        assert_eq!(effect.target_name, SCENE_BINARY_NONE_ID);
        assert_eq!(effect.pass_index, 9);
        assert_eq!(effect.first_texture_slot, 3);
        assert_eq!(effect.texture_slot_count, 2);
        assert_eq!(effect.first_effect_uv_transform, SCENE_BINARY_NONE_ID);
        assert_eq!(effect.effect_uv_transform_count, 1);
        assert_eq!(effect.first_parameter, 11);
        assert_eq!(effect.parameter_count, 5);
        assert_eq!(effect.kind, 0x10);
        assert_eq!(effect.evaluation_boundary, 0x11);
        assert_eq!(effect.depth_test, 0x12);
        assert_eq!(effect.depth_write, 0x13);
        assert_eq!(effect.cull_mode, 0x14);
        assert_eq!(effect.alpha_write, 0);
        assert_eq!(effect.flags, 0x15);

        let effect = decode_effect_pass_record(&vec![0; SCENE_BINARY_EFFECT_PASS_RECORD_SIZE_V12])
            .expect("v12 empty effect pass");
        assert_eq!(effect.kind, 0);
        assert_eq!(effect.evaluation_boundary, 0);
        assert_eq!(effect.depth_test, 0);
        assert_eq!(effect.flags, 0);

        let mut material_bytes = vec![0; SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE_V12];
        write_u16_at(&mut material_bytes, 54, 0x77);
        let material = decode_material_pass_record(&material_bytes).expect("v12 material pass");
        assert_eq!(material.alpha_write, 0);
        assert_eq!(material.flags, 0x77);

        let mut puppet_bytes = vec![0; SCENE_BINARY_PUPPET_RECORD_SIZE_V12];
        write_u32_at(&mut puppet_bytes, 60, 31);
        write_u32_at(&mut puppet_bytes, 64, 7);
        let puppet = decode_puppet_record(&puppet_bytes).expect("v12 puppet");
        assert_eq!(puppet.first_clipping_record, SCENE_BINARY_NONE_ID);
        assert_eq!(puppet.clipping_record_count, 0);
        assert_eq!(puppet.first_clipping_bone, SCENE_BINARY_NONE_ID);
        assert_eq!(puppet.clipping_bone_count, 0);
        assert_eq!(puppet.first_clipping_frame_key, SCENE_BINARY_NONE_ID);
        assert_eq!(puppet.clipping_frame_key_count, 0);
        assert_eq!(puppet.first_active_source, SCENE_BINARY_NONE_ID);
        assert_eq!(puppet.active_source_count, 0);
        assert_eq!(puppet.flags, 31);
        assert_eq!(puppet.dirty_range_count, 7);
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

    fn version12_empty_container() -> Vec<u8> {
        let chunk_count = SceneBinaryChunkKind::REQUIRED_ORDER_V12.len() as u32;
        let table_size = SCENE_BINARY_HEADER_SIZE
            + SceneBinaryChunkKind::REQUIRED_ORDER_V12.len() * SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE;
        let payload_offset = align_usize(table_size, usize::from(SCENE_BINARY_ALIGNMENT));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SCENE_BINARY_MAGIC);
        bytes.extend_from_slice(&SCENE_BINARY_VERSION_V12.to_le_bytes());
        bytes.push(SCENE_BINARY_ENDIAN_LITTLE);
        bytes.push(SCENE_BINARY_ALIGNMENT);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&chunk_count.to_le_bytes());
        bytes.extend_from_slice(&(SCENE_BINARY_HEADER_SIZE as u64).to_le_bytes());
        for kind in SceneBinaryChunkKind::REQUIRED_ORDER_V12 {
            write_chunk_descriptor(
                &mut bytes,
                &SceneBinaryChunkDescriptor {
                    kind,
                    record_count: 0,
                    offset: payload_offset as u64,
                    length: 0,
                },
            );
        }
        bytes.resize(payload_offset, 0);
        bytes
    }

    fn write_u32_at(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u16_at(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
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
                        command: Some("draw".to_owned()),
                        source: Some("previous".to_owned()),
                        target: Some("_rt_Flutter".to_owned()),
                        binds: BTreeMap::from([(0, "previous".to_owned())]),
                        shader: Some("effects/flutter".to_owned()),
                        blending: Some("additive".to_owned()),
                        depthtest: Some("false".to_owned()),
                        depthwrite: Some("false".to_owned()),
                        cullmode: Some("none".to_owned()),
                        alphawriting: Some("enabled".to_owned()),
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
        assert_eq!(shape.effect_parameter_records, 5);
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
            blend_mode_code(SceneBlendMode::Screen)
        );
        assert_eq!(materials[0].depth_test, material_flag_code(Some("false")));
        assert_eq!(materials[0].depth_write, material_flag_code(Some("false")));
        assert_eq!(materials[0].cull_mode, cull_mode_code(Some("none")));
        assert_eq!(
            materials[0].alpha_write,
            material_flag_code(Some("enabled"))
        );
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
        assert_eq!(effect_passes[0].parameter_count, 4);
        assert_eq!(
            debug_names
                .name(effect_passes[0].command_name)
                .expect("effect command"),
            Some("draw")
        );
        assert_eq!(
            debug_names
                .name(effect_passes[0].source_name)
                .expect("effect source"),
            Some("previous")
        );
        assert_eq!(
            debug_names
                .name(effect_passes[0].target_name)
                .expect("effect target"),
            Some("_rt_Flutter")
        );
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
        assert_eq!(
            effect_passes[0].alpha_write,
            material_flag_code(Some("enabled"))
        );
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
        assert_eq!(parameters.len(), 5);
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
        assert_eq!(
            debug_names
                .name(parameters[4].parameter_name)
                .expect("bind parameter name"),
            Some("0")
        );
        assert_eq!(
            debug_names
                .name(parameters[4].value_name)
                .expect("bind parameter value"),
            Some("previous")
        );
        assert_eq!(
            parameters[4].role_flags,
            SCENE_BINARY_PARAMETER_ROLE_PASS_BIND
        );
        assert_eq!(parameters[4].integer_value, 0);
        let pass_parameters = layout
            .effect_parameter_record_range(&bytes, effect_passes[0])
            .expect("effect pass parameter range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded effect pass parameter range");
        assert_eq!(pass_parameters.len(), 4);
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
        assert_eq!(flutter[0].parameter_count, 5);
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
        assert_eq!(flutter_parameters.len(), 5);
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
    fn binary_material_omits_explicitly_hidden_effects() {
        let document = SceneDocument {
            version: SCENE_BINARY_VERSION as u32,
            profile: SceneProfile::NativeVulkanFullScene,
            source: SceneSourceMetadata::default(),
            size: None,
            render: SceneRenderSettings::default(),
            camera: SceneCamera::default(),
            import: SceneImportMetadata::default(),
            properties: BTreeMap::new(),
            resources: vec![SceneResource {
                id: "image".to_owned(),
                kind: SceneResourceKind::Image,
                source: PackagePath::new("assets/image.gtex").unwrap(),
                width: Some(64),
                height: Some(64),
                original_source: None,
                role: None,
            }],
            nodes: vec![SceneNode {
                id: "node-48-models-6-json".to_owned(),
                kind: SceneNodeKind::Image,
                name: None,
                visible: true,
                opacity: 1.0,
                transform: SceneTransform::default(),
                provenance: Option::<SceneNodeProvenance>::None,
                resource: Some("image".to_owned()),
                effects: vec![
                    SceneEffect {
                        file: "effects/workshop/3392386920/auto_sway/effect.json".to_owned(),
                        runtime: Some("native-effect-motion".to_owned()),
                        visible: Some(json!(false)),
                        passes: vec![SceneEffectPass {
                            shader: Some("workshop/3392386920/effects/auto_sway".to_owned()),
                            texture_resources: vec![Some("image".to_owned())],
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                    SceneEffect {
                        file: "effects/waterwaves/effect.json".to_owned(),
                        runtime: Some("native-effect-motion".to_owned()),
                        visible: Some(json!(true)),
                        passes: vec![SceneEffectPass {
                            shader: Some("effects/waterwaves".to_owned()),
                            blending: Some("normal".to_owned()),
                            texture_resources: vec![Some("image".to_owned())],
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                ],
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
                properties: BTreeMap::new(),
                children: Vec::new(),
            }],
            timelines: Vec::<SceneTimeline>::new(),
            property_bindings: Vec::<ScenePropertyBinding>::new(),
            systems: SceneSystems::default(),
            native_lowering: SceneNativeLowering::default(),
            unsupported_features: Vec::new(),
        };

        let bytes = scene_binary_payloads_from_document(&document)
            .encode_container(0)
            .expect("encode");
        let layout = decode_scene_binary_container(&bytes).expect("decode");
        let debug_names = layout.debug_names(&bytes).expect("debug names");
        let nodes = layout
            .node_records(&bytes)
            .expect("node records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded node records");
        assert_eq!(nodes[0].effect_count, 1);
        let materials = layout
            .material_pass_records(&bytes)
            .expect("material records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded material records");
        assert_eq!(materials[0].effect_pass_count, 1);
        assert_eq!(materials[0].effect_kind_flags, 1 << (4 - 1));
        let effect_passes = layout
            .material_effect_pass_records(&bytes, materials[0])
            .expect("material effect pass range")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded material effect pass range");
        assert_eq!(effect_passes.len(), 1);
        assert_eq!(
            debug_names
                .name(effect_passes[0].effect_name)
                .expect("effect name"),
            Some("effects/waterwaves/effect.json")
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
    fn binary_node_flags_resolve_default_visibility_conditions() {
        let document: SceneDocument = serde_json::from_value(json!({
            "properties": {
                "theme": {
                    "type": "choice",
                    "default": "1"
                }
            },
            "nodes": [
                {
                    "id": "hidden-theme",
                    "type": "rectangle",
                    "visible": true,
                    "width": 16.0,
                    "height": 16.0,
                    "color": "#00b7ff",
                    "properties": {
                        "visibility_condition": {
                            "runtime": "wallpaper-engine-user-condition",
                            "property": "theme",
                            "condition": "2",
                            "authored_value": false,
                            "default_visible": false
                        }
                    }
                },
                {
                    "id": "active-theme",
                    "type": "rectangle",
                    "visible": true,
                    "width": 16.0,
                    "height": 16.0,
                    "color": "#ffffff",
                    "properties": {
                        "visibility_condition": {
                            "runtime": "wallpaper-engine-user-condition",
                            "property": "theme",
                            "condition": "1",
                            "authored_value": false,
                            "default_visible": true
                        }
                    }
                },
                {
                    "id": "hidden-parent",
                    "type": "group",
                    "visible": true,
                    "properties": {
                        "visibility_condition": {
                            "runtime": "wallpaper-engine-user-condition",
                            "property": "theme",
                            "condition": "2",
                            "authored_value": false,
                            "default_visible": false
                        }
                    },
                    "children": [
                        {
                            "id": "hidden-child",
                            "type": "rectangle",
                            "visible": true,
                            "width": 8.0,
                            "height": 8.0,
                            "color": "#ff00ff"
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");

        let bytes = scene_binary_payloads_from_document(&document)
            .encode_container(0)
            .expect("encode");
        let layout = decode_scene_binary_container(&bytes).expect("decode");
        let names = layout.debug_names(&bytes).expect("debug names");
        let nodes = layout
            .node_records(&bytes)
            .expect("node records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded node records");
        let visible_by_id = nodes
            .iter()
            .map(|node| {
                (
                    names.name(node.id_name).unwrap().unwrap().to_owned(),
                    node.flags & 1 != 0,
                )
            })
            .collect::<BTreeMap<_, _>>();

        assert_eq!(visible_by_id.get("hidden-theme"), Some(&false));
        assert_eq!(visible_by_id.get("active-theme"), Some(&true));
        assert_eq!(visible_by_id.get("hidden-parent"), Some(&false));
        assert_eq!(visible_by_id.get("hidden-child"), Some(&false));
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
    fn binary_particle_emitter_payload_carries_runtime_fields() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "spark", "type": "image", "source": "assets/spark.gtex", "width": 16, "height": 16 }
            ],
            "nodes": [
                {
                    "id": "spark-emitter",
                    "type": "particle-emitter",
                    "resource": "spark",
                    "properties": {
                        "particle": {
                            "count": 12,
                            "seed": 77,
                            "lifetime_ms": 1500,
                            "loop": false,
                            "spawn_width": 100.0,
                            "spawn_height": 50.0,
                            "width": 6.0,
                            "height": 8.0,
                            "speed_min": 2.0,
                            "speed_max": 5.0,
                            "direction_deg": -45.0,
                            "spread_deg": 30.0,
                            "gravity_x": 0.5,
                            "gravity_y": -1.5,
                            "fade": true,
                            "color": "#123456",
                            "shape": "ellipse"
                        }
                    }
                }
            ]
        }))
        .expect("scene document");

        let payloads = scene_binary_payloads_from_document(&document);
        assert_eq!(payloads.shape.particle_emitter_records, 1);
        assert_eq!(
            payloads
                .chunk(SceneBinaryChunkKind::ParticleEmitter)
                .expect("particle payload")
                .bytes
                .len(),
            SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE
        );

        let bytes = payloads.encode_container(0).expect("encode");
        assert!(
            !bytes
                .windows("spawn_width".len())
                .any(|window| window == b"spawn_width")
        );
        let layout = decode_scene_binary_container(&bytes).expect("decode");
        let nodes = layout
            .node_records(&bytes)
            .expect("node records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded nodes");
        assert_eq!(nodes[0].particle_index, 0);
        let particles = layout
            .particle_emitter_records(&bytes)
            .expect("particle records")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded particles");
        assert_eq!(particles.len(), 1);
        assert_eq!(particles[0].count, 12);
        assert_eq!(particles[0].seed, 77);
        assert_eq!(particles[0].lifetime_ms, 1500);
        assert_eq!(particles[0].spawn_width, 100.0);
        assert_eq!(particles[0].spawn_height, 50.0);
        assert_eq!(particles[0].particle_width, 6.0);
        assert_eq!(particles[0].particle_height, 8.0);
        assert_eq!(particles[0].speed_min, 2.0);
        assert_eq!(particles[0].speed_max, 5.0);
        assert_eq!(particles[0].direction_deg, -45.0);
        assert_eq!(particles[0].spread_deg, 30.0);
        assert_eq!(particles[0].gravity_x, 0.5);
        assert_eq!(particles[0].gravity_y, -1.5);
        assert_eq!(particles[0].color_rgba, 0x123456ff);
        assert_eq!(particles[0].flags, SCENE_BINARY_PARTICLE_FLAG_FADE);
        assert_eq!(particles[0].shape, SCENE_BINARY_PARTICLE_SHAPE_ELLIPSE);
        assert!(particles[0].opacity_and_transform_at(2_000, 0).is_some());
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
                        ],
                        "puppet_clipping_records": [
                            {
                                "mask": "masks/clipping_mask_eye",
                                "source_name": "eye-right",
                                "duration_frames": 1680,
                                "flags": 3,
                                "bones": [0, 1],
                                "frame_keys": [0, 1, 2]
                            }
                        ],
                        "puppet_clipping_active_sources": [
                            {
                                "source_name": "eye-right",
                                "source_id": 1234605616436508552u64,
                                "scalar_bits": 1065353216,
                                "source_scale": 6,
                                "flags": 2,
                                "transform_index": 4,
                                "parameter0": -1.0,
                                "parameter1": 0.5
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
                    ],
                    "children": [
                        {
                            "id": "socket-child",
                            "type": "group",
                            "puppet_attachment": "socket"
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
        assert_eq!(payloads.shape.puppet_clipping_records, 1);
        assert_eq!(payloads.shape.puppet_clipping_bone_records, 2);
        assert_eq!(payloads.shape.puppet_clipping_frame_key_records, 3);
        assert_eq!(payloads.shape.puppet_active_source_records, 1);
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
        assert_ne!(nodes[1].puppet_attachment_name, SCENE_BINARY_NONE_ID);
        let names = layout.debug_names(&bytes).expect("debug names");
        assert_eq!(
            names
                .name(nodes[1].puppet_attachment_name)
                .expect("attachment name"),
            Some("socket")
        );
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
        assert_eq!(puppet.clipping_record_count, 1);
        assert_eq!(puppet.clipping_bone_count, 2);
        assert_eq!(puppet.clipping_frame_key_count, 3);
        assert_eq!(puppet.active_source_count, 1);
        assert!(puppet.flags & SCENE_BINARY_PUPPET_FLAG_MESH != 0);
        assert!(puppet.flags & SCENE_BINARY_PUPPET_FLAG_SKIN != 0);
        assert!(puppet.flags & SCENE_BINARY_PUPPET_FLAG_CLIPS != 0);
        assert!(puppet.flags & SCENE_BINARY_PUPPET_FLAG_ATTACHMENTS != 0);
        assert!(puppet.flags & SCENE_BINARY_PUPPET_FLAG_ANIMATION_LAYERS != 0);
        assert!(puppet.flags & SCENE_BINARY_PUPPET_FLAG_CLIPPING != 0);

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

        let clipping = layout
            .puppet_clipping_record_range(&bytes, puppet)
            .expect("clipping")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded clipping");
        assert_eq!(clipping[0].duration_frames, 1680);
        assert_eq!(clipping[0].flags, 3);
        assert_eq!(
            names.name(clipping[0].mask_name).expect("clipping mask"),
            Some("masks/clipping_mask_eye")
        );
        assert_eq!(
            names.name(clipping[0].owner_name).expect("clipping source"),
            Some("eye-right")
        );
        let clipping_bones = layout
            .puppet_clipping_bone_record_range(&bytes, clipping[0])
            .expect("clipping bones")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded clipping bones");
        assert_eq!(
            clipping_bones
                .iter()
                .map(|record| record.bone_index)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        let clipping_frame_keys = layout
            .puppet_clipping_frame_key_record_range(&bytes, clipping[0])
            .expect("clipping frame keys")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded clipping frame keys");
        assert_eq!(
            clipping_frame_keys
                .iter()
                .map(|record| record.frame_key)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        let active_sources = layout
            .puppet_active_source_record_range(&bytes, puppet)
            .expect("active sources")
            .collect::<Result<Vec<_>, _>>()
            .expect("decoded active sources");
        assert_eq!(
            names
                .name(active_sources[0].source_name)
                .expect("active source name"),
            Some("eye-right")
        );
        assert_eq!(active_sources[0].source_id, 0x1122_3344_5566_7788);
        assert_eq!(active_sources[0].scalar_bits, 1.0f32.to_bits());
        assert_eq!(active_sources[0].source_scale, 6);
        assert_eq!(active_sources[0].flags, 2);
        assert_eq!(active_sources[0].transform_index, 4);
        assert_eq!(active_sources[0].parameter0, -1.0);
        assert_eq!(active_sources[0].parameter1, 0.5);

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
    fn binary_material_pass_keeps_scene_alpha_when_effect_material_blends_normal() {
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
                    "properties": {
                        "material": {
                            "passes": [{
                                "shader": "genericimage4",
                                "blending": "translucent",
                                "depthtest": "disabled",
                                "depthwrite": "disabled",
                                "cullmode": "nocull"
                            }]
                        }
                    },
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
            blend_mode_code(SceneBlendMode::Alpha)
        );
        assert_eq!(
            layout
                .debug_names(&bytes)
                .expect("debug names")
                .name(materials[0].shader_name)
                .expect("shader name"),
            Some("genericimage4")
        );
        assert_eq!(
            layout
                .debug_names(&bytes)
                .expect("debug names")
                .name(materials[0].blending_name)
                .expect("blending name"),
            Some("translucent")
        );
    }
}

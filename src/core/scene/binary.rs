use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use super::{
    SceneAlphaTextureMode, SceneAnimatedProperty, SceneBlendMode, SceneDocument, SceneEffect,
    SceneEffectPass, SceneNode, SceneNodeKind, SceneResourceKind, SceneTimelineChannel,
};

pub const SCENE_BINARY_MAGIC: [u8; 4] = *b"GSCN";
pub const SCENE_BINARY_VERSION: u16 = 1;
pub const SCENE_BINARY_ENDIAN_LITTLE: u8 = 1;
pub const SCENE_BINARY_ALIGNMENT: u8 = 8;
pub const SCENE_BINARY_HEADER_SIZE: usize = 24;
pub const SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE: usize = 24;
pub const SCENE_BINARY_RESOURCE_RECORD_SIZE: usize = 32;
pub const SCENE_BINARY_NODE_RECORD_SIZE: usize = 48;
pub const SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE: usize = 72;
pub const SCENE_BINARY_GEOMETRY_RECORD_SIZE: usize = 32;
pub const SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE: usize = 32;
pub const SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE: usize = 32;
pub const SCENE_BINARY_EFFECT_PASS_RECORD_SIZE: usize = 40;
pub const SCENE_BINARY_FLUTTER_STATE_RECORD_SIZE: usize = 24;
pub const SCENE_BINARY_PUPPET_RECORD_SIZE: usize = 24;
pub const SCENE_BINARY_RENDER_STATE_RECORD_SIZE: usize = 32;
pub const SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE: usize = 24;
pub const SCENE_BINARY_DEBUG_NAME_RECORD_SIZE: usize = 16;

const SCENE_BINARY_NONE_ID: u32 = u32::MAX;
const SCENE_BINARY_DEFAULT_TRANSFORM_PROPERTY: u16 = 0;
const SCENE_BINARY_RETAINED_RESOURCE: u16 = 1;
const SCENE_BINARY_RETAINED_TEXTURE_SLOT: u16 = 2;
const SCENE_BINARY_RETAINED_MATERIAL_PASS: u16 = 3;
const SCENE_BINARY_RETAINED_EFFECT_PASS: u16 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SceneBinaryChunkKind {
    ResourceTable,
    NodeTable,
    TransformTimeline,
    Geometry,
    TextureSlots,
    MaterialPass,
    EffectPass,
    FlutterState,
    Puppet,
    RenderState,
    RetainedGpuState,
    DebugNames,
}

impl SceneBinaryChunkKind {
    pub const REQUIRED_ORDER: [Self; 12] = [
        Self::ResourceTable,
        Self::NodeTable,
        Self::TransformTimeline,
        Self::Geometry,
        Self::TextureSlots,
        Self::MaterialPass,
        Self::EffectPass,
        Self::FlutterState,
        Self::Puppet,
        Self::RenderState,
        Self::RetainedGpuState,
        Self::DebugNames,
    ];

    pub fn code(self) -> u32 {
        match self {
            Self::ResourceTable => u32::from_le_bytes(*b"REST"),
            Self::NodeTable => u32::from_le_bytes(*b"NODE"),
            Self::TransformTimeline => u32::from_le_bytes(*b"XFRM"),
            Self::Geometry => u32::from_le_bytes(*b"GEOM"),
            Self::TextureSlots => u32::from_le_bytes(*b"TEXS"),
            Self::MaterialPass => u32::from_le_bytes(*b"MATP"),
            Self::EffectPass => u32::from_le_bytes(*b"EFTP"),
            Self::FlutterState => u32::from_le_bytes(*b"FLUT"),
            Self::Puppet => u32::from_le_bytes(*b"PUPT"),
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
            Self::Geometry => "geometry",
            Self::TextureSlots => "texture_slots",
            Self::MaterialPass => "material_pass",
            Self::EffectPass => "effect_pass",
            Self::FlutterState => "flutter_state",
            Self::Puppet => "puppet",
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
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryNodeRecord {
    pub id_name: u32,
    pub display_name: u32,
    pub parent_index: u32,
    pub resource_name: u32,
    pub kind: u16,
    pub flags: u16,
    pub draw_order: u32,
    pub child_count: u32,
    pub effect_count: u32,
    pub audio_count: u32,
    pub property_count: u32,
    pub material_index: u32,
    pub geometry_index: u32,
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
        write_u32(out, self.effect_count);
        write_u32(out, self.audio_count);
        write_u32(out, self.property_count);
        write_u32(out, self.material_index);
        write_u32(out, self.geometry_index);
        debug_assert_eq!(SCENE_BINARY_NODE_RECORD_SIZE, 48);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryTransformTimelineRecord {
    pub owner_name: u32,
    pub timeline_name: u32,
    pub property: u16,
    pub flags: u16,
    pub keyframe_count: u32,
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
        debug_assert_eq!(SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE, 72);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryGeometryRecord {
    pub owner_name: u32,
    pub kind: u16,
    pub flags: u16,
    pub width: f32,
    pub height: f32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub material_uv_count: u32,
    pub topology_id: u32,
}

impl SceneBinaryGeometryRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u16(out, self.kind);
        write_u16(out, self.flags);
        write_f32(out, self.width);
        write_f32(out, self.height);
        write_u32(out, self.vertex_count);
        write_u32(out, self.index_count);
        write_u32(out, self.material_uv_count);
        write_u32(out, self.topology_id);
        debug_assert_eq!(SCENE_BINARY_GEOMETRY_RECORD_SIZE, 32);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryTextureSlotRecord {
    pub owner_name: u32,
    pub pass_name: u32,
    pub resource_name: u32,
    pub texture_name: u32,
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
        write_u32(out, self.resource_name);
        write_u32(out, self.texture_name);
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
    pub flags: u32,
    pub texture_slot_count: u32,
    pub effect_pass_count: u32,
    pub first_texture_slot: u32,
    pub blend_mode: u16,
    pub alpha_texture_mode: u16,
}

impl SceneBinaryMaterialPassRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.shader_name);
        write_u32(out, self.blending_name);
        write_u32(out, self.flags);
        write_u32(out, self.texture_slot_count);
        write_u32(out, self.effect_pass_count);
        write_u32(out, self.first_texture_slot);
        write_u16(out, self.blend_mode);
        write_u16(out, self.alpha_texture_mode);
        debug_assert_eq!(SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE, 32);
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
    pub combo_count: u32,
    pub parameter_count: u32,
    pub kind: u16,
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
        write_u32(out, self.combo_count);
        write_u32(out, self.parameter_count);
        write_u16(out, self.kind);
        write_u16(out, self.flags);
        debug_assert_eq!(SCENE_BINARY_EFFECT_PASS_RECORD_SIZE, 40);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryFlutterStateRecord {
    pub owner_name: u32,
    pub effect_name: u32,
    pub pass_count: u32,
    pub parameter_count: u32,
    pub family_flags: u32,
    pub reserved: u32,
}

impl SceneBinaryFlutterStateRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.effect_name);
        write_u32(out, self.pass_count);
        write_u32(out, self.parameter_count);
        write_u32(out, self.family_flags);
        write_u32(out, self.reserved);
        debug_assert_eq!(SCENE_BINARY_FLUTTER_STATE_RECORD_SIZE, 24);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryPuppetRecord {
    pub owner_name: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub animation_layer_count: u32,
    pub flags: u32,
    pub reserved: u32,
}

impl SceneBinaryPuppetRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.vertex_count);
        write_u32(out, self.index_count);
        write_u32(out, self.animation_layer_count);
        write_u32(out, self.flags);
        write_u32(out, self.reserved);
        debug_assert_eq!(SCENE_BINARY_PUPPET_RECORD_SIZE, 24);
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
    pub reserved: u32,
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
        write_u32(out, self.reserved);
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
    pub reserved: u32,
}

impl SceneBinaryRetainedGpuStateRecord {
    fn encode(self, out: &mut Vec<u8>) {
        write_u16(out, self.owner_kind);
        write_u16(out, self.flags);
        write_u32(out, self.owner_name);
        write_u64(out, self.stable_id);
        write_u32(out, self.record_index);
        write_u32(out, self.reserved);
        debug_assert_eq!(SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE, 24);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneBinaryDocumentShape {
    pub resource_table_records: u32,
    pub node_table_records: u32,
    pub transform_timeline_records: u32,
    pub geometry_records: u32,
    pub texture_slot_records: u32,
    pub material_pass_records: u32,
    pub effect_pass_records: u32,
    pub flutter_state_records: u32,
    pub puppet_records: u32,
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
            render_state_records: 1,
            debug_name_records: saturating_u32(document.resources.len()),
            ..Default::default()
        };
        for node in &document.nodes {
            shape.include_node(node);
        }
        shape.retained_gpu_state_records = shape
            .resource_table_records
            .saturating_add(shape.texture_slot_records)
            .saturating_add(shape.material_pass_records)
            .saturating_add(shape.effect_pass_records);
        shape
    }

    pub fn record_count(self, kind: SceneBinaryChunkKind) -> u32 {
        match kind {
            SceneBinaryChunkKind::ResourceTable => self.resource_table_records,
            SceneBinaryChunkKind::NodeTable => self.node_table_records,
            SceneBinaryChunkKind::TransformTimeline => self.transform_timeline_records,
            SceneBinaryChunkKind::Geometry => self.geometry_records,
            SceneBinaryChunkKind::TextureSlots => self.texture_slot_records,
            SceneBinaryChunkKind::MaterialPass => self.material_pass_records,
            SceneBinaryChunkKind::EffectPass => self.effect_pass_records,
            SceneBinaryChunkKind::FlutterState => self.flutter_state_records,
            SceneBinaryChunkKind::Puppet => self.puppet_records,
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
        }
        if node_has_material(node) {
            self.material_pass_records = self.material_pass_records.saturating_add(1);
        }
        if node.mesh.is_some() || !node.puppet_animation_layers.is_empty() {
            self.puppet_records = self.puppet_records.saturating_add(1);
        }
        for effect in &node.effects {
            self.include_effect(effect);
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
        if effect_is_motion_family(effect) {
            self.flutter_state_records = self.flutter_state_records.saturating_add(1);
        }
        for pass in &effect.passes {
            self.texture_slot_records = self
                .texture_slot_records
                .saturating_add(effect_pass_texture_slot_count(pass));
        }
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
    geometry: SceneBinaryChunkWriter,
    texture_slots: SceneBinaryChunkWriter,
    material_pass: SceneBinaryChunkWriter,
    effect_pass: SceneBinaryChunkWriter,
    flutter_state: SceneBinaryChunkWriter,
    puppet: SceneBinaryChunkWriter,
    render_state: SceneBinaryChunkWriter,
    retained_gpu_state: SceneBinaryChunkWriter,
}

impl SceneBinaryPayloadBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn include_document(&mut self, document: &SceneDocument) {
        for resource in &document.resources {
            self.include_resource(resource_id_fields(resource));
        }
        let mut draw_order = 0;
        for node in &document.nodes {
            self.include_node(node, None, &mut draw_order);
        }
        for timeline in &document.timelines {
            let timeline_name = self
                .names
                .intern(SceneBinaryNameKind::Timeline, &timeline.id);
            let owner_name = self
                .names
                .intern_optional(SceneBinaryNameKind::NodeId, timeline.target_node.as_deref());
            for channel in &timeline.channels {
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
                reserved: 0,
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

    fn include_node(&mut self, node: &SceneNode, parent_index: Option<u32>, draw_order: &mut u32) {
        let node_index = self.node_table.record_count;
        let id_name = self.names.intern(SceneBinaryNameKind::NodeId, &node.id);
        let display_name = self
            .names
            .intern_optional(SceneBinaryNameKind::DisplayName, node.name.as_deref());
        let resource_name = self
            .names
            .intern_optional(SceneBinaryNameKind::ResourceId, node.resource.as_deref());
        let base_texture_start = if node.resource.is_some() {
            let first = self.texture_slots.record_count;
            self.push_texture_slot(SceneBinaryTextureSlotRecord {
                owner_name: id_name,
                pass_name: SCENE_BINARY_NONE_ID,
                resource_name,
                texture_name: SCENE_BINARY_NONE_ID,
                slot: 0,
                width: 0,
                height: 0,
                role_flags: 1,
                sampler_flags: 0,
            });
            first
        } else {
            SCENE_BINARY_NONE_ID
        };
        let geometry_index = if node_has_geometry(node) {
            self.push_geometry(id_name, node)
        } else {
            SCENE_BINARY_NONE_ID
        };
        let material_index = if node_has_material(node) {
            let effect_count = node_effect_pass_count(&node.effects);
            let texture_count = u32::from(node.resource.is_some());
            let index = self.material_pass.record_count;
            self.material_pass.push_record(|out| {
                SceneBinaryMaterialPassRecord {
                    owner_name: id_name,
                    shader_name: SCENE_BINARY_NONE_ID,
                    blending_name: SCENE_BINARY_NONE_ID,
                    flags: u32::from(node.resource.is_some())
                        | (u32::from(!node.effects.is_empty()) << 1),
                    texture_slot_count: texture_count,
                    effect_pass_count: effect_count,
                    first_texture_slot: base_texture_start,
                    blend_mode: blend_mode_code(SceneBlendMode::Alpha),
                    alpha_texture_mode: alpha_texture_mode_code(SceneAlphaTextureMode::Multiply),
                }
                .encode(out)
            });
            self.push_retained(SCENE_BINARY_RETAINED_MATERIAL_PASS, id_name, index);
            index
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
                effect_count: saturating_u32(node.effects.len()),
                audio_count: saturating_u32(node.audio.len()),
                property_count: saturating_u32(node.properties.len()),
                material_index,
                geometry_index,
            }
            .encode(out)
        });
        self.push_default_transform(id_name, node);
        if node.mesh.is_some() || !node.puppet_animation_layers.is_empty() {
            self.push_puppet(id_name, node);
        }
        *draw_order = draw_order.saturating_add(1);
        for effect in &node.effects {
            self.include_effect(id_name, effect);
        }
        for child in &node.children {
            self.include_node(child, Some(node_index), draw_order);
        }
    }

    fn include_effect(&mut self, owner_name: u32, effect: &SceneEffect) {
        let effect_name = self
            .names
            .intern(SceneBinaryNameKind::EffectFile, &effect.file);
        if effect.passes.is_empty() {
            self.push_effect_record(
                owner_name,
                effect_name,
                effect,
                None,
                0,
                SCENE_BINARY_NONE_ID,
                0,
            );
        } else {
            for (pass_index, pass) in effect.passes.iter().enumerate() {
                let first_texture_slot = self.texture_slots.record_count;
                let texture_slot_count =
                    self.push_effect_texture_slots(owner_name, effect_name, pass);
                self.push_effect_record(
                    owner_name,
                    effect_name,
                    effect,
                    Some(pass),
                    pass_index,
                    first_texture_slot,
                    texture_slot_count,
                );
            }
        }
        if effect_is_motion_family(effect) {
            self.flutter_state.push_record(|out| {
                SceneBinaryFlutterStateRecord {
                    owner_name,
                    effect_name,
                    pass_count: saturating_u32(effect.passes.len().max(1)),
                    parameter_count: effect_parameter_count(effect),
                    family_flags: effect_motion_family_flags(effect),
                    reserved: 0,
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
        let combo_count = pass.map_or(0, |pass| saturating_u32(pass.combos.len()));
        let parameter_count =
            pass.map_or(0, |pass| saturating_u32(pass.constant_shader_values.len()));
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
                combo_count,
                parameter_count,
                kind: effect_kind_code(effect),
                flags: effect_flags(effect, pass),
            }
            .encode(out)
        });
        self.push_retained(SCENE_BINARY_RETAINED_EFFECT_PASS, effect_name, record_index);
    }

    fn push_effect_texture_slots(
        &mut self,
        owner_name: u32,
        effect_name: u32,
        pass: &SceneEffectPass,
    ) -> u32 {
        let before = self.texture_slots.record_count;
        let slot_count = pass.textures.len().max(pass.texture_resources.len());
        for slot in 0..slot_count {
            let texture_name = pass
                .textures
                .get(slot)
                .and_then(|value| value.as_deref())
                .map_or(SCENE_BINARY_NONE_ID, |texture| {
                    self.names
                        .intern(SceneBinaryNameKind::ResourcePath, texture)
                });
            let resource_name = pass
                .texture_resources
                .get(slot)
                .and_then(|value| value.as_deref())
                .map_or(SCENE_BINARY_NONE_ID, |resource| {
                    self.names.intern(SceneBinaryNameKind::ResourceId, resource)
                });
            if texture_name == SCENE_BINARY_NONE_ID && resource_name == SCENE_BINARY_NONE_ID {
                continue;
            }
            self.push_texture_slot(SceneBinaryTextureSlotRecord {
                owner_name,
                pass_name: effect_name,
                resource_name,
                texture_name,
                slot: slot.min(u32::MAX as usize) as u32,
                width: 0,
                height: 0,
                role_flags: 2,
                sampler_flags: 0,
            });
        }
        self.texture_slots.record_count.saturating_sub(before)
    }

    fn push_texture_slot(&mut self, record: SceneBinaryTextureSlotRecord) {
        let owner_name = record.owner_name;
        let record_index = self.texture_slots.push_record(|out| record.encode(out));
        self.push_retained(SCENE_BINARY_RETAINED_TEXTURE_SLOT, owner_name, record_index);
    }

    fn push_geometry(&mut self, owner_name: u32, node: &SceneNode) -> u32 {
        self.geometry.push_record(|out| {
            let (vertex_count, index_count) = node.mesh.as_ref().map_or((0, 0), |mesh| {
                (
                    saturating_u32(mesh.vertices.len()),
                    saturating_u32(mesh.indices.len()),
                )
            });
            SceneBinaryGeometryRecord {
                owner_name,
                kind: node_kind_code(node.kind),
                flags: geometry_flags(node),
                width: node.width.unwrap_or(0.0) as f32,
                height: node.height.unwrap_or(0.0) as f32,
                vertex_count,
                index_count,
                material_uv_count: u32::from(node.mesh.is_some()),
                topology_id: vertex_count ^ index_count.rotate_left(16),
            }
            .encode(out)
        })
    }

    fn push_default_transform(&mut self, owner_name: u32, node: &SceneNode) {
        self.transform_timeline.push_record(|out| {
            SceneBinaryTransformTimelineRecord {
                owner_name,
                timeline_name: SCENE_BINARY_NONE_ID,
                property: SCENE_BINARY_DEFAULT_TRANSFORM_PROPERTY,
                flags: 0,
                keyframe_count: 0,
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

    fn push_puppet(&mut self, owner_name: u32, node: &SceneNode) {
        self.puppet.push_record(|out| {
            let (vertex_count, index_count) = node.mesh.as_ref().map_or((0, 0), |mesh| {
                (
                    saturating_u32(mesh.vertices.len()),
                    saturating_u32(mesh.indices.len()),
                )
            });
            SceneBinaryPuppetRecord {
                owner_name,
                vertex_count,
                index_count,
                animation_layer_count: saturating_u32(node.puppet_animation_layers.len()),
                flags: u32::from(node.mesh.is_some())
                    | (u32::from(!node.puppet_animation_layers.is_empty()) << 1),
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
                reserved: 0,
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
            geometry_records: self.geometry.record_count,
            texture_slot_records: self.texture_slots.record_count,
            material_pass_records: self.material_pass.record_count,
            effect_pass_records: self.effect_pass.record_count,
            flutter_state_records: self.flutter_state.record_count,
            puppet_records: self.puppet.record_count,
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
                self.geometry.into_payload(SceneBinaryChunkKind::Geometry),
                self.texture_slots
                    .into_payload(SceneBinaryChunkKind::TextureSlots),
                self.material_pass
                    .into_payload(SceneBinaryChunkKind::MaterialPass),
                self.effect_pass
                    .into_payload(SceneBinaryChunkKind::EffectPass),
                self.flutter_state
                    .into_payload(SceneBinaryChunkKind::FlutterState),
                self.puppet.into_payload(SceneBinaryChunkKind::Puppet),
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
    UnknownChunk {
        code: u32,
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
            Self::UnknownChunk { code } => write!(f, "unknown scene binary chunk code {code:#x}"),
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
            container_len: bytes.len(),
        }
    })?;
    let table_size = usize::try_from(chunk_count)
        .ok()
        .and_then(|count| count.checked_mul(SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE))
        .ok_or(SceneBinaryError::ChunkTableOutOfBounds {
            offset: chunk_table_offset,
            count: chunk_count,
            container_len: bytes.len(),
        })?;
    let table_end =
        table_start
            .checked_add(table_size)
            .ok_or(SceneBinaryError::ChunkTableOutOfBounds {
                offset: chunk_table_offset,
                count: chunk_count,
                container_len: bytes.len(),
            })?;
    if table_end > bytes.len() {
        return Err(SceneBinaryError::ChunkTableOutOfBounds {
            offset: chunk_table_offset,
            count: chunk_count,
            container_len: bytes.len(),
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
        validate_chunk_bounds(bytes, alignment, table_end, chunks.last(), &chunk)?;
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

fn node_flags(node: &SceneNode) -> u16 {
    u16::from(node.visible)
        | (u16::from(node.resource.is_some()) << 1)
        | (u16::from(!node.effects.is_empty()) << 2)
        | (u16::from(!node.children.is_empty()) << 3)
        | (u16::from(node.mesh.is_some()) << 4)
        | (u16::from(!node.puppet_animation_layers.is_empty()) << 5)
        | (u16::from(!node.audio.is_empty()) << 6)
}

fn geometry_flags(node: &SceneNode) -> u16 {
    u16::from(node.width.is_some())
        | (u16::from(node.height.is_some()) << 1)
        | (u16::from(node.mesh.is_some()) << 2)
        | (u16::from(node.path_data.is_some()) << 3)
        | (u16::from(node.text.is_some()) << 4)
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
    } else {
        11
    }
}

fn effect_motion_family_flags(effect: &SceneEffect) -> u32 {
    let file = effect.file.to_ascii_lowercase();
    let runtime = effect.runtime.as_deref().unwrap_or_default();
    u32::from(file.contains("flutter") || runtime.contains("flutter"))
        | (u32::from(file.contains("sway") || runtime.contains("sway")) << 1)
        | (u32::from(file.contains("shake") || runtime.contains("shake")) << 2)
        | (u32::from(file.contains("drift") || runtime.contains("drift")) << 3)
}

fn effect_parameter_count(effect: &SceneEffect) -> u32 {
    let pass_parameter_count = effect
        .passes
        .iter()
        .map(|pass| pass.constant_shader_values.len())
        .sum::<usize>();
    saturating_u32(effect.properties.len().saturating_add(pass_parameter_count))
}

fn node_effect_pass_count(effects: &[SceneEffect]) -> u32 {
    saturating_u32(
        effects
            .iter()
            .map(|effect| effect.passes.len().max(1))
            .sum::<usize>(),
    )
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
    bytes: &[u8],
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
        container_len: bytes.len(),
    })?;
    let length = usize::try_from(chunk.length).map_err(|_| SceneBinaryError::ChunkOutOfBounds {
        kind: chunk.kind,
        offset: chunk.offset,
        length: chunk.length,
        container_len: bytes.len(),
    })?;
    let end = start
        .checked_add(length)
        .ok_or(SceneBinaryError::ChunkOutOfBounds {
            kind: chunk.kind,
            offset: chunk.offset,
            length: chunk.length,
            container_len: bytes.len(),
        })?;
    if start < payload_min_offset || end > bytes.len() {
        return Err(SceneBinaryError::ChunkOutOfBounds {
            kind: chunk.kind,
            offset: chunk.offset,
            length: chunk.length,
            container_len: bytes.len(),
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
                container_len: bytes.len(),
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

fn align_usize(value: usize, alignment: usize) -> usize {
    debug_assert!(alignment.is_power_of_two());
    (value + alignment - 1) & !(alignment - 1)
}

fn node_has_geometry(node: &SceneNode) -> bool {
    matches!(
        node.kind,
        SceneNodeKind::Image
            | SceneNodeKind::Video
            | SceneNodeKind::Color
            | SceneNodeKind::Rectangle
            | SceneNodeKind::Ellipse
            | SceneNodeKind::Text
            | SceneNodeKind::Path
            | SceneNodeKind::ParticleEmitter
            | SceneNodeKind::AudioResponse
    )
}

fn node_has_material(node: &SceneNode) -> bool {
    node_has_geometry(node) || node.resource.is_some() || !node.effects.is_empty()
}

fn effect_is_motion_family(effect: &SceneEffect) -> bool {
    let file = effect.file.to_ascii_lowercase();
    let runtime = effect.runtime.as_deref().unwrap_or_default();
    file.contains("flutter")
        || file.contains("sway")
        || file.contains("shake")
        || file.contains("drift")
        || runtime.contains("flutter")
        || runtime.contains("sway")
        || runtime.contains("shake")
        || runtime.contains("drift")
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
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
                    passes: vec![SceneEffectPass {
                        textures: vec![Some("g_Texture0".to_owned())],
                        texture_resources: vec![Some("image".to_owned())],
                        constant_shader_values: BTreeMap::from([("speed".to_owned(), json!(1.0))]),
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
                properties: BTreeMap::new(),
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
        assert_eq!(shape.geometry_records, 1);
        assert_eq!(shape.texture_slot_records, 2);
        assert_eq!(shape.material_pass_records, 1);
        assert_eq!(shape.effect_pass_records, 1);
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
            2 * SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE
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
            2
        );
    }
}

use super::{
    SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE, SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
    SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE, SCENE_BINARY_EFFECT_PASS_RECORD_SIZE,
    SCENE_BINARY_EFFECT_PASS_RECORD_SIZE_V12, SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE,
    SCENE_BINARY_FLUTTER_STATE_RECORD_SIZE, SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
    SCENE_BINARY_GEOMETRY_RECORD_SIZE, SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
    SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE, SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE_V12,
    SCENE_BINARY_NODE_RECORD_SIZE, SCENE_BINARY_NODE_RECORD_SIZE_V12,
    SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE, SCENE_BINARY_PUPPET_ACTIVE_SOURCE_RECORD_SIZE,
    SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE, SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE,
    SCENE_BINARY_PUPPET_CLIPPING_BONE_RECORD_SIZE,
    SCENE_BINARY_PUPPET_CLIPPING_FRAME_KEY_RECORD_SIZE, SCENE_BINARY_PUPPET_CLIPPING_RECORD_SIZE,
    SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE, SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE,
    SCENE_BINARY_PUPPET_RECORD_SIZE, SCENE_BINARY_PUPPET_RECORD_SIZE_V12,
    SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE, SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE,
    SCENE_BINARY_RENDER_STATE_RECORD_SIZE, SCENE_BINARY_RESOURCE_RECORD_SIZE,
    SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE, SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
    SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE, SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
    SCENE_BINARY_VERSION, SCENE_BINARY_VERSION_V12, SceneBinaryDocumentShape, SceneBinaryError,
    encode_scene_binary_container,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SceneBinaryChunkKind {
    ResourceTable,
    NodeTable,
    TransformTimeline,
    TransformKeyframes,
    Geometry,
    GeometryVertices,
    GeometryIndices,
    ParticleEmitter,
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
    PuppetClipping,
    PuppetClippingBones,
    PuppetClippingFrameKeys,
    PuppetActiveSources,
    RenderState,
    RetainedGpuState,
    DebugNames,
}

impl SceneBinaryChunkKind {
    pub const REQUIRED_ORDER: [Self; 28] = [
        Self::ResourceTable,
        Self::NodeTable,
        Self::TransformTimeline,
        Self::TransformKeyframes,
        Self::Geometry,
        Self::GeometryVertices,
        Self::GeometryIndices,
        Self::ParticleEmitter,
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
        Self::PuppetClipping,
        Self::PuppetClippingBones,
        Self::PuppetClippingFrameKeys,
        Self::PuppetActiveSources,
        Self::RenderState,
        Self::RetainedGpuState,
        Self::DebugNames,
    ];

    pub const REQUIRED_ORDER_V12: [Self; 24] = [
        Self::ResourceTable,
        Self::NodeTable,
        Self::TransformTimeline,
        Self::TransformKeyframes,
        Self::Geometry,
        Self::GeometryVertices,
        Self::GeometryIndices,
        Self::ParticleEmitter,
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
            Self::ParticleEmitter => u32::from_le_bytes(*b"PART"),
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
            Self::PuppetClipping => u32::from_le_bytes(*b"PCLM"),
            Self::PuppetClippingBones => u32::from_le_bytes(*b"PCBN"),
            Self::PuppetClippingFrameKeys => u32::from_le_bytes(*b"PCFK"),
            Self::PuppetActiveSources => u32::from_le_bytes(*b"PCAS"),
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

    pub fn required_order_for_version(version: u16) -> Option<&'static [Self]> {
        match version {
            SCENE_BINARY_VERSION_V12 => Some(&Self::REQUIRED_ORDER_V12),
            SCENE_BINARY_VERSION => Some(&Self::REQUIRED_ORDER),
            _ => None,
        }
    }

    pub fn record_size_for_version(self, version: u16) -> Option<usize> {
        Some(match self {
            Self::ResourceTable => SCENE_BINARY_RESOURCE_RECORD_SIZE,
            Self::NodeTable => match version {
                SCENE_BINARY_VERSION_V12 => SCENE_BINARY_NODE_RECORD_SIZE_V12,
                _ => SCENE_BINARY_NODE_RECORD_SIZE,
            },
            Self::TransformTimeline => SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
            Self::TransformKeyframes => SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE,
            Self::Geometry => SCENE_BINARY_GEOMETRY_RECORD_SIZE,
            Self::GeometryVertices => SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
            Self::GeometryIndices => SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
            Self::ParticleEmitter => SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE,
            Self::TextureSlots => SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
            Self::MaterialPass => match version {
                SCENE_BINARY_VERSION_V12 => SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE_V12,
                _ => SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE,
            },
            Self::EffectPass => match version {
                SCENE_BINARY_VERSION_V12 => SCENE_BINARY_EFFECT_PASS_RECORD_SIZE_V12,
                _ => SCENE_BINARY_EFFECT_PASS_RECORD_SIZE,
            },
            Self::EffectUvTransform => SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE,
            Self::EffectParameter => SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
            Self::FlutterState => SCENE_BINARY_FLUTTER_STATE_RECORD_SIZE,
            Self::Puppet => match version {
                SCENE_BINARY_VERSION_V12 => SCENE_BINARY_PUPPET_RECORD_SIZE_V12,
                _ => SCENE_BINARY_PUPPET_RECORD_SIZE,
            },
            Self::PuppetSkinBones => SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE,
            Self::PuppetSkinVertices => SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE,
            Self::PuppetAttachments => SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE,
            Self::PuppetClips => SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE,
            Self::PuppetFrames => SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE,
            Self::PuppetLayers => SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE,
            Self::PuppetClipping => SCENE_BINARY_PUPPET_CLIPPING_RECORD_SIZE,
            Self::PuppetClippingBones => SCENE_BINARY_PUPPET_CLIPPING_BONE_RECORD_SIZE,
            Self::PuppetClippingFrameKeys => SCENE_BINARY_PUPPET_CLIPPING_FRAME_KEY_RECORD_SIZE,
            Self::PuppetActiveSources => SCENE_BINARY_PUPPET_ACTIVE_SOURCE_RECORD_SIZE,
            Self::RenderState => SCENE_BINARY_RENDER_STATE_RECORD_SIZE,
            Self::RetainedGpuState => SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE,
            Self::DebugNames => SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
        })
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
            Self::ParticleEmitter => "particle_emitter",
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
            Self::PuppetClipping => "puppet_clipping",
            Self::PuppetClippingBones => "puppet_clipping_bones",
            Self::PuppetClippingFrameKeys => "puppet_clipping_frame_keys",
            Self::PuppetActiveSources => "puppet_active_sources",
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

impl SceneBinaryChunkPayload<'_> {
    pub(super) fn table_size(payload_count: usize) -> usize {
        payload_count
            .checked_mul(SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE)
            .and_then(|size| size.checked_add(super::SCENE_BINARY_HEADER_SIZE))
            .expect("scene binary table size overflow")
    }
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

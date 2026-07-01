use super::{
    SCENE_BINARY_NONE_ID, SceneBinaryError, ScenePuppetTransform, read_f32, read_u32, write_f32,
    write_u32,
};

pub const SCENE_BINARY_PUPPET_RECORD_SIZE: usize = 68;
pub const SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE: usize = 48;
pub const SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE: usize = 40;
pub const SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE: usize = 40;
pub const SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE: usize = 40;
pub const SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE: usize = 56;
pub const SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE: usize = 32;

pub const SCENE_BINARY_PUPPET_FLAG_MESH: u32 = 1;
pub const SCENE_BINARY_PUPPET_FLAG_ANIMATION_LAYERS: u32 = 1 << 1;
pub const SCENE_BINARY_PUPPET_FLAG_SKIN: u32 = 1 << 2;
pub const SCENE_BINARY_PUPPET_FLAG_CLIPS: u32 = 1 << 3;
pub const SCENE_BINARY_PUPPET_FLAG_ATTACHMENTS: u32 = 1 << 4;
pub const SCENE_BINARY_PUPPET_CLIP_FLAG_LOOPING: u32 = 1;
pub const SCENE_BINARY_PUPPET_LAYER_FLAG_ADDITIVE: u32 = 1;
pub const SCENE_BINARY_PUPPET_LAYER_FLAG_LOCK_TRANSFORMS: u32 = 1 << 1;
pub const SCENE_BINARY_PUPPET_LAYER_FLAG_VISIBLE: u32 = 1 << 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryPuppetRecord {
    pub owner_name: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub first_bone: u32,
    pub bone_count: u32,
    pub first_skin_vertex: u32,
    pub skin_vertex_count: u32,
    pub first_attachment: u32,
    pub attachment_count: u32,
    pub first_clip: u32,
    pub clip_count: u32,
    pub first_clip_frame: u32,
    pub clip_frame_count: u32,
    pub first_layer: u32,
    pub animation_layer_count: u32,
    pub flags: u32,
    pub dirty_range_count: u32,
}

impl SceneBinaryPuppetRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.vertex_count);
        write_u32(out, self.index_count);
        write_u32(out, self.first_bone);
        write_u32(out, self.bone_count);
        write_u32(out, self.first_skin_vertex);
        write_u32(out, self.skin_vertex_count);
        write_u32(out, self.first_attachment);
        write_u32(out, self.attachment_count);
        write_u32(out, self.first_clip);
        write_u32(out, self.clip_count);
        write_u32(out, self.first_clip_frame);
        write_u32(out, self.clip_frame_count);
        write_u32(out, self.first_layer);
        write_u32(out, self.animation_layer_count);
        write_u32(out, self.flags);
        write_u32(out, self.dirty_range_count);
        debug_assert_eq!(SCENE_BINARY_PUPPET_RECORD_SIZE, 68);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryPuppetSkinBoneRecord {
    pub owner_name: u32,
    pub parent_index: u32,
    pub transform: ScenePuppetTransform,
}

impl SceneBinaryPuppetSkinBoneRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.parent_index);
        encode_puppet_transform(out, self.transform);
        debug_assert_eq!(SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE, 48);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryPuppetSkinVertexRecord {
    pub owner_name: u32,
    pub bone_indices: [u32; 4],
    pub weights: [f32; 4],
    pub weight_count: u32,
}

impl SceneBinaryPuppetSkinVertexRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        for index in self.bone_indices {
            write_u32(out, index);
        }
        for weight in self.weights {
            write_f32(out, weight);
        }
        write_u32(out, self.weight_count);
        debug_assert_eq!(SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE, 40);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryPuppetAttachmentRecord {
    pub owner_name: u32,
    pub name: u32,
    pub bone_index: u32,
    pub local_position: [f32; 3],
    pub bind_position: [f32; 3],
    pub flags: u32,
}

impl SceneBinaryPuppetAttachmentRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.name);
        write_u32(out, self.bone_index);
        for value in self.local_position {
            write_f32(out, value);
        }
        for value in self.bind_position {
            write_f32(out, value);
        }
        write_u32(out, self.flags);
        debug_assert_eq!(SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE, 40);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryPuppetClipRecord {
    pub owner_name: u32,
    pub clip_name: u32,
    pub clip_id: u32,
    pub first_frame: u32,
    pub bone_count: u32,
    pub frame_count: u32,
    pub frame_record_count: u32,
    pub fps: f32,
    pub flags: u32,
    pub dirty_range_count: u32,
}

impl SceneBinaryPuppetClipRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.clip_name);
        write_u32(out, self.clip_id);
        write_u32(out, self.first_frame);
        write_u32(out, self.bone_count);
        write_u32(out, self.frame_count);
        write_u32(out, self.frame_record_count);
        write_f32(out, self.fps);
        write_u32(out, self.flags);
        write_u32(out, self.dirty_range_count);
        debug_assert_eq!(SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE, 40);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryPuppetFrameRecord {
    pub owner_name: u32,
    pub clip_id: u32,
    pub bone_index: u32,
    pub frame_index: u32,
    pub transform: ScenePuppetTransform,
}

impl SceneBinaryPuppetFrameRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.clip_id);
        write_u32(out, self.bone_index);
        write_u32(out, self.frame_index);
        encode_puppet_transform(out, self.transform);
        debug_assert_eq!(SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE, 56);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryPuppetLayerRecord {
    pub owner_name: u32,
    pub layer_name: u32,
    pub clip_id: u32,
    pub layer_index: u32,
    pub flags: u32,
    pub blend: f32,
    pub rate: f32,
    pub initial_phase: f32,
}

impl SceneBinaryPuppetLayerRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.layer_name);
        write_u32(out, self.clip_id);
        write_u32(out, self.layer_index);
        write_u32(out, self.flags);
        write_f32(out, self.blend);
        write_f32(out, self.rate);
        write_f32(out, self.initial_phase);
        debug_assert_eq!(SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE, 32);
    }
}

pub(crate) fn decode_puppet_record(
    bytes: &[u8],
) -> Result<SceneBinaryPuppetRecord, SceneBinaryError> {
    Ok(SceneBinaryPuppetRecord {
        owner_name: read_u32(bytes, 0)?,
        vertex_count: read_u32(bytes, 4)?,
        index_count: read_u32(bytes, 8)?,
        first_bone: read_u32(bytes, 12)?,
        bone_count: read_u32(bytes, 16)?,
        first_skin_vertex: read_u32(bytes, 20)?,
        skin_vertex_count: read_u32(bytes, 24)?,
        first_attachment: read_u32(bytes, 28)?,
        attachment_count: read_u32(bytes, 32)?,
        first_clip: read_u32(bytes, 36)?,
        clip_count: read_u32(bytes, 40)?,
        first_clip_frame: read_u32(bytes, 44)?,
        clip_frame_count: read_u32(bytes, 48)?,
        first_layer: read_u32(bytes, 52)?,
        animation_layer_count: read_u32(bytes, 56)?,
        flags: read_u32(bytes, 60)?,
        dirty_range_count: read_u32(bytes, 64)?,
    })
}

pub(crate) fn decode_puppet_skin_bone_record(
    bytes: &[u8],
) -> Result<SceneBinaryPuppetSkinBoneRecord, SceneBinaryError> {
    Ok(SceneBinaryPuppetSkinBoneRecord {
        owner_name: read_u32(bytes, 0)?,
        parent_index: read_u32(bytes, 4)?,
        transform: decode_puppet_transform(bytes, 8)?,
    })
}

pub(crate) fn decode_puppet_skin_vertex_record(
    bytes: &[u8],
) -> Result<SceneBinaryPuppetSkinVertexRecord, SceneBinaryError> {
    Ok(SceneBinaryPuppetSkinVertexRecord {
        owner_name: read_u32(bytes, 0)?,
        bone_indices: [
            read_u32(bytes, 4)?,
            read_u32(bytes, 8)?,
            read_u32(bytes, 12)?,
            read_u32(bytes, 16)?,
        ],
        weights: [
            read_f32(bytes, 20)?,
            read_f32(bytes, 24)?,
            read_f32(bytes, 28)?,
            read_f32(bytes, 32)?,
        ],
        weight_count: read_u32(bytes, 36)?,
    })
}

pub(crate) fn decode_puppet_attachment_record(
    bytes: &[u8],
) -> Result<SceneBinaryPuppetAttachmentRecord, SceneBinaryError> {
    Ok(SceneBinaryPuppetAttachmentRecord {
        owner_name: read_u32(bytes, 0)?,
        name: read_u32(bytes, 4)?,
        bone_index: read_u32(bytes, 8)?,
        local_position: [
            read_f32(bytes, 12)?,
            read_f32(bytes, 16)?,
            read_f32(bytes, 20)?,
        ],
        bind_position: [
            read_f32(bytes, 24)?,
            read_f32(bytes, 28)?,
            read_f32(bytes, 32)?,
        ],
        flags: read_u32(bytes, 36)?,
    })
}

pub(crate) fn decode_puppet_clip_record(
    bytes: &[u8],
) -> Result<SceneBinaryPuppetClipRecord, SceneBinaryError> {
    Ok(SceneBinaryPuppetClipRecord {
        owner_name: read_u32(bytes, 0)?,
        clip_name: read_u32(bytes, 4)?,
        clip_id: read_u32(bytes, 8)?,
        first_frame: read_u32(bytes, 12)?,
        bone_count: read_u32(bytes, 16)?,
        frame_count: read_u32(bytes, 20)?,
        frame_record_count: read_u32(bytes, 24)?,
        fps: read_f32(bytes, 28)?,
        flags: read_u32(bytes, 32)?,
        dirty_range_count: read_u32(bytes, 36)?,
    })
}

pub(crate) fn decode_puppet_frame_record(
    bytes: &[u8],
) -> Result<SceneBinaryPuppetFrameRecord, SceneBinaryError> {
    Ok(SceneBinaryPuppetFrameRecord {
        owner_name: read_u32(bytes, 0)?,
        clip_id: read_u32(bytes, 4)?,
        bone_index: read_u32(bytes, 8)?,
        frame_index: read_u32(bytes, 12)?,
        transform: decode_puppet_transform(bytes, 16)?,
    })
}

pub(crate) fn decode_puppet_layer_record(
    bytes: &[u8],
) -> Result<SceneBinaryPuppetLayerRecord, SceneBinaryError> {
    Ok(SceneBinaryPuppetLayerRecord {
        owner_name: read_u32(bytes, 0)?,
        layer_name: read_u32(bytes, 4)?,
        clip_id: read_u32(bytes, 8)?,
        layer_index: read_u32(bytes, 12)?,
        flags: read_u32(bytes, 16)?,
        blend: read_f32(bytes, 20)?,
        rate: read_f32(bytes, 24)?,
        initial_phase: read_f32(bytes, 28)?,
    })
}

pub(super) fn puppet_first_record(first: u32, count: u32) -> u32 {
    if count == 0 {
        SCENE_BINARY_NONE_ID
    } else {
        first
    }
}

pub(super) fn puppet_flags(
    has_mesh: bool,
    has_animation_layers: bool,
    has_skin: bool,
    has_clips: bool,
    has_attachments: bool,
) -> u32 {
    u32::from(has_mesh) * SCENE_BINARY_PUPPET_FLAG_MESH
        | u32::from(has_animation_layers) * SCENE_BINARY_PUPPET_FLAG_ANIMATION_LAYERS
        | u32::from(has_skin) * SCENE_BINARY_PUPPET_FLAG_SKIN
        | u32::from(has_clips) * SCENE_BINARY_PUPPET_FLAG_CLIPS
        | u32::from(has_attachments) * SCENE_BINARY_PUPPET_FLAG_ATTACHMENTS
}

pub(super) fn puppet_clip_flags(looping: bool) -> u32 {
    u32::from(looping) * SCENE_BINARY_PUPPET_CLIP_FLAG_LOOPING
}

pub(super) fn puppet_layer_flags(additive: bool, lock_transforms: bool, visible: bool) -> u32 {
    u32::from(additive) * SCENE_BINARY_PUPPET_LAYER_FLAG_ADDITIVE
        | u32::from(lock_transforms) * SCENE_BINARY_PUPPET_LAYER_FLAG_LOCK_TRANSFORMS
        | u32::from(visible) * SCENE_BINARY_PUPPET_LAYER_FLAG_VISIBLE
}

fn encode_puppet_transform(out: &mut Vec<u8>, transform: ScenePuppetTransform) {
    for value in transform.translation {
        write_f32(out, value as f32);
    }
    for value in transform.rotation {
        write_f32(out, value as f32);
    }
    for value in transform.scale {
        write_f32(out, value as f32);
    }
    write_f32(out, transform.opacity as f32);
}

fn decode_puppet_transform(
    bytes: &[u8],
    offset: usize,
) -> Result<ScenePuppetTransform, SceneBinaryError> {
    Ok(ScenePuppetTransform {
        translation: [
            f64::from(read_f32(bytes, offset)?),
            f64::from(read_f32(bytes, offset + 4)?),
            f64::from(read_f32(bytes, offset + 8)?),
        ],
        rotation: [
            f64::from(read_f32(bytes, offset + 12)?),
            f64::from(read_f32(bytes, offset + 16)?),
            f64::from(read_f32(bytes, offset + 20)?),
        ],
        scale: [
            f64::from(read_f32(bytes, offset + 24)?),
            f64::from(read_f32(bytes, offset + 28)?),
            f64::from(read_f32(bytes, offset + 32)?),
        ],
        opacity: f64::from(read_f32(bytes, offset + 36)?),
    })
}

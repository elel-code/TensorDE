use super::{
    SceneBinaryError, SceneEffectUvTransform, read_f32, read_u16, read_u32, write_f32, write_u16,
    write_u32,
};

pub const SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE: usize = 64;

pub const SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneBinaryEffectUvTransformRecord {
    pub owner_name: u32,
    pub effect_name: u32,
    pub pass_index: u32,
    pub source_slot: u32,
    pub mask_slot: u32,
    pub input_width: u32,
    pub input_height: u32,
    pub mask_width: u32,
    pub mask_height: u32,
    pub backing_width: u32,
    pub backing_height: u32,
    pub scale_u: f32,
    pub scale_v: f32,
    pub offset_u: f32,
    pub offset_v: f32,
    pub mapping: u16,
    pub flags: u16,
}

impl SceneBinaryEffectUvTransformRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.effect_name);
        write_u32(out, self.pass_index);
        write_u32(out, self.source_slot);
        write_u32(out, self.mask_slot);
        write_u32(out, self.input_width);
        write_u32(out, self.input_height);
        write_u32(out, self.mask_width);
        write_u32(out, self.mask_height);
        write_u32(out, self.backing_width);
        write_u32(out, self.backing_height);
        write_f32(out, self.scale_u);
        write_f32(out, self.scale_v);
        write_f32(out, self.offset_u);
        write_f32(out, self.offset_v);
        write_u16(out, self.mapping);
        write_u16(out, self.flags);
        debug_assert_eq!(SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE, 64);
    }
}

pub(super) fn effect_uv_transform_mapping_code(_transform: SceneEffectUvTransform) -> u16 {
    SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION
}

pub(super) fn effect_uv_transform_flags(transform: SceneEffectUvTransform) -> u16 {
    u16::from(transform.input_extent.is_some())
        | (u16::from(transform.mask_extent.is_some()) << 1)
        | (u16::from(transform.mask_backing_extent.is_some()) << 2)
}

pub(crate) fn decode_effect_uv_transform_record(
    bytes: &[u8],
) -> Result<SceneBinaryEffectUvTransformRecord, SceneBinaryError> {
    Ok(SceneBinaryEffectUvTransformRecord {
        owner_name: read_u32(bytes, 0)?,
        effect_name: read_u32(bytes, 4)?,
        pass_index: read_u32(bytes, 8)?,
        source_slot: read_u32(bytes, 12)?,
        mask_slot: read_u32(bytes, 16)?,
        input_width: read_u32(bytes, 20)?,
        input_height: read_u32(bytes, 24)?,
        mask_width: read_u32(bytes, 28)?,
        mask_height: read_u32(bytes, 32)?,
        backing_width: read_u32(bytes, 36)?,
        backing_height: read_u32(bytes, 40)?,
        scale_u: read_f32(bytes, 44)?,
        scale_v: read_f32(bytes, 48)?,
        offset_u: read_f32(bytes, 52)?,
        offset_v: read_f32(bytes, 56)?,
        mapping: read_u16(bytes, 60)?,
        flags: read_u16(bytes, 62)?,
    })
}

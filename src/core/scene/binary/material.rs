use super::{
    SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE, SCENE_BINARY_EFFECT_PASS_RECORD_SIZE,
    SCENE_BINARY_EFFECT_PASS_RECORD_SIZE_V12, SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE,
    SCENE_BINARY_NONE_ID, SCENE_BINARY_RENDER_STATE_RECORD_SIZE,
    SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE, SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
    SceneBinaryError, read_f32, read_i64, read_u16, read_u16_or, read_u32, read_u64, write_f32,
    write_i64, write_u16, write_u32, write_u64,
};

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
    pub(super) fn encode(self, out: &mut Vec<u8>) {
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
    pub alpha_write: u16,
    pub flags: u16,
}

impl SceneBinaryMaterialPassRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
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
        write_u16(out, self.alpha_write);
        write_u16(out, self.flags);
        debug_assert_eq!(SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE, 58);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryEffectPassRecord {
    pub owner_name: u32,
    pub effect_name: u32,
    pub shader_name: u32,
    pub blending_name: u32,
    pub command_name: u32,
    pub source_name: u32,
    pub target_name: u32,
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
    pub alpha_write: u16,
    pub flags: u16,
}

impl SceneBinaryEffectPassRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.owner_name);
        write_u32(out, self.effect_name);
        write_u32(out, self.shader_name);
        write_u32(out, self.blending_name);
        write_u32(out, self.command_name);
        write_u32(out, self.source_name);
        write_u32(out, self.target_name);
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
        write_u16(out, self.alpha_write);
        write_u16(out, self.flags);
        debug_assert_eq!(SCENE_BINARY_EFFECT_PASS_RECORD_SIZE, 70);
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
    pub(super) fn encode(self, out: &mut Vec<u8>) {
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
    pub(super) fn encode(self, out: &mut Vec<u8>) {
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
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u16(out, self.owner_kind);
        write_u16(out, self.flags);
        write_u32(out, self.owner_name);
        write_u64(out, self.stable_id);
        write_u32(out, self.record_index);
        write_u32(out, self.dirty_range_count);
        debug_assert_eq!(SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE, 24);
    }
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
        alpha_write: if bytes.len() >= SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE {
            read_u16(bytes, 54)?
        } else {
            0
        },
        flags: if bytes.len() >= SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE {
            read_u16(bytes, 56)?
        } else {
            read_u16_or(bytes, 54, 0)?
        },
    })
}

pub(crate) fn decode_effect_pass_record(
    bytes: &[u8],
) -> Result<SceneBinaryEffectPassRecord, SceneBinaryError> {
    if bytes.len() == SCENE_BINARY_EFFECT_PASS_RECORD_SIZE_V12 {
        return Ok(SceneBinaryEffectPassRecord {
            owner_name: read_u32(bytes, 0)?,
            effect_name: read_u32(bytes, 4)?,
            shader_name: read_u32(bytes, 8)?,
            blending_name: read_u32(bytes, 12)?,
            command_name: SCENE_BINARY_NONE_ID,
            source_name: SCENE_BINARY_NONE_ID,
            target_name: SCENE_BINARY_NONE_ID,
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
            alpha_write: 0,
            flags: read_u16(bytes, 54)?,
        });
    }
    Ok(SceneBinaryEffectPassRecord {
        owner_name: read_u32(bytes, 0)?,
        effect_name: read_u32(bytes, 4)?,
        shader_name: read_u32(bytes, 8)?,
        blending_name: read_u32(bytes, 12)?,
        command_name: read_u32(bytes, 16)?,
        source_name: read_u32(bytes, 20)?,
        target_name: read_u32(bytes, 24)?,
        pass_index: read_u32(bytes, 28)?,
        first_texture_slot: read_u32(bytes, 32)?,
        texture_slot_count: read_u32(bytes, 36)?,
        first_effect_uv_transform: read_u32(bytes, 40)?,
        effect_uv_transform_count: read_u32(bytes, 44)?,
        first_parameter: read_u32(bytes, 48)?,
        parameter_count: read_u32(bytes, 52)?,
        kind: read_u16_or(bytes, 56, 0)?,
        evaluation_boundary: read_u16_or(bytes, 58, 0)?,
        depth_test: read_u16_or(bytes, 60, 0)?,
        depth_write: read_u16_or(bytes, 62, 0)?,
        cull_mode: read_u16_or(bytes, 64, 0)?,
        alpha_write: read_u16_or(bytes, 66, 0)?,
        flags: read_u16_or(bytes, 68, 0)?,
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

//! Optimized scene-owned SPIR-V program chunk.

use super::codec::{Decoder, checked_u32, put_string_id, put_u32};
use super::{
    SceneBinaryError, SceneShaderBindingKind, SceneShaderBindingRecord, SceneShaderIoDirection,
    SceneShaderProgramRecord, SceneShaderScalarType, SceneShaderStage, SceneShaderStageIoRecord,
    SceneShaderUniformBufferRecord, SceneShaderUniformMemberRecord,
};

pub(super) fn encode_shader_programs(
    programs: &[SceneShaderProgramRecord],
    bindings: &[SceneShaderBindingRecord],
    stage_io: &[SceneShaderStageIoRecord],
    uniform_buffers: &[SceneShaderUniformBufferRecord],
    uniform_members: &[SceneShaderUniformMemberRecord],
    spirv: &[u32],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut output = Vec::new();
    put_u32(
        &mut output,
        checked_u32(programs.len(), "shader program count")?,
    );
    for program in programs {
        put_string_id(&mut output, program.program_key);
        put_u32(&mut output, program.stage.to_u32());
        put_string_id(&mut output, program.entry_point);
        put_u32(&mut output, program.spirv_start);
        put_u32(&mut output, program.spirv_count);
        put_u32(&mut output, program.binding_start);
        put_u32(&mut output, program.binding_count);
        put_u32(&mut output, program.stage_io_start);
        put_u32(&mut output, program.stage_io_count);
        put_u32(&mut output, program.uniform_buffer_start);
        put_u32(&mut output, program.uniform_buffer_count);
        put_u32(&mut output, program.push_constant_bytes);
    }
    put_u32(
        &mut output,
        checked_u32(stage_io.len(), "shader stage-I/O count")?,
    );
    for item in stage_io {
        put_string_id(&mut output, item.name);
        put_u32(&mut output, item.direction.to_u32());
        put_u32(&mut output, item.location);
        put_u32(&mut output, item.scalar_type.to_u32());
        put_u32(&mut output, item.rows);
        put_u32(&mut output, item.columns);
        put_u32(&mut output, item.location_count);
    }
    put_u32(
        &mut output,
        checked_u32(uniform_buffers.len(), "shader uniform-buffer count")?,
    );
    for buffer in uniform_buffers {
        put_string_id(&mut output, buffer.name);
        put_u32(&mut output, buffer.register);
        put_u32(&mut output, buffer.byte_size);
        put_u32(&mut output, buffer.member_start);
        put_u32(&mut output, buffer.member_count);
    }
    put_u32(
        &mut output,
        checked_u32(uniform_members.len(), "shader uniform-member count")?,
    );
    for member in uniform_members {
        put_string_id(&mut output, member.name);
        put_string_id(&mut output, member.material_parameter);
        put_u32(&mut output, member.byte_offset);
        put_u32(&mut output, member.byte_size);
        put_u32(&mut output, member.scalar_type.to_u32());
        put_u32(&mut output, member.rows);
        put_u32(&mut output, member.columns);
        put_u32(&mut output, member.array_count);
        put_u32(&mut output, member.array_stride);
        put_u32(&mut output, member.matrix_stride);
    }
    put_u32(
        &mut output,
        checked_u32(bindings.len(), "shader binding count")?,
    );
    for binding in bindings {
        put_u32(&mut output, binding.kind.to_u32());
        put_u32(&mut output, binding.register);
        put_u32(&mut output, binding.descriptor_count);
        put_u32(&mut output, binding.push_offset);
    }
    put_u32(
        &mut output,
        checked_u32(spirv.len(), "shader SPIR-V word count")?,
    );
    for word in spirv {
        put_u32(&mut output, *word);
    }
    Ok(output)
}

pub(super) struct DecodedShaderPrograms {
    pub programs: Vec<SceneShaderProgramRecord>,
    pub bindings: Vec<SceneShaderBindingRecord>,
    pub stage_io: Vec<SceneShaderStageIoRecord>,
    pub uniform_buffers: Vec<SceneShaderUniformBufferRecord>,
    pub uniform_members: Vec<SceneShaderUniformMemberRecord>,
    pub spirv: Vec<u32>,
}

pub(super) fn decode_shader_programs(
    data: &[u8],
) -> Result<DecodedShaderPrograms, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let program_count = decoder.u32()? as usize;
    let mut programs = Vec::with_capacity(program_count);
    for _ in 0..program_count {
        let program_key = decoder.string_id()?;
        let stage_raw = decoder.u32()?;
        let stage = SceneShaderStage::from_u32(stage_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("shader program stage", stage_raw),
        )?;
        programs.push(SceneShaderProgramRecord {
            program_key,
            stage,
            entry_point: decoder.string_id()?,
            spirv_start: decoder.u32()?,
            spirv_count: decoder.u32()?,
            binding_start: decoder.u32()?,
            binding_count: decoder.u32()?,
            stage_io_start: decoder.u32()?,
            stage_io_count: decoder.u32()?,
            uniform_buffer_start: decoder.u32()?,
            uniform_buffer_count: decoder.u32()?,
            push_constant_bytes: decoder.u32()?,
        });
    }
    let stage_io_count = decoder.u32()? as usize;
    let mut stage_io = Vec::with_capacity(stage_io_count);
    for _ in 0..stage_io_count {
        let name = decoder.string_id()?;
        let direction_raw = decoder.u32()?;
        let direction = SceneShaderIoDirection::from_u32(direction_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("shader I/O direction", direction_raw),
        )?;
        let location = decoder.u32()?;
        let scalar_raw = decoder.u32()?;
        let scalar_type = SceneShaderScalarType::from_u32(scalar_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("shader scalar type", scalar_raw),
        )?;
        stage_io.push(SceneShaderStageIoRecord {
            name,
            direction,
            location,
            scalar_type,
            rows: decoder.u32()?,
            columns: decoder.u32()?,
            location_count: decoder.u32()?,
        });
    }
    let uniform_buffer_count = decoder.u32()? as usize;
    let mut uniform_buffers = Vec::with_capacity(uniform_buffer_count);
    for _ in 0..uniform_buffer_count {
        uniform_buffers.push(SceneShaderUniformBufferRecord {
            name: decoder.string_id()?,
            register: decoder.u32()?,
            byte_size: decoder.u32()?,
            member_start: decoder.u32()?,
            member_count: decoder.u32()?,
        });
    }
    let uniform_member_count = decoder.u32()? as usize;
    let mut uniform_members = Vec::with_capacity(uniform_member_count);
    for _ in 0..uniform_member_count {
        let name = decoder.string_id()?;
        let material_parameter = decoder.string_id()?;
        let byte_offset = decoder.u32()?;
        let byte_size = decoder.u32()?;
        let scalar_raw = decoder.u32()?;
        let scalar_type = SceneShaderScalarType::from_u32(scalar_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("shader scalar type", scalar_raw),
        )?;
        uniform_members.push(SceneShaderUniformMemberRecord {
            name,
            material_parameter,
            byte_offset,
            byte_size,
            scalar_type,
            rows: decoder.u32()?,
            columns: decoder.u32()?,
            array_count: decoder.u32()?,
            array_stride: decoder.u32()?,
            matrix_stride: decoder.u32()?,
        });
    }
    let binding_count = decoder.u32()? as usize;
    let mut bindings = Vec::with_capacity(binding_count);
    for _ in 0..binding_count {
        let kind_raw = decoder.u32()?;
        let kind = SceneShaderBindingKind::from_u32(kind_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("shader binding kind", kind_raw),
        )?;
        bindings.push(SceneShaderBindingRecord {
            kind,
            register: decoder.u32()?,
            descriptor_count: decoder.u32()?,
            push_offset: decoder.u32()?,
        });
    }
    let spirv_count = decoder.u32()? as usize;
    let mut spirv = Vec::with_capacity(spirv_count);
    for _ in 0..spirv_count {
        spirv.push(decoder.u32()?);
    }
    Ok(DecodedShaderPrograms {
        programs,
        bindings,
        stage_io,
        uniform_buffers,
        uniform_members,
        spirv,
    })
}

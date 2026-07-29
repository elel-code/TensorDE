//! Optimized scene-owned SPIR-V program chunk.

use super::codec::{Decoder, checked_u32, put_string_id, put_u32};
use super::{
    SceneBinaryError, SceneShaderBindingKind, SceneShaderBindingRecord, SceneShaderProgramRecord,
    SceneShaderStage,
};

pub(super) fn encode_shader_programs(
    programs: &[SceneShaderProgramRecord],
    bindings: &[SceneShaderBindingRecord],
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
        put_u32(&mut output, program.push_constant_bytes);
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
            push_constant_bytes: decoder.u32()?,
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
        spirv,
    })
}

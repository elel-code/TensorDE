//! Lower cold-compiled shader programs into the scene-owned SPIR-V ABI.

use crate::engine::scene::{
    SceneShaderBindingKind, SceneShaderBindingRecord, SceneShaderProgramRecord, SceneShaderStage,
};

use super::super::ir::{WeIrShaderBindingKind, WeIrShaderProgram, WeIrShaderStage, WeSceneIr};
use super::{StringInterner, WeLowerError};

pub(super) struct LoweredShaderPrograms {
    pub programs: Vec<SceneShaderProgramRecord>,
    pub bindings: Vec<SceneShaderBindingRecord>,
    pub spirv: Vec<u32>,
}

pub(super) fn lower_shader_programs(
    ir: &WeSceneIr,
    strings: &mut StringInterner,
) -> Result<LoweredShaderPrograms, WeLowerError> {
    let mut lowered = LoweredShaderPrograms {
        programs: Vec::with_capacity(ir.shader_programs.len()),
        bindings: Vec::new(),
        spirv: Vec::new(),
    };
    for program in &ir.shader_programs {
        lower_program(&mut lowered, program, strings)?;
    }
    Ok(lowered)
}

fn lower_program(
    lowered: &mut LoweredShaderPrograms,
    program: &WeIrShaderProgram,
    strings: &mut StringInterner,
) -> Result<(), WeLowerError> {
    let binding_start = count(lowered.bindings.len(), "shader binding start")?;
    let spirv_start = count(lowered.spirv.len(), "shader SPIR-V start")?;
    lowered.bindings.extend(
        program
            .bindings
            .iter()
            .map(|binding| SceneShaderBindingRecord {
                kind: lower_binding_kind(binding.kind),
                register: binding.register,
                descriptor_count: binding.descriptor_count,
                push_offset: binding.push_offset,
            }),
    );
    lowered.spirv.extend_from_slice(&program.spirv);
    lowered.programs.push(SceneShaderProgramRecord {
        program_key: strings.id(&program.program_key),
        stage: lower_stage(program.stage),
        entry_point: strings.id(&program.entry_point),
        spirv_start,
        spirv_count: count(program.spirv.len(), "shader SPIR-V word count")?,
        binding_start,
        binding_count: count(program.bindings.len(), "shader binding count")?,
        push_constant_bytes: program.push_constant_bytes,
    });
    Ok(())
}

fn lower_stage(stage: WeIrShaderStage) -> SceneShaderStage {
    match stage {
        WeIrShaderStage::Vertex => SceneShaderStage::Vertex,
        WeIrShaderStage::Fragment => SceneShaderStage::Fragment,
        WeIrShaderStage::Compute => SceneShaderStage::Compute,
    }
}

fn lower_binding_kind(kind: WeIrShaderBindingKind) -> SceneShaderBindingKind {
    match kind {
        WeIrShaderBindingKind::SampledImage => SceneShaderBindingKind::SampledImage,
        WeIrShaderBindingKind::StorageImage => SceneShaderBindingKind::StorageImage,
        WeIrShaderBindingKind::Sampler => SceneShaderBindingKind::Sampler,
        WeIrShaderBindingKind::UniformBuffer => SceneShaderBindingKind::UniformBuffer,
        WeIrShaderBindingKind::StorageBuffer => SceneShaderBindingKind::StorageBuffer,
    }
}

fn count(value: usize, field: &'static str) -> Result<u32, WeLowerError> {
    u32::try_from(value).map_err(|_| WeLowerError::SizeOverflow(field))
}

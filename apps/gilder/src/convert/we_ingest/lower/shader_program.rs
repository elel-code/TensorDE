//! Lower cold-compiled shader programs into the scene-owned SPIR-V ABI.

use crate::engine::scene::{
    SceneShaderBindingKind, SceneShaderBindingRecord, SceneShaderIoDirection,
    SceneShaderProgramRecord, SceneShaderScalarType, SceneShaderStage, SceneShaderStageIoRecord,
    SceneShaderUniformBufferRecord, SceneShaderUniformMemberRecord,
};

use super::super::ir::{
    WeIrShaderBindingKind, WeIrShaderIoDirection, WeIrShaderProgram, WeIrShaderScalarType,
    WeIrShaderStage, WeSceneIr,
};
use super::{StringInterner, WeLowerError};

pub(super) struct LoweredShaderPrograms {
    pub programs: Vec<SceneShaderProgramRecord>,
    pub bindings: Vec<SceneShaderBindingRecord>,
    pub stage_io: Vec<SceneShaderStageIoRecord>,
    pub uniform_buffers: Vec<SceneShaderUniformBufferRecord>,
    pub uniform_members: Vec<SceneShaderUniformMemberRecord>,
    pub spirv: Vec<u32>,
}

pub(super) fn lower_shader_programs(
    ir: &WeSceneIr,
    strings: &mut StringInterner,
) -> Result<LoweredShaderPrograms, WeLowerError> {
    let mut lowered = LoweredShaderPrograms {
        programs: Vec::with_capacity(ir.shader_programs.len()),
        bindings: Vec::new(),
        stage_io: Vec::new(),
        uniform_buffers: Vec::new(),
        uniform_members: Vec::new(),
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
    let stage_io_start = count(lowered.stage_io.len(), "shader stage-I/O start")?;
    let uniform_buffer_start = count(lowered.uniform_buffers.len(), "shader uniform-buffer start")?;
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
    lowered.stage_io.extend(
        program
            .stage_io
            .iter()
            .map(|item| SceneShaderStageIoRecord {
                name: strings.id(&item.name),
                direction: lower_io_direction(item.direction),
                location: item.location,
                scalar_type: lower_scalar_type(item.scalar_type),
                rows: item.rows,
                columns: item.columns,
                location_count: item.location_count,
            }),
    );
    for buffer in &program.uniform_buffers {
        let member_start = count(lowered.uniform_members.len(), "shader uniform-member start")?;
        lowered
            .uniform_members
            .extend(
                buffer
                    .members
                    .iter()
                    .map(|member| SceneShaderUniformMemberRecord {
                        name: strings.id(&member.name),
                        byte_offset: member.byte_offset,
                        byte_size: member.byte_size,
                        scalar_type: lower_scalar_type(member.scalar_type),
                        rows: member.rows,
                        columns: member.columns,
                        array_count: member.array_count,
                        array_stride: member.array_stride,
                        matrix_stride: member.matrix_stride,
                    }),
            );
        lowered
            .uniform_buffers
            .push(SceneShaderUniformBufferRecord {
                name: strings.id(&buffer.name),
                register: buffer.register,
                byte_size: buffer.byte_size,
                member_start,
                member_count: count(buffer.members.len(), "shader uniform-member count")?,
            });
    }
    lowered.spirv.extend_from_slice(&program.spirv);
    lowered.programs.push(SceneShaderProgramRecord {
        program_key: strings.id(&program.program_key),
        stage: lower_stage(program.stage),
        entry_point: strings.id(&program.entry_point),
        spirv_start,
        spirv_count: count(program.spirv.len(), "shader SPIR-V word count")?,
        binding_start,
        binding_count: count(program.bindings.len(), "shader binding count")?,
        stage_io_start,
        stage_io_count: count(program.stage_io.len(), "shader stage-I/O count")?,
        uniform_buffer_start,
        uniform_buffer_count: count(program.uniform_buffers.len(), "shader uniform-buffer count")?,
        push_constant_bytes: program.push_constant_bytes,
    });
    Ok(())
}

fn lower_io_direction(direction: WeIrShaderIoDirection) -> SceneShaderIoDirection {
    match direction {
        WeIrShaderIoDirection::Input => SceneShaderIoDirection::Input,
        WeIrShaderIoDirection::Output => SceneShaderIoDirection::Output,
    }
}

fn lower_scalar_type(scalar_type: WeIrShaderScalarType) -> SceneShaderScalarType {
    match scalar_type {
        WeIrShaderScalarType::Bool => SceneShaderScalarType::Bool,
        WeIrShaderScalarType::I32 => SceneShaderScalarType::I32,
        WeIrShaderScalarType::U32 => SceneShaderScalarType::U32,
        WeIrShaderScalarType::F32 => SceneShaderScalarType::F32,
    }
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

//! Strict scene-owned optimized SPIR-V validation.

use std::collections::BTreeSet;

use super::super::*;
use super::{validate_range, validate_string};

const SPIRV_MAGIC: u32 = 0x0723_0203;
const SPIRV_CAPABILITY_DESCRIPTOR_HEAP_EXT: u32 = 5_128;
const SPIRV_DECORATION_BINDING: u32 = 33;
const SPIRV_DECORATION_DESCRIPTOR_SET: u32 = 34;

pub(super) fn validate_shader_programs(
    document: &SceneBinaryDocument,
) -> Result<(), SceneStorageError> {
    let mut identities = BTreeSet::new();
    for program in &document.shader_programs {
        validate_string(document, "shader_program.program_key", program.program_key)?;
        validate_string(document, "shader_program.entry_point", program.entry_point)?;
        if !program.program_key.is_some() || !program.entry_point.is_some() {
            return invalid(program, "program key or entry point is empty");
        }
        if !identities.insert((program.program_key, program.stage)) {
            return invalid(program, "duplicate program key and stage");
        }
        validate_range(
            "shader_program.spirv_range",
            program.spirv_start,
            program.spirv_count,
            document.shader_spirv.len(),
        )?;
        validate_range(
            "shader_program.binding_range",
            program.binding_start,
            program.binding_count,
            document.shader_bindings.len(),
        )?;
        validate_range(
            "shader_program.stage_io_range",
            program.stage_io_start,
            program.stage_io_count,
            document.shader_stage_io.len(),
        )?;
        validate_range(
            "shader_program.uniform_buffer_range",
            program.uniform_buffer_start,
            program.uniform_buffer_count,
            document.shader_uniform_buffers.len(),
        )?;
        if !program.push_constant_bytes.is_multiple_of(4) {
            return invalid(program, "push-constant byte size is not word aligned");
        }
        let spirv = slice(
            &document.shader_spirv,
            program.spirv_start,
            program.spirv_count,
        );
        let bindings = slice(
            &document.shader_bindings,
            program.binding_start,
            program.binding_count,
        );
        let stage_io = slice(
            &document.shader_stage_io,
            program.stage_io_start,
            program.stage_io_count,
        );
        let uniform_buffers = slice(
            &document.shader_uniform_buffers,
            program.uniform_buffer_start,
            program.uniform_buffer_count,
        );
        validate_spirv(program, spirv, !bindings.is_empty())?;
        validate_bindings(program, bindings)?;
        validate_stage_io(document, program, stage_io)?;
        validate_uniform_buffers(document, program, bindings, uniform_buffers)?;
    }
    validate_stage_links(document)?;
    Ok(())
}

fn validate_stage_io(
    document: &SceneBinaryDocument,
    program: &SceneShaderProgramRecord,
    stage_io: &[SceneShaderStageIoRecord],
) -> Result<(), SceneStorageError> {
    if program.stage == SceneShaderStage::Compute && !stage_io.is_empty() {
        return invalid(program, "compute program exposes graphics stage I/O");
    }
    let mut names = BTreeSet::new();
    let mut locations = BTreeSet::new();
    for item in stage_io {
        validate_string(document, "shader_stage_io.name", item.name)?;
        if !item.name.is_some() {
            return invalid(program, "shader stage-I/O name is empty");
        }
        if item.rows == 0
            || item.columns == 0
            || item.location_count == 0
            || !item.location_count.is_multiple_of(item.columns)
        {
            return invalid(program, "shader stage-I/O shape is invalid");
        }
        if !names.insert((item.direction, item.name)) {
            return invalid(program, "shader stage-I/O repeats a name");
        }
        let Some(location_end) = item.location.checked_add(item.location_count) else {
            return invalid(program, "shader stage-I/O location range overflows");
        };
        for location in item.location..location_end {
            if !locations.insert((item.direction, location)) {
                return invalid(program, "shader stage-I/O locations overlap");
            }
        }
    }
    Ok(())
}

fn validate_uniform_buffers(
    document: &SceneBinaryDocument,
    program: &SceneShaderProgramRecord,
    bindings: &[SceneShaderBindingRecord],
    buffers: &[SceneShaderUniformBufferRecord],
) -> Result<(), SceneStorageError> {
    let mut names = BTreeSet::new();
    let mut registers = BTreeSet::new();
    for buffer in buffers {
        validate_string(document, "shader_uniform_buffer.name", buffer.name)?;
        validate_range(
            "shader_uniform_buffer.member_range",
            buffer.member_start,
            buffer.member_count,
            document.shader_uniform_members.len(),
        )?;
        if !buffer.name.is_some() || buffer.byte_size == 0 || buffer.member_count == 0 {
            return invalid(program, "shader uniform buffer is empty");
        }
        if !names.insert(buffer.name) || !registers.insert(buffer.register) {
            return invalid(program, "shader uniform buffer repeats a name or register");
        }
        validate_uniform_members(
            document,
            program,
            buffer,
            slice(
                &document.shader_uniform_members,
                buffer.member_start,
                buffer.member_count,
            ),
        )?;
    }
    let reflected_registers = buffers
        .iter()
        .map(|buffer| buffer.register)
        .collect::<BTreeSet<_>>();
    let bound_registers = bindings
        .iter()
        .filter(|binding| binding.kind == SceneShaderBindingKind::UniformBuffer)
        .map(|binding| binding.register)
        .collect::<BTreeSet<_>>();
    if reflected_registers != bound_registers {
        return invalid(
            program,
            "shader uniform-buffer bindings do not match typed reflection",
        );
    }
    Ok(())
}

fn validate_uniform_members(
    document: &SceneBinaryDocument,
    program: &SceneShaderProgramRecord,
    buffer: &SceneShaderUniformBufferRecord,
    members: &[SceneShaderUniformMemberRecord],
) -> Result<(), SceneStorageError> {
    let mut names = BTreeSet::new();
    for member in members {
        validate_string(document, "shader_uniform_member.name", member.name)?;
        validate_string(
            document,
            "shader_uniform_member.material_parameter",
            member.material_parameter,
        )?;
        if member.material_parameter.is_some()
            && document.strings[member.material_parameter.0 as usize].is_empty()
        {
            return invalid(program, "shader uniform material parameter is empty");
        }
        if !member.name.is_some() || !names.insert(member.name) {
            return invalid(
                program,
                "shader uniform member has an empty or duplicate name",
            );
        }
        if member.byte_size == 0
            || member.rows == 0
            || member.columns == 0
            || member.array_count == 0
        {
            return invalid(program, "shader uniform member shape is empty");
        }
        if member
            .byte_offset
            .checked_add(member.byte_size)
            .is_none_or(|end| end > buffer.byte_size)
        {
            return invalid(program, "shader uniform member exceeds its buffer");
        }
        if (member.array_count == 1) != (member.array_stride == 0) {
            return invalid(program, "shader uniform member array stride is invalid");
        }
        if member.array_count > 1
            && member
                .array_count
                .checked_sub(1)
                .and_then(|count| count.checked_mul(member.array_stride))
                .is_none_or(|last_offset| last_offset >= member.byte_size)
        {
            return invalid(program, "shader uniform member array extent is invalid");
        }
        if (member.columns == 1) != (member.matrix_stride == 0) {
            return invalid(program, "shader uniform member matrix stride is invalid");
        }
    }
    Ok(())
}

fn validate_stage_links(document: &SceneBinaryDocument) -> Result<(), SceneStorageError> {
    for fragment in document
        .shader_programs
        .iter()
        .filter(|program| program.stage == SceneShaderStage::Fragment)
    {
        let Some(vertex) = document.shader_programs.iter().find(|program| {
            program.program_key == fragment.program_key && program.stage == SceneShaderStage::Vertex
        }) else {
            continue;
        };
        let vertex_io = slice(
            &document.shader_stage_io,
            vertex.stage_io_start,
            vertex.stage_io_count,
        );
        let fragment_io = slice(
            &document.shader_stage_io,
            fragment.stage_io_start,
            fragment.stage_io_count,
        );
        for input in fragment_io
            .iter()
            .filter(|item| item.direction == SceneShaderIoDirection::Input)
        {
            let compatible = vertex_io.iter().any(|output| {
                output.direction == SceneShaderIoDirection::Output
                    && output.location == input.location
                    && output.scalar_type == input.scalar_type
                    && output.rows == input.rows
                    && output.columns == input.columns
                    && output.location_count == input.location_count
            });
            if !compatible {
                return invalid_with_detail(
                    document,
                    fragment,
                    "fragment input has no compatible vertex output",
                    format!(
                        "fragment input {}; available vertex outputs: [{}]",
                        stage_io_diagnostic(document, input),
                        vertex_io
                            .iter()
                            .filter(|item| item.direction == SceneShaderIoDirection::Output)
                            .map(|output| stage_io_diagnostic(document, output))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
        }
    }
    Ok(())
}

fn validate_bindings(
    program: &SceneShaderProgramRecord,
    bindings: &[SceneShaderBindingRecord],
) -> Result<(), SceneStorageError> {
    let mut registers = BTreeSet::new();
    let mut push_offsets = BTreeSet::new();
    for binding in bindings {
        if binding.descriptor_count == 0 {
            return invalid(program, "shader binding has zero descriptors");
        }
        if !registers.insert((binding.kind, binding.register)) {
            return invalid(program, "shader binding repeats a kind/register pair");
        }
        if !binding.push_offset.is_multiple_of(4)
            || binding
                .push_offset
                .checked_add(4)
                .is_none_or(|end| end > program.push_constant_bytes)
        {
            return invalid(
                program,
                "shader binding push offset is outside its push ABI",
            );
        }
        if !push_offsets.insert(binding.push_offset) {
            return invalid(program, "shader bindings share a push offset");
        }
    }
    Ok(())
}

fn validate_spirv(
    program: &SceneShaderProgramRecord,
    words: &[u32],
    requires_heap: bool,
) -> Result<(), SceneStorageError> {
    if words.len() < 5 || words[0] != SPIRV_MAGIC || words[4] != 0 {
        return invalid(program, "SPIR-V header is invalid");
    }
    let mut heap_capability = false;
    let mut heap_extension = false;
    let mut offset = 5;
    while offset < words.len() {
        let word_count = (words[offset] >> 16) as usize;
        let Some(end) = offset.checked_add(word_count) else {
            return invalid(program, "SPIR-V instruction range overflows");
        };
        if word_count == 0 || end > words.len() {
            return invalid(program, "SPIR-V instruction stream is truncated");
        }
        let opcode = words[offset] & 0xffff;
        let operands = &words[offset + 1..end];
        match opcode {
            10 => heap_extension |= spirv_string(operands) == "SPV_EXT_descriptor_heap",
            17 => {
                heap_capability |= operands.first() == Some(&SPIRV_CAPABILITY_DESCRIPTOR_HEAP_EXT)
            }
            71 if operands.get(1).is_some_and(|decoration| {
                matches!(
                    *decoration,
                    SPIRV_DECORATION_BINDING | SPIRV_DECORATION_DESCRIPTOR_SET
                )
            }) =>
            {
                return invalid(program, "SPIR-V contains legacy descriptor decorations");
            }
            _ => {}
        }
        offset = end;
    }
    if requires_heap && !(heap_capability && heap_extension) {
        return invalid(
            program,
            "SPIR-V binding metadata requires the descriptor heap",
        );
    }
    if !requires_heap && (heap_capability || heap_extension) {
        return invalid(program, "descriptor-heap SPIR-V has no binding metadata");
    }
    Ok(())
}

fn spirv_string(words: &[u32]) -> String {
    let bytes = words
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .take_while(|byte| *byte != 0)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn slice<T>(values: &[T], start: u32, count: u32) -> &[T] {
    let start = start as usize;
    &values[start..start + count as usize]
}

fn invalid<T>(
    program: &SceneShaderProgramRecord,
    reason: &'static str,
) -> Result<T, SceneStorageError> {
    Err(SceneStorageError::InvalidShaderProgram {
        program: program.program_key,
        program_key: None,
        reason,
        detail: None,
    })
}

fn invalid_with_detail<T>(
    document: &SceneBinaryDocument,
    program: &SceneShaderProgramRecord,
    reason: &'static str,
    detail: String,
) -> Result<T, SceneStorageError> {
    Err(SceneStorageError::InvalidShaderProgram {
        program: program.program_key,
        program_key: document
            .strings
            .get(program.program_key.0 as usize)
            .cloned(),
        reason,
        detail: Some(detail),
    })
}

fn stage_io_diagnostic(document: &SceneBinaryDocument, item: &SceneShaderStageIoRecord) -> String {
    let name = document
        .strings
        .get(item.name.0 as usize)
        .map_or("<invalid-name>", String::as_str);
    format!(
        "{name} at location {} ({:?}, rows={}, columns={}, locations={})",
        item.location, item.scalar_type, item.rows, item.columns, item.location_count
    )
}

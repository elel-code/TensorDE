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
        validate_spirv(program, spirv, !bindings.is_empty())?;
        validate_bindings(program, bindings)?;
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
            "SPIR-V binding metadata requires native descriptor heap",
        );
    }
    if !requires_heap && (heap_capability || heap_extension) {
        return invalid(
            program,
            "native descriptor-heap SPIR-V has no binding metadata",
        );
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
        reason,
    })
}

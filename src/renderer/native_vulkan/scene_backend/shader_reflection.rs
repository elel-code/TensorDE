//! Minimal SPIR-V resource reflection for descriptor-heap shader mappings.
//!
//! References:
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

use std::collections::{BTreeMap, BTreeSet};

const SPIRV_MAGIC: u32 = 0x0723_0203;
const OP_DECORATE: u16 = 71;
const OP_VARIABLE: u16 = 59;
const DECORATION_BINDING: u32 = 33;
const DECORATION_RESOURCE_GROUP: u32 = 34;
const STORAGE_CLASS_UNIFORM: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneSpirvResourceReflection {
    pub uniform_buffer_bindings: BTreeSet<u32>,
    pub command_order: [&'static str; 4],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_reflect_scene_spirv_resources(
    words: &[u32],
    label: &'static str,
) -> Result<NativeVulkanSceneSpirvResourceReflection, String> {
    if words.len() < 5 || words[0] != SPIRV_MAGIC {
        return Err(format!(
            "{label} is not valid SPIR-V for resource reflection"
        ));
    }
    let mut bindings = BTreeMap::<u32, u32>::new();
    let mut resource_groups = BTreeMap::<u32, u32>::new();
    let mut uniform_variables = BTreeSet::<u32>::new();
    let mut cursor = 5usize;
    while cursor < words.len() {
        let instruction = words[cursor];
        let word_count = (instruction >> 16) as usize;
        let opcode = (instruction & 0xffff) as u16;
        if word_count == 0 {
            return Err(format!(
                "{label} SPIR-V instruction at word {cursor} has zero word count"
            ));
        }
        let end = cursor
            .checked_add(word_count)
            .ok_or_else(|| format!("{label} SPIR-V instruction at word {cursor} overflows"))?;
        if end > words.len() {
            return Err(format!(
                "{label} SPIR-V instruction at word {cursor} exceeds module length"
            ));
        }
        let operands = &words[cursor + 1..end];
        match opcode {
            OP_DECORATE => {
                reflect_decoration(label, operands, &mut bindings, &mut resource_groups)?
            }
            OP_VARIABLE => reflect_variable(operands, &mut uniform_variables),
            _ => {}
        }
        cursor = end;
    }

    let mut uniform_buffer_bindings = BTreeSet::new();
    for variable in uniform_variables {
        let Some(binding) = bindings.get(&variable).copied() else {
            continue;
        };
        let resource_group = resource_groups.get(&variable).copied().unwrap_or(0);
        if resource_group != 0 {
            return Err(format!(
                "{label} uniform variable {variable} uses SPIR-V resource group {resource_group}; scene descriptor heap only accepts group 0"
            ));
        }
        uniform_buffer_bindings.insert(binding);
    }
    Ok(NativeVulkanSceneSpirvResourceReflection {
        uniform_buffer_bindings,
        command_order: [
            "scan_spirv_decorations",
            "scan_spirv_uniform_variables",
            "join_uniform_variables_to_binding_decorations",
            "reject_nonzero_resource_groups",
        ],
    })
}

fn reflect_decoration(
    label: &'static str,
    operands: &[u32],
    bindings: &mut BTreeMap<u32, u32>,
    resource_groups: &mut BTreeMap<u32, u32>,
) -> Result<(), String> {
    if operands.len() < 2 {
        return Err(format!("{label} SPIR-V OpDecorate is truncated"));
    }
    let target = operands[0];
    let decoration = operands[1];
    match decoration {
        DECORATION_BINDING => {
            let binding = operands.get(2).copied().ok_or_else(|| {
                format!("{label} SPIR-V Binding decoration for id {target} is truncated")
            })?;
            bindings.insert(target, binding);
        }
        DECORATION_RESOURCE_GROUP => {
            let resource_group = operands.get(2).copied().ok_or_else(|| {
                format!("{label} SPIR-V resource group decoration for id {target} is truncated")
            })?;
            resource_groups.insert(target, resource_group);
        }
        _ => {}
    }
    Ok(())
}

fn reflect_variable(operands: &[u32], uniform_variables: &mut BTreeSet<u32>) {
    if operands.len() < 3 {
        return;
    }
    let result_id = operands[1];
    let storage_class = operands[2];
    if storage_class == STORAGE_CLASS_UNIFORM {
        uniform_variables.insert(result_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spirv_reflection_extracts_set_zero_uniform_buffer_binding() {
        let words = spirv_words(vec![
            instr(OP_DECORATE, &[7, DECORATION_BINDING, 5]),
            instr(OP_DECORATE, &[7, DECORATION_RESOURCE_GROUP, 0]),
            instr(OP_VARIABLE, &[3, 7, STORAGE_CLASS_UNIFORM]),
        ]);

        let reflection =
            native_vulkan_reflect_scene_spirv_resources(&words, "test").expect("reflection");

        assert_eq!(reflection.uniform_buffer_bindings, BTreeSet::from([5]));
    }

    #[test]
    fn spirv_reflection_rejects_nonzero_resource_group_uniforms() {
        let words = spirv_words(vec![
            instr(OP_DECORATE, &[7, DECORATION_BINDING, 5]),
            instr(OP_DECORATE, &[7, DECORATION_RESOURCE_GROUP, 1]),
            instr(OP_VARIABLE, &[3, 7, STORAGE_CLASS_UNIFORM]),
        ]);

        let err = native_vulkan_reflect_scene_spirv_resources(&words, "test")
            .expect_err("group 1 must fail");

        assert!(err.contains("resource group 1"));
    }

    #[test]
    fn spirv_reflection_rejects_truncated_instruction() {
        let mut words = vec![SPIRV_MAGIC, 0x0001_0000, 0, 8, 0];
        words.push((3u32 << 16) | u32::from(OP_DECORATE));
        words.push(7);

        let err = native_vulkan_reflect_scene_spirv_resources(&words, "test")
            .expect_err("truncated instruction must fail");

        assert!(err.contains("exceeds module length"));
    }

    fn spirv_words(instructions: Vec<Vec<u32>>) -> Vec<u32> {
        let mut words = vec![SPIRV_MAGIC, 0x0001_0000, 0, 32, 0];
        for instruction in instructions {
            words.extend(instruction);
        }
        words
    }

    fn instr(opcode: u16, operands: &[u32]) -> Vec<u32> {
        let mut words = vec![((operands.len() as u32 + 1) << 16) | u32::from(opcode)];
        words.extend_from_slice(operands);
        words
    }
}

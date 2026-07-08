//! Minimal SPIR-V resource reflection for descriptor-heap shader mappings.
//!
//! References:
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

use std::collections::{BTreeMap, BTreeSet};

const SPIRV_MAGIC: u32 = 0x0723_0203;
const OP_TYPE_IMAGE: u16 = 25;
const OP_TYPE_SAMPLED_IMAGE: u16 = 27;
const OP_TYPE_POINTER: u16 = 32;
const OP_DECORATE: u16 = 71;
const OP_VARIABLE: u16 = 59;
const DECORATION_BINDING: u32 = 33;
const DECORATION_RESOURCE_GROUP: u32 = 34;
const STORAGE_CLASS_UNIFORM_CONSTANT: u32 = 0;
const STORAGE_CLASS_UNIFORM: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneSpirvResourceReflection {
    pub uniform_buffer_bindings: BTreeSet<u32>,
    pub sampled_image_bindings: BTreeSet<u32>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneSpirvTypeKind {
    Image,
    SampledImage,
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
    let mut type_kinds = BTreeMap::<u32, SceneSpirvTypeKind>::new();
    let mut pointer_types = BTreeMap::<u32, u32>::new();
    let mut uniform_variables = BTreeSet::<u32>::new();
    let mut sampled_image_variables = BTreeSet::<u32>::new();
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
            OP_TYPE_IMAGE => reflect_type_image(operands, &mut type_kinds),
            OP_TYPE_SAMPLED_IMAGE => reflect_type_sampled_image(operands, &mut type_kinds),
            OP_TYPE_POINTER => reflect_type_pointer(operands, &mut pointer_types),
            OP_DECORATE => {
                reflect_decoration(label, operands, &mut bindings, &mut resource_groups)?
            }
            OP_VARIABLE => reflect_variable(
                operands,
                &type_kinds,
                &pointer_types,
                &mut uniform_variables,
                &mut sampled_image_variables,
            ),
            _ => {}
        }
        cursor = end;
    }

    let mut uniform_buffer_bindings = BTreeSet::new();
    for variable in uniform_variables {
        if let Some(binding) = reflected_resource_binding(
            label,
            variable,
            "uniform variable",
            &bindings,
            &resource_groups,
        )? {
            uniform_buffer_bindings.insert(binding);
        }
    }
    let mut sampled_image_bindings = BTreeSet::new();
    for variable in sampled_image_variables {
        if let Some(binding) = reflected_resource_binding(
            label,
            variable,
            "sampled image variable",
            &bindings,
            &resource_groups,
        )? {
            sampled_image_bindings.insert(binding);
        }
    }
    Ok(NativeVulkanSceneSpirvResourceReflection {
        uniform_buffer_bindings,
        sampled_image_bindings,
        command_order: [
            "scan_spirv_decorations",
            "scan_spirv_resource_types",
            "scan_spirv_uniform_variables",
            "scan_spirv_sampled_image_variables",
            "join_resource_variables_to_binding_decorations",
            "reject_nonzero_resource_groups",
        ],
    })
}

fn reflect_type_image(operands: &[u32], type_kinds: &mut BTreeMap<u32, SceneSpirvTypeKind>) {
    if let Some(result_id) = operands.first().copied() {
        type_kinds.insert(result_id, SceneSpirvTypeKind::Image);
    }
}

fn reflect_type_sampled_image(
    operands: &[u32],
    type_kinds: &mut BTreeMap<u32, SceneSpirvTypeKind>,
) {
    if let Some(result_id) = operands.first().copied() {
        type_kinds.insert(result_id, SceneSpirvTypeKind::SampledImage);
    }
}

fn reflect_type_pointer(operands: &[u32], pointer_types: &mut BTreeMap<u32, u32>) {
    if operands.len() < 3 {
        return;
    }
    let result_id = operands[0];
    let storage_class = operands[1];
    let pointee_type = operands[2];
    if storage_class == STORAGE_CLASS_UNIFORM_CONSTANT || storage_class == STORAGE_CLASS_UNIFORM {
        pointer_types.insert(result_id, pointee_type);
    }
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

fn reflect_variable(
    operands: &[u32],
    type_kinds: &BTreeMap<u32, SceneSpirvTypeKind>,
    pointer_types: &BTreeMap<u32, u32>,
    uniform_variables: &mut BTreeSet<u32>,
    sampled_image_variables: &mut BTreeSet<u32>,
) {
    if operands.len() < 3 {
        return;
    }
    let result_type_id = operands[0];
    let result_id = operands[1];
    let storage_class = operands[2];
    if storage_class == STORAGE_CLASS_UNIFORM {
        uniform_variables.insert(result_id);
    } else if storage_class == STORAGE_CLASS_UNIFORM_CONSTANT
        && pointer_type_is_sampled_image(result_type_id, type_kinds, pointer_types)
    {
        sampled_image_variables.insert(result_id);
    }
}

fn pointer_type_is_sampled_image(
    pointer_type_id: u32,
    type_kinds: &BTreeMap<u32, SceneSpirvTypeKind>,
    pointer_types: &BTreeMap<u32, u32>,
) -> bool {
    pointer_types
        .get(&pointer_type_id)
        .and_then(|pointee| type_kinds.get(pointee))
        .is_some_and(|kind| {
            matches!(
                kind,
                SceneSpirvTypeKind::Image | SceneSpirvTypeKind::SampledImage
            )
        })
}

fn reflected_resource_binding(
    label: &'static str,
    variable: u32,
    kind: &'static str,
    bindings: &BTreeMap<u32, u32>,
    resource_groups: &BTreeMap<u32, u32>,
) -> Result<Option<u32>, String> {
    let Some(binding) = bindings.get(&variable).copied() else {
        return Ok(None);
    };
    let resource_group = resource_groups.get(&variable).copied().unwrap_or(0);
    if resource_group != 0 {
        return Err(format!(
            "{label} {kind} {variable} uses SPIR-V resource group {resource_group}; scene descriptor heap only accepts group 0"
        ));
    }
    Ok(Some(binding))
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
        assert!(reflection.sampled_image_bindings.is_empty());
    }

    #[test]
    fn spirv_reflection_extracts_sampled_image_binding() {
        let words = spirv_words(vec![
            instr(OP_TYPE_IMAGE, &[3, 1, 2, 0, 0, 0, 1]),
            instr(OP_TYPE_SAMPLED_IMAGE, &[4, 3]),
            instr(OP_TYPE_POINTER, &[5, STORAGE_CLASS_UNIFORM_CONSTANT, 4]),
            instr(OP_DECORATE, &[7, DECORATION_BINDING, 2]),
            instr(OP_DECORATE, &[7, DECORATION_RESOURCE_GROUP, 0]),
            instr(OP_VARIABLE, &[5, 7, STORAGE_CLASS_UNIFORM_CONSTANT]),
        ]);

        let reflection =
            native_vulkan_reflect_scene_spirv_resources(&words, "test").expect("reflection");

        assert_eq!(reflection.sampled_image_bindings, BTreeSet::from([2]));
        assert!(reflection.uniform_buffer_bindings.is_empty());
    }

    #[test]
    fn spirv_reflection_rejects_nonzero_resource_group_uniforms_and_textures() {
        let words = spirv_words(vec![
            instr(OP_DECORATE, &[7, DECORATION_BINDING, 5]),
            instr(OP_DECORATE, &[7, DECORATION_RESOURCE_GROUP, 1]),
            instr(OP_VARIABLE, &[3, 7, STORAGE_CLASS_UNIFORM]),
        ]);

        let err = native_vulkan_reflect_scene_spirv_resources(&words, "test")
            .expect_err("group 1 must fail");

        assert!(err.contains("resource group 1"));

        let words = spirv_words(vec![
            instr(OP_TYPE_SAMPLED_IMAGE, &[4, 3]),
            instr(OP_TYPE_POINTER, &[5, STORAGE_CLASS_UNIFORM_CONSTANT, 4]),
            instr(OP_DECORATE, &[7, DECORATION_BINDING, 2]),
            instr(OP_DECORATE, &[7, DECORATION_RESOURCE_GROUP, 1]),
            instr(OP_VARIABLE, &[5, 7, STORAGE_CLASS_UNIFORM_CONSTANT]),
        ]);
        let err = native_vulkan_reflect_scene_spirv_resources(&words, "test")
            .expect_err("group 1 texture must fail");

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

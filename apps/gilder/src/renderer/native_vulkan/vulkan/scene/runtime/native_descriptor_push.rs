//! Typed push-data payloads for native `SPV_EXT_descriptor_heap` scene shaders.

use vulkan_renderer::descriptor_heap_element_index;

use crate::engine::scene::{SceneRenderingDeviceMeshDraw, SceneStorage};
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneDescriptorBinding, BuiltinSceneDescriptorBindingKind,
    BuiltinSceneLocalReadShader, BuiltinSceneShader, BuiltinSceneVertexShader,
    native_vulkan_scene_shader_for_key,
};
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
};

use super::shader_descriptor_push::{
    SceneOwnedDescriptorHeapIndex, write_scene_owned_descriptor_push,
};
use super::shader_program::{
    SceneOwnedStageResourcePlan, SceneResolvedGraphicsProgram, resolve_scene_graphics_program,
    scene_owned_stage_resource_plan,
};
use super::{SceneGpuDrawCommand, ScenePipelineDescriptorLayout};

#[derive(Clone, Copy)]
enum BuiltinSceneShaderStage {
    Fragment,
    Vertex,
}

impl BuiltinSceneShaderStage {
    fn label(self) -> &'static str {
        match self {
            Self::Fragment => "fragment",
            Self::Vertex => "vertex",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SceneNativeDescriptorPush {
    EngineBuiltIn(Vec<u8>),
    SceneOwned(Vec<u8>),
}

impl SceneNativeDescriptorPush {
    pub(super) fn bytes(&self) -> &[u8] {
        match self {
            Self::EngineBuiltIn(bytes) => bytes,
            Self::SceneOwned(bytes) => bytes,
        }
    }

    fn byte_len(&self) -> u64 {
        match self {
            Self::EngineBuiltIn(bytes) => bytes.len() as u64,
            Self::SceneOwned(bytes) => bytes.len() as u64,
        }
    }
}

pub(super) fn resolve_scene_native_descriptor_pushes(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    layout: &ScenePipelineDescriptorLayout,
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    commands: &mut [SceneGpuDrawCommand],
) -> Result<(), String> {
    if draws.len() != commands.len() {
        return Err("scene native descriptor push draw count does not match commands".to_owned());
    }
    for (draw, command) in draws.iter().zip(commands) {
        let program = resolve_scene_graphics_program(storage, draw.shader_key, draw.primitive)?;
        command.native_descriptor_push = match program {
            SceneResolvedGraphicsProgram::SceneOwned {
                key,
                vertex,
                fragment,
            } => {
                let vertex = scene_owned_stage_resource_plan(storage, vertex)?;
                let fragment = scene_owned_stage_resource_plan(storage, fragment)?;
                let push = scene_owned_pipeline_push(&vertex, &fragment, layout, plan, command)?;
                validate_native_push_size(key, &push, plan.max_push_data_size)?;
                Some(push)
            }
            SceneResolvedGraphicsProgram::EngineBuiltIn {
                key,
                shader,
                vertex,
            } => {
                let push = builtin_pipeline_push(shader, vertex, layout, plan, command)?;
                validate_native_push_size(key, &push, plan.max_push_data_size)?;
                Some(push)
            }
        };
        command.disabled_native_descriptor_push = if command.disabled_pipeline_index.is_some() {
            let passthrough = native_vulkan_scene_shader_for_key("we/passthrough")
                .ok_or_else(|| "engine-owned passthrough shader is not built in".to_owned())?;
            let push = match program {
                SceneResolvedGraphicsProgram::EngineBuiltIn { vertex, .. } => {
                    builtin_pipeline_push(passthrough, vertex, layout, plan, command)?
                }
                SceneResolvedGraphicsProgram::SceneOwned { vertex, .. } => {
                    let vertex = scene_owned_stage_resource_plan(storage, vertex)?;
                    scene_owned_vertex_builtin_fragment_push(
                        &vertex,
                        passthrough,
                        layout,
                        plan,
                        command,
                    )?
                }
            };
            validate_native_push_size("we/passthrough", &push, plan.max_push_data_size)?;
            Some(push)
        } else {
            None
        };
    }
    Ok(())
}

fn builtin_pipeline_push(
    shader: &BuiltinSceneShader,
    vertex: BuiltinSceneVertexShader,
    layout: &ScenePipelineDescriptorLayout,
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    draw: &SceneGpuDrawCommand,
) -> Result<SceneNativeDescriptorPush, String> {
    let byte_count = shader
        .fragment_push_constant_bytes
        .max(vertex.push_constant_bytes)
        .max(
            shader
                .local_read_shader
                .as_ref()
                .map_or(0, |local_read| local_read.push_constant_bytes),
        ) as usize;
    let mut bytes = vec![0; byte_count];
    let resource_slice_base = descriptor_slice_base(
        &plan.resource_descriptor_offsets,
        draw.resource_descriptor_base,
        plan.resource_heap_alignment,
        "resource",
    )?;
    let sampler_slice_base = shader
        .fragment_bindings
        .iter()
        .any(|binding| binding.kind == BuiltinSceneDescriptorBindingKind::Sampler)
        .then(|| {
            descriptor_slice_base(
                &plan.sampler_descriptor_offsets,
                draw.sampler_descriptor_base,
                plan.sampler_heap_alignment,
                "sampler",
            )
        })
        .transpose()?;
    for (stage, bindings) in [
        (
            BuiltinSceneShaderStage::Fragment,
            shader.fragment_bindings,
        ),
        (BuiltinSceneShaderStage::Vertex, vertex.bindings),
    ] {
        for binding in bindings {
            let element_index = builtin_stage_binding_index(
                stage, binding, layout, plan, draw, resource_slice_base, sampler_slice_base,
            )?;
            let offset = binding.push_offset as usize;
            let push_byte_count = bytes.len();
            let target = bytes.get_mut(offset..offset + 4).ok_or_else(|| {
                format!(
                    "built-in scene {stage} binding {:?} register {} exceeds {} push bytes",
                    binding.kind,
                    binding.register,
                    push_byte_count,
                    stage = stage.label()
                )
            })?;
            target.copy_from_slice(&element_index.to_le_bytes());
        }
    }
    if let Some(local_read) = shader.local_read_shader.as_ref() {
        for binding in local_read.bindings {
            let Some(element_index) = builtin_local_read_binding_index(
                local_read,
                binding,
                layout,
                plan,
                draw,
                resource_slice_base,
            )?
            else {
                continue;
            };
            if vertex
                .bindings
                .iter()
                .any(|vertex_binding| vertex_binding.push_offset == binding.push_offset)
            {
                return Err(format!(
                    "built-in vertex push overlaps local-read input at byte {}",
                    binding.push_offset
                ));
            }
            let offset = binding.push_offset as usize;
            let push_byte_count = bytes.len();
            let target = bytes.get_mut(offset..offset + 4).ok_or_else(|| {
                format!(
                    "built-in scene local-read binding {:?} register {} exceeds {} push bytes",
                    binding.kind, binding.register, push_byte_count
                )
            })?;
            target.copy_from_slice(&element_index.to_le_bytes());
        }
    }
    Ok(SceneNativeDescriptorPush::EngineBuiltIn(bytes))
}

fn scene_owned_vertex_builtin_fragment_push(
    vertex: &SceneOwnedStageResourcePlan<'_>,
    shader: &BuiltinSceneShader,
    layout: &ScenePipelineDescriptorLayout,
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    draw: &SceneGpuDrawCommand,
) -> Result<SceneNativeDescriptorPush, String> {
    let local_read_push_bytes = shader
        .local_read_shader
        .as_ref()
        .filter(|local_read| {
            local_read.input_attachments.iter().any(|input| {
                layout.input_attachment_slots.contains(&input.slot)
            })
        })
        .map_or(0, |local_read| local_read.push_constant_bytes);
    let byte_count = shader
        .fragment_push_constant_bytes
        .max(local_read_push_bytes)
        .max(vertex.push_constant_bytes) as usize;
    let mut bytes = vec![0; byte_count];
    let resource_slice_base = descriptor_slice_base(
        &plan.resource_descriptor_offsets,
        draw.resource_descriptor_base,
        plan.resource_heap_alignment,
        "resource",
    )?;
    let sampler_slice_base = shader
        .fragment_bindings
        .iter()
        .any(|binding| binding.kind == BuiltinSceneDescriptorBindingKind::Sampler)
        .then(|| {
            descriptor_slice_base(
                &plan.sampler_descriptor_offsets,
                draw.sampler_descriptor_base,
                plan.sampler_heap_alignment,
                "sampler",
            )
        })
        .transpose()?;
    if let Some(overlap) = shader.fragment_bindings.iter().find_map(|fragment| {
        vertex
            .bindings
            .iter()
            .find(|vertex| vertex.push_offset == fragment.push_offset)
            .map(|vertex| (fragment, vertex))
    }) {
        return Err(format!(
            "built-in passthrough fragment {:?} register {} overlaps scene-owned vertex {:?} register {} at byte {}",
            overlap.0.kind,
            overlap.0.register,
            overlap.1.kind,
            overlap.1.register,
            overlap.0.push_offset
        ));
    }
    for binding in shader.fragment_bindings {
        let element_index = builtin_stage_binding_index(
            BuiltinSceneShaderStage::Fragment,
            binding,
            layout,
            plan,
            draw,
            resource_slice_base,
            sampler_slice_base,
        )?;
        write_binding_word(
            &mut bytes,
            binding.push_offset,
            element_index,
            "built-in passthrough fragment",
        )?;
    }
    if let Some(local_read) = shader.local_read_shader.as_ref() {
        for binding in local_read.bindings {
            let Some(element_index) = builtin_local_read_binding_index(
                local_read,
                binding,
                layout,
                plan,
                draw,
                resource_slice_base,
            )?
            else {
                continue;
            };
            if vertex
                .bindings
                .iter()
                .any(|vertex_binding| vertex_binding.push_offset == binding.push_offset)
            {
                return Err(format!(
                    "scene-owned vertex push overlaps local-read input at byte {}",
                    binding.push_offset
                ));
            }
            write_binding_word(
                &mut bytes,
                binding.push_offset,
                element_index,
                "built-in local-read fragment",
            )?;
        }
    }
    let mut uniform_index = 0;
    let indices = scene_owned_stage_indices(
        vertex,
        layout,
        plan,
        draw,
        resource_slice_base,
        sampler_slice_base,
        &mut uniform_index,
    )?;
    write_scene_owned_descriptor_push(vertex, &indices, &mut bytes)?;
    Ok(SceneNativeDescriptorPush::EngineBuiltIn(bytes))
}

fn write_binding_word(
    bytes: &mut [u8],
    push_offset: u32,
    element_index: u32,
    role: &str,
) -> Result<(), String> {
    let byte_count = bytes.len();
    let offset = push_offset as usize;
    let target = bytes
        .get_mut(offset..offset + 4)
        .ok_or_else(|| format!("{role} binding exceeds {byte_count} push bytes"))?;
    target.copy_from_slice(&element_index.to_le_bytes());
    Ok(())
}

fn builtin_local_read_binding_index(
    shader: &BuiltinSceneLocalReadShader,
    binding: &BuiltinSceneDescriptorBinding,
    layout: &ScenePipelineDescriptorLayout,
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    draw: &SceneGpuDrawCommand,
    resource_slice_base: u64,
) -> Result<Option<u32>, String> {
    if binding.kind != BuiltinSceneDescriptorBindingKind::InputAttachment {
        return Err(format!(
            "built-in local-read {:?} register {} is not an input attachment",
            binding.kind, binding.register
        ));
    }
    let slot = shader
        .input_attachments
        .iter()
        .find(|input| input.binding == binding.register)
        .map(|input| input.slot)
        .ok_or_else(|| {
            format!(
                "built-in local-read register {} has no typed input slot",
                binding.register
            )
        })?;
    let Some(input_index) = layout
        .input_attachment_slots
        .iter()
        .position(|candidate| *candidate == slot)
    else {
        return Ok(None);
    };
    let descriptor = draw.input_attachment_resource_descriptor_base + input_index;
    validate_resource_kind(
        plan,
        descriptor,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::InputAttachment,
        "built-in input attachment",
    )?;
    relative_element_index(
        &plan.resource_descriptor_offsets,
        descriptor,
        resource_slice_base,
        plan.image_descriptor_size,
        "built-in input attachment",
    )
    .map(Some)
}

fn builtin_stage_binding_index(
    stage: BuiltinSceneShaderStage,
    binding: &BuiltinSceneDescriptorBinding,
    layout: &ScenePipelineDescriptorLayout,
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    draw: &SceneGpuDrawCommand,
    resource_slice_base: u64,
    sampler_slice_base: Option<u64>,
) -> Result<u32, String> {
    if matches!(stage, BuiltinSceneShaderStage::Vertex) {
        return builtin_vertex_binding_index(binding, plan, draw, resource_slice_base);
    }
    match binding.kind {
        BuiltinSceneDescriptorBindingKind::UniformBuffer => {
            let descriptor = draw.material_resource_descriptor.ok_or_else(|| {
                "built-in fragment material uniform has no retained descriptor".to_owned()
            })?;
            validate_resource_kind(
                plan,
                descriptor,
                NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                "built-in material uniform",
            )?;
            relative_element_index(
                &plan.resource_descriptor_offsets,
                descriptor,
                resource_slice_base,
                plan.buffer_descriptor_size,
                "built-in material uniform",
            )
        }
        BuiltinSceneDescriptorBindingKind::SampledImage => {
            let descriptor = sampled_descriptor(layout, draw, builtin_texture_slot(binding.register))?;
            validate_resource_kind(
                plan,
                descriptor,
                NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                "built-in sampled image",
            )?;
            relative_element_index(
                &plan.resource_descriptor_offsets,
                descriptor,
                resource_slice_base,
                plan.image_descriptor_size,
                "built-in sampled image",
            )
        }
        BuiltinSceneDescriptorBindingKind::Sampler => {
            let sampled_index = sampled_slot_index(layout, builtin_texture_slot(binding.register))?;
            relative_element_index(
                &plan.sampler_descriptor_offsets,
                draw.sampler_descriptor_base + sampled_index,
                sampler_slice_base.ok_or_else(|| {
                    "built-in fragment sampler has no bound sampler heap slice".to_owned()
                })?,
                plan.sampler_descriptor_size,
                "built-in sampler",
            )
        }
        BuiltinSceneDescriptorBindingKind::InputAttachment
        | BuiltinSceneDescriptorBindingKind::StorageImage
        | BuiltinSceneDescriptorBindingKind::StorageBuffer => Err(format!(
            "built-in fragment {:?} register {} has no retained descriptor source",
            binding.kind, binding.register
        )),
    }
}

fn builtin_vertex_binding_index(
    binding: &BuiltinSceneDescriptorBinding,
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    draw: &SceneGpuDrawCommand,
    resource_slice_base: u64,
) -> Result<u32, String> {
    let (descriptor, kind, label) = match (binding.kind, binding.register) {
        (BuiltinSceneDescriptorBindingKind::UniformBuffer, 2) => (
            draw.resource_descriptor_base,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
            "built-in draw uniform",
        ),
        (BuiltinSceneDescriptorBindingKind::UniformBuffer, 3) => (
            draw.material_resource_descriptor.ok_or_else(|| {
                "built-in vertex material uniform has no retained descriptor".to_owned()
            })?,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
            "built-in vertex material uniform",
        ),
        (BuiltinSceneDescriptorBindingKind::StorageBuffer, 4) => (
            draw.skinning_resource_descriptor.ok_or_else(|| {
                "built-in vertex skinning buffer has no retained descriptor".to_owned()
            })?,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
            "built-in vertex skinning buffer",
        ),
        _ => {
            return Err(format!(
                "built-in vertex {:?} register {} has no retained descriptor source",
                binding.kind, binding.register
            ));
        }
    };
    validate_resource_kind(plan, descriptor, kind, label)?;
    relative_element_index(
        &plan.resource_descriptor_offsets,
        descriptor,
        resource_slice_base,
        plan.buffer_descriptor_size,
        label,
    )
}

fn builtin_texture_slot(register: u32) -> u32 {
    if register == 35 { 3 } else { register }
}

fn validate_native_push_size(
    key: &str,
    push: &SceneNativeDescriptorPush,
    max_push_data_size: u64,
) -> Result<(), String> {
    if push.byte_len() > max_push_data_size {
        return Err(format!(
            "native descriptor-heap scene shader {key:?} requires {} push-data bytes, device \
             supports {max_push_data_size}",
            push.byte_len()
        ));
    }
    Ok(())
}

fn scene_owned_pipeline_push(
    vertex: &SceneOwnedStageResourcePlan<'_>,
    fragment: &SceneOwnedStageResourcePlan<'_>,
    layout: &ScenePipelineDescriptorLayout,
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    draw: &SceneGpuDrawCommand,
) -> Result<SceneNativeDescriptorPush, String> {
    let byte_count = vertex.push_constant_bytes.max(fragment.push_constant_bytes) as usize;
    let mut bytes = vec![0; byte_count];
    let resource_slice_base = descriptor_slice_base(
        &plan.resource_descriptor_offsets,
        draw.resource_descriptor_base,
        plan.resource_heap_alignment,
        "resource",
    )?;
    let sampler_slice_base = (!fragment
        .bindings
        .iter()
        .all(|binding| binding.kind != crate::engine::scene::SceneShaderBindingKind::Sampler))
    .then(|| {
        descriptor_slice_base(
            &plan.sampler_descriptor_offsets,
            draw.sampler_descriptor_base,
            plan.sampler_heap_alignment,
            "sampler",
        )
    })
    .transpose()?;
    let mut uniform_index = 0usize;
    for stage in [vertex, fragment] {
        let indices = scene_owned_stage_indices(
            stage,
            layout,
            plan,
            draw,
            resource_slice_base,
            sampler_slice_base,
            &mut uniform_index,
        )?;
        write_scene_owned_descriptor_push(stage, &indices, &mut bytes)?;
    }
    Ok(SceneNativeDescriptorPush::SceneOwned(bytes))
}

fn scene_owned_stage_indices(
    stage: &SceneOwnedStageResourcePlan<'_>,
    layout: &ScenePipelineDescriptorLayout,
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    draw: &SceneGpuDrawCommand,
    resource_slice_base: u64,
    sampler_slice_base: Option<u64>,
    uniform_index: &mut usize,
) -> Result<Vec<SceneOwnedDescriptorHeapIndex>, String> {
    let uniform_base = *uniform_index;
    *uniform_index = uniform_index
        .checked_add(stage.uniform_buffers.len())
        .ok_or_else(|| "scene-owned uniform descriptor count overflows".to_owned())?;
    stage
        .bindings
        .iter()
        .map(|binding| {
            if binding.descriptor_count != 1 {
                return Err(format!(
                    "scene-owned {:?} {:?} register {} has unsupported descriptor count {}",
                    stage.stage, binding.kind, binding.register, binding.descriptor_count
                ));
            }
            let element_index = match binding.kind {
                crate::engine::scene::SceneShaderBindingKind::UniformBuffer => {
                    let local_index = stage
                        .uniform_buffers
                        .iter()
                        .position(|buffer| buffer.register == binding.register)
                        .ok_or_else(|| {
                            format!(
                                "scene-owned {:?} uniform register {} has no typed buffer",
                                stage.stage, binding.register
                            )
                        })?;
                    let descriptor = draw.scene_owned_uniform_descriptor_base
                        + uniform_base
                        + local_index;
                    validate_resource_kind(
                        plan,
                        descriptor,
                        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                        "scene-owned uniform",
                    )?;
                    relative_element_index(
                        &plan.resource_descriptor_offsets,
                        descriptor,
                        resource_slice_base,
                        plan.buffer_descriptor_size,
                        "scene-owned uniform",
                    )?
                }
                crate::engine::scene::SceneShaderBindingKind::SampledImage => {
                    let descriptor = sampled_descriptor(layout, draw, binding.register)?;
                    validate_resource_kind(
                        plan,
                        descriptor,
                        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                        "scene-owned sampled image",
                    )?;
                    relative_element_index(
                        &plan.resource_descriptor_offsets,
                        descriptor,
                        resource_slice_base,
                        plan.image_descriptor_size,
                        "scene-owned sampled image",
                    )?
                }
                crate::engine::scene::SceneShaderBindingKind::Sampler => {
                    let sampled_index = sampled_slot_index(layout, binding.register)?;
                    let descriptor = draw.sampler_descriptor_base + sampled_index;
                    relative_element_index(
                        &plan.sampler_descriptor_offsets,
                        descriptor,
                        sampler_slice_base.ok_or_else(|| {
                            "scene-owned sampler has no bound sampler heap slice".to_owned()
                        })?,
                        plan.sampler_descriptor_size,
                        "scene-owned sampler",
                    )?
                }
                unsupported => {
                    return Err(format!(
                        "scene-owned {:?} {unsupported:?} register {} has no retained resource source",
                        stage.stage, binding.register
                    ));
                }
            };
            Ok(SceneOwnedDescriptorHeapIndex {
                kind: binding.kind,
                register: binding.register,
                element_index,
            })
        })
        .collect()
}

fn sampled_descriptor(
    layout: &ScenePipelineDescriptorLayout,
    draw: &SceneGpuDrawCommand,
    register: u32,
) -> Result<usize, String> {
    Ok(draw.sampled_resource_descriptor_base + sampled_slot_index(layout, register)?)
}

fn sampled_slot_index(
    layout: &ScenePipelineDescriptorLayout,
    register: u32,
) -> Result<usize, String> {
    layout
        .sampled_slots
        .iter()
        .position(|slot| *slot == register)
        .ok_or_else(|| format!("scene-owned shader requires unplanned sampled slot {register}"))
}

fn descriptor_slice_base(
    offsets: &[u64],
    descriptor: usize,
    alignment: u64,
    role: &str,
) -> Result<u64, String> {
    let offset = offsets
        .get(descriptor)
        .copied()
        .ok_or_else(|| format!("native {role} heap descriptor {descriptor} has no byte offset"))?;
    Ok(if alignment <= 1 {
        offset
    } else {
        offset - offset % alignment
    })
}

fn relative_element_index(
    offsets: &[u64],
    descriptor: usize,
    slice_base: u64,
    descriptor_size: u64,
    role: &str,
) -> Result<u32, String> {
    let offset = offsets
        .get(descriptor)
        .copied()
        .ok_or_else(|| format!("native {role} descriptor {descriptor} has no byte offset"))?;
    let relative = offset
        .checked_sub(slice_base)
        .ok_or_else(|| format!("native {role} descriptor precedes its bound heap slice"))?;
    descriptor_heap_element_index(relative, descriptor_size)
        .map_err(|error| format!("resolve native {role} heap index: {error}"))
}

fn validate_resource_kind(
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor: usize,
    expected: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    role: &str,
) -> Result<(), String> {
    let actual = plan
        .resource_descriptor_kinds
        .get(descriptor)
        .copied()
        .ok_or_else(|| format!("native {role} descriptor {descriptor} is missing"))?;
    if actual != expected {
        return Err(format!(
            "native {role} descriptor {descriptor} has kind {actual:?}, expected {expected:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

//! Dense absolute heap-element pushes for shared renderer descriptor heaps.

use vulkan_renderer::DescriptorSlotKind;

use crate::engine::scene::{SceneRenderingDeviceMeshDraw, SceneStorage};
use crate::renderer::rendering_device::scene::{
    BuiltinSceneDescriptorBinding, BuiltinSceneDescriptorBindingKind, BuiltinSceneLocalReadShader,
    BuiltinSceneShader, BuiltinSceneVertexShader, rendering_device_scene_shader_for_key,
    rendering_device_scene_vertex_shader_for_primitive,
};

use super::super::shader_descriptor_push::{
    SceneOwnedDescriptorHeapIndex, write_scene_owned_descriptor_push,
};
use super::super::shader_program::{
    SceneOwnedStageResourcePlan, SceneResolvedGraphicsProgram, resolve_scene_graphics_program,
    scene_owned_stage_resource_plan,
};
use super::{
    BuiltinSceneShaderStage, SceneGpuDrawCommand, SceneDescriptorPush,
    ScenePipelineDescriptorLayout, sampled_descriptor, sampled_slot_index,
    validate_descriptor_push_size,
};

pub(super) struct DenseHeapPlan<'a> {
    pub(super) resource_kinds: &'a [DescriptorSlotKind],
    pub(super) sampler_count: usize,
}

impl DenseHeapPlan<'_> {
    fn resource_index(
        &self,
        descriptor: usize,
        expected: DescriptorSlotKind,
        role: &str,
    ) -> Result<u32, String> {
        let actual = self
            .resource_kinds
            .get(descriptor)
            .copied()
            .ok_or_else(|| format!("shared {role} descriptor {descriptor} is missing"))?;
        if actual != expected {
            return Err(format!(
                "shared {role} descriptor {descriptor} has kind {actual:?}, expected {expected:?}"
            ));
        }
        u32::try_from(descriptor).map_err(|_| format!("shared {role} descriptor index exceeds u32"))
    }

    fn sampler_index(&self, descriptor: usize, role: &str) -> Result<u32, String> {
        if descriptor >= self.sampler_count {
            return Err(format!(
                "shared {role} sampler descriptor {descriptor} exceeds {} elements",
                self.sampler_count
            ));
        }
        u32::try_from(descriptor)
            .map_err(|_| format!("shared {role} sampler descriptor index exceeds u32"))
    }
}

pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn resolve_scene_shared_descriptor_pushes(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    layout: &ScenePipelineDescriptorLayout,
    resource_kinds: &[DescriptorSlotKind],
    sampler_count: usize,
    max_push_data_size: u64,
    commands: &mut [SceneGpuDrawCommand],
) -> Result<(), String> {
    if draws.len() != commands.len() {
        return Err("shared scene descriptor push draw count does not match commands".to_owned());
    }
    let plan = DenseHeapPlan {
        resource_kinds,
        sampler_count,
    };
    for (draw, command) in draws.iter().zip(commands) {
        let program = resolve_scene_graphics_program(storage, draw.shader_key, draw.primitive)?;
        command.descriptor_push = match program {
            SceneResolvedGraphicsProgram::SceneOwned {
                key,
                vertex,
                fragment,
            } => {
                let vertex = scene_owned_stage_resource_plan(storage, vertex)?;
                let fragment = scene_owned_stage_resource_plan(storage, fragment)?;
                let push = scene_owned_pipeline_push(&vertex, &fragment, layout, &plan, command)?;
                validate_descriptor_push_size(key, &push, max_push_data_size)?;
                Some(push)
            }
            SceneResolvedGraphicsProgram::EngineBuiltIn {
                key,
                shader,
                vertex,
            } => {
                let push = builtin_pipeline_push(shader, vertex, layout, &plan, command)?;
                validate_descriptor_push_size(key, &push, max_push_data_size)?;
                Some(push)
            }
        };
        command.disabled_descriptor_push = if command.disabled_pipeline_index.is_some() {
            let passthrough = rendering_device_scene_shader_for_key("we/passthrough")
                .ok_or_else(|| "engine-owned passthrough shader is not built in".to_owned())?;
            let vertex =
                rendering_device_scene_vertex_shader_for_primitive(passthrough, draw.primitive)
                    .ok_or_else(|| {
                        format!(
                            "engine-owned passthrough shader has no {:?} vertex program",
                            draw.primitive
                        )
                    })?;
            let push = builtin_pipeline_push(passthrough, vertex, layout, &plan, command)?;
            validate_descriptor_push_size("we/passthrough", &push, max_push_data_size)?;
            Some(push)
        } else {
            None
        };
    }
    Ok(())
}

pub(super) fn builtin_pipeline_push(
    shader: &BuiltinSceneShader,
    vertex: BuiltinSceneVertexShader,
    layout: &ScenePipelineDescriptorLayout,
    plan: &DenseHeapPlan<'_>,
    draw: &SceneGpuDrawCommand,
) -> Result<SceneDescriptorPush, String> {
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
    for (stage, bindings) in [
        (BuiltinSceneShaderStage::Fragment, shader.fragment_bindings),
        (BuiltinSceneShaderStage::Vertex, vertex.bindings),
    ] {
        for binding in bindings {
            let element_index = builtin_stage_binding_index(stage, binding, layout, plan, draw)?;
            write_push_word(
                &mut bytes,
                binding.push_offset,
                element_index,
                &format!(
                    "built-in scene {} {:?} register {}",
                    stage.label(),
                    binding.kind,
                    binding.register
                ),
            )?;
        }
    }
    if let Some(local_read) = shader.local_read_shader.as_ref() {
        for binding in local_read.bindings {
            let Some(element_index) =
                builtin_local_read_binding_index(local_read, binding, layout, plan, draw)?
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
            write_push_word(
                &mut bytes,
                binding.push_offset,
                element_index,
                "built-in scene local-read input attachment",
            )?;
        }
    }
    Ok(SceneDescriptorPush::EngineBuiltIn(bytes))
}

fn builtin_local_read_binding_index(
    shader: &BuiltinSceneLocalReadShader,
    binding: &BuiltinSceneDescriptorBinding,
    layout: &ScenePipelineDescriptorLayout,
    plan: &DenseHeapPlan<'_>,
    draw: &SceneGpuDrawCommand,
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
    plan.resource_index(
        draw.input_attachment_resource_descriptor_base + input_index,
        DescriptorSlotKind::InputAttachment,
        "built-in input attachment",
    )
    .map(Some)
}

fn builtin_stage_binding_index(
    stage: BuiltinSceneShaderStage,
    binding: &BuiltinSceneDescriptorBinding,
    layout: &ScenePipelineDescriptorLayout,
    plan: &DenseHeapPlan<'_>,
    draw: &SceneGpuDrawCommand,
) -> Result<u32, String> {
    if matches!(stage, BuiltinSceneShaderStage::Vertex) {
        return builtin_vertex_binding_index(binding, plan, draw);
    }
    match binding.kind {
        BuiltinSceneDescriptorBindingKind::UniformBuffer => plan.resource_index(
            draw.material_resource_descriptor.ok_or_else(|| {
                "built-in fragment material uniform has no retained descriptor".to_owned()
            })?,
            DescriptorSlotKind::UniformBuffer,
            "built-in material uniform",
        ),
        BuiltinSceneDescriptorBindingKind::SampledImage => plan.resource_index(
            sampled_descriptor(layout, draw, builtin_texture_slot(binding.register))?,
            DescriptorSlotKind::SampledImage,
            "built-in sampled image",
        ),
        BuiltinSceneDescriptorBindingKind::Sampler => {
            let sampled_index = sampled_slot_index(layout, builtin_texture_slot(binding.register))?;
            plan.sampler_index(
                draw.sampler_descriptor_base + sampled_index,
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
    plan: &DenseHeapPlan<'_>,
    draw: &SceneGpuDrawCommand,
) -> Result<u32, String> {
    let (descriptor, kind, label) = match (binding.kind, binding.register) {
        (BuiltinSceneDescriptorBindingKind::UniformBuffer, 2) => (
            draw.resource_descriptor_base,
            DescriptorSlotKind::UniformBuffer,
            "built-in draw uniform",
        ),
        (BuiltinSceneDescriptorBindingKind::UniformBuffer, 3) => (
            draw.material_resource_descriptor.ok_or_else(|| {
                "built-in vertex material uniform has no retained descriptor".to_owned()
            })?,
            DescriptorSlotKind::UniformBuffer,
            "built-in vertex material uniform",
        ),
        (BuiltinSceneDescriptorBindingKind::StorageBuffer, 4) => (
            draw.skinning_resource_descriptor.ok_or_else(|| {
                "built-in vertex skinning buffer has no retained descriptor".to_owned()
            })?,
            DescriptorSlotKind::StorageBuffer,
            "built-in vertex skinning buffer",
        ),
        (BuiltinSceneDescriptorBindingKind::StorageBuffer, 5) => (
            draw.particle_resource_descriptor.ok_or_else(|| {
                "built-in vertex particle buffer has no retained descriptor".to_owned()
            })?,
            DescriptorSlotKind::StorageBuffer,
            "built-in vertex particle buffer",
        ),
        _ => {
            return Err(format!(
                "built-in vertex {:?} register {} has no retained descriptor source",
                binding.kind, binding.register
            ));
        }
    };
    plan.resource_index(descriptor, kind, label)
}

pub(super) fn scene_owned_pipeline_push(
    vertex: &SceneOwnedStageResourcePlan<'_>,
    fragment: &SceneOwnedStageResourcePlan<'_>,
    layout: &ScenePipelineDescriptorLayout,
    plan: &DenseHeapPlan<'_>,
    draw: &SceneGpuDrawCommand,
) -> Result<SceneDescriptorPush, String> {
    let byte_count = vertex.push_constant_bytes.max(fragment.push_constant_bytes) as usize;
    let mut bytes = vec![0; byte_count];
    let mut uniform_index = 0usize;
    for stage in [vertex, fragment] {
        let indices = scene_owned_stage_indices(stage, layout, plan, draw, &mut uniform_index)?;
        write_scene_owned_descriptor_push(stage, &indices, &mut bytes)?;
    }
    Ok(SceneDescriptorPush::SceneOwned(bytes))
}

fn scene_owned_stage_indices(
    stage: &SceneOwnedStageResourcePlan<'_>,
    layout: &ScenePipelineDescriptorLayout,
    plan: &DenseHeapPlan<'_>,
    draw: &SceneGpuDrawCommand,
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
                    plan.resource_index(
                        draw.scene_owned_uniform_descriptor_base + uniform_base + local_index,
                        DescriptorSlotKind::UniformBuffer,
                        "scene-owned uniform",
                    )?
                }
                crate::engine::scene::SceneShaderBindingKind::SampledImage => plan.resource_index(
                    sampled_descriptor(layout, draw, binding.register)?,
                    DescriptorSlotKind::SampledImage,
                    "scene-owned sampled image",
                )?,
                crate::engine::scene::SceneShaderBindingKind::Sampler => {
                    let sampled_index = sampled_slot_index(layout, binding.register)?;
                    plan.sampler_index(
                        draw.sampler_descriptor_base + sampled_index,
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

fn builtin_texture_slot(register: u32) -> u32 {
    if register == 35 { 3 } else { register }
}

fn write_push_word(
    bytes: &mut [u8],
    push_offset: u32,
    element_index: u32,
    role: &str,
) -> Result<(), String> {
    let offset = push_offset as usize;
    let byte_count = bytes.len();
    let target = bytes
        .get_mut(offset..offset + 4)
        .ok_or_else(|| format!("{role} exceeds {byte_count} push bytes"))?;
    target.copy_from_slice(&element_index.to_le_bytes());
    Ok(())
}

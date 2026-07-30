//! Typed push-data payloads for native `SPV_EXT_descriptor_heap` scene shaders.

use vulkan_renderer::descriptor_heap_element_index;

use crate::engine::scene::{SceneRenderingDeviceMeshDraw, SceneStorage};
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneDescriptorBinding, BuiltinSceneDescriptorBindingKind,
    BuiltinSceneDescriptorHeapMode, BuiltinSceneShader, BuiltinSceneVertexShader,
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
                match shader.fragment_descriptor_heap_mode {
                    BuiltinSceneDescriptorHeapMode::Mapped => None,
                    BuiltinSceneDescriptorHeapMode::Native => {
                        let push = builtin_pipeline_push(shader, vertex, layout, plan, command)?;
                        validate_native_push_size(key, &push, plan.max_push_data_size)?;
                        Some(push)
                    }
                }
            }
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
    Ok(SceneNativeDescriptorPush::EngineBuiltIn(bytes))
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
        BuiltinSceneDescriptorBindingKind::StorageImage
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
mod tests {
    use super::super::shader_program::{
        SceneOwnedDescriptorBindingPlan, SceneOwnedUniformBufferPlan,
    };
    use super::*;
    use crate::engine::scene::SceneRenderingDeviceDrawPrimitive;
    use crate::renderer::native_vulkan::{
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
        native_vulkan_vulkanalia_descriptor_heap_resource_plan,
    };

    #[test]
    fn scene_owned_graphics_push_uses_one_pipeline_global_address_space() {
        let layout = ScenePipelineDescriptorLayout {
            sampled_slots: vec![0],
            input_attachment_slots: Vec::new(),
            material_uniform_enabled: false,
            skinning_storage_enabled: false,
            scene_owned_uniform_count: 2,
        };
        let plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: descriptor_properties(),
            },
        );
        assert!(plan.backend_ready);
        let draw = draw_command();
        let vertex = owned_stage(
            crate::engine::scene::SceneShaderStage::Vertex,
            4,
            vec![owned_binding(
                crate::engine::scene::SceneShaderBindingKind::UniformBuffer,
                0,
                0,
            )],
        );
        let fragment = owned_stage(
            crate::engine::scene::SceneShaderStage::Fragment,
            16,
            vec![
                owned_binding(
                    crate::engine::scene::SceneShaderBindingKind::SampledImage,
                    0,
                    4,
                ),
                owned_binding(crate::engine::scene::SceneShaderBindingKind::Sampler, 0, 8),
                owned_binding(
                    crate::engine::scene::SceneShaderBindingKind::UniformBuffer,
                    0,
                    12,
                ),
            ],
        );

        let push = scene_owned_pipeline_push(&vertex, &fragment, &layout, &plan, &draw)
            .expect("owned push");
        let bytes = match push {
            SceneNativeDescriptorPush::SceneOwned(bytes) => bytes,
            SceneNativeDescriptorPush::EngineBuiltIn(_) => panic!("expected scene-owned push"),
        };
        assert_eq!(
            bytes
                .chunks_exact(4)
                .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
                .collect::<Vec<_>>(),
            [1, 3, 0, 2]
        );
    }

    #[test]
    fn built_in_fragment_push_uses_native_heap_indices_without_mapping_chain() {
        let layout = ScenePipelineDescriptorLayout {
            sampled_slots: vec![0],
            input_attachment_slots: Vec::new(),
            material_uniform_enabled: true,
            skinning_storage_enabled: false,
            scene_owned_uniform_count: 0,
        };
        let plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: descriptor_properties(),
            },
        );
        let shader = crate::renderer::native_vulkan::scene::native_vulkan_scene_shader_for_key(
            "gilder/dynamic-text",
        )
        .expect("dynamic-text built-in shader");
        let mut draw = draw_command();
        draw.material_resource_descriptor = Some(1);
        draw.sampled_resource_descriptor_base = 2;

        let push = builtin_pipeline_push(shader, shader.vertex, &layout, &plan, &draw)
            .expect("built-in native pipeline push");
        let bytes = match push {
            SceneNativeDescriptorPush::EngineBuiltIn(bytes) => bytes,
            SceneNativeDescriptorPush::SceneOwned(_) => panic!("expected built-in push"),
        };
        assert_eq!(
            bytes
                .chunks_exact(4)
                .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
                .collect::<Vec<_>>(),
            [2, 0, 1, 0]
        );
    }

    fn owned_stage(
        stage: crate::engine::scene::SceneShaderStage,
        push_constant_bytes: u32,
        bindings: Vec<SceneOwnedDescriptorBindingPlan>,
    ) -> SceneOwnedStageResourcePlan<'static> {
        SceneOwnedStageResourcePlan {
            stage,
            push_constant_bytes,
            bindings,
            uniform_buffers: vec![SceneOwnedUniformBufferPlan {
                name: "GlobalParams",
                register: 0,
                byte_size: 16,
                members: Vec::new(),
            }],
        }
    }

    fn owned_binding(
        kind: crate::engine::scene::SceneShaderBindingKind,
        register: u32,
        push_offset: u32,
    ) -> SceneOwnedDescriptorBindingPlan {
        SceneOwnedDescriptorBindingPlan {
            kind,
            register,
            descriptor_count: 1,
            push_offset,
        }
    }

    fn draw_command() -> SceneGpuDrawCommand {
        SceneGpuDrawCommand {
            enabled: true,
            primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            pipeline_index: 0,
            authored_pipeline_index: 0,
            disabled_pipeline_index: None,
            first_index: 0,
            index_count: 0,
            vertex_offset: 0,
            vertex_count: 3,
            instance_count: 1,
            instance_capacity: 1,
            first_instance: 0,
            dynamic_text: false,
            particle_indirect_index: None,
            resource_descriptor_base: 0,
            material_resource_descriptor: None,
            skinning_resource_descriptor: None,
            scene_owned_uniform_descriptor_base: 1,
            sampled_resource_descriptor_base: 3,
            input_attachment_resource_descriptor_base: 4,
            sampler_descriptor_base: 0,
            native_descriptor_push: None,
            skinning_byte_offset: 0,
            skinning_byte_count: 0,
            scissor: None,
        }
    }

    fn descriptor_properties() -> NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
            resource_heap_alignment: 1,
            sampler_heap_alignment: 1,
            max_resource_heap_size: 4096,
            max_sampler_heap_size: 4096,
            image_descriptor_size: 32,
            image_descriptor_alignment: 32,
            buffer_descriptor_size: 32,
            buffer_descriptor_alignment: 32,
            sampler_descriptor_size: 16,
            sampler_descriptor_alignment: 16,
            ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
        }
    }
}

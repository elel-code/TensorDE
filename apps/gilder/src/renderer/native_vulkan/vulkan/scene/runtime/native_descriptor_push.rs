//! Typed push-data payloads for native `SPV_EXT_descriptor_heap` scene shaders.

use vulkan_renderer::descriptor_heap_element_index;

use crate::engine::scene::{SceneRenderingDeviceMeshDraw, SceneStorage};
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneDescriptorHeapMode, BuiltinSceneParameterLayout,
    native_vulkan_scene_shader_for_key,
};
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
};

use super::{SceneGpuDrawCommand, ScenePipelineDescriptorLayout};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SceneAudioLineHeapPush {
    pub texture_index: u32,
    pub sampler_index: u32,
    pub material_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SceneNativeFragmentPush {
    AudioLine(SceneAudioLineHeapPush),
}

impl SceneNativeFragmentPush {
    pub(super) fn bytes(self) -> [u8; 12] {
        let words = match self {
            Self::AudioLine(push) => [
                push.texture_index,
                push.sampler_index,
                push.material_index,
            ],
        };
        let mut bytes = [0; 12];
        for (destination, word) in bytes.chunks_exact_mut(4).zip(words) {
            destination.copy_from_slice(&word.to_ne_bytes());
        }
        bytes
    }
}

pub(super) fn resolve_scene_native_fragment_pushes(
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
        let key = storage
            .string(draw.shader_key)
            .ok_or_else(|| "scene native descriptor push draw has no shader key".to_owned())?;
        let shader = native_vulkan_scene_shader_for_key(key)
            .ok_or_else(|| format!("scene shader {key:?} is not built into the catalog"))?;
        command.native_fragment_push = match shader.fragment_descriptor_heap_mode {
            BuiltinSceneDescriptorHeapMode::Mapped => None,
            BuiltinSceneDescriptorHeapMode::Native => match shader.parameter_layout {
                BuiltinSceneParameterLayout::AudioLine => Some(
                    SceneNativeFragmentPush::AudioLine(audio_line_heap_push(
                        layout, plan, command,
                    )?),
                ),
                parameter_layout => {
                    return Err(format!(
                        "native descriptor-heap scene shader {key:?} has unsupported push layout \
                         {parameter_layout:?}"
                    ));
                }
            },
        };
    }
    Ok(())
}

fn audio_line_heap_push(
    layout: &ScenePipelineDescriptorLayout,
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    draw: &SceneGpuDrawCommand,
) -> Result<SceneAudioLineHeapPush, String> {
    let sampled_index = layout
        .sampled_slots
        .iter()
        .position(|slot| *slot == 0)
        .ok_or_else(|| "native audioline shader requires sampled texture slot 0".to_owned())?;
    let resource_slice_base = descriptor_slice_base(
        &plan.resource_descriptor_offsets,
        draw.resource_descriptor_base,
        plan.resource_heap_alignment,
        "resource",
    )?;
    let sampler_slice_base = descriptor_slice_base(
        &plan.sampler_descriptor_offsets,
        draw.sampler_descriptor_base,
        plan.sampler_heap_alignment,
        "sampler",
    )?;
    let texture_descriptor = draw.sampled_resource_descriptor_base + sampled_index;
    validate_resource_kind(
        plan,
        texture_descriptor,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
        "audioline texture",
    )?;
    let material_descriptor = draw.material_resource_descriptor.ok_or_else(|| {
        "native audioline shader requires a material uniform descriptor".to_owned()
    })?;
    validate_resource_kind(
        plan,
        material_descriptor,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
        "audioline material",
    )?;
    let sampler_descriptor = draw.sampler_descriptor_base + sampled_index;

    Ok(SceneAudioLineHeapPush {
        texture_index: relative_element_index(
            &plan.resource_descriptor_offsets,
            texture_descriptor,
            resource_slice_base,
            plan.image_descriptor_size,
            "audioline texture",
        )?,
        sampler_index: relative_element_index(
            &plan.sampler_descriptor_offsets,
            sampler_descriptor,
            sampler_slice_base,
            plan.sampler_descriptor_size,
            "audioline sampler",
        )?,
        material_index: relative_element_index(
            &plan.resource_descriptor_offsets,
            material_descriptor,
            resource_slice_base,
            plan.buffer_descriptor_size,
            "audioline material",
        )?,
    })
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
    use super::*;
    use crate::engine::scene::SceneRenderingDeviceDrawPrimitive;
    use crate::renderer::native_vulkan::{
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
        native_vulkan_vulkanalia_descriptor_heap_resource_plan,
    };

    #[test]
    fn audioline_indices_are_relative_to_aligned_bound_heap_slices() {
        let layout = ScenePipelineDescriptorLayout {
            sampled_slots: vec![0],
            input_attachment_slots: Vec::new(),
            material_uniform_enabled: true,
            skinning_storage_enabled: false,
        };
        let kinds = [
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
        ]
        .repeat(2);
        let plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: kinds,
                sampler_count: 2,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 128,
                    sampler_heap_alignment: 64,
                    max_resource_heap_size: 4096,
                    max_sampler_heap_size: 4096,
                    image_descriptor_size: 32,
                    image_descriptor_alignment: 32,
                    buffer_descriptor_size: 32,
                    buffer_descriptor_alignment: 32,
                    sampler_descriptor_size: 16,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );
        assert!(plan.backend_ready);
        let draw = |base, sampler_base| SceneGpuDrawCommand {
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
            resource_descriptor_base: base,
            material_resource_descriptor: Some(base + 1),
            skinning_resource_descriptor: None,
            sampled_resource_descriptor_base: base + 2,
            input_attachment_resource_descriptor_base: base + 3,
            sampler_descriptor_base: sampler_base,
            native_fragment_push: None,
            skinning_byte_offset: 0,
            skinning_byte_count: 0,
            scissor: None,
        };

        assert_eq!(
            audio_line_heap_push(&layout, &plan, &draw(0, 0)).unwrap(),
            SceneAudioLineHeapPush {
                texture_index: 2,
                sampler_index: 0,
                material_index: 1,
            }
        );
        assert_eq!(
            audio_line_heap_push(&layout, &plan, &draw(3, 1)).unwrap(),
            SceneAudioLineHeapPush {
                texture_index: 5,
                sampler_index: 1,
                material_index: 4,
            }
        );
    }
}

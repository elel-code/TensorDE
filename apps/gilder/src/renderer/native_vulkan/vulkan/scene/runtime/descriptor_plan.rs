//! Retained descriptor arena slices for each scene draw.

use crate::engine::scene::{
    SceneParticleGpuEmitterPlan, SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceMeshDraw,
    SceneStorage,
};
use crate::renderer::native_vulkan::{
    NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
};

use super::{SceneGpuDrawCommand, ScenePipelineDescriptorLayout};

pub(super) fn scene_descriptor_plan_inputs(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    particle_emitters: &[SceneParticleGpuEmitterPlan],
    layout: &ScenePipelineDescriptorLayout,
    pipeline_indices: &[u32],
    disabled_pipeline_indices: &[Option<u32>],
) -> (
    Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    Vec<SceneGpuDrawCommand>,
) {
    let per_draw_resource_count = layout.per_draw_resource_count();
    let mut resources = Vec::with_capacity(draws.len().saturating_mul(per_draw_resource_count));
    let mut commands = Vec::with_capacity(draws.len());
    for (index, draw) in draws.iter().enumerate() {
        let base = index * per_draw_resource_count;
        resources.push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer);
        if layout.material_uniform_enabled {
            resources
                .push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer);
        }
        let (skinning_byte_offset, skinning_byte_count) = if layout.skinning_storage_enabled {
            resources
                .push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer);
            scene_draw_skinning_range(draw)
        } else {
            (0, 0)
        };
        resources
            .extend((0..layout.scene_owned_uniform_count).map(|_| {
                NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer
            }));
        resources.extend(
            layout
                .sampled_slots
                .iter()
                .map(|_| NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage),
        );
        resources.extend(
            layout.input_attachment_slots.iter().map(|_| {
                NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::InputAttachment
            }),
        );
        commands.push(SceneGpuDrawCommand {
            enabled: true,
            primitive: draw.primitive,
            pipeline_index: pipeline_indices.get(index).copied().unwrap_or(0),
            authored_pipeline_index: pipeline_indices.get(index).copied().unwrap_or(0),
            disabled_pipeline_index: disabled_pipeline_indices.get(index).copied().flatten(),
            first_index: draw.index_start,
            index_count: draw.index_count,
            vertex_offset: draw.vertex_start as i32,
            vertex_count: draw.vertex_count,
            instance_count: draw.instance_count,
            instance_capacity: draw.instance_count,
            first_instance: storage
                .dynamic_texts()
                .iter()
                .take_while(|text| text.object != draw.object)
                .map(|text| text.max_glyph_count)
                .sum(),
            dynamic_text: storage.dynamic_text_for_object(draw.object).is_some()
                && storage
                    .string(draw.shader_key)
                    .is_some_and(|key| key == "gilder/dynamic-text"),
            particle_indirect_index: particle_emitters
                .iter()
                .find(|emitter| {
                    draw.primitive == SceneRenderingDeviceDrawPrimitive::ParticleBillboard
                        && emitter.object == draw.object
                })
                .map(|emitter| emitter.indirect_draw_index),
            resource_descriptor_base: base,
            material_resource_descriptor: layout
                .material_resource_offset()
                .map(|offset| base + offset),
            skinning_resource_descriptor: layout
                .skinning_resource_offset()
                .map(|offset| base + offset),
            scene_owned_uniform_descriptor_base: base
                + layout.scene_owned_uniform_resource_offset(),
            sampled_resource_descriptor_base: base + layout.sampled_resource_offset(),
            input_attachment_resource_descriptor_base: base
                + layout.input_attachment_resource_offset(),
            sampler_descriptor_base: index * layout.sampler_count_per_draw(),
            native_descriptor_push: None,
            disabled_native_descriptor_push: None,
            skinning_byte_offset,
            skinning_byte_count,
            scissor: None,
        });
    }
    (resources, commands)
}

fn scene_draw_skinning_range(draw: &SceneRenderingDeviceMeshDraw) -> (u64, u64) {
    if draw.skinning_palette_count == 0 {
        return (
            0,
            NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64,
        );
    }
    (
        draw.skinning_palette_start.saturating_add(1) as u64
            * NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64,
        draw.skinning_palette_count as u64
            * NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64,
    )
}

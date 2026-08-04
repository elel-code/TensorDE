//! Retained descriptor arena slices for each scene draw.

use crate::engine::scene::{
    SceneParticleGpuEmitterPlan, SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceMeshDraw,
    SceneStorage,
};
use crate::renderer::rendering_device::RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES;
use vulkan_renderer::DescriptorSlotKind;

use super::{SceneGpuDrawCommand, ScenePipelineDescriptorLayout};

pub(super) fn scene_descriptor_plan_inputs(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    particle_emitters: &[SceneParticleGpuEmitterPlan],
    layout: &ScenePipelineDescriptorLayout,
    pipeline_indices: &[u32],
    disabled_pipeline_indices: &[Option<u32>],
) -> (Vec<DescriptorSlotKind>, Vec<SceneGpuDrawCommand>) {
    let per_draw_resource_count = layout.per_draw_resource_count();
    let mut resources = Vec::with_capacity(draws.len().saturating_mul(per_draw_resource_count));
    let mut commands = Vec::with_capacity(draws.len());
    let fullscreen_utility_count = draws
        .iter()
        .filter(|draw| draw.primitive == SceneRenderingDeviceDrawPrimitive::FullscreenTriangle)
        .count();
    let scene_owned_utility_quad_count = draws
        .iter()
        .filter(|draw| {
            draw.primitive == SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad
                && storage
                    .shader_program(
                        draw.shader_key,
                        crate::engine::scene::SceneShaderStage::Vertex,
                    )
                    .is_some()
        })
        .count();
    let mut fullscreen_utility_index = 0usize;
    let mut scene_owned_utility_quad_index = 0usize;
    let mut object_composite_vertex_start = 0usize;
    for (index, draw) in draws.iter().enumerate() {
        let base = index * per_draw_resource_count;
        resources.push(super::SLANG_CONSTANT_BUFFER_DESCRIPTOR_KIND);
        if layout.material_uniform_enabled {
            resources.push(super::SLANG_CONSTANT_BUFFER_DESCRIPTOR_KIND);
        }
        let (skinning_byte_offset, skinning_byte_count) = if layout.skinning_storage_enabled {
            resources.push(DescriptorSlotKind::StorageBuffer);
            scene_draw_skinning_range(draw)
        } else {
            (0, 0)
        };
        if layout.particle_storage_enabled {
            resources.push(DescriptorSlotKind::StorageBuffer);
        }
        resources.extend(
            (0..layout.scene_owned_uniform_count)
                .map(|_| super::SLANG_CONSTANT_BUFFER_DESCRIPTOR_KIND),
        );
        resources.extend(
            layout
                .sampled_slots
                .iter()
                .map(|_| DescriptorSlotKind::SampledImage),
        );
        resources.extend(
            layout
                .input_attachment_slots
                .iter()
                .map(|_| DescriptorSlotKind::InputAttachment),
        );
        let vertex_buffer_byte_offset = scene_draw_vertex_buffer_byte_offset(
            storage,
            draw,
            fullscreen_utility_count,
            fullscreen_utility_index,
            scene_owned_utility_quad_count,
            scene_owned_utility_quad_index,
            object_composite_vertex_start,
        );
        if draw.primitive == SceneRenderingDeviceDrawPrimitive::FullscreenTriangle {
            fullscreen_utility_index += 1;
        }
        if draw.primitive == SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad
            && storage
                .shader_program(
                    draw.shader_key,
                    crate::engine::scene::SceneShaderStage::Vertex,
                )
                .is_some()
        {
            scene_owned_utility_quad_index += 1;
        }
        if draw.uv_inset_texels > 0.0 {
            object_composite_vertex_start =
                object_composite_vertex_start.saturating_add(draw.vertex_count as usize);
        }
        commands.push(SceneGpuDrawCommand {
            enabled: true,
            primitive: draw.primitive,
            pipeline_index: pipeline_indices.get(index).copied().unwrap_or(0),
            authored_pipeline_index: pipeline_indices.get(index).copied().unwrap_or(0),
            disabled_pipeline_index: disabled_pipeline_indices.get(index).copied().flatten(),
            first_index: draw.index_start,
            index_count: draw.index_count,
            vertex_offset: if draw.uv_inset_texels > 0.0 {
                0
            } else {
                draw.vertex_start as i32
            },
            vertex_buffer_byte_offset,
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
                    .is_some_and(|key| key == "tensor-wallpaper/dynamic-text"),
            video_media_instance: None,
            video_vertex_byte_offset: None,
            particle_indirect_index: particle_emitters
                .iter()
                .find(|emitter| {
                    draw.primitive == SceneRenderingDeviceDrawPrimitive::ParticleBillboard
                        && emitter.particle_index == draw.particle_index
                })
                .map(|emitter| emitter.indirect_draw_index),
            resource_descriptor_base: base,
            material_resource_descriptor: layout
                .material_resource_offset()
                .map(|offset| base + offset),
            skinning_resource_descriptor: layout
                .skinning_resource_offset()
                .map(|offset| base + offset),
            particle_resource_descriptor: layout
                .particle_resource_offset()
                .map(|offset| base + offset),
            scene_owned_uniform_descriptor_base: base
                + layout.scene_owned_uniform_resource_offset(),
            sampled_resource_descriptor_base: base + layout.sampled_resource_offset(),
            input_attachment_resource_descriptor_base: base
                + layout.input_attachment_resource_offset(),
            sampler_descriptor_base: index * layout.sampler_count_per_draw(),
            descriptor_push: None,
            disabled_descriptor_push: None,
            skinning_byte_offset,
            skinning_byte_count,
            scissor: None,
        });
    }
    (resources, commands)
}

fn scene_draw_vertex_buffer_byte_offset(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    fullscreen_utility_count: usize,
    fullscreen_utility_index: usize,
    scene_owned_utility_quad_count: usize,
    scene_owned_utility_quad_index: usize,
    object_composite_vertex_start: usize,
) -> Option<u64> {
    if draw.uv_inset_texels > 0.0 {
        return Some(object_composite_vertex_buffer_byte_offset(
            storage.document().mesh_vertices.len(),
            fullscreen_utility_count,
            scene_owned_utility_quad_count,
            object_composite_vertex_start,
        ));
    }
    let scene_owned_vertex = storage
        .shader_program(
            draw.shader_key,
            crate::engine::scene::SceneShaderStage::Vertex,
        )
        .is_some();
    vertex_buffer_byte_offset(
        storage.document().mesh_vertices.len(),
        fullscreen_utility_count,
        fullscreen_utility_index,
        scene_owned_utility_quad_index,
        draw.primitive,
        scene_owned_vertex,
    )
}

pub(super) fn object_composite_vertex_buffer_byte_offset(
    mesh_vertex_count: usize,
    fullscreen_utility_count: usize,
    scene_owned_utility_quad_count: usize,
    object_composite_vertex_start: usize,
) -> u64 {
    mesh_vertex_count
        .saturating_add(fullscreen_utility_count.saturating_mul(3))
        .saturating_add(scene_owned_utility_quad_count.saturating_mul(6))
        .saturating_add(object_composite_vertex_start) as u64
        * u64::from(super::SCENE_MESH_VERTEX_STRIDE_BYTES)
}

pub(super) fn vertex_buffer_byte_offset(
    mesh_vertex_count: usize,
    fullscreen_utility_count: usize,
    fullscreen_utility_index: usize,
    scene_owned_utility_quad_index: usize,
    primitive: SceneRenderingDeviceDrawPrimitive,
    scene_owned_vertex: bool,
) -> Option<u64> {
    match primitive {
        SceneRenderingDeviceDrawPrimitive::ObjectMesh => Some(0),
        SceneRenderingDeviceDrawPrimitive::FullscreenTriangle if scene_owned_vertex => Some(
            mesh_vertex_count.saturating_add(fullscreen_utility_index.saturating_mul(3)) as u64
                * u64::from(super::SCENE_MESH_VERTEX_STRIDE_BYTES),
        ),
        SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad if scene_owned_vertex => Some(
            mesh_vertex_count
                .saturating_add(fullscreen_utility_count.saturating_mul(3))
                .saturating_add(scene_owned_utility_quad_index.saturating_mul(6))
                as u64
                * u64::from(super::SCENE_MESH_VERTEX_STRIDE_BYTES),
        ),
        SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
        | SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad
        | SceneRenderingDeviceDrawPrimitive::ParticleBillboard => None,
    }
}

fn scene_draw_skinning_range(draw: &SceneRenderingDeviceMeshDraw) -> (u64, u64) {
    if draw.skinning_palette_count == 0 {
        return (
            0,
            RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64,
        );
    }
    (
        draw.skinning_palette_start.saturating_add(1) as u64
            * RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64,
        draw.skinning_palette_count as u64
            * RENDERING_DEVICE_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES as u64,
    )
}

//! Fullscreen utility primitive payloads for scene effect passes.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/effect-format.md`

use crate::engine::scene::SceneRenderingDeviceGraphPlan;

pub(in crate::renderer::native_vulkan) fn graph_uses_fullscreen_utility_primitive(
    graph: &SceneRenderingDeviceGraphPlan,
) -> bool {
    graph.uses_fullscreen_utility_primitive()
}

pub(in crate::renderer::native_vulkan) fn append_fullscreen_triangle_vertices(
    payload: &mut Vec<u8>,
) {
    for (x, y, u, v) in [
        (-1.0f32, -1.0f32, 0.0f32, 0.0f32),
        (3.0f32, -1.0f32, 2.0f32, 0.0f32),
        (-1.0f32, 3.0f32, 0.0f32, 2.0f32),
    ] {
        for value in [x, y, u, v, 1.0] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        payload.extend_from_slice(&[0; 32]);
    }
}

pub(in crate::renderer::native_vulkan) fn append_fullscreen_triangle_indices(
    payload: &mut Vec<u8>,
) {
    for index in [0u32, 1, 2] {
        payload.extend_from_slice(&index.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceMeshDraw};

    #[test]
    fn fullscreen_triangle_payload_matches_three_vertex_copyback_primitive() {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        append_fullscreen_triangle_vertices(&mut vertices);
        append_fullscreen_triangle_indices(&mut indices);

        assert_eq!(vertices.len(), 3 * 52);
        assert_eq!(indices.len(), 3 * 4);
        assert_eq!(f32_at(&vertices, 0), -1.0);
        assert_eq!(f32_at(&vertices, 52), 3.0);
        assert_eq!(f32_at(&vertices, 60), 2.0);
        assert_eq!(f32_at(&vertices, 108), 3.0);
        assert_eq!(f32_at(&vertices, 112), 0.0);
        assert_eq!(f32_at(&vertices, 116), 2.0);
        assert_eq!(u32_at(&vertices, 20), 0);
        assert_eq!(u32_at(&indices, 8), 2);
    }

    #[test]
    fn graph_detects_fullscreen_utility_draws() {
        let graph = SceneRenderingDeviceGraphPlan {
            mesh_draws: vec![SceneRenderingDeviceMeshDraw {
                primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
                shader_key: crate::engine::scene::SceneStringId::NONE,
                mesh_index: crate::engine::scene::INVALID_OBJECT_ID,
                resolved_object_index: crate::engine::scene::INVALID_OBJECT_ID,
                clip_transform: [[0.0; 4]; 4],
                authored_source_extent: [0.0; 2],
                skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
                skinning_palette_count: 0,
                resolved_color: crate::engine::scene::SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                resolved_alpha: 1.0,
                apply_resolved_visual: true,
                effect_batch_atlas_tile: u32::MAX,
                effect_batch_atlas_grid: [0; 2],
                object: crate::engine::scene::SceneObjectHandle(
                    crate::engine::scene::INVALID_OBJECT_ID,
                ),
                material: crate::engine::scene::SceneMaterialHandle(
                    crate::engine::scene::INVALID_MATERIAL_ID,
                ),
                vertex_start: 0,
                vertex_count: 3,
                index_start: 0,
                index_count: 3,
                instance_count: 1,
            }],
            target_allocations: Vec::new(),
            effect_batches: Vec::new(),
            effect_batch_instances: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            pass_nodes: Vec::new(),
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: Vec::new(),
            resolved_object_count: 0,
            resolved_visible_object_count: 0,
            resolved_attachment_link_count: 0,
            resolved_visible_effect_instance_count: 0,
            resolved_visible_effect_pass_count: 0,
            resolved_visible_effect_fbo_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 0,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 0,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        };

        assert!(graph_uses_fullscreen_utility_primitive(&graph));
    }

    fn f32_at(payload: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }

    fn u32_at(payload: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }
}

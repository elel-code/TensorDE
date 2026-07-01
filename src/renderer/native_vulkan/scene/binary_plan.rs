use crate::core::scene::binary::{
    SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE, SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
    SCENE_BINARY_NONE_ID, SceneBinaryError, SceneBinaryLayoutPlan, decode_scene_binary_container,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryPlan {
    pub(in crate::renderer::native_vulkan::scene) feature_flags: u32,
    pub(in crate::renderer::native_vulkan::scene) resource_count: u32,
    pub(in crate::renderer::native_vulkan::scene) node_count: u32,
    pub(in crate::renderer::native_vulkan::scene) draw_record_count: u32,
    pub(in crate::renderer::native_vulkan::scene) geometry_record_count: u32,
    pub(in crate::renderer::native_vulkan::scene) generated_vertex_count: u32,
    pub(in crate::renderer::native_vulkan::scene) generated_index_count: u32,
    pub(in crate::renderer::native_vulkan::scene) mesh_vertex_count: u32,
    pub(in crate::renderer::native_vulkan::scene) mesh_index_count: u32,
    pub(in crate::renderer::native_vulkan::scene) mesh_vertex_stream_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) mesh_index_stream_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) texture_slot_count: u32,
    pub(in crate::renderer::native_vulkan::scene) material_pass_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_pass_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_parameter_count: u32,
    pub(in crate::renderer::native_vulkan::scene) flutter_state_count: u32,
    pub(in crate::renderer::native_vulkan::scene) puppet_count: u32,
    pub(in crate::renderer::native_vulkan::scene) retained_gpu_state_count: u32,
    pub(in crate::renderer::native_vulkan::scene) draw_records:
        Vec<NativeVulkanSceneBinaryDrawRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryDrawRecord {
    pub(in crate::renderer::native_vulkan::scene) node_name: u32,
    pub(in crate::renderer::native_vulkan::scene) geometry_index: u32,
    pub(in crate::renderer::native_vulkan::scene) material_index: u32,
    pub(in crate::renderer::native_vulkan::scene) primitive_kind: u16,
    pub(in crate::renderer::native_vulkan::scene) vertex_layout: u16,
    pub(in crate::renderer::native_vulkan::scene) first_vertex: u32,
    pub(in crate::renderer::native_vulkan::scene) vertex_count: u32,
    pub(in crate::renderer::native_vulkan::scene) first_index: u32,
    pub(in crate::renderer::native_vulkan::scene) index_count: u32,
    pub(in crate::renderer::native_vulkan::scene) material_texture_slot_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_pass_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_texture_slot_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_parameter_count: u32,
    pub(in crate::renderer::native_vulkan::scene) descriptor_layout: u16,
    pub(in crate::renderer::native_vulkan::scene) blend_mode: u16,
    pub(in crate::renderer::native_vulkan::scene) effect_kind_flags: u32,
}

pub(in crate::renderer::native_vulkan::scene) fn native_vulkan_scene_binary_plan_from_container(
    container: &[u8],
) -> Result<NativeVulkanSceneBinaryPlan, SceneBinaryError> {
    let layout = decode_scene_binary_container(container)?;
    native_vulkan_scene_binary_plan_from_layout(container, &layout)
}

fn native_vulkan_scene_binary_plan_from_layout(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<NativeVulkanSceneBinaryPlan, SceneBinaryError> {
    let resource_count = record_len(layout.resource_records(container)?);
    let node_count = record_len(layout.node_records(container)?);
    let geometry_record_count = record_len(layout.geometry_records(container)?);
    let texture_slot_count = record_len(layout.texture_slot_records(container)?);
    let material_pass_count = record_len(layout.material_pass_records(container)?);
    let effect_pass_count = record_len(layout.effect_pass_records(container)?);
    let effect_parameter_count = record_len(layout.effect_parameter_records(container)?);
    let flutter_state_count = record_len(layout.flutter_state_records(container)?);
    let puppet_count = record_len(layout.puppet_records(container)?);
    let retained_gpu_state_count = record_len(layout.retained_gpu_state_records(container)?);

    let mut draw_records = Vec::new();
    let mut generated_vertex_count = 0u32;
    let mut generated_index_count = 0u32;
    let mut mesh_vertex_count = 0u32;
    let mut mesh_index_count = 0u32;
    for node in layout.node_records(container)? {
        let node = node?;
        if node.geometry_index == SCENE_BINARY_NONE_ID {
            continue;
        }
        let geometry = layout.geometry_record_at(container, node.geometry_index)?;
        let material = if node.material_index == SCENE_BINARY_NONE_ID {
            None
        } else {
            Some(layout.material_pass_record_at(container, node.material_index)?)
        };

        if geometry.first_vertex == SCENE_BINARY_NONE_ID {
            generated_vertex_count = generated_vertex_count.saturating_add(geometry.vertex_count);
            generated_index_count = generated_index_count.saturating_add(geometry.index_count);
        } else {
            let vertex_count =
                record_len(layout.geometry_vertex_record_range(container, geometry)?);
            let index_count = record_len(layout.geometry_index_record_range(container, geometry)?);
            mesh_vertex_count = mesh_vertex_count.saturating_add(vertex_count);
            mesh_index_count = mesh_index_count.saturating_add(index_count);
        }

        let mut material_texture_slot_count = 0u32;
        let mut material_effect_pass_count = 0u32;
        let mut effect_texture_slot_count = 0u32;
        let mut draw_effect_parameter_count = 0u32;
        let mut descriptor_layout = 0u16;
        let mut blend_mode = 0u16;
        let mut effect_kind_flags = 0u32;
        if let Some(material) = material {
            material_texture_slot_count =
                record_len(layout.material_texture_slot_records(container, material)?);
            descriptor_layout = material.descriptor_layout;
            blend_mode = material.blend_mode;
            effect_kind_flags = material.effect_kind_flags;
            for effect_pass in layout.material_effect_pass_records(container, material)? {
                let effect_pass = effect_pass?;
                material_effect_pass_count = material_effect_pass_count.saturating_add(1);
                effect_texture_slot_count = effect_texture_slot_count.saturating_add(record_len(
                    layout.effect_texture_slot_records(container, effect_pass)?,
                ));
                draw_effect_parameter_count = draw_effect_parameter_count.saturating_add(
                    record_len(layout.effect_parameter_record_range(container, effect_pass)?),
                );
            }
        }

        draw_records.push(NativeVulkanSceneBinaryDrawRecord {
            node_name: node.id_name,
            geometry_index: node.geometry_index,
            material_index: node.material_index,
            primitive_kind: geometry.primitive_kind,
            vertex_layout: geometry.vertex_layout,
            first_vertex: geometry.first_vertex,
            vertex_count: geometry.vertex_count,
            first_index: geometry.first_index,
            index_count: geometry.index_count,
            material_texture_slot_count,
            effect_pass_count: material_effect_pass_count,
            effect_texture_slot_count,
            effect_parameter_count: draw_effect_parameter_count,
            descriptor_layout,
            blend_mode,
            effect_kind_flags,
        });
    }

    for flutter in layout.flutter_state_records(container)? {
        let flutter = flutter?;
        let _ = layout.flutter_parameter_records(container, flutter)?;
    }

    Ok(NativeVulkanSceneBinaryPlan {
        feature_flags: layout.feature_flags,
        resource_count,
        node_count,
        draw_record_count: record_len_from_usize(draw_records.len()),
        geometry_record_count,
        generated_vertex_count,
        generated_index_count,
        mesh_vertex_count,
        mesh_index_count,
        mesh_vertex_stream_bytes: u64::from(mesh_vertex_count)
            .saturating_mul(SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE as u64),
        mesh_index_stream_bytes: u64::from(mesh_index_count)
            .saturating_mul(SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE as u64),
        texture_slot_count,
        material_pass_count,
        effect_pass_count,
        effect_parameter_count,
        flutter_state_count,
        puppet_count,
        retained_gpu_state_count,
        draw_records,
    })
}

fn record_len<T>(records: impl ExactSizeIterator<Item = Result<T, SceneBinaryError>>) -> u32 {
    record_len_from_usize(records.len())
}

fn record_len_from_usize(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::core::scene::SceneDocument;
    use crate::core::scene::binary::{
        SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH, SCENE_BINARY_GEOMETRY_PRIMITIVE_QUAD,
        SCENE_BINARY_GEOMETRY_QUAD_INDEX_COUNT, SCENE_BINARY_GEOMETRY_QUAD_VERTEX_COUNT,
        SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_GENERATED,
        SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY,
        scene_binary_payloads_from_document,
    };

    #[test]
    fn binary_plan_reads_generated_geometry_material_and_effect_ranges() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "base", "type": "image", "source": "assets/base.gtex", "width": 128, "height": 64 },
                { "id": "mask", "type": "image", "source": "assets/mask.gtex", "width": 128, "height": 64 }
            ],
            "nodes": [
                {
                    "id": "layer",
                    "type": "image",
                    "resource": "base",
                    "width": 128,
                    "height": 64,
                    "effects": [
                        {
                            "file": "effects/opacity/effect.json",
                            "passes": [
                                {
                                    "shader": "effects/opacity",
                                    "texture_resources": ["base", "mask"],
                                    "constant_shader_values": { "speed": 2.0 },
                                    "combos": { "MASK": 1 }
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        let bytes = scene_binary_payloads_from_document(&document)
            .encode_container(0x40)
            .expect("binary scene");

        let plan = native_vulkan_scene_binary_plan_from_container(&bytes).expect("binary plan");

        assert_eq!(plan.feature_flags, 0x40);
        assert_eq!(plan.resource_count, 2);
        assert_eq!(plan.node_count, 1);
        assert_eq!(plan.draw_record_count, 1);
        assert_eq!(
            plan.generated_vertex_count,
            SCENE_BINARY_GEOMETRY_QUAD_VERTEX_COUNT
        );
        assert_eq!(
            plan.generated_index_count,
            SCENE_BINARY_GEOMETRY_QUAD_INDEX_COUNT
        );
        assert_eq!(plan.mesh_vertex_count, 0);
        assert_eq!(plan.mesh_index_count, 0);
        assert_eq!(plan.material_pass_count, 1);
        assert_eq!(plan.effect_pass_count, 1);
        assert_eq!(plan.effect_parameter_count, 2);
        assert_eq!(plan.texture_slot_count, 2);
        assert_eq!(
            plan.retained_gpu_state_count,
            plan.resource_count
                + plan.geometry_record_count
                + plan.texture_slot_count
                + plan.material_pass_count
                + plan.effect_pass_count
                + plan.effect_parameter_count
        );
        assert_eq!(
            plan.draw_records[0].primitive_kind,
            SCENE_BINARY_GEOMETRY_PRIMITIVE_QUAD
        );
        assert_eq!(
            plan.draw_records[0].vertex_layout,
            SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_GENERATED
        );
        assert_eq!(plan.draw_records[0].material_texture_slot_count, 2);
        assert_eq!(plan.draw_records[0].effect_pass_count, 1);
        assert_eq!(plan.draw_records[0].effect_texture_slot_count, 2);
        assert_eq!(plan.draw_records[0].effect_parameter_count, 2);
    }

    #[test]
    fn binary_plan_reads_mesh_stream_ranges_without_copying_json_payloads() {
        let document: SceneDocument = serde_json::from_value(json!({
            "nodes": [
                {
                    "id": "mesh-node",
                    "type": "image",
                    "mesh": {
                        "vertices": [
                            { "x": -2.0, "y": 1.0, "u": 0.25, "v": 0.75, "opacity": 0.5 },
                            { "x": 4.0, "y": -3.0, "u": 1.0, "v": 0.0 },
                            { "x": 2.0, "y": 5.0, "u": 0.0, "v": 1.0 }
                        ],
                        "indices": [2, 1, 0]
                    }
                }
            ]
        }))
        .expect("scene document");
        let bytes = scene_binary_payloads_from_document(&document)
            .encode_container(0)
            .expect("binary scene");

        let plan = native_vulkan_scene_binary_plan_from_container(&bytes).expect("binary plan");

        assert_eq!(plan.draw_record_count, 1);
        assert_eq!(plan.generated_vertex_count, 0);
        assert_eq!(plan.generated_index_count, 0);
        assert_eq!(plan.mesh_vertex_count, 3);
        assert_eq!(plan.mesh_index_count, 3);
        assert_eq!(
            plan.mesh_vertex_stream_bytes,
            3 * SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE as u64
        );
        assert_eq!(
            plan.mesh_index_stream_bytes,
            3 * SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE as u64
        );
        assert_eq!(
            plan.draw_records[0].primitive_kind,
            SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH
        );
        assert_eq!(
            plan.draw_records[0].vertex_layout,
            SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY
        );
        assert_eq!(plan.draw_records[0].vertex_count, 3);
        assert_eq!(plan.draw_records[0].index_count, 3);
    }
}

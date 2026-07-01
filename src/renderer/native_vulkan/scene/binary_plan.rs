use crate::core::scene::binary::{
    SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE, SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
    SCENE_BINARY_NONE_ID, SceneBinaryError, SceneBinaryLayoutPlan, decode_scene_binary_container,
};

mod flutter;
mod geometry;
mod material;
mod node;
mod resource;
mod retained;

pub(in crate::renderer::native_vulkan::scene) use self::flutter::NativeVulkanSceneBinaryFlutterRecord;
use self::flutter::native_vulkan_scene_binary_flutter_records;
pub(in crate::renderer::native_vulkan::scene) use self::geometry::NativeVulkanSceneBinaryGeometryRecord;
use self::geometry::native_vulkan_scene_binary_geometry_records;
use self::material::native_vulkan_scene_binary_material_records;
pub(in crate::renderer::native_vulkan::scene) use self::material::{
    NativeVulkanSceneBinaryEffectRecord, NativeVulkanSceneBinaryMaterialRecord,
    NativeVulkanSceneBinaryTextureSlotRecord,
};
pub(in crate::renderer::native_vulkan::scene) use self::node::NativeVulkanSceneBinaryNodeRecord;
use self::node::native_vulkan_scene_binary_node_records;
pub(in crate::renderer::native_vulkan::scene) use self::resource::NativeVulkanSceneBinaryResourceRecord;
use self::resource::native_vulkan_scene_binary_resource_records;
pub(in crate::renderer::native_vulkan::scene) use self::retained::NativeVulkanSceneBinaryRetainedGpuRecord;
use self::retained::{
    native_vulkan_scene_binary_retained_dirty_range_count,
    native_vulkan_scene_binary_retained_gpu_records,
};

#[derive(Debug, Clone, PartialEq)]
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
    pub(in crate::renderer::native_vulkan::scene) retained_dirty_range_count: u32,
    pub(in crate::renderer::native_vulkan::scene) resource_records:
        Vec<NativeVulkanSceneBinaryResourceRecord>,
    pub(in crate::renderer::native_vulkan::scene) node_records:
        Vec<NativeVulkanSceneBinaryNodeRecord>,
    pub(in crate::renderer::native_vulkan::scene) geometry_records:
        Vec<NativeVulkanSceneBinaryGeometryRecord>,
    pub(in crate::renderer::native_vulkan::scene) draw_records:
        Vec<NativeVulkanSceneBinaryDrawRecord>,
    pub(in crate::renderer::native_vulkan::scene) texture_slots:
        Vec<NativeVulkanSceneBinaryTextureSlotRecord>,
    pub(in crate::renderer::native_vulkan::scene) material_records:
        Vec<NativeVulkanSceneBinaryMaterialRecord>,
    pub(in crate::renderer::native_vulkan::scene) effect_records:
        Vec<NativeVulkanSceneBinaryEffectRecord>,
    pub(in crate::renderer::native_vulkan::scene) retained_records:
        Vec<NativeVulkanSceneBinaryRetainedGpuRecord>,
    pub(in crate::renderer::native_vulkan::scene) flutter_records:
        Vec<NativeVulkanSceneBinaryFlutterRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryDrawRecord {
    pub(in crate::renderer::native_vulkan::scene) node_index: u32,
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
    let resource_records = native_vulkan_scene_binary_resource_records(container, layout)?;
    let resource_count = record_len_from_usize(resource_records.len());
    let node_records = native_vulkan_scene_binary_node_records(container, layout)?;
    let node_count = record_len_from_usize(node_records.len());
    let geometry_records = native_vulkan_scene_binary_geometry_records(container, layout)?;
    let geometry_record_count = record_len_from_usize(geometry_records.records.len());
    let texture_slot_count = record_len(layout.texture_slot_records(container)?);
    let material_pass_count = record_len(layout.material_pass_records(container)?);
    let effect_pass_count = record_len(layout.effect_pass_records(container)?);
    let effect_parameter_count = record_len(layout.effect_parameter_records(container)?);
    let puppet_count = record_len(layout.puppet_records(container)?);
    let material_records = native_vulkan_scene_binary_material_records(container, layout)?;
    let retained_records = native_vulkan_scene_binary_retained_gpu_records(container, layout)?;
    let retained_gpu_state_count = record_len_from_usize(retained_records.len());
    let retained_dirty_range_count =
        native_vulkan_scene_binary_retained_dirty_range_count(&retained_records);
    let flutter_records = native_vulkan_scene_binary_flutter_records(container, layout)?;
    let flutter_state_count = record_len_from_usize(flutter_records.len());

    let mut draw_records = Vec::with_capacity(node_records.len());
    for (node_index, node) in node_records.iter().enumerate() {
        if node.geometry_index == SCENE_BINARY_NONE_ID {
            continue;
        }
        let _ = layout.geometry_record_at(container, node.geometry_index)?;
        if node.material_index != SCENE_BINARY_NONE_ID {
            let _ = layout.material_pass_record_at(container, node.material_index)?;
        }

        draw_records.push(NativeVulkanSceneBinaryDrawRecord {
            node_index: record_len_from_usize(node_index),
        });
    }

    Ok(NativeVulkanSceneBinaryPlan {
        feature_flags: layout.feature_flags,
        resource_count,
        node_count,
        draw_record_count: record_len_from_usize(draw_records.len()),
        geometry_record_count,
        generated_vertex_count: geometry_records.generated_vertex_count,
        generated_index_count: geometry_records.generated_index_count,
        mesh_vertex_count: geometry_records.mesh_vertex_count,
        mesh_index_count: geometry_records.mesh_index_count,
        mesh_vertex_stream_bytes: u64::from(geometry_records.mesh_vertex_count)
            .saturating_mul(SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE as u64),
        mesh_index_stream_bytes: u64::from(geometry_records.mesh_index_count)
            .saturating_mul(SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE as u64),
        texture_slot_count,
        material_pass_count,
        effect_pass_count,
        effect_parameter_count,
        flutter_state_count,
        puppet_count,
        retained_gpu_state_count,
        retained_dirty_range_count,
        resource_records,
        node_records,
        geometry_records: geometry_records.records,
        draw_records,
        texture_slots: material_records.texture_slots,
        material_records: material_records.materials,
        effect_records: material_records.effects,
        retained_records,
        flutter_records,
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
        SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY, SCENE_BINARY_MOTION_FAMILY_FLUTTER,
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
        assert_eq!(plan.resource_records.len(), 2);
        assert_eq!(plan.resource_records[0].width, 128);
        assert_eq!(plan.resource_records[0].height, 64);
        assert_eq!(plan.resource_records[1].width, 128);
        assert_eq!(plan.resource_records[1].height, 64);
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
        assert_eq!(plan.node_records.len(), 1);
        assert_eq!(plan.geometry_records.len(), 1);
        assert_eq!(plan.draw_records[0].node_index, 0);
        let node = plan.node_records[plan.draw_records[0].node_index as usize];
        assert_eq!(node.geometry_index, 0);
        assert_eq!(node.material_index, 0);
        assert_eq!(node.draw_order, 0);
        let geometry = plan.geometry_records[node.geometry_index as usize];
        assert_eq!(
            geometry.primitive_kind,
            SCENE_BINARY_GEOMETRY_PRIMITIVE_QUAD
        );
        assert_eq!(
            geometry.vertex_layout,
            SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_GENERATED
        );
        assert_eq!(geometry.vertices.first_record, SCENE_BINARY_NONE_ID);
        assert_eq!(
            geometry.vertices.record_count,
            SCENE_BINARY_GEOMETRY_QUAD_VERTEX_COUNT
        );
        assert_eq!(geometry.indices.first_record, SCENE_BINARY_NONE_ID);
        assert_eq!(
            geometry.indices.record_count,
            SCENE_BINARY_GEOMETRY_QUAD_INDEX_COUNT
        );
        assert_eq!(plan.material_pass_count, 1);
        assert_eq!(plan.effect_pass_count, 1);
        assert_eq!(plan.effect_parameter_count, 2);
        assert_eq!(plan.texture_slot_count, 2);
        assert!(plan.flutter_records.is_empty());
        assert_eq!(plan.texture_slots.len(), 2);
        assert_eq!(plan.texture_slots[0].resource_index, 0);
        assert_eq!(plan.texture_slots[1].resource_index, 1);
        assert_eq!(
            plan.resource_records[plan.texture_slots[1].resource_index as usize].source_name,
            plan.resource_records[1].source_name
        );
        assert_eq!(plan.material_records.len(), 1);
        assert_eq!(plan.material_records[0].texture_slots.first_record, 0);
        assert_eq!(plan.material_records[0].texture_slots.record_count, 2);
        assert_eq!(plan.material_records[0].effect_passes.first_record, 0);
        assert_eq!(plan.material_records[0].effect_passes.record_count, 1);
        assert_eq!(plan.effect_records.len(), 1);
        assert_eq!(plan.effect_records[0].texture_slots.first_record, 0);
        assert_eq!(plan.effect_records[0].texture_slots.record_count, 2);
        assert_eq!(plan.effect_records[0].parameters.first_record, 0);
        assert_eq!(plan.effect_records[0].parameters.record_count, 2);
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
            plan.retained_records.len() as u32,
            plan.retained_gpu_state_count
        );
        assert_eq!(
            plan.retained_dirty_range_count,
            plan.retained_gpu_state_count
        );
        assert!(
            plan.retained_records
                .iter()
                .all(|record| record.stable_id != 0 && record.dirty_range_count > 0)
        );
        assert_ne!(plan.material_records[0].descriptor_layout, 0);
    }

    #[test]
    fn binary_plan_reads_flutter_retained_state_without_json_payloads() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "hair", "type": "image", "source": "assets/hair.gtex", "width": 128, "height": 256 }
            ],
            "nodes": [
                {
                    "id": "hair",
                    "type": "image",
                    "resource": "hair",
                    "width": 128,
                    "height": 256,
                    "effects": [
                        {
                            "file": "effects/flutter/effect.json",
                            "properties": { "phase": 0.25 },
                            "passes": [
                                {
                                    "shader": "effects/flutter",
                                    "texture_resources": ["hair"],
                                    "constant_shader_values": {
                                        "speed": 1.5,
                                        "wind": [1.0, 0.0]
                                    },
                                    "combos": { "WIND_MODE": 2 }
                                }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        let bytes = scene_binary_payloads_from_document(&document)
            .encode_container(0)
            .expect("binary scene");

        let plan = native_vulkan_scene_binary_plan_from_container(&bytes).expect("binary plan");

        assert_eq!(plan.flutter_state_count, 1);
        assert_eq!(plan.resource_records.len(), 1);
        assert_eq!(plan.resource_records[0].height, 256);
        assert_eq!(plan.effect_parameter_count, 4);
        assert_eq!(plan.flutter_records.len(), 1);
        assert_eq!(
            plan.flutter_records[0].owner_name,
            plan.node_records[plan.draw_records[0].node_index as usize].id_name
        );
        assert_eq!(
            plan.flutter_records[0].motion_family_flags,
            SCENE_BINARY_MOTION_FAMILY_FLUTTER
        );
        assert_eq!(plan.flutter_records[0].first_parameter, 0);
        assert_eq!(plan.flutter_records[0].parameter_count, 4);
        assert_eq!(plan.flutter_records[0].pass_count, 1);
        assert_eq!(plan.flutter_records[0].dirty_range_count, 3);
        assert_eq!(plan.material_records.len(), 1);
        assert_eq!(plan.effect_records.len(), 1);
        assert_eq!(plan.effect_records[0].parameters.record_count, 3);
        assert_eq!(
            plan.retained_records.len() as u32,
            plan.retained_gpu_state_count
        );
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
        assert_eq!(plan.geometry_records.len(), 1);
        let node = plan.node_records[plan.draw_records[0].node_index as usize];
        let geometry = plan.geometry_records[node.geometry_index as usize];
        assert_eq!(
            geometry.primitive_kind,
            SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH
        );
        assert_eq!(
            geometry.vertex_layout,
            SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY
        );
        assert_eq!(geometry.vertices.first_record, 0);
        assert_eq!(geometry.vertices.record_count, 3);
        assert_eq!(geometry.indices.first_record, 0);
        assert_eq!(geometry.indices.record_count, 3);
        assert_eq!(plan.material_records.len(), 1);
        assert_eq!(plan.material_records[0].texture_slots.record_count, 0);
        assert_eq!(plan.material_records[0].effect_passes.record_count, 0);
        assert_eq!(
            plan.mesh_vertex_stream_bytes,
            3 * SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE as u64
        );
        assert_eq!(
            plan.mesh_index_stream_bytes,
            3 * SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE as u64
        );
    }
}

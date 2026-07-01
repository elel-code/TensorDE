use crate::core::scene::binary::{
    SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE, SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
    SCENE_BINARY_NONE_ID, SceneBinaryError, SceneBinaryLayoutPlan, decode_scene_binary_container,
};

mod blend;
mod debug_name;
mod effect_parameter;
mod flutter;
mod geometry;
mod material;
mod node;
mod puppet;
mod render_state;
mod resource;
mod retained;
mod transform;

pub(in crate::renderer::native_vulkan::scene) use self::debug_name::NativeVulkanSceneBinaryDebugNameSummary;
use self::debug_name::native_vulkan_scene_binary_debug_names;
pub(in crate::renderer::native_vulkan::scene) use self::effect_parameter::NativeVulkanSceneBinaryEffectParameterIngestPlan;
use self::effect_parameter::native_vulkan_scene_binary_effect_parameter_ingest_plan;
pub(in crate::renderer::native_vulkan::scene) use self::flutter::NativeVulkanSceneBinaryFlutterRecord;
use self::flutter::native_vulkan_scene_binary_flutter_records;
pub(in crate::renderer::native_vulkan::scene) use self::geometry::NativeVulkanSceneBinaryGeometryRecord;
use self::geometry::native_vulkan_scene_binary_geometry_records;
use self::material::native_vulkan_scene_binary_material_records;
pub(in crate::renderer::native_vulkan::scene) use self::material::{
    NativeVulkanSceneBinaryEffectRecord, NativeVulkanSceneBinaryEffectUvTransformRecord,
    NativeVulkanSceneBinaryMaterialRecord, NativeVulkanSceneBinaryTextureSlotRecord,
};
pub(in crate::renderer::native_vulkan::scene) use self::node::NativeVulkanSceneBinaryNodeRecord;
use self::node::native_vulkan_scene_binary_node_records;
pub(in crate::renderer::native_vulkan::scene) use self::puppet::NativeVulkanSceneBinaryPuppetRecord;
use self::puppet::native_vulkan_scene_binary_puppet_records;
pub(in crate::renderer::native_vulkan::scene) use self::render_state::NativeVulkanSceneBinaryRenderStateRecord;
use self::render_state::native_vulkan_scene_binary_render_state_records;
pub(in crate::renderer::native_vulkan::scene) use self::resource::NativeVulkanSceneBinaryResourceRecord;
use self::resource::native_vulkan_scene_binary_resource_records;
use self::retained::native_vulkan_scene_binary_retained_ingest_plan;
pub(in crate::renderer::native_vulkan::scene) use self::retained::{
    NativeVulkanSceneBinaryRetainedIngestPlan, NativeVulkanSceneBinaryRetainedUpdatePlan,
};
pub(in crate::renderer::native_vulkan::scene) use self::transform::{
    NativeVulkanSceneBinaryTransformKeyframeRecord, NativeVulkanSceneBinaryTransformRecord,
};
use self::transform::{
    native_vulkan_scene_binary_transform_keyframe_records,
    native_vulkan_scene_binary_transform_records,
};

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryPlan {
    pub(in crate::renderer::native_vulkan::scene) feature_flags: u32,
    pub(in crate::renderer::native_vulkan::scene) resource_count: u32,
    pub(in crate::renderer::native_vulkan::scene) node_count: u32,
    pub(in crate::renderer::native_vulkan::scene) draw_record_count: u32,
    pub(in crate::renderer::native_vulkan::scene) geometry_record_count: u32,
    pub(in crate::renderer::native_vulkan::scene) transform_timeline_count: u32,
    pub(in crate::renderer::native_vulkan::scene) transform_keyframe_count: u32,
    pub(in crate::renderer::native_vulkan::scene) generated_vertex_count: u32,
    pub(in crate::renderer::native_vulkan::scene) generated_index_count: u32,
    pub(in crate::renderer::native_vulkan::scene) mesh_vertex_count: u32,
    pub(in crate::renderer::native_vulkan::scene) mesh_index_count: u32,
    pub(in crate::renderer::native_vulkan::scene) mesh_vertex_stream_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) mesh_index_stream_bytes: u64,
    pub(in crate::renderer::native_vulkan::scene) texture_slot_count: u32,
    pub(in crate::renderer::native_vulkan::scene) material_pass_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_pass_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_uv_transform_count: u32,
    pub(in crate::renderer::native_vulkan::scene) effect_parameter_count: u32,
    pub(in crate::renderer::native_vulkan::scene) flutter_state_count: u32,
    pub(in crate::renderer::native_vulkan::scene) puppet_count: u32,
    pub(in crate::renderer::native_vulkan::scene) render_state_count: u32,
    pub(in crate::renderer::native_vulkan::scene) retained_gpu_state_count: u32,
    pub(in crate::renderer::native_vulkan::scene) retained_dirty_range_count: u32,
    pub(in crate::renderer::native_vulkan::scene) debug_name_count: u32,
    pub(in crate::renderer::native_vulkan::scene) retained_update_plan:
        NativeVulkanSceneBinaryRetainedUpdatePlan,
    pub(in crate::renderer::native_vulkan::scene) retained_ingest_plan:
        NativeVulkanSceneBinaryRetainedIngestPlan,
    pub(in crate::renderer::native_vulkan::scene) effect_parameter_ingest_plan:
        NativeVulkanSceneBinaryEffectParameterIngestPlan,
    pub(in crate::renderer::native_vulkan::scene) debug_names:
        NativeVulkanSceneBinaryDebugNameSummary,
    pub(in crate::renderer::native_vulkan::scene) resource_records:
        Vec<NativeVulkanSceneBinaryResourceRecord>,
    pub(in crate::renderer::native_vulkan::scene) node_records:
        Vec<NativeVulkanSceneBinaryNodeRecord>,
    pub(in crate::renderer::native_vulkan::scene) transform_records:
        Vec<NativeVulkanSceneBinaryTransformRecord>,
    pub(in crate::renderer::native_vulkan::scene) transform_keyframe_records:
        Vec<NativeVulkanSceneBinaryTransformKeyframeRecord>,
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
    pub(in crate::renderer::native_vulkan::scene) effect_uv_transform_records:
        Vec<NativeVulkanSceneBinaryEffectUvTransformRecord>,
    pub(in crate::renderer::native_vulkan::scene) flutter_records:
        Vec<NativeVulkanSceneBinaryFlutterRecord>,
    pub(in crate::renderer::native_vulkan::scene) puppet_records:
        Vec<NativeVulkanSceneBinaryPuppetRecord>,
    pub(in crate::renderer::native_vulkan::scene) render_state_records:
        Vec<NativeVulkanSceneBinaryRenderStateRecord>,
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
    let transform_records = native_vulkan_scene_binary_transform_records(container, layout)?;
    let transform_timeline_count = record_len_from_usize(transform_records.len());
    let transform_keyframe_records =
        native_vulkan_scene_binary_transform_keyframe_records(container, layout)?;
    let transform_keyframe_count = record_len_from_usize(transform_keyframe_records.len());
    let geometry_records = native_vulkan_scene_binary_geometry_records(container, layout)?;
    let geometry_record_count = record_len_from_usize(geometry_records.records.len());
    let texture_slot_count = record_len(layout.texture_slot_records(container)?);
    let material_pass_count = record_len(layout.material_pass_records(container)?);
    let effect_pass_count = record_len(layout.effect_pass_records(container)?);
    let effect_uv_transform_count = record_len(layout.effect_uv_transform_records(container)?);
    let effect_parameter_ingest_plan =
        native_vulkan_scene_binary_effect_parameter_ingest_plan(container, layout)?;
    let effect_parameter_count = effect_parameter_ingest_plan.record_count;
    let puppet_records = native_vulkan_scene_binary_puppet_records(container, layout)?;
    let puppet_count = record_len_from_usize(puppet_records.len());
    let render_state_records = native_vulkan_scene_binary_render_state_records(container, layout)?;
    let render_state_count = record_len_from_usize(render_state_records.len());
    let material_records = native_vulkan_scene_binary_material_records(container, layout)?;
    let retained_ingest_plan = native_vulkan_scene_binary_retained_ingest_plan(container, layout)?;
    let retained_gpu_state_count = retained_ingest_plan.record_count;
    let retained_dirty_range_count = retained_ingest_plan.dirty_range_count;
    let retained_update_plan = retained_ingest_plan.update_plan;
    let debug_names = native_vulkan_scene_binary_debug_names(container, layout)?;
    let debug_name_count = debug_names.record_count;
    let flutter_records = native_vulkan_scene_binary_flutter_records(container, layout)?;
    let flutter_state_count = record_len_from_usize(flutter_records.len());

    let mut draw_records = Vec::with_capacity(node_records.len());
    for (node_index, node) in node_records.iter().enumerate() {
        let source_node = layout.node_record_at(container, record_len_from_usize(node_index))?;
        let _ = layout.node_transform_records(container, source_node)?;
        if node.puppet_index != SCENE_BINARY_NONE_ID {
            let puppet = layout.puppet_record_at(container, node.puppet_index)?;
            let _ = layout.puppet_skin_bone_record_range(container, puppet)?;
            let _ = layout.puppet_skin_vertex_record_range(container, puppet)?;
            let _ = layout.puppet_attachment_record_range(container, puppet)?;
            let _ = layout.puppet_layer_record_range(container, puppet)?;
            for clip in layout.puppet_clip_record_range(container, puppet)? {
                let _ = layout.puppet_frame_record_range(container, clip?)?;
            }
        }
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
        transform_timeline_count,
        transform_keyframe_count,
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
        effect_uv_transform_count,
        effect_parameter_count,
        flutter_state_count,
        puppet_count,
        render_state_count,
        retained_gpu_state_count,
        retained_dirty_range_count,
        debug_name_count,
        retained_update_plan,
        retained_ingest_plan,
        effect_parameter_ingest_plan,
        debug_names,
        resource_records,
        node_records,
        transform_records,
        transform_keyframe_records,
        geometry_records: geometry_records.records,
        draw_records,
        texture_slots: material_records.texture_slots,
        material_records: material_records.materials,
        effect_records: material_records.effects,
        effect_uv_transform_records: material_records.effect_uv_transforms,
        flutter_records,
        puppet_records,
        render_state_records,
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
        SCENE_BINARY_PUPPET_FLAG_MESH, scene_binary_payloads_from_document,
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
                    "opacity": 0.75,
                    "color": "#abcdef",
                    "stroke_color": "#010203",
                    "stroke_width": 1.5,
                    "corner_radius": 2.5,
                    "fit": "contain",
                    "effects": [
                        {
                            "file": "effects/opacity/effect.json",
                            "passes": [
                                {
                                    "shader": "effects/opacity",
                                    "texture_resources": ["base", "mask"],
                                    "effect_uv_transform": {
                                        "mapping": "texture-resolution",
                                        "source_slot": 0,
                                        "mask_slot": 1,
                                        "scale": [1.0, 1.0],
                                        "offset": [0.1, 0.0],
                                        "input_extent": { "width": 128, "height": 64 },
                                        "mask_extent": { "width": 128, "height": 64 },
                                        "mask_backing_extent": { "width": 128, "height": 64 }
                                    },
                                    "constant_shader_values": { "speed": 2.0 },
                                    "combos": { "MASK": 1 }
                                }
                            ]
                        }
                    ]
                }
            ],
            "timelines": [
                {
                    "id": "layer-x",
                    "target_node": "layer",
                    "channels": [
                        {
                            "property": "x",
                            "loop": true,
                            "keyframes": [
                                { "time_ms": 0, "value": 0.0 },
                                { "time_ms": 1000, "value": 5.0, "curve": "ease-out" }
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
        assert!(plan.debug_name_count >= 2);
        assert_eq!(plan.debug_name_count, plan.debug_names.record_count);
        assert!(plan.debug_names.string_bytes > 0);
        assert_eq!(plan.resource_records[0].width, 128);
        assert_eq!(plan.resource_records[0].height, 64);
        assert_eq!(plan.resource_records[1].width, 128);
        assert_eq!(plan.resource_records[1].height, 64);
        assert_eq!(plan.node_count, 1);
        assert_eq!(plan.draw_record_count, 1);
        assert_eq!(plan.transform_timeline_count, 2);
        assert_eq!(plan.transform_keyframe_count, 2);
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
        assert_eq!(plan.transform_records.len(), 2);
        assert_eq!(plan.transform_keyframe_records.len(), 2);
        assert_eq!(
            plan.transform_records[0].owner_name,
            plan.node_records[0].id_name
        );
        assert_eq!(plan.node_records[0].first_transform, 0);
        assert_eq!(plan.node_records[0].transform_count, 2);
        assert_eq!(plan.transform_records[1].first_keyframe, 0);
        assert_eq!(plan.transform_records[1].keyframe_count, 2);
        assert_eq!(plan.transform_keyframe_records[1].time_ms, 1000);
        assert_eq!(plan.transform_keyframe_records[1].value, 5.0);
        assert_ne!(plan.transform_keyframe_records[1].curve, 0);
        assert_eq!(plan.geometry_records.len(), 1);
        assert_eq!(plan.draw_records[0].node_index, 0);
        let node = plan.node_records[plan.draw_records[0].node_index as usize];
        assert_eq!(node.geometry_index, 0);
        assert_eq!(node.material_index, 0);
        assert_eq!(node.draw_order, 0);
        assert_eq!(node.opacity, 0.75);
        assert_eq!(node.color_rgba, 0xabcdefff);
        assert_eq!(node.stroke_color_rgba, 0x010203ff);
        assert_eq!(node.stroke_width, 1.5);
        assert_eq!(node.corner_radius, 2.5);
        assert_eq!(node.fit, 2);
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
        assert_eq!(plan.effect_uv_transform_count, 1);
        assert_eq!(plan.effect_parameter_count, 2);
        assert_eq!(plan.effect_parameter_ingest_plan.record_count, 2);
        assert_eq!(plan.effect_parameter_ingest_plan.pass_constant_count, 1);
        assert_eq!(plan.effect_parameter_ingest_plan.pass_switch_count, 1);
        assert_eq!(plan.texture_slot_count, 2);
        assert_eq!(plan.render_state_count, 1);
        assert_eq!(plan.render_state_records.len(), 1);
        assert_eq!(
            plan.render_state_records[0].resource_count,
            plan.resource_count
        );
        assert_eq!(plan.render_state_records[0].node_count, plan.node_count);
        assert_eq!(
            plan.render_state_records[0].material_count,
            plan.material_pass_count
        );
        assert_eq!(
            plan.render_state_records[0].effect_count,
            plan.effect_pass_count
        );
        assert_eq!(
            plan.render_state_records[0].texture_slot_count,
            plan.texture_slot_count
        );
        assert!(plan.puppet_records.is_empty());
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
        assert_ne!(plan.material_records[0].pass_state.blend.mode, 0);
        assert_eq!(plan.material_records[0].pass_state.alpha_texture_slot, 1);
        assert_eq!(
            plan.material_records[0].pass_state.blend.blending_name,
            SCENE_BINARY_NONE_ID
        );
        assert_eq!(plan.effect_records.len(), 1);
        assert_eq!(plan.effect_records[0].texture_slots.first_record, 0);
        assert_eq!(plan.effect_records[0].texture_slots.record_count, 2);
        assert_eq!(plan.effect_records[0].effect_uv_transforms.first_record, 0);
        assert_eq!(plan.effect_records[0].effect_uv_transforms.record_count, 1);
        assert_eq!(plan.effect_records[0].parameters.first_record, 0);
        assert_eq!(plan.effect_records[0].parameters.record_count, 2);
        assert_eq!(plan.effect_uv_transform_records.len(), 1);
        assert_eq!(plan.effect_uv_transform_records[0].mask_slot, 1);
        assert_eq!(plan.effect_uv_transform_records[0].offset_u, 0.1);
        assert_eq!(
            plan.effect_records[0].pass_state.blending_name,
            SCENE_BINARY_NONE_ID
        );
        assert_eq!(
            plan.retained_gpu_state_count,
            plan.resource_count
                + plan.geometry_record_count
                + plan.texture_slot_count
                + plan.material_pass_count
                + plan.effect_pass_count
                + plan.effect_uv_transform_count
                + plan.effect_parameter_count
        );
        assert_eq!(
            plan.retained_ingest_plan.record_count,
            plan.retained_gpu_state_count
        );
        assert_eq!(
            plan.retained_dirty_range_count,
            plan.retained_gpu_state_count
        );
        assert_eq!(plan.retained_update_plan.resource_count, 2);
        assert_eq!(plan.retained_update_plan.geometry_count, 1);
        assert_eq!(plan.retained_update_plan.texture_slot_count, 2);
        assert_eq!(plan.retained_update_plan.material_pass_count, 1);
        assert_eq!(plan.retained_update_plan.effect_pass_count, 1);
        assert_eq!(plan.retained_update_plan.effect_uv_transform_count, 1);
        assert_eq!(plan.retained_update_plan.effect_parameter_count, 2);
        assert_eq!(
            plan.retained_update_plan.dirty_range_count,
            plan.retained_dirty_range_count
        );
        assert_eq!(
            plan.retained_ingest_plan.stable_id_count,
            plan.retained_gpu_state_count
        );
        assert_eq!(
            plan.retained_ingest_plan.dirty_record_count,
            plan.retained_gpu_state_count
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
        assert_eq!(plan.transform_timeline_count, 1);
        assert_eq!(plan.transform_keyframe_count, 0);
        assert_eq!(plan.effect_parameter_count, 4);
        assert_eq!(plan.effect_parameter_ingest_plan.record_count, 4);
        assert_eq!(plan.effect_parameter_ingest_plan.effect_property_count, 1);
        assert_eq!(plan.effect_parameter_ingest_plan.pass_constant_count, 2);
        assert_eq!(plan.effect_parameter_ingest_plan.pass_switch_count, 1);
        assert_eq!(plan.flutter_records.len(), 1);
        assert_eq!(
            plan.flutter_records[0].owner_name,
            plan.node_records[plan.draw_records[0].node_index as usize].id_name
        );
        assert_eq!(
            plan.flutter_records[0].motion_family_mask,
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
            plan.retained_ingest_plan.record_count,
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
        assert_eq!(plan.transform_timeline_count, 1);
        assert_eq!(plan.transform_keyframe_count, 0);
        assert_eq!(plan.generated_vertex_count, 0);
        assert_eq!(plan.generated_index_count, 0);
        assert_eq!(plan.mesh_vertex_count, 3);
        assert_eq!(plan.mesh_index_count, 3);
        assert_eq!(plan.puppet_count, 1);
        assert_eq!(plan.puppet_records.len(), 1);
        assert_eq!(
            plan.puppet_records[0].owner_name,
            plan.node_records[plan.draw_records[0].node_index as usize].id_name
        );
        assert_eq!(plan.puppet_records[0].vertex_count, 3);
        assert_eq!(plan.puppet_records[0].index_count, 3);
        assert_eq!(plan.puppet_records[0].first_bone, SCENE_BINARY_NONE_ID);
        assert_eq!(plan.puppet_records[0].bone_count, 0);
        assert_eq!(plan.puppet_records[0].first_layer, SCENE_BINARY_NONE_ID);
        assert_eq!(plan.puppet_records[0].animation_layer_count, 0);
        assert!(plan.puppet_records[0].flags & SCENE_BINARY_PUPPET_FLAG_MESH != 0);
        assert_eq!(plan.puppet_records[0].dirty_range_count, 1);
        assert_eq!(plan.retained_update_plan.puppet_count, 1);
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

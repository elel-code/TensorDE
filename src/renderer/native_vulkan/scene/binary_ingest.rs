use crate::core::scene::binary::{
    SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE, SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
    SCENE_BINARY_NONE_ID, SCENE_BINARY_PARAMETER_ROLE_EFFECT_PROPERTY,
    SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO, SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT,
    SCENE_BINARY_RETAINED_EFFECT_PARAMETER, SCENE_BINARY_RETAINED_EFFECT_PASS,
    SCENE_BINARY_RETAINED_GEOMETRY, SCENE_BINARY_RETAINED_MATERIAL_PASS,
    SCENE_BINARY_RETAINED_RESOURCE, SCENE_BINARY_RETAINED_TEXTURE_SLOT, SceneBinaryChunkKind,
    SceneBinaryError, SceneBinaryLayoutPlan, decode_scene_binary_container,
};

mod summary;

pub(in crate::renderer::native_vulkan::scene) use self::summary::NativeVulkanSceneBinaryIngestSummary;

pub(in crate::renderer::native_vulkan::scene) fn native_vulkan_scene_binary_ingest_from_container(
    container: &[u8],
) -> Result<NativeVulkanSceneBinaryIngestSummary, SceneBinaryError> {
    let layout = decode_scene_binary_container(container)?;
    native_vulkan_scene_binary_ingest_from_layout(container, &layout)
}

fn native_vulkan_scene_binary_ingest_from_layout(
    container: &[u8],
    layout: &SceneBinaryLayoutPlan,
) -> Result<NativeVulkanSceneBinaryIngestSummary, SceneBinaryError> {
    let mut summary = NativeVulkanSceneBinaryIngestSummary {
        feature_flags: layout.feature_flags,
        chunk_count: layout.chunks.len().min(u32::MAX as usize) as u32,
        ..Default::default()
    };

    for resource in layout.resource_records(container)? {
        let _ = resource?;
        summary.resource_count = summary.resource_count.saturating_add(1);
    }

    for node in layout.node_records(container)? {
        let node = node?;
        summary.node_count = summary.node_count.saturating_add(1);
        if node.geometry_index != SCENE_BINARY_NONE_ID {
            summary.draw_record_count = summary.draw_record_count.saturating_add(1);
        }
    }

    for transform in layout.transform_timeline_records(container)? {
        let _ = transform?;
        summary.transform_timeline_count = summary.transform_timeline_count.saturating_add(1);
    }

    for geometry in layout.geometry_records(container)? {
        let geometry = geometry?;
        summary.geometry_record_count = summary.geometry_record_count.saturating_add(1);
        if geometry.first_vertex == SCENE_BINARY_NONE_ID {
            summary.generated_vertex_count = summary
                .generated_vertex_count
                .saturating_add(geometry.vertex_count);
        } else {
            for vertex in layout.geometry_vertex_record_range(container, geometry)? {
                let _ = vertex?;
                summary.mesh_vertex_count = summary.mesh_vertex_count.saturating_add(1);
            }
        }
        if geometry.first_index == SCENE_BINARY_NONE_ID {
            summary.generated_index_count = summary
                .generated_index_count
                .saturating_add(geometry.index_count);
        } else {
            for index in layout.geometry_index_record_range(container, geometry)? {
                let _ = index?;
                summary.mesh_index_count = summary.mesh_index_count.saturating_add(1);
            }
        }
    }

    summary.mesh_vertex_stream_bytes = u64::from(summary.mesh_vertex_count)
        .saturating_mul(SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE as u64);
    summary.mesh_index_stream_bytes = u64::from(summary.mesh_index_count)
        .saturating_mul(SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE as u64);

    for texture_slot in layout.texture_slot_records(container)? {
        let _ = texture_slot?;
        summary.texture_slot_count = summary.texture_slot_count.saturating_add(1);
    }

    for material in layout.material_pass_records(container)? {
        let _ = material?;
        summary.material_pass_count = summary.material_pass_count.saturating_add(1);
    }

    for effect in layout.effect_pass_records(container)? {
        let _ = effect?;
        summary.effect_pass_count = summary.effect_pass_count.saturating_add(1);
    }

    for parameter in layout.effect_parameter_records(container)? {
        let parameter = parameter?;
        summary.effect_parameter_count = summary.effect_parameter_count.saturating_add(1);
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_EFFECT_PROPERTY != 0 {
            summary.effect_property_count = summary.effect_property_count.saturating_add(1);
        }
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT != 0 {
            summary.effect_pass_constant_count =
                summary.effect_pass_constant_count.saturating_add(1);
        }
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO != 0 {
            summary.effect_pass_switch_count = summary.effect_pass_switch_count.saturating_add(1);
        }
    }

    for flutter in layout.flutter_state_records(container)? {
        let _ = flutter?;
        summary.flutter_state_count = summary.flutter_state_count.saturating_add(1);
    }

    for puppet in layout.puppet_records(container)? {
        let puppet = puppet?;
        summary.puppet_count = summary.puppet_count.saturating_add(1);
        summary.puppet_vertex_count = summary
            .puppet_vertex_count
            .saturating_add(puppet.vertex_count);
        summary.puppet_index_count = summary
            .puppet_index_count
            .saturating_add(puppet.index_count);
        summary.puppet_animation_layer_count = summary
            .puppet_animation_layer_count
            .saturating_add(puppet.animation_layer_count);
    }

    for render_state in layout.render_state_records(container)? {
        let _ = render_state?;
        summary.render_state_count = summary.render_state_count.saturating_add(1);
    }

    for retained in layout.retained_gpu_state_records(container)? {
        let retained = retained?;
        summary.retained.record_count = summary.retained.record_count.saturating_add(1);
        summary.retained.dirty_range_count = summary
            .retained
            .dirty_range_count
            .saturating_add(retained.dirty_range_count);
        if retained.stable_id != 0 {
            summary.retained.stable_id_count = summary.retained.stable_id_count.saturating_add(1);
        }
        if retained.dirty_range_count > 0 {
            summary.retained.dirty_record_count =
                summary.retained.dirty_record_count.saturating_add(1);
        }
        match retained.owner_kind {
            SCENE_BINARY_RETAINED_RESOURCE => {
                summary.retained.resource_count = summary.retained.resource_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_TEXTURE_SLOT => {
                summary.retained.texture_slot_count =
                    summary.retained.texture_slot_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_MATERIAL_PASS => {
                summary.retained.material_pass_count =
                    summary.retained.material_pass_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_EFFECT_PASS => {
                summary.retained.effect_pass_count =
                    summary.retained.effect_pass_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_EFFECT_PARAMETER => {
                summary.retained.effect_parameter_count =
                    summary.retained.effect_parameter_count.saturating_add(1);
            }
            SCENE_BINARY_RETAINED_GEOMETRY => {
                summary.retained.geometry_count = summary.retained.geometry_count.saturating_add(1);
            }
            owner_kind => {
                return Err(SceneBinaryError::UnknownRetainedOwnerKind { owner_kind });
            }
        }
    }

    let debug_names = layout.debug_names(container)?;
    summary.debug_name_count = debug_names.len().min(u32::MAX as usize) as u32;
    let descriptor =
        layout
            .chunk(SceneBinaryChunkKind::DebugNames)
            .ok_or(SceneBinaryError::MissingChunk {
                kind: SceneBinaryChunkKind::DebugNames,
            })?;
    let debug_record_bytes = debug_names
        .len()
        .saturating_mul(crate::core::scene::binary::SCENE_BINARY_DEBUG_NAME_RECORD_SIZE);
    summary.debug_name_string_bytes = descriptor
        .length
        .saturating_sub(debug_record_bytes.min(u64::MAX as usize) as u64)
        .min(u64::from(u32::MAX)) as u32;

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::core::scene::SceneDocument;
    use crate::core::scene::binary::scene_binary_payloads_from_document;

    #[test]
    fn binary_ingest_streams_scene_chunks_without_retaining_record_tables() {
        let document: SceneDocument = serde_json::from_value(json!({
            "resources": [
                { "id": "base", "type": "image", "source": "assets/base.gtex", "width": 64, "height": 64 },
                { "id": "mask", "type": "image", "source": "assets/mask.gtex", "width": 64, "height": 64 }
            ],
            "nodes": [
                {
                    "id": "mesh-node",
                    "type": "image",
                    "resource": "base",
                    "mesh": {
                        "vertices": [
                            { "x": -1.0, "y": -1.0, "u": 0.0, "v": 0.0 },
                            { "x": 1.0, "y": -1.0, "u": 1.0, "v": 0.0 },
                            { "x": 0.0, "y": 1.0, "u": 0.5, "v": 1.0 }
                        ],
                        "indices": [0, 1, 2]
                    },
                    "effects": [
                        {
                            "file": "effects/opacity/effect.json",
                            "properties": { "phase": 0.5 },
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
            .encode_container(0x80)
            .expect("binary scene");

        let ingest =
            native_vulkan_scene_binary_ingest_from_container(&bytes).expect("binary ingest");

        assert_eq!(ingest.feature_flags, 0x80);
        assert_eq!(ingest.resource_count, 2);
        assert_eq!(ingest.node_count, 1);
        assert_eq!(ingest.draw_record_count, 1);
        assert_eq!(ingest.mesh_vertex_count, 3);
        assert_eq!(ingest.mesh_index_count, 3);
        assert_eq!(ingest.texture_slot_count, 2);
        assert_eq!(ingest.material_pass_count, 1);
        assert_eq!(ingest.effect_pass_count, 1);
        assert_eq!(ingest.effect_parameter_count, 3);
        assert_eq!(ingest.effect_property_count, 1);
        assert_eq!(ingest.effect_pass_constant_count, 1);
        assert_eq!(ingest.effect_pass_switch_count, 1);
        assert_eq!(ingest.puppet_count, 1);
        assert_eq!(ingest.puppet_vertex_count, 3);
        assert_eq!(ingest.puppet_index_count, 3);
        assert_eq!(ingest.render_state_count, 1);
        assert_eq!(
            ingest.retained.record_count,
            ingest.retained.stable_id_count
        );
        assert_eq!(
            ingest.retained.record_count,
            ingest.retained.dirty_record_count
        );
        assert_eq!(
            ingest.retained.dirty_range_count,
            ingest.retained.record_count
        );
        assert!(ingest.debug_name_count > 0);
        assert!(ingest.debug_name_string_bytes > 0);
    }
}

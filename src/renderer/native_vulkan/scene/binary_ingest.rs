use crate::core::scene::binary::{
    SCENE_BINARY_DEBUG_NAME_RECORD_SIZE, SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
    SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE, SCENE_BINARY_NONE_ID,
    SCENE_BINARY_PARAMETER_ROLE_EFFECT_PROPERTY, SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO,
    SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT, SCENE_BINARY_RETAINED_EFFECT_PARAMETER,
    SCENE_BINARY_RETAINED_EFFECT_PASS, SCENE_BINARY_RETAINED_EFFECT_UV_TRANSFORM,
    SCENE_BINARY_RETAINED_GEOMETRY, SCENE_BINARY_RETAINED_MATERIAL_PASS,
    SCENE_BINARY_RETAINED_PUPPET, SCENE_BINARY_RETAINED_RESOURCE,
    SCENE_BINARY_RETAINED_TEXTURE_SLOT, SceneBinaryChunkDescriptor, SceneBinaryChunkKind,
    SceneBinaryEffectParameterRecord, SceneBinaryError, SceneBinaryGeometryRecord,
    SceneBinaryLayoutPlan, SceneBinaryNodeRecord, SceneBinaryPuppetRecord,
    SceneBinaryRetainedGpuStateRecord, SceneBinaryTransformTimelineRecord,
    decode_scene_binary_container,
};

mod stream;
mod summary;

pub(in crate::renderer::native_vulkan::scene) use self::stream::native_vulkan_scene_binary_ingest_from_reader;
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

    let node_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::NodeTable,
    )?;
    let transform_timeline_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::TransformTimeline,
    )?;
    let transform_keyframe_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::TransformKeyframes,
    )?;
    let puppet_record_count =
        native_vulkan_scene_binary_ingest_chunk_record_count(layout, SceneBinaryChunkKind::Puppet)?;
    let puppet_skin_bone_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::PuppetSkinBones,
    )?;
    let puppet_skin_vertex_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::PuppetSkinVertices,
    )?;
    let puppet_attachment_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::PuppetAttachments,
    )?;
    let puppet_clip_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::PuppetClips,
    )?;
    let puppet_frame_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::PuppetFrames,
    )?;
    let puppet_layer_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::PuppetLayers,
    )?;
    let material_pass_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::MaterialPass,
    )?;
    let geometry_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::Geometry,
    )?;

    for (node_index, node) in layout.node_records(container)?.enumerate() {
        native_vulkan_scene_binary_ingest_node_record(
            &mut summary,
            node?,
            node_index.min(u32::MAX as usize) as u32,
            node_record_count,
            transform_timeline_record_count,
            puppet_record_count,
            material_pass_record_count,
            geometry_record_count,
        )?;
    }

    for transform in layout.transform_timeline_records(container)? {
        native_vulkan_scene_binary_ingest_transform_record(
            &mut summary,
            transform?,
            transform_keyframe_record_count,
        )?;
    }

    for keyframe in layout.transform_keyframe_records(container)? {
        let _ = keyframe?;
        summary.transform_keyframe_count = summary.transform_keyframe_count.saturating_add(1);
    }

    let geometry_vertex_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::GeometryVertices,
    )?;
    let geometry_index_record_count = native_vulkan_scene_binary_ingest_chunk_record_count(
        layout,
        SceneBinaryChunkKind::GeometryIndices,
    )?;
    for geometry in layout.geometry_records(container)? {
        native_vulkan_scene_binary_ingest_geometry_record(
            &mut summary,
            geometry?,
            geometry_vertex_record_count,
            geometry_index_record_count,
        )?;
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

    for transform in layout.effect_uv_transform_records(container)? {
        let _ = transform?;
        summary.effect_uv_transform_count = summary.effect_uv_transform_count.saturating_add(1);
    }

    for parameter in layout.effect_parameter_records(container)? {
        native_vulkan_scene_binary_ingest_effect_parameter_record(&mut summary, parameter?);
    }

    for flutter in layout.flutter_state_records(container)? {
        let _ = flutter?;
        summary.flutter_state_count = summary.flutter_state_count.saturating_add(1);
    }

    for puppet in layout.puppet_records(container)? {
        native_vulkan_scene_binary_ingest_puppet_record(
            &mut summary,
            puppet?,
            puppet_skin_bone_record_count,
            puppet_skin_vertex_record_count,
            puppet_attachment_record_count,
            puppet_clip_record_count,
            puppet_frame_record_count,
            puppet_layer_record_count,
        )?;
    }

    for render_state in layout.render_state_records(container)? {
        let _ = render_state?;
        summary.render_state_count = summary.render_state_count.saturating_add(1);
    }

    for retained in layout.retained_gpu_state_records(container)? {
        native_vulkan_scene_binary_ingest_retained_record(&mut summary, retained?)?;
    }

    let descriptor =
        layout
            .chunk(SceneBinaryChunkKind::DebugNames)
            .ok_or(SceneBinaryError::MissingChunk {
                kind: SceneBinaryChunkKind::DebugNames,
            })?;
    native_vulkan_scene_binary_ingest_debug_name_chunk(&mut summary, descriptor)?;

    Ok(summary)
}

pub(super) fn native_vulkan_scene_binary_ingest_node_record(
    summary: &mut NativeVulkanSceneBinaryIngestSummary,
    node: SceneBinaryNodeRecord,
    node_index: u32,
    node_record_count: u32,
    transform_timeline_record_count: u32,
    puppet_record_count: u32,
    material_pass_record_count: u32,
    geometry_record_count: u32,
) -> Result<(), SceneBinaryError> {
    summary.node_count = summary.node_count.saturating_add(1);
    native_vulkan_scene_binary_ingest_validate_record_range(
        SceneBinaryChunkKind::NodeTable,
        node_index,
        node.subtree_node_count,
        node_record_count,
    )?;
    if node.child_count == 0 {
        if node.first_child_index != SCENE_BINARY_NONE_ID {
            return Err(SceneBinaryError::RecordRangeOutOfBounds {
                kind: SceneBinaryChunkKind::NodeTable,
                first_record: node.first_child_index,
                record_count: 1,
                chunk_record_count: node_record_count,
            });
        }
    } else {
        native_vulkan_scene_binary_ingest_validate_record_range(
            SceneBinaryChunkKind::NodeTable,
            node.first_child_index,
            1,
            node_record_count,
        )?;
    }
    native_vulkan_scene_binary_ingest_validate_record_range(
        SceneBinaryChunkKind::TransformTimeline,
        node.first_transform,
        node.transform_count,
        transform_timeline_record_count,
    )?;
    if node.puppet_index != SCENE_BINARY_NONE_ID {
        native_vulkan_scene_binary_ingest_validate_record_range(
            SceneBinaryChunkKind::Puppet,
            node.puppet_index,
            1,
            puppet_record_count,
        )?;
    }
    if node.material_index != SCENE_BINARY_NONE_ID {
        native_vulkan_scene_binary_ingest_validate_record_range(
            SceneBinaryChunkKind::MaterialPass,
            node.material_index,
            1,
            material_pass_record_count,
        )?;
    }
    if node.geometry_index != SCENE_BINARY_NONE_ID {
        native_vulkan_scene_binary_ingest_validate_record_range(
            SceneBinaryChunkKind::Geometry,
            node.geometry_index,
            1,
            geometry_record_count,
        )?;
        summary.draw_record_count = summary.draw_record_count.saturating_add(1);
    }
    Ok(())
}

pub(super) fn native_vulkan_scene_binary_ingest_transform_record(
    summary: &mut NativeVulkanSceneBinaryIngestSummary,
    transform: SceneBinaryTransformTimelineRecord,
    transform_keyframe_record_count: u32,
) -> Result<(), SceneBinaryError> {
    summary.transform_timeline_count = summary.transform_timeline_count.saturating_add(1);
    let (first_keyframe, keyframe_count) = if transform.first_keyframe == SCENE_BINARY_NONE_ID {
        if transform.keyframe_count != 0 {
            return Err(SceneBinaryError::RecordRangeOutOfBounds {
                kind: SceneBinaryChunkKind::TransformKeyframes,
                first_record: transform.first_keyframe,
                record_count: transform.keyframe_count,
                chunk_record_count: transform_keyframe_record_count,
            });
        }
        (0, 0)
    } else {
        (transform.first_keyframe, transform.keyframe_count)
    };
    native_vulkan_scene_binary_ingest_validate_record_range(
        SceneBinaryChunkKind::TransformKeyframes,
        first_keyframe,
        keyframe_count,
        transform_keyframe_record_count,
    )
}

pub(super) fn native_vulkan_scene_binary_ingest_geometry_record(
    summary: &mut NativeVulkanSceneBinaryIngestSummary,
    geometry: SceneBinaryGeometryRecord,
    geometry_vertex_record_count: u32,
    geometry_index_record_count: u32,
) -> Result<(), SceneBinaryError> {
    summary.geometry_record_count = summary.geometry_record_count.saturating_add(1);
    if geometry.first_vertex == SCENE_BINARY_NONE_ID {
        summary.generated_vertex_count = summary
            .generated_vertex_count
            .saturating_add(geometry.vertex_count);
    } else {
        native_vulkan_scene_binary_ingest_validate_record_range(
            SceneBinaryChunkKind::GeometryVertices,
            geometry.first_vertex,
            geometry.vertex_count,
            geometry_vertex_record_count,
        )?;
        summary.mesh_vertex_count = summary
            .mesh_vertex_count
            .saturating_add(geometry.vertex_count);
    }
    if geometry.first_index == SCENE_BINARY_NONE_ID {
        summary.generated_index_count = summary
            .generated_index_count
            .saturating_add(geometry.index_count);
    } else {
        native_vulkan_scene_binary_ingest_validate_record_range(
            SceneBinaryChunkKind::GeometryIndices,
            geometry.first_index,
            geometry.index_count,
            geometry_index_record_count,
        )?;
        summary.mesh_index_count = summary
            .mesh_index_count
            .saturating_add(geometry.index_count);
    }
    Ok(())
}

pub(super) fn native_vulkan_scene_binary_ingest_effect_parameter_record(
    summary: &mut NativeVulkanSceneBinaryIngestSummary,
    parameter: SceneBinaryEffectParameterRecord,
) {
    summary.effect_parameter_count = summary.effect_parameter_count.saturating_add(1);
    if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_EFFECT_PROPERTY != 0 {
        summary.effect_property_count = summary.effect_property_count.saturating_add(1);
    }
    if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT != 0 {
        summary.effect_pass_constant_count = summary.effect_pass_constant_count.saturating_add(1);
    }
    if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO != 0 {
        summary.effect_pass_switch_count = summary.effect_pass_switch_count.saturating_add(1);
    }
}

pub(super) fn native_vulkan_scene_binary_ingest_puppet_record(
    summary: &mut NativeVulkanSceneBinaryIngestSummary,
    puppet: SceneBinaryPuppetRecord,
    puppet_skin_bone_record_count: u32,
    puppet_skin_vertex_record_count: u32,
    puppet_attachment_record_count: u32,
    puppet_clip_record_count: u32,
    puppet_frame_record_count: u32,
    puppet_layer_record_count: u32,
) -> Result<(), SceneBinaryError> {
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
    native_vulkan_scene_binary_ingest_validate_record_range(
        SceneBinaryChunkKind::PuppetSkinBones,
        puppet.first_bone,
        puppet.bone_count,
        puppet_skin_bone_record_count,
    )?;
    native_vulkan_scene_binary_ingest_validate_record_range(
        SceneBinaryChunkKind::PuppetSkinVertices,
        puppet.first_skin_vertex,
        puppet.skin_vertex_count,
        puppet_skin_vertex_record_count,
    )?;
    native_vulkan_scene_binary_ingest_validate_record_range(
        SceneBinaryChunkKind::PuppetAttachments,
        puppet.first_attachment,
        puppet.attachment_count,
        puppet_attachment_record_count,
    )?;
    native_vulkan_scene_binary_ingest_validate_record_range(
        SceneBinaryChunkKind::PuppetClips,
        puppet.first_clip,
        puppet.clip_count,
        puppet_clip_record_count,
    )?;
    native_vulkan_scene_binary_ingest_validate_record_range(
        SceneBinaryChunkKind::PuppetFrames,
        puppet.first_clip_frame,
        puppet.clip_frame_count,
        puppet_frame_record_count,
    )?;
    native_vulkan_scene_binary_ingest_validate_record_range(
        SceneBinaryChunkKind::PuppetLayers,
        puppet.first_layer,
        puppet.animation_layer_count,
        puppet_layer_record_count,
    )?;
    Ok(())
}

pub(super) fn native_vulkan_scene_binary_ingest_retained_record(
    summary: &mut NativeVulkanSceneBinaryIngestSummary,
    retained: SceneBinaryRetainedGpuStateRecord,
) -> Result<(), SceneBinaryError> {
    summary.retained.record_count = summary.retained.record_count.saturating_add(1);
    summary.retained.dirty_range_count = summary
        .retained
        .dirty_range_count
        .saturating_add(retained.dirty_range_count);
    if retained.stable_id != 0 {
        summary.retained.stable_id_count = summary.retained.stable_id_count.saturating_add(1);
    }
    if retained.dirty_range_count > 0 {
        summary.retained.dirty_record_count = summary.retained.dirty_record_count.saturating_add(1);
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
        SCENE_BINARY_RETAINED_EFFECT_UV_TRANSFORM => {
            summary.retained.effect_uv_transform_count =
                summary.retained.effect_uv_transform_count.saturating_add(1);
        }
        SCENE_BINARY_RETAINED_EFFECT_PARAMETER => {
            summary.retained.effect_parameter_count =
                summary.retained.effect_parameter_count.saturating_add(1);
        }
        SCENE_BINARY_RETAINED_GEOMETRY => {
            summary.retained.geometry_count = summary.retained.geometry_count.saturating_add(1);
        }
        SCENE_BINARY_RETAINED_PUPPET => {
            summary.retained.puppet_count = summary.retained.puppet_count.saturating_add(1);
        }
        owner_kind => {
            return Err(SceneBinaryError::UnknownRetainedOwnerKind { owner_kind });
        }
    }
    Ok(())
}

pub(super) fn native_vulkan_scene_binary_ingest_debug_name_chunk(
    summary: &mut NativeVulkanSceneBinaryIngestSummary,
    descriptor: &SceneBinaryChunkDescriptor,
) -> Result<(), SceneBinaryError> {
    native_vulkan_scene_binary_ingest_validate_debug_name_payload(descriptor)?;
    let debug_record_bytes =
        u64::from(descriptor.record_count) * SCENE_BINARY_DEBUG_NAME_RECORD_SIZE as u64;
    summary.debug_name_count = descriptor.record_count;
    summary.debug_name_string_bytes = descriptor
        .length
        .saturating_sub(debug_record_bytes)
        .min(u64::from(u32::MAX)) as u32;
    Ok(())
}

pub(super) fn native_vulkan_scene_binary_ingest_validate_record_payload(
    descriptor: &SceneBinaryChunkDescriptor,
    record_size: usize,
) -> Result<(), SceneBinaryError> {
    let expected = u64::from(descriptor.record_count)
        .checked_mul(record_size as u64)
        .ok_or(SceneBinaryError::InvalidRecordPayload {
            kind: descriptor.kind,
            record_size,
            record_count: descriptor.record_count,
            length: scene_binary_ingest_usize_for_error(descriptor.length),
        })?;
    if descriptor.length != expected {
        return Err(SceneBinaryError::InvalidRecordPayload {
            kind: descriptor.kind,
            record_size,
            record_count: descriptor.record_count,
            length: scene_binary_ingest_usize_for_error(descriptor.length),
        });
    }
    Ok(())
}

pub(super) fn scene_binary_ingest_usize_for_error(value: u64) -> usize {
    value.min(usize::MAX as u64) as usize
}

fn native_vulkan_scene_binary_ingest_validate_debug_name_payload(
    descriptor: &SceneBinaryChunkDescriptor,
) -> Result<(), SceneBinaryError> {
    let record_bytes = u64::from(descriptor.record_count)
        .checked_mul(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE as u64)
        .ok_or(SceneBinaryError::InvalidRecordPayload {
            kind: descriptor.kind,
            record_size: SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
            record_count: descriptor.record_count,
            length: scene_binary_ingest_usize_for_error(descriptor.length),
        })?;
    if descriptor.length < record_bytes {
        return Err(SceneBinaryError::InvalidRecordPayload {
            kind: descriptor.kind,
            record_size: SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
            record_count: descriptor.record_count,
            length: scene_binary_ingest_usize_for_error(descriptor.length),
        });
    }
    Ok(())
}

fn native_vulkan_scene_binary_ingest_validate_record_range(
    kind: SceneBinaryChunkKind,
    first_record: u32,
    record_count: u32,
    chunk_record_count: u32,
) -> Result<(), SceneBinaryError> {
    if record_count == 0 {
        return Ok(());
    }
    let end_record =
        first_record
            .checked_add(record_count)
            .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count,
            })?;
    if end_record > chunk_record_count {
        return Err(SceneBinaryError::RecordRangeOutOfBounds {
            kind,
            first_record,
            record_count,
            chunk_record_count,
        });
    }
    Ok(())
}

fn native_vulkan_scene_binary_ingest_chunk_record_count(
    layout: &SceneBinaryLayoutPlan,
    kind: SceneBinaryChunkKind,
) -> Result<u32, SceneBinaryError> {
    layout
        .chunk(kind)
        .map(|chunk| chunk.record_count)
        .ok_or(SceneBinaryError::MissingChunk { kind })
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Result as IoResult, Seek, SeekFrom};

    use serde_json::json;

    use super::*;
    use crate::core::scene::SceneDocument;
    use crate::core::scene::binary::{
        SCENE_BINARY_GEOMETRY_RECORD_SIZE, SCENE_BINARY_NODE_RECORD_SIZE,
        SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE, scene_binary_payloads_from_document,
    };

    fn binary_ingest_test_bytes() -> Vec<u8> {
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
                                    "effect_uv_transform": {
                                        "mapping": "texture-resolution",
                                        "source_slot": 0,
                                        "mask_slot": 1,
                                        "scale": [1.0, 1.0],
                                        "offset": [0.0, 0.0],
                                        "input_extent": { "width": 64, "height": 64 },
                                        "mask_extent": { "width": 64, "height": 64 },
                                        "mask_backing_extent": { "width": 64, "height": 64 }
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
                    "id": "mesh-x",
                    "target_node": "mesh-node",
                    "channels": [
                        {
                            "property": "x",
                            "keyframes": [
                                { "time_ms": 0, "value": 0.0 },
                                { "time_ms": 500, "value": 3.0, "curve": "ease-in" }
                            ]
                        }
                    ]
                }
            ]
        }))
        .expect("scene document");
        scene_binary_payloads_from_document(&document)
            .encode_container(0x80)
            .expect("binary scene")
    }

    struct RecordBoundReadCursor {
        inner: Cursor<Vec<u8>>,
        max_read_len: usize,
    }

    impl Read for RecordBoundReadCursor {
        fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
            assert!(
                buf.len() <= self.max_read_len,
                "binary stream read buffer {} exceeded {}",
                buf.len(),
                self.max_read_len
            );
            self.inner.read(buf)
        }
    }

    impl Seek for RecordBoundReadCursor {
        fn seek(&mut self, pos: SeekFrom) -> IoResult<u64> {
            self.inner.seek(pos)
        }
    }

    #[test]
    fn binary_ingest_streams_scene_chunks_without_retaining_record_tables() {
        let bytes = binary_ingest_test_bytes();

        let ingest =
            native_vulkan_scene_binary_ingest_from_container(&bytes).expect("binary ingest");

        assert_eq!(ingest.feature_flags, 0x80);
        assert_eq!(ingest.resource_count, 2);
        assert_eq!(ingest.node_count, 1);
        assert_eq!(ingest.draw_record_count, 1);
        assert_eq!(ingest.transform_timeline_count, 2);
        assert_eq!(ingest.transform_keyframe_count, 2);
        assert_eq!(ingest.mesh_vertex_count, 3);
        assert_eq!(ingest.mesh_index_count, 3);
        assert_eq!(ingest.texture_slot_count, 2);
        assert_eq!(ingest.material_pass_count, 1);
        assert_eq!(ingest.effect_pass_count, 1);
        assert_eq!(ingest.effect_uv_transform_count, 1);
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
        assert_eq!(ingest.retained.effect_uv_transform_count, 1);
        assert_eq!(ingest.retained.puppet_count, 1);
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

    #[test]
    fn binary_ingest_reader_keeps_record_sized_read_boundary() {
        let bytes = binary_ingest_test_bytes();
        let container_ingest =
            native_vulkan_scene_binary_ingest_from_container(&bytes).expect("container ingest");
        let mut reader = RecordBoundReadCursor {
            inner: Cursor::new(bytes),
            max_read_len: SCENE_BINARY_GEOMETRY_RECORD_SIZE
                .max(SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE)
                .max(SCENE_BINARY_NODE_RECORD_SIZE),
        };

        let stream_ingest =
            native_vulkan_scene_binary_ingest_from_reader(&mut reader).expect("reader ingest");

        assert_eq!(stream_ingest, container_ingest);
        assert_eq!(stream_ingest.mesh_vertex_stream_bytes, 60);
        assert_eq!(stream_ingest.mesh_index_stream_bytes, 12);
    }
}

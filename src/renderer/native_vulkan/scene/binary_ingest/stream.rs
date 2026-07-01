use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom};

use crate::core::scene::binary::{
    SCENE_BINARY_ALIGNMENT, SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE,
    SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE, SCENE_BINARY_EFFECT_PASS_RECORD_SIZE,
    SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE, SCENE_BINARY_ENDIAN_LITTLE,
    SCENE_BINARY_FLUTTER_STATE_RECORD_SIZE, SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
    SCENE_BINARY_GEOMETRY_RECORD_SIZE, SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
    SCENE_BINARY_HEADER_SIZE, SCENE_BINARY_MAGIC, SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE,
    SCENE_BINARY_NODE_RECORD_SIZE, SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE,
    SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE, SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE,
    SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE, SCENE_BINARY_PUPPET_RECORD_SIZE,
    SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE, SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE,
    SCENE_BINARY_RENDER_STATE_RECORD_SIZE, SCENE_BINARY_RESOURCE_RECORD_SIZE,
    SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE, SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
    SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE, SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
    SCENE_BINARY_VERSION, SceneBinaryChunkDescriptor, SceneBinaryChunkKind, SceneBinaryError,
    SceneBinaryLayoutPlan, decode_effect_parameter_record, decode_geometry_record,
    decode_node_record, decode_puppet_record, decode_retained_gpu_state_record,
    decode_transform_keyframe_record, decode_transform_timeline_record,
};

use super::{
    NativeVulkanSceneBinaryIngestSummary, native_vulkan_scene_binary_ingest_debug_name_chunk,
    native_vulkan_scene_binary_ingest_effect_parameter_record,
    native_vulkan_scene_binary_ingest_geometry_record,
    native_vulkan_scene_binary_ingest_node_record, native_vulkan_scene_binary_ingest_puppet_record,
    native_vulkan_scene_binary_ingest_retained_record,
    native_vulkan_scene_binary_ingest_transform_record,
    native_vulkan_scene_binary_ingest_validate_record_payload, scene_binary_ingest_usize_for_error,
};

pub(in crate::renderer::native_vulkan::scene) fn native_vulkan_scene_binary_ingest_from_reader<
    R: Read + Seek,
>(
    reader: &mut R,
) -> Result<NativeVulkanSceneBinaryIngestSummary, SceneBinaryError> {
    let stream_len = native_vulkan_scene_binary_reader_len(reader)?;
    let layout = native_vulkan_scene_binary_layout_from_reader(reader, stream_len)?;
    let mut summary = NativeVulkanSceneBinaryIngestSummary {
        feature_flags: layout.feature_flags,
        chunk_count: layout.chunks.len().min(u32::MAX as usize) as u32,
        ..Default::default()
    };
    let geometry_vertex_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::GeometryVertices)?
            .record_count;
    let geometry_index_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::GeometryIndices)?
            .record_count;
    let node_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::NodeTable)?
            .record_count;
    let transform_timeline_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::TransformTimeline)?
            .record_count;
    let transform_keyframe_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::TransformKeyframes)?
            .record_count;
    let puppet_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::Puppet)?
            .record_count;
    let puppet_skin_bone_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::PuppetSkinBones)?
            .record_count;
    let puppet_skin_vertex_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::PuppetSkinVertices)?
            .record_count;
    let puppet_attachment_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::PuppetAttachments)?
            .record_count;
    let puppet_clip_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::PuppetClips)?
            .record_count;
    let puppet_frame_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::PuppetFrames)?
            .record_count;
    let puppet_layer_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::PuppetLayers)?
            .record_count;
    let material_pass_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::MaterialPass)?
            .record_count;
    let geometry_record_count =
        native_vulkan_scene_binary_stream_chunk(&layout, SceneBinaryChunkKind::Geometry)?
            .record_count;

    for descriptor in &layout.chunks {
        match descriptor.kind {
            SceneBinaryChunkKind::ResourceTable => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_RESOURCE_RECORD_SIZE,
                )?;
                summary.resource_count = descriptor.record_count;
            }
            SceneBinaryChunkKind::NodeTable => {
                let mut node_index = 0u32;
                native_vulkan_scene_binary_stream_records(
                    reader,
                    descriptor,
                    SCENE_BINARY_NODE_RECORD_SIZE,
                    |bytes| {
                        let node = decode_node_record(bytes)?;
                        native_vulkan_scene_binary_ingest_node_record(
                            &mut summary,
                            node,
                            node_index,
                            node_record_count,
                            transform_timeline_record_count,
                            puppet_record_count,
                            material_pass_record_count,
                            geometry_record_count,
                        )?;
                        node_index = node_index.saturating_add(1);
                        Ok(())
                    },
                )?;
            }
            SceneBinaryChunkKind::TransformTimeline => {
                native_vulkan_scene_binary_stream_records(
                    reader,
                    descriptor,
                    SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
                    |bytes| {
                        let transform = decode_transform_timeline_record(bytes)?;
                        native_vulkan_scene_binary_ingest_transform_record(
                            &mut summary,
                            transform,
                            transform_keyframe_record_count,
                        )
                    },
                )?;
            }
            SceneBinaryChunkKind::TransformKeyframes => {
                native_vulkan_scene_binary_stream_records(
                    reader,
                    descriptor,
                    SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE,
                    |bytes| {
                        let _ = decode_transform_keyframe_record(bytes)?;
                        summary.transform_keyframe_count =
                            summary.transform_keyframe_count.saturating_add(1);
                        Ok(())
                    },
                )?;
            }
            SceneBinaryChunkKind::Geometry => {
                native_vulkan_scene_binary_stream_records(
                    reader,
                    descriptor,
                    SCENE_BINARY_GEOMETRY_RECORD_SIZE,
                    |bytes| {
                        let geometry = decode_geometry_record(bytes)?;
                        native_vulkan_scene_binary_ingest_geometry_record(
                            &mut summary,
                            geometry,
                            geometry_vertex_record_count,
                            geometry_index_record_count,
                        )
                    },
                )?;
            }
            SceneBinaryChunkKind::GeometryVertices => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
                )?;
            }
            SceneBinaryChunkKind::GeometryIndices => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
                )?;
            }
            SceneBinaryChunkKind::TextureSlots => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
                )?;
                summary.texture_slot_count = descriptor.record_count;
            }
            SceneBinaryChunkKind::MaterialPass => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE,
                )?;
                summary.material_pass_count = descriptor.record_count;
            }
            SceneBinaryChunkKind::EffectPass => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_EFFECT_PASS_RECORD_SIZE,
                )?;
                summary.effect_pass_count = descriptor.record_count;
            }
            SceneBinaryChunkKind::EffectUvTransform => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE,
                )?;
                summary.effect_uv_transform_count = descriptor.record_count;
            }
            SceneBinaryChunkKind::EffectParameter => {
                native_vulkan_scene_binary_stream_records(
                    reader,
                    descriptor,
                    SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
                    |bytes| {
                        let parameter = decode_effect_parameter_record(bytes)?;
                        native_vulkan_scene_binary_ingest_effect_parameter_record(
                            &mut summary,
                            parameter,
                        );
                        Ok(())
                    },
                )?;
            }
            SceneBinaryChunkKind::FlutterState => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_FLUTTER_STATE_RECORD_SIZE,
                )?;
                summary.flutter_state_count = descriptor.record_count;
            }
            SceneBinaryChunkKind::Puppet => {
                native_vulkan_scene_binary_stream_records(
                    reader,
                    descriptor,
                    SCENE_BINARY_PUPPET_RECORD_SIZE,
                    |bytes| {
                        let puppet = decode_puppet_record(bytes)?;
                        native_vulkan_scene_binary_ingest_puppet_record(
                            &mut summary,
                            puppet,
                            puppet_skin_bone_record_count,
                            puppet_skin_vertex_record_count,
                            puppet_attachment_record_count,
                            puppet_clip_record_count,
                            puppet_frame_record_count,
                            puppet_layer_record_count,
                        )
                    },
                )?;
            }
            SceneBinaryChunkKind::PuppetSkinBones => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE,
                )?;
            }
            SceneBinaryChunkKind::PuppetSkinVertices => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE,
                )?;
            }
            SceneBinaryChunkKind::PuppetAttachments => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE,
                )?;
            }
            SceneBinaryChunkKind::PuppetClips => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE,
                )?;
            }
            SceneBinaryChunkKind::PuppetFrames => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE,
                )?;
            }
            SceneBinaryChunkKind::PuppetLayers => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE,
                )?;
            }
            SceneBinaryChunkKind::RenderState => {
                native_vulkan_scene_binary_ingest_validate_record_payload(
                    descriptor,
                    SCENE_BINARY_RENDER_STATE_RECORD_SIZE,
                )?;
                summary.render_state_count = descriptor.record_count;
            }
            SceneBinaryChunkKind::RetainedGpuState => {
                native_vulkan_scene_binary_stream_records(
                    reader,
                    descriptor,
                    SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE,
                    |bytes| {
                        let retained = decode_retained_gpu_state_record(bytes)?;
                        native_vulkan_scene_binary_ingest_retained_record(&mut summary, retained)
                    },
                )?;
            }
            SceneBinaryChunkKind::DebugNames => {
                native_vulkan_scene_binary_ingest_debug_name_chunk(&mut summary, descriptor)?;
            }
        }
    }

    summary.mesh_vertex_stream_bytes = u64::from(summary.mesh_vertex_count)
        .saturating_mul(SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE as u64);
    summary.mesh_index_stream_bytes = u64::from(summary.mesh_index_count)
        .saturating_mul(SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE as u64);
    Ok(summary)
}

fn native_vulkan_scene_binary_stream_records<R: Read + Seek>(
    reader: &mut R,
    descriptor: &SceneBinaryChunkDescriptor,
    record_size: usize,
    mut visit: impl FnMut(&[u8]) -> Result<(), SceneBinaryError>,
) -> Result<(), SceneBinaryError> {
    native_vulkan_scene_binary_ingest_validate_record_payload(descriptor, record_size)?;
    native_vulkan_scene_binary_seek(reader, descriptor.offset, "seek chunk")?;
    let mut record = vec![0; record_size];
    for _ in 0..descriptor.record_count {
        native_vulkan_scene_binary_read_exact(reader, &mut record, "read record")?;
        visit(&record)?;
    }
    Ok(())
}

fn native_vulkan_scene_binary_layout_from_reader<R: Read + Seek>(
    reader: &mut R,
    stream_len: u64,
) -> Result<SceneBinaryLayoutPlan, SceneBinaryError> {
    let mut header = [0; SCENE_BINARY_HEADER_SIZE];
    native_vulkan_scene_binary_read_exact_at(reader, 0, &mut header, "read header")?;
    let magic = [header[0], header[1], header[2], header[3]];
    if magic != SCENE_BINARY_MAGIC {
        return Err(SceneBinaryError::BadMagic { actual: magic });
    }
    let version = native_vulkan_scene_binary_read_u16(&header, 4);
    if version != SCENE_BINARY_VERSION {
        return Err(SceneBinaryError::UnsupportedVersion { version });
    }
    let endian = header[6];
    if endian != SCENE_BINARY_ENDIAN_LITTLE {
        return Err(SceneBinaryError::UnsupportedEndian { endian });
    }
    let alignment = header[7];
    if alignment != SCENE_BINARY_ALIGNMENT {
        return Err(SceneBinaryError::InvalidAlignment { alignment });
    }
    let feature_flags = native_vulkan_scene_binary_read_u32(&header, 8);
    let chunk_count = native_vulkan_scene_binary_read_u32(&header, 12);
    let expected_chunk_count = SceneBinaryChunkKind::REQUIRED_ORDER.len();
    if chunk_count as usize != expected_chunk_count {
        return Err(SceneBinaryError::RequiredChunkCount {
            expected: expected_chunk_count,
            actual: chunk_count as usize,
        });
    }
    let chunk_table_offset = native_vulkan_scene_binary_read_u64(&header, 16);
    let table_size = u64::from(chunk_count)
        .checked_mul(SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE as u64)
        .ok_or(SceneBinaryError::ChunkTableOutOfBounds {
            offset: chunk_table_offset,
            count: chunk_count,
            container_len: scene_binary_ingest_usize_for_error(stream_len),
        })?;
    let table_end = chunk_table_offset.checked_add(table_size).ok_or(
        SceneBinaryError::ChunkTableOutOfBounds {
            offset: chunk_table_offset,
            count: chunk_count,
            container_len: scene_binary_ingest_usize_for_error(stream_len),
        },
    )?;
    if table_end > stream_len {
        return Err(SceneBinaryError::ChunkTableOutOfBounds {
            offset: chunk_table_offset,
            count: chunk_count,
            container_len: scene_binary_ingest_usize_for_error(stream_len),
        });
    }

    let mut seen = BTreeSet::new();
    let mut chunks = Vec::with_capacity(expected_chunk_count);
    let mut descriptor_bytes = [0; SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE];
    for index in 0..expected_chunk_count {
        let descriptor_offset = chunk_table_offset
            .checked_add((index * SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE) as u64)
            .ok_or(SceneBinaryError::ChunkTableOutOfBounds {
                offset: chunk_table_offset,
                count: chunk_count,
                container_len: scene_binary_ingest_usize_for_error(stream_len),
            })?;
        native_vulkan_scene_binary_read_exact_at(
            reader,
            descriptor_offset,
            &mut descriptor_bytes,
            "read chunk descriptor",
        )?;
        let chunk = native_vulkan_scene_binary_decode_chunk_descriptor(&descriptor_bytes)?;
        let expected = SceneBinaryChunkKind::REQUIRED_ORDER[index];
        if chunk.kind != expected {
            return Err(SceneBinaryError::InvalidChunkOrder {
                index,
                expected,
                actual: chunk.kind,
            });
        }
        if !seen.insert(chunk.kind) {
            return Err(SceneBinaryError::DuplicateChunk { kind: chunk.kind });
        }
        native_vulkan_scene_binary_validate_stream_chunk_bounds(
            stream_len,
            alignment,
            table_end,
            chunks.last(),
            &chunk,
        )?;
        chunks.push(chunk);
    }

    Ok(SceneBinaryLayoutPlan {
        feature_flags,
        chunks,
    })
}

fn native_vulkan_scene_binary_stream_chunk(
    layout: &SceneBinaryLayoutPlan,
    kind: SceneBinaryChunkKind,
) -> Result<&SceneBinaryChunkDescriptor, SceneBinaryError> {
    layout
        .chunk(kind)
        .ok_or(SceneBinaryError::MissingChunk { kind })
}

fn native_vulkan_scene_binary_decode_chunk_descriptor(
    bytes: &[u8; SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE],
) -> Result<SceneBinaryChunkDescriptor, SceneBinaryError> {
    let code = native_vulkan_scene_binary_read_u32(bytes, 0);
    let kind =
        SceneBinaryChunkKind::from_code(code).ok_or(SceneBinaryError::UnknownChunk { code })?;
    Ok(SceneBinaryChunkDescriptor {
        kind,
        record_count: native_vulkan_scene_binary_read_u32(bytes, 4),
        offset: native_vulkan_scene_binary_read_u64(bytes, 8),
        length: native_vulkan_scene_binary_read_u64(bytes, 16),
    })
}

fn native_vulkan_scene_binary_validate_stream_chunk_bounds(
    stream_len: u64,
    alignment: u8,
    payload_min_offset: u64,
    previous: Option<&SceneBinaryChunkDescriptor>,
    chunk: &SceneBinaryChunkDescriptor,
) -> Result<(), SceneBinaryError> {
    if chunk.offset % u64::from(alignment) != 0 {
        return Err(SceneBinaryError::MisalignedChunk {
            kind: chunk.kind,
            offset: chunk.offset,
            alignment,
        });
    }
    let end = chunk
        .offset
        .checked_add(chunk.length)
        .ok_or(SceneBinaryError::ChunkOutOfBounds {
            kind: chunk.kind,
            offset: chunk.offset,
            length: chunk.length,
            container_len: scene_binary_ingest_usize_for_error(stream_len),
        })?;
    if chunk.offset < payload_min_offset || end > stream_len {
        return Err(SceneBinaryError::ChunkOutOfBounds {
            kind: chunk.kind,
            offset: chunk.offset,
            length: chunk.length,
            container_len: scene_binary_ingest_usize_for_error(stream_len),
        });
    }
    if let Some(previous) = previous {
        let previous_end = previous.offset.checked_add(previous.length).ok_or(
            SceneBinaryError::ChunkOutOfBounds {
                kind: previous.kind,
                offset: previous.offset,
                length: previous.length,
                container_len: scene_binary_ingest_usize_for_error(stream_len),
            },
        )?;
        if chunk.offset < previous_end {
            return Err(SceneBinaryError::ChunkOverlap {
                previous: previous.kind,
                current: chunk.kind,
            });
        }
    }
    Ok(())
}

fn native_vulkan_scene_binary_read_exact_at<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    bytes: &mut [u8],
    operation: &'static str,
) -> Result<(), SceneBinaryError> {
    native_vulkan_scene_binary_seek(reader, offset, operation)?;
    native_vulkan_scene_binary_read_exact(reader, bytes, operation)
}

fn native_vulkan_scene_binary_reader_len<R: Seek>(reader: &mut R) -> Result<u64, SceneBinaryError> {
    let current = reader
        .stream_position()
        .map_err(|err| native_vulkan_scene_binary_io_error("read stream position", err))?;
    let len = reader
        .seek(SeekFrom::End(0))
        .map_err(|err| native_vulkan_scene_binary_io_error("seek stream end", err))?;
    reader
        .seek(SeekFrom::Start(current))
        .map_err(|err| native_vulkan_scene_binary_io_error("restore stream position", err))?;
    Ok(len)
}

fn native_vulkan_scene_binary_seek<R: Seek>(
    reader: &mut R,
    offset: u64,
    operation: &'static str,
) -> Result<(), SceneBinaryError> {
    reader
        .seek(SeekFrom::Start(offset))
        .map(|_| ())
        .map_err(|err| native_vulkan_scene_binary_io_error(operation, err))
}

fn native_vulkan_scene_binary_read_exact<R: Read>(
    reader: &mut R,
    bytes: &mut [u8],
    operation: &'static str,
) -> Result<(), SceneBinaryError> {
    reader
        .read_exact(bytes)
        .map_err(|err| native_vulkan_scene_binary_io_error(operation, err))
}

fn native_vulkan_scene_binary_io_error(
    operation: &'static str,
    err: std::io::Error,
) -> SceneBinaryError {
    SceneBinaryError::StreamIo {
        operation,
        message: err.to_string(),
    }
}

fn native_vulkan_scene_binary_read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn native_vulkan_scene_binary_read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn native_vulkan_scene_binary_read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

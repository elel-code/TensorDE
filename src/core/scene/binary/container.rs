use std::collections::BTreeSet;

use super::*;

pub fn encode_scene_binary_container(
    feature_flags: u32,
    payloads: &[SceneBinaryChunkPayload<'_>],
) -> Result<Vec<u8>, SceneBinaryError> {
    validate_required_payload_order(payloads)?;
    let table_size = SceneBinaryChunkPayload::table_size(payloads.len());
    let mut next_payload_offset = align_usize(table_size, usize::from(SCENE_BINARY_ALIGNMENT));
    let mut descriptors = Vec::with_capacity(payloads.len());
    for payload in payloads {
        let offset = next_payload_offset;
        let length = payload.bytes.len();
        descriptors.push(SceneBinaryChunkDescriptor {
            kind: payload.kind,
            record_count: payload.record_count,
            offset: offset.min(u64::MAX as usize) as u64,
            length: length.min(u64::MAX as usize) as u64,
        });
        next_payload_offset = align_usize(
            offset
                .checked_add(length)
                .expect("scene binary payload size overflow"),
            usize::from(SCENE_BINARY_ALIGNMENT),
        );
    }

    let mut bytes = Vec::with_capacity(next_payload_offset);
    write_header(
        &mut bytes,
        feature_flags,
        payloads.len().min(u32::MAX as usize) as u32,
    );
    for descriptor in &descriptors {
        write_chunk_descriptor(&mut bytes, descriptor);
    }
    bytes.resize(
        descriptors
            .first()
            .map_or(table_size, |chunk| chunk.offset as usize),
        0,
    );
    for (descriptor, payload) in descriptors.iter().zip(payloads) {
        bytes.resize(descriptor.offset as usize, 0);
        bytes.extend_from_slice(payload.bytes);
        let aligned = align_usize(bytes.len(), usize::from(SCENE_BINARY_ALIGNMENT));
        bytes.resize(aligned, 0);
    }
    Ok(bytes)
}

pub fn decode_scene_binary_container(
    bytes: &[u8],
) -> Result<SceneBinaryLayoutPlan, SceneBinaryError> {
    decode_scene_binary_header_table(bytes, bytes.len())
}

pub fn decode_scene_binary_header_table(
    bytes: &[u8],
    container_len: usize,
) -> Result<SceneBinaryLayoutPlan, SceneBinaryError> {
    if bytes.len() < SCENE_BINARY_HEADER_SIZE {
        return Err(SceneBinaryError::BufferTooSmall {
            needed: SCENE_BINARY_HEADER_SIZE,
            actual: bytes.len(),
        });
    }
    let magic = read_array_4(bytes, 0)?;
    if magic != SCENE_BINARY_MAGIC {
        return Err(SceneBinaryError::BadMagic { actual: magic });
    }
    let version = read_u16(bytes, 4)?;
    let Some(required_order) = SceneBinaryChunkKind::required_order_for_version(version) else {
        return Err(SceneBinaryError::UnsupportedVersion { version });
    };
    let endian = bytes[6];
    if endian != SCENE_BINARY_ENDIAN_LITTLE {
        return Err(SceneBinaryError::UnsupportedEndian { endian });
    }
    let alignment = bytes[7];
    if alignment != SCENE_BINARY_ALIGNMENT {
        return Err(SceneBinaryError::InvalidAlignment { alignment });
    }
    let feature_flags = read_u32(bytes, 8)?;
    let chunk_count = read_u32(bytes, 12)?;
    let chunk_table_offset = read_u64(bytes, 16)?;
    let table_start = usize::try_from(chunk_table_offset).map_err(|_| {
        SceneBinaryError::ChunkTableOutOfBounds {
            offset: chunk_table_offset,
            count: chunk_count,
            container_len,
        }
    })?;
    let table_size = usize::try_from(chunk_count)
        .ok()
        .and_then(|count| count.checked_mul(SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE))
        .ok_or(SceneBinaryError::ChunkTableOutOfBounds {
            offset: chunk_table_offset,
            count: chunk_count,
            container_len,
        })?;
    let table_end =
        table_start
            .checked_add(table_size)
            .ok_or(SceneBinaryError::ChunkTableOutOfBounds {
                offset: chunk_table_offset,
                count: chunk_count,
                container_len,
            })?;
    if table_end > bytes.len() || table_end > container_len {
        return Err(SceneBinaryError::ChunkTableOutOfBounds {
            offset: chunk_table_offset,
            count: chunk_count,
            container_len,
        });
    }

    let mut seen = BTreeSet::new();
    let mut chunks = Vec::with_capacity(chunk_count as usize);
    for index in 0..chunk_count as usize {
        let descriptor_offset = table_start + index * SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE;
        let chunk = read_chunk_descriptor(bytes, descriptor_offset)?;
        let expected =
            required_order
                .get(index)
                .copied()
                .ok_or(SceneBinaryError::InvalidChunkOrder {
                    index,
                    expected: SceneBinaryChunkKind::DebugNames,
                    actual: chunk.kind,
                })?;
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
        validate_chunk_bounds(container_len, alignment, table_end, chunks.last(), &chunk)?;
        chunks.push(chunk);
    }
    if chunks.len() != required_order.len() {
        return Err(SceneBinaryError::RequiredChunkCount {
            expected: required_order.len(),
            actual: chunks.len(),
        });
    }
    Ok(SceneBinaryLayoutPlan {
        version,
        feature_flags,
        chunks,
    })
}

pub fn scene_binary_empty_payloads_for_shape(
    shape: SceneBinaryDocumentShape,
) -> Vec<SceneBinaryChunkPayload<'static>> {
    SceneBinaryChunkKind::REQUIRED_ORDER
        .into_iter()
        .map(|kind| SceneBinaryChunkPayload {
            kind,
            record_count: shape.record_count(kind),
            bytes: &[],
        })
        .collect()
}

fn validate_required_payload_order(
    payloads: &[SceneBinaryChunkPayload<'_>],
) -> Result<(), SceneBinaryError> {
    if payloads.len() != SceneBinaryChunkKind::REQUIRED_ORDER.len() {
        return Err(SceneBinaryError::RequiredChunkCount {
            expected: SceneBinaryChunkKind::REQUIRED_ORDER.len(),
            actual: payloads.len(),
        });
    }
    let mut seen = BTreeSet::new();
    for (index, payload) in payloads.iter().enumerate() {
        let expected = SceneBinaryChunkKind::REQUIRED_ORDER[index];
        if payload.kind != expected {
            return Err(SceneBinaryError::InvalidChunkOrder {
                index,
                expected,
                actual: payload.kind,
            });
        }
        if !seen.insert(payload.kind) {
            return Err(SceneBinaryError::DuplicateChunk { kind: payload.kind });
        }
    }
    Ok(())
}

fn write_header(bytes: &mut Vec<u8>, feature_flags: u32, chunk_count: u32) {
    bytes.extend_from_slice(&SCENE_BINARY_MAGIC);
    bytes.extend_from_slice(&SCENE_BINARY_VERSION.to_le_bytes());
    bytes.push(SCENE_BINARY_ENDIAN_LITTLE);
    bytes.push(SCENE_BINARY_ALIGNMENT);
    bytes.extend_from_slice(&feature_flags.to_le_bytes());
    bytes.extend_from_slice(&chunk_count.to_le_bytes());
    bytes.extend_from_slice(&(SCENE_BINARY_HEADER_SIZE as u64).to_le_bytes());
}

pub(super) fn write_chunk_descriptor(bytes: &mut Vec<u8>, descriptor: &SceneBinaryChunkDescriptor) {
    bytes.extend_from_slice(&descriptor.kind.code().to_le_bytes());
    bytes.extend_from_slice(&descriptor.record_count.to_le_bytes());
    bytes.extend_from_slice(&descriptor.offset.to_le_bytes());
    bytes.extend_from_slice(&descriptor.length.to_le_bytes());
}

fn read_chunk_descriptor(
    bytes: &[u8],
    offset: usize,
) -> Result<SceneBinaryChunkDescriptor, SceneBinaryError> {
    let code = read_u32(bytes, offset)?;
    let kind =
        SceneBinaryChunkKind::from_code(code).ok_or(SceneBinaryError::UnknownChunk { code })?;
    Ok(SceneBinaryChunkDescriptor {
        kind,
        record_count: read_u32(bytes, offset + 4)?,
        offset: read_u64(bytes, offset + 8)?,
        length: read_u64(bytes, offset + 16)?,
    })
}

fn validate_chunk_bounds(
    container_len: usize,
    alignment: u8,
    payload_min_offset: usize,
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
    let start = usize::try_from(chunk.offset).map_err(|_| SceneBinaryError::ChunkOutOfBounds {
        kind: chunk.kind,
        offset: chunk.offset,
        length: chunk.length,
        container_len,
    })?;
    let length = usize::try_from(chunk.length).map_err(|_| SceneBinaryError::ChunkOutOfBounds {
        kind: chunk.kind,
        offset: chunk.offset,
        length: chunk.length,
        container_len,
    })?;
    let end = start
        .checked_add(length)
        .ok_or(SceneBinaryError::ChunkOutOfBounds {
            kind: chunk.kind,
            offset: chunk.offset,
            length: chunk.length,
            container_len,
        })?;
    if start < payload_min_offset || end > container_len {
        return Err(SceneBinaryError::ChunkOutOfBounds {
            kind: chunk.kind,
            offset: chunk.offset,
            length: chunk.length,
            container_len,
        });
    }
    if let Some(previous) = previous {
        let previous_end = usize::try_from(previous.offset)
            .ok()
            .and_then(|offset| offset.checked_add(usize::try_from(previous.length).ok()?))
            .ok_or(SceneBinaryError::ChunkOutOfBounds {
                kind: previous.kind,
                offset: previous.offset,
                length: previous.length,
                container_len,
            })?;
        if start < previous_end {
            return Err(SceneBinaryError::ChunkOverlap {
                previous: previous.kind,
                current: chunk.kind,
            });
        }
    }
    Ok(())
}

fn read_array_4(bytes: &[u8], offset: usize) -> Result<[u8; 4], SceneBinaryError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or(SceneBinaryError::BufferTooSmall {
            needed: offset + 4,
            actual: bytes.len(),
        })?;
    Ok([slice[0], slice[1], slice[2], slice[3]])
}

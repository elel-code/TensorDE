//! Bounded `.gscn` record range streaming.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use crate::core::scene::binary::{SceneBinaryChunkKind, SceneBinaryError, SceneBinaryLayoutPlan};
use crate::renderer::RendererPlanError;

use crate::renderer::scene_binary::binary_plan_error;

const BINARY_SCENE_RECORD_STREAM_BYTES: usize = 64 * 1024;

pub(super) fn binary_scene_read_record_range<T>(
    file: &mut File,
    file_len: usize,
    layout: &SceneBinaryLayoutPlan,
    kind: SceneBinaryChunkKind,
    record_size: usize,
    first_record: u32,
    record_count: u32,
    decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
) -> Result<Vec<T>, RendererPlanError> {
    let descriptor = layout
        .chunk(kind)
        .cloned()
        .ok_or_else(|| binary_plan_error(SceneBinaryError::MissingChunk { kind }))?;
    if record_size == 0 {
        return Err(binary_plan_error(SceneBinaryError::InvalidRecordPayload {
            kind,
            record_size,
            record_count,
            length: usize::try_from(descriptor.length).unwrap_or(usize::MAX),
        }));
    }
    if record_count == 0 {
        return Ok(Vec::new());
    }
    let first = usize::try_from(first_record).map_err(|_| {
        binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
            kind,
            first_record,
            record_count,
            chunk_record_count: descriptor.record_count,
        })
    })?;
    let count = usize::try_from(record_count).map_err(|_| {
        binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
            kind,
            first_record,
            record_count,
            chunk_record_count: descriptor.record_count,
        })
    })?;
    let end_record = first.checked_add(count).ok_or_else(|| {
        binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
            kind,
            first_record,
            record_count,
            chunk_record_count: descriptor.record_count,
        })
    })?;
    if end_record > descriptor.record_count as usize {
        return Err(binary_plan_error(
            SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            },
        ));
    }
    let byte_offset = first.checked_mul(record_size).ok_or_else(|| {
        binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
            kind,
            first_record,
            record_count,
            chunk_record_count: descriptor.record_count,
        })
    })?;
    let byte_len = count.checked_mul(record_size).ok_or_else(|| {
        binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
            kind,
            first_record,
            record_count,
            chunk_record_count: descriptor.record_count,
        })
    })?;
    let end_offset = byte_offset.checked_add(byte_len).ok_or_else(|| {
        binary_plan_error(SceneBinaryError::InvalidRecordPayload {
            kind,
            record_size,
            record_count,
            length: usize::try_from(descriptor.length).unwrap_or(usize::MAX),
        })
    })?;
    let descriptor_len = usize::try_from(descriptor.length).map_err(|_| {
        binary_plan_error(SceneBinaryError::ChunkOutOfBounds {
            kind,
            offset: descriptor.offset,
            length: descriptor.length,
            container_len: file_len,
        })
    })?;
    if end_offset > descriptor_len {
        return Err(binary_plan_error(SceneBinaryError::InvalidRecordPayload {
            kind,
            record_size,
            record_count,
            length: descriptor_len,
        }));
    }
    let absolute_offset = descriptor
        .offset
        .checked_add(byte_offset as u64)
        .ok_or_else(|| {
            binary_plan_error(SceneBinaryError::ChunkOutOfBounds {
                kind,
                offset: descriptor.offset,
                length: descriptor.length,
                container_len: file_len,
            })
        })?;
    binary_scene_stream_records(
        file,
        absolute_offset,
        byte_len,
        record_size,
        record_count,
        descriptor_len,
        kind,
        decode,
    )
}

fn binary_scene_stream_records<T>(
    file: &mut File,
    absolute_offset: u64,
    byte_len: usize,
    record_size: usize,
    record_count: u32,
    descriptor_len: usize,
    kind: SceneBinaryChunkKind,
    decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
) -> Result<Vec<T>, RendererPlanError> {
    let mut records = Vec::with_capacity(record_count as usize);
    if byte_len == 0 {
        return Ok(records);
    }
    file.seek(SeekFrom::Start(absolute_offset)).map_err(|err| {
        binary_plan_error(SceneBinaryError::StreamIo {
            operation: "seek",
            message: err.to_string(),
        })
    })?;
    let stream_bytes = byte_len.min(BINARY_SCENE_RECORD_STREAM_BYTES);
    let records_per_read = (stream_bytes / record_size).max(1);
    let mut buffer = vec![0; records_per_read.saturating_mul(record_size)];
    let mut remaining_records = record_count as usize;
    while remaining_records > 0 {
        let read_records = remaining_records.min(records_per_read);
        let read_len = read_records.checked_mul(record_size).ok_or_else(|| {
            binary_plan_error(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count,
                length: descriptor_len,
            })
        })?;
        file.read_exact(&mut buffer[..read_len]).map_err(|err| {
            binary_plan_error(SceneBinaryError::StreamIo {
                operation: "read",
                message: err.to_string(),
            })
        })?;
        for chunk in buffer[..read_len].chunks_exact(record_size) {
            records.push(decode(chunk).map_err(binary_plan_error)?);
        }
        remaining_records -= read_records;
    }
    Ok(records)
}

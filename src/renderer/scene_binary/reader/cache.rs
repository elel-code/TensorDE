//! Cached `.gscn` record indexing helpers.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`

use crate::core::scene::binary::{SceneBinaryChunkKind, SceneBinaryError};
use crate::renderer::RendererPlanError;

use crate::renderer::scene_binary::binary_plan_error;

pub(in crate::renderer::scene_binary) fn binary_scene_cached_record_at<T: Copy>(
    records: &[T],
    kind: SceneBinaryChunkKind,
    record_index: u32,
    chunk_record_count: usize,
) -> Result<T, RendererPlanError> {
    records.get(record_index as usize).copied().ok_or_else(|| {
        binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
            kind,
            first_record: record_index,
            record_count: 1,
            chunk_record_count: chunk_record_count.min(u32::MAX as usize) as u32,
        })
    })
}

pub(in crate::renderer::scene_binary) fn binary_scene_cached_record_slice<T>(
    records: &[T],
    kind: SceneBinaryChunkKind,
    first_record: u32,
    record_count: u32,
    chunk_record_count: usize,
) -> Result<&[T], RendererPlanError> {
    let first = usize::try_from(first_record).map_err(|_| {
        binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
            kind,
            first_record,
            record_count,
            chunk_record_count: chunk_record_count.min(u32::MAX as usize) as u32,
        })
    })?;
    let count = usize::try_from(record_count).map_err(|_| {
        binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
            kind,
            first_record,
            record_count,
            chunk_record_count: chunk_record_count.min(u32::MAX as usize) as u32,
        })
    })?;
    let end = first.checked_add(count).ok_or_else(|| {
        binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
            kind,
            first_record,
            record_count,
            chunk_record_count: chunk_record_count.min(u32::MAX as usize) as u32,
        })
    })?;
    records.get(first..end).ok_or_else(|| {
        binary_plan_error(SceneBinaryError::RecordRangeOutOfBounds {
            kind,
            first_record,
            record_count,
            chunk_record_count: chunk_record_count.min(u32::MAX as usize) as u32,
        })
    })
}

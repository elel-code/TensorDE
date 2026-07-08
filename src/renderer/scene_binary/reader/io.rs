//! `.gscn` bounded file IO helpers.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use crate::core::scene::binary::SceneBinaryError;
use crate::renderer::RendererPlanError;

use crate::renderer::scene_binary::binary_plan_error;

pub(super) fn binary_scene_read_exact_at(
    file: &mut File,
    offset: u64,
    len: usize,
) -> Result<Vec<u8>, RendererPlanError> {
    file.seek(SeekFrom::Start(offset)).map_err(|err| {
        binary_plan_error(SceneBinaryError::StreamIo {
            operation: "seek",
            message: err.to_string(),
        })
    })?;
    let mut bytes = vec![0; len];
    file.read_exact(&mut bytes).map_err(|err| {
        binary_plan_error(SceneBinaryError::StreamIo {
            operation: "read",
            message: err.to_string(),
        })
    })?;
    Ok(bytes)
}

pub(super) fn binary_scene_read_u32(bytes: &[u8], offset: usize) -> Result<u32, SceneBinaryError> {
    let end = offset.saturating_add(4);
    let value = bytes
        .get(offset..end)
        .ok_or(SceneBinaryError::BufferTooSmall {
            needed: end,
            actual: bytes.len(),
        })?;
    Ok(u32::from_le_bytes(value.try_into().unwrap()))
}

pub(super) fn binary_scene_read_u64(bytes: &[u8], offset: usize) -> Result<u64, SceneBinaryError> {
    let end = offset.saturating_add(8);
    let value = bytes
        .get(offset..end)
        .ok_or(SceneBinaryError::BufferTooSmall {
            needed: end,
            actual: bytes.len(),
        })?;
    Ok(u64::from_le_bytes(value.try_into().unwrap()))
}

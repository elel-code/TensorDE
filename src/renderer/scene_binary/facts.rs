//! `.gscn` immutable record facts used by the engine ingest adapter.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `references/godot/servers/rendering/storage/`

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::core::SceneSize;
use crate::core::scene::binary::{
    SCENE_BINARY_DEBUG_NAME_RECORD_SIZE, SCENE_BINARY_NONE_ID,
    SCENE_BINARY_RENDER_STATE_RECORD_SIZE, SCENE_BINARY_RESOURCE_RECORD_SIZE,
    SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE, SceneBinaryChunkKind, SceneBinaryError,
    SceneBinaryResourceRecord, decode_debug_name_record, decode_puppet_record,
    decode_render_state_record, decode_resource_record, decode_transform_timeline_record,
};
use crate::engine::scene_engine::SceneTextureFormat;
use crate::renderer::RendererPlanError;

use super::binary_plan_error;
use super::reader::BinarySceneReader;
use super::texture::binary_scene_texture_metadata;

#[derive(Debug, Clone)]
pub(super) struct BinarySceneResource {
    pub(super) id_name: u32,
    pub(super) source: Option<PathBuf>,
    pub(super) original_source: Option<PathBuf>,
    pub(super) kind: u16,
    pub(super) role: Option<String>,
    pub(super) width: Option<u32>,
    pub(super) height: Option<u32>,
    pub(super) format: Option<SceneTextureFormat>,
    pub(super) mip_count: Option<u32>,
    pub(super) payload_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub(super) struct BinarySceneNames {
    names: Vec<Option<String>>,
}

impl BinarySceneNames {
    fn name(&self, id: u32) -> Option<&str> {
        if id == SCENE_BINARY_NONE_ID {
            return None;
        }
        self.names
            .get(id as usize)
            .and_then(|value| value.as_deref())
    }
}

pub(super) fn binary_scene_resources(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    package_root: &Path,
) -> Result<Vec<BinarySceneResource>, RendererPlanError> {
    let records = reader.records(
        SceneBinaryChunkKind::ResourceTable,
        SCENE_BINARY_RESOURCE_RECORD_SIZE,
        decode_resource_record,
    )?;
    let mut resources = Vec::with_capacity(records.len());
    for record in records {
        resources.push(binary_scene_resource(record, names, package_root)?);
    }
    Ok(resources)
}

fn binary_scene_resource(
    record: SceneBinaryResourceRecord,
    names: &BinarySceneNames,
    package_root: &Path,
) -> Result<BinarySceneResource, RendererPlanError> {
    let source = binary_name(names, record.source_name)
        .map(|source| binary_scene_resource_path(package_root, source));
    let original_source = binary_name(names, record.original_source_name)
        .map(|source| binary_scene_resource_path(package_root, source));
    let texture = source
        .as_ref()
        .map(|source| binary_scene_texture_metadata(source))
        .transpose()?
        .flatten();
    Ok(BinarySceneResource {
        id_name: record.id_name,
        source,
        original_source,
        kind: record.kind,
        role: binary_name(names, record.role_name).map(str::to_owned),
        width: texture
            .as_ref()
            .map(|texture| texture.width)
            .or_else(|| (record.width > 0).then_some(record.width)),
        height: texture
            .as_ref()
            .map(|texture| texture.height)
            .or_else(|| (record.height > 0).then_some(record.height)),
        format: texture.as_ref().map(|texture| texture.format),
        mip_count: texture.as_ref().map(|texture| texture.mip_count),
        payload_bytes: texture.as_ref().map(|texture| texture.payload_bytes),
    })
}

pub(super) fn binary_scene_names(
    reader: &mut BinarySceneReader,
) -> Result<BinarySceneNames, RendererPlanError> {
    let descriptor = reader
        .layout
        .chunk(SceneBinaryChunkKind::DebugNames)
        .cloned()
        .ok_or_else(|| {
            binary_plan_error(SceneBinaryError::MissingChunk {
                kind: SceneBinaryChunkKind::DebugNames,
            })
        })?;
    let payload = reader.chunk_payload(SceneBinaryChunkKind::DebugNames)?;
    let record_bytes = usize::try_from(descriptor.record_count)
        .ok()
        .and_then(|count| count.checked_mul(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE))
        .ok_or_else(|| {
            binary_plan_error(SceneBinaryError::InvalidRecordPayload {
                kind: SceneBinaryChunkKind::DebugNames,
                record_size: SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
                record_count: descriptor.record_count,
                length: payload.len(),
            })
        })?;
    if payload.len() < record_bytes {
        return Err(binary_plan_error(SceneBinaryError::InvalidRecordPayload {
            kind: SceneBinaryChunkKind::DebugNames,
            record_size: SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
            record_count: descriptor.record_count,
            length: payload.len(),
        }));
    }
    let (record_bytes, string_bytes) = payload.split_at(record_bytes);
    let mut names = Vec::<Option<String>>::new();
    for record in record_bytes.chunks_exact(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE) {
        let record = decode_debug_name_record(record).map_err(binary_plan_error)?;
        let start = usize::try_from(record.offset).map_err(|_| {
            binary_plan_error(SceneBinaryError::NameOutOfBounds {
                id: record.id,
                offset: record.offset,
                length: record.length,
                string_table_len: string_bytes.len(),
            })
        })?;
        let length = usize::try_from(record.length).map_err(|_| {
            binary_plan_error(SceneBinaryError::NameOutOfBounds {
                id: record.id,
                offset: record.offset,
                length: record.length,
                string_table_len: string_bytes.len(),
            })
        })?;
        let end = start.checked_add(length).ok_or_else(|| {
            binary_plan_error(SceneBinaryError::NameOutOfBounds {
                id: record.id,
                offset: record.offset,
                length: record.length,
                string_table_len: string_bytes.len(),
            })
        })?;
        let Some(bytes) = string_bytes.get(start..end) else {
            return Err(binary_plan_error(SceneBinaryError::NameOutOfBounds {
                id: record.id,
                offset: record.offset,
                length: record.length,
                string_table_len: string_bytes.len(),
            }));
        };
        let name = std::str::from_utf8(bytes)
            .map_err(|_| binary_plan_error(SceneBinaryError::InvalidNameUtf8 { id: record.id }))?;
        let id = record.id as usize;
        if names.len() <= id {
            names.resize_with(id + 1, || None);
        }
        names[id] = Some(name.to_owned());
    }
    Ok(BinarySceneNames { names })
}

pub(super) fn binary_scene_size(
    reader: &mut BinarySceneReader,
) -> Result<Option<SceneSize>, RendererPlanError> {
    let render_state = reader
        .records(
            SceneBinaryChunkKind::RenderState,
            SCENE_BINARY_RENDER_STATE_RECORD_SIZE,
            decode_render_state_record,
        )?
        .into_iter()
        .next();
    Ok(render_state.and_then(|state| {
        (state.width > 0 && state.height > 0).then_some(SceneSize {
            width: state.width,
            height: state.height,
        })
    }))
}

pub(super) fn binary_scene_timeline_counts(
    reader: &mut BinarySceneReader,
) -> Result<(usize, usize), RendererPlanError> {
    let records = reader.records(
        SceneBinaryChunkKind::TransformTimeline,
        SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
        decode_transform_timeline_record,
    )?;
    let mut channel_count = 0usize;
    let mut owner_names = BTreeSet::new();
    for record in records {
        if record.keyframe_count == 0 {
            continue;
        }
        channel_count = channel_count.saturating_add(1);
        owner_names.insert(record.owner_name);
    }
    Ok((channel_count, owner_names.len()))
}

pub(super) fn binary_scene_puppet_animation_layer_count(
    reader: &mut BinarySceneReader,
) -> Result<usize, RendererPlanError> {
    let records = reader.records(
        SceneBinaryChunkKind::Puppet,
        reader.layout_record_size(SceneBinaryChunkKind::Puppet)?,
        decode_puppet_record,
    )?;
    let mut count = 0usize;
    for record in records {
        count = count.saturating_add(record.animation_layer_count as usize);
    }
    Ok(count)
}

pub(super) fn binary_name(names: &BinarySceneNames, id: u32) -> Option<&str> {
    names.name(id)
}

pub(super) fn binary_scene_resource_path(package_root: &Path, source: &str) -> PathBuf {
    let source_path = Path::new(source);
    if source_path.is_absolute() {
        return source_path.to_path_buf();
    }
    package_root.join(source_path)
}

pub(super) fn binary_scene_package_root(source_path: &Path) -> PathBuf {
    let Some(parent) = source_path.parent() else {
        return PathBuf::from(".");
    };
    if parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "assets")
        && let Some(root) = parent.parent()
    {
        return root.to_path_buf();
    }
    parent.to_path_buf()
}

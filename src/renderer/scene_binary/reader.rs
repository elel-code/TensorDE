//! `.gscn` chunk reader and bounded record cache.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::core::scene::binary::{
    SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE, SCENE_BINARY_HEADER_SIZE, SceneBinaryChunkKind,
    SceneBinaryError, SceneBinaryGeometryRecord, SceneBinaryLayoutPlan,
    SceneBinaryMaterialPassRecord, SceneBinaryNodeRecord, SceneBinaryParticleEmitterRecord,
    SceneBinaryPuppetRecord, SceneBinaryTransformKeyframeRecord,
    SceneBinaryTransformTimelineRecord, decode_scene_binary_header_table,
};
use crate::core::scene::{
    SceneMesh, ScenePuppetAnimationClip, ScenePuppetAnimationLayer, ScenePuppetAttachmentDelta,
};
use crate::renderer::{RendererPlanError, SceneRenderImageEffectPass, SceneRenderTextureSlot};

use super::binary_plan_error;
use super::facts::binary_scene_package_root;

mod cache;
mod io;
mod record_stream;
mod records;

pub(super) use cache::binary_scene_cached_record_slice;
use io::{binary_scene_read_exact_at, binary_scene_read_u32, binary_scene_read_u64};
use record_stream::binary_scene_read_record_range;

pub(super) struct BinarySceneReader {
    file: File,
    file_len: usize,
    pub(super) package_root: PathBuf,
    pub(super) layout: SceneBinaryLayoutPlan,
    node_records_cache: Option<Arc<Vec<SceneBinaryNodeRecord>>>,
    geometry_records_cache: Option<Arc<Vec<SceneBinaryGeometryRecord>>>,
    material_records_cache: Option<Arc<Vec<SceneBinaryMaterialPassRecord>>>,
    particle_records_cache: Option<Arc<Vec<SceneBinaryParticleEmitterRecord>>>,
    puppet_records_cache: Option<Arc<Vec<SceneBinaryPuppetRecord>>>,
    transform_timeline_records_cache: Option<Arc<Vec<SceneBinaryTransformTimelineRecord>>>,
    transform_keyframe_records_cache: Option<Arc<Vec<SceneBinaryTransformKeyframeRecord>>>,
    pub(super) geometry_mesh_cache: BTreeMap<(u32, u32), Arc<SceneMesh>>,
    pub(super) material_texture_slots_cache: BTreeMap<u32, Arc<Vec<SceneRenderTextureSlot>>>,
    pub(super) material_effect_passes_cache: BTreeMap<u32, Arc<Vec<SceneRenderImageEffectPass>>>,
    pub(super) puppet_attachment_mesh_cache: BTreeMap<u32, Arc<SceneMesh>>,
    pub(super) puppet_attachment_delta_cache:
        BTreeMap<(u32, u64), Option<Arc<BTreeMap<String, ScenePuppetAttachmentDelta>>>>,
    pub(super) puppet_clips_cache: BTreeMap<u32, Arc<Vec<ScenePuppetAnimationClip>>>,
    pub(super) puppet_layers_cache: BTreeMap<u32, Arc<Vec<ScenePuppetAnimationLayer>>>,
}

impl BinarySceneReader {
    pub(super) fn open(path: &Path) -> Result<Self, RendererPlanError> {
        let mut file = File::open(path).map_err(|err| {
            RendererPlanError::PackageLoad(format!(
                "failed to open binary scene {}: {err}",
                path.display()
            ))
        })?;
        let file_len = usize::try_from(
            file.metadata()
                .map_err(|err| {
                    RendererPlanError::PackageLoad(format!(
                        "failed to stat binary scene {}: {err}",
                        path.display()
                    ))
                })?
                .len(),
        )
        .map_err(|_| {
            RendererPlanError::PackageLoad(format!(
                "binary scene {} is too large to address",
                path.display()
            ))
        })?;
        let header = binary_scene_read_exact_at(&mut file, 0, SCENE_BINARY_HEADER_SIZE)?;
        let chunk_count = binary_scene_read_u32(&header, 12).map_err(binary_plan_error)?;
        let chunk_table_offset = binary_scene_read_u64(&header, 16).map_err(binary_plan_error)?;
        let table_start = usize::try_from(chunk_table_offset).map_err(|_| {
            binary_plan_error(SceneBinaryError::ChunkTableOutOfBounds {
                offset: chunk_table_offset,
                count: chunk_count,
                container_len: file_len,
            })
        })?;
        let table_size = usize::try_from(chunk_count)
            .ok()
            .and_then(|count| count.checked_mul(SCENE_BINARY_CHUNK_DESCRIPTOR_SIZE))
            .ok_or_else(|| {
                binary_plan_error(SceneBinaryError::ChunkTableOutOfBounds {
                    offset: chunk_table_offset,
                    count: chunk_count,
                    container_len: file_len,
                })
            })?;
        let header_table_len = table_start.checked_add(table_size).ok_or_else(|| {
            binary_plan_error(SceneBinaryError::ChunkTableOutOfBounds {
                offset: chunk_table_offset,
                count: chunk_count,
                container_len: file_len,
            })
        })?;
        let header_table = if header_table_len == SCENE_BINARY_HEADER_SIZE {
            header
        } else {
            binary_scene_read_exact_at(&mut file, 0, header_table_len)?
        };
        let layout =
            decode_scene_binary_header_table(&header_table, file_len).map_err(binary_plan_error)?;
        Ok(Self {
            file,
            file_len,
            package_root: binary_scene_package_root(path),
            layout,
            node_records_cache: None,
            geometry_records_cache: None,
            material_records_cache: None,
            particle_records_cache: None,
            puppet_records_cache: None,
            transform_timeline_records_cache: None,
            transform_keyframe_records_cache: None,
            geometry_mesh_cache: BTreeMap::new(),
            material_texture_slots_cache: BTreeMap::new(),
            material_effect_passes_cache: BTreeMap::new(),
            puppet_attachment_mesh_cache: BTreeMap::new(),
            puppet_attachment_delta_cache: BTreeMap::new(),
            puppet_clips_cache: BTreeMap::new(),
            puppet_layers_cache: BTreeMap::new(),
        })
    }

    pub(super) fn chunk_count(&self, kind: SceneBinaryChunkKind) -> usize {
        self.layout
            .chunk(kind)
            .map_or(0, |chunk| chunk.record_count as usize)
    }

    pub(super) fn layout_record_size(
        &self,
        kind: SceneBinaryChunkKind,
    ) -> Result<usize, RendererPlanError> {
        self.layout
            .required_record_size(kind)
            .map_err(binary_plan_error)
    }

    pub(super) fn chunk_payload(
        &mut self,
        kind: SceneBinaryChunkKind,
    ) -> Result<Vec<u8>, RendererPlanError> {
        let descriptor = self
            .layout
            .chunk(kind)
            .ok_or_else(|| binary_plan_error(SceneBinaryError::MissingChunk { kind }))?;
        let length = usize::try_from(descriptor.length).map_err(|_| {
            binary_plan_error(SceneBinaryError::ChunkOutOfBounds {
                kind,
                offset: descriptor.offset,
                length: descriptor.length,
                container_len: self.file_len,
            })
        })?;
        binary_scene_read_exact_at(&mut self.file, descriptor.offset, length)
    }

    pub(super) fn records<T>(
        &mut self,
        kind: SceneBinaryChunkKind,
        record_size: usize,
        decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
    ) -> Result<Vec<T>, RendererPlanError> {
        let descriptor = self
            .layout
            .chunk(kind)
            .ok_or_else(|| binary_plan_error(SceneBinaryError::MissingChunk { kind }))?;
        self.record_range(kind, record_size, 0, descriptor.record_count, decode)
    }

    pub(super) fn record_range<T>(
        &mut self,
        kind: SceneBinaryChunkKind,
        record_size: usize,
        first_record: u32,
        record_count: u32,
        decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
    ) -> Result<Vec<T>, RendererPlanError> {
        binary_scene_read_record_range(
            &mut self.file,
            self.file_len,
            &self.layout,
            kind,
            record_size,
            first_record,
            record_count,
            decode,
        )
    }
}

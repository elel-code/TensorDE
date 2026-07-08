//! Typed `.gscn` record-cache accessors for `BinarySceneReader`.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`

use std::sync::Arc;

use crate::core::scene::binary::{
    SCENE_BINARY_GEOMETRY_RECORD_SIZE, SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE,
    SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE, SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
    SceneBinaryChunkKind, SceneBinaryGeometryRecord, SceneBinaryMaterialPassRecord,
    SceneBinaryNodeRecord, SceneBinaryParticleEmitterRecord, SceneBinaryPuppetRecord,
    SceneBinaryTransformKeyframeRecord, SceneBinaryTransformTimelineRecord, decode_geometry_record,
    decode_material_pass_record, decode_node_record, decode_particle_emitter_record,
    decode_puppet_record, decode_transform_keyframe_record, decode_transform_timeline_record,
};
use crate::renderer::RendererPlanError;

use super::BinarySceneReader;
use super::cache::binary_scene_cached_record_at;

impl BinarySceneReader {
    pub(in crate::renderer::scene_binary) fn node_records_cached(
        &mut self,
    ) -> Result<Arc<Vec<SceneBinaryNodeRecord>>, RendererPlanError> {
        if let Some(records) = self.node_records_cache.as_ref() {
            return Ok(Arc::clone(records));
        }
        let records = Arc::new(self.records(
            SceneBinaryChunkKind::NodeTable,
            self.layout_record_size(SceneBinaryChunkKind::NodeTable)?,
            decode_node_record,
        )?);
        self.node_records_cache = Some(Arc::clone(&records));
        Ok(records)
    }

    fn geometry_records_cached(
        &mut self,
    ) -> Result<Arc<Vec<SceneBinaryGeometryRecord>>, RendererPlanError> {
        if let Some(records) = self.geometry_records_cache.as_ref() {
            return Ok(Arc::clone(records));
        }
        let records = Arc::new(self.records(
            SceneBinaryChunkKind::Geometry,
            SCENE_BINARY_GEOMETRY_RECORD_SIZE,
            decode_geometry_record,
        )?);
        self.geometry_records_cache = Some(Arc::clone(&records));
        Ok(records)
    }

    fn material_records_cached(
        &mut self,
    ) -> Result<Arc<Vec<SceneBinaryMaterialPassRecord>>, RendererPlanError> {
        if let Some(records) = self.material_records_cache.as_ref() {
            return Ok(Arc::clone(records));
        }
        let records = Arc::new(self.records(
            SceneBinaryChunkKind::MaterialPass,
            self.layout_record_size(SceneBinaryChunkKind::MaterialPass)?,
            decode_material_pass_record,
        )?);
        self.material_records_cache = Some(Arc::clone(&records));
        Ok(records)
    }

    fn particle_records_cached(
        &mut self,
    ) -> Result<Arc<Vec<SceneBinaryParticleEmitterRecord>>, RendererPlanError> {
        if let Some(records) = self.particle_records_cache.as_ref() {
            return Ok(Arc::clone(records));
        }
        let records = Arc::new(self.records(
            SceneBinaryChunkKind::ParticleEmitter,
            SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE,
            decode_particle_emitter_record,
        )?);
        self.particle_records_cache = Some(Arc::clone(&records));
        Ok(records)
    }

    fn puppet_records_cached(
        &mut self,
    ) -> Result<Arc<Vec<SceneBinaryPuppetRecord>>, RendererPlanError> {
        if let Some(records) = self.puppet_records_cache.as_ref() {
            return Ok(Arc::clone(records));
        }
        let records = Arc::new(self.records(
            SceneBinaryChunkKind::Puppet,
            self.layout_record_size(SceneBinaryChunkKind::Puppet)?,
            decode_puppet_record,
        )?);
        self.puppet_records_cache = Some(Arc::clone(&records));
        Ok(records)
    }

    pub(in crate::renderer::scene_binary) fn transform_timeline_records_cached(
        &mut self,
    ) -> Result<Arc<Vec<SceneBinaryTransformTimelineRecord>>, RendererPlanError> {
        if let Some(records) = self.transform_timeline_records_cache.as_ref() {
            return Ok(Arc::clone(records));
        }
        let records = Arc::new(self.records(
            SceneBinaryChunkKind::TransformTimeline,
            SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
            decode_transform_timeline_record,
        )?);
        self.transform_timeline_records_cache = Some(Arc::clone(&records));
        Ok(records)
    }

    pub(in crate::renderer::scene_binary) fn transform_keyframe_records_cached(
        &mut self,
    ) -> Result<Arc<Vec<SceneBinaryTransformKeyframeRecord>>, RendererPlanError> {
        if let Some(records) = self.transform_keyframe_records_cache.as_ref() {
            return Ok(Arc::clone(records));
        }
        let records = Arc::new(self.records(
            SceneBinaryChunkKind::TransformKeyframes,
            SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE,
            decode_transform_keyframe_record,
        )?);
        self.transform_keyframe_records_cache = Some(Arc::clone(&records));
        Ok(records)
    }

    pub(in crate::renderer::scene_binary) fn geometry_record_cached(
        &mut self,
        record_index: u32,
    ) -> Result<SceneBinaryGeometryRecord, RendererPlanError> {
        let records = self.geometry_records_cached()?;
        binary_scene_cached_record_at(
            &records,
            SceneBinaryChunkKind::Geometry,
            record_index,
            self.chunk_count(SceneBinaryChunkKind::Geometry),
        )
    }

    pub(in crate::renderer::scene_binary) fn material_record_cached(
        &mut self,
        record_index: u32,
    ) -> Result<SceneBinaryMaterialPassRecord, RendererPlanError> {
        let records = self.material_records_cached()?;
        binary_scene_cached_record_at(
            &records,
            SceneBinaryChunkKind::MaterialPass,
            record_index,
            self.chunk_count(SceneBinaryChunkKind::MaterialPass),
        )
    }

    pub(in crate::renderer::scene_binary) fn particle_record_cached(
        &mut self,
        record_index: u32,
    ) -> Result<SceneBinaryParticleEmitterRecord, RendererPlanError> {
        let records = self.particle_records_cached()?;
        binary_scene_cached_record_at(
            &records,
            SceneBinaryChunkKind::ParticleEmitter,
            record_index,
            self.chunk_count(SceneBinaryChunkKind::ParticleEmitter),
        )
    }

    pub(in crate::renderer::scene_binary) fn puppet_record_cached(
        &mut self,
        record_index: u32,
    ) -> Result<SceneBinaryPuppetRecord, RendererPlanError> {
        let records = self.puppet_records_cached()?;
        binary_scene_cached_record_at(
            &records,
            SceneBinaryChunkKind::Puppet,
            record_index,
            self.chunk_count(SceneBinaryChunkKind::Puppet),
        )
    }
}

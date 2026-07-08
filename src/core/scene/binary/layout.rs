use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneBinaryLayoutPlan {
    pub version: u16,
    pub feature_flags: u32,
    pub chunks: Vec<SceneBinaryChunkDescriptor>,
}

impl SceneBinaryLayoutPlan {
    pub fn chunk(&self, kind: SceneBinaryChunkKind) -> Option<&SceneBinaryChunkDescriptor> {
        self.chunks.iter().find(|chunk| chunk.kind == kind)
    }

    pub fn record_size(&self, kind: SceneBinaryChunkKind) -> Option<usize> {
        kind.record_size_for_version(self.version)
    }

    pub fn required_record_size(
        &self,
        kind: SceneBinaryChunkKind,
    ) -> Result<usize, SceneBinaryError> {
        self.record_size(kind)
            .ok_or(SceneBinaryError::UnsupportedVersion {
                version: self.version,
            })
    }

    pub fn resource_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryResourceRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::ResourceTable,
            SCENE_BINARY_RESOURCE_RECORD_SIZE,
            decode_resource_record,
        )
    }

    pub fn node_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryNodeRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::NodeTable,
            self.required_record_size(SceneBinaryChunkKind::NodeTable)?,
            decode_node_record,
        )
    }

    pub fn node_record_at(
        &self,
        container: &[u8],
        record_index: u32,
    ) -> Result<SceneBinaryNodeRecord, SceneBinaryError> {
        self.record_at(
            container,
            SceneBinaryChunkKind::NodeTable,
            self.required_record_size(SceneBinaryChunkKind::NodeTable)?,
            record_index,
            decode_node_record,
        )
    }

    pub fn transform_timeline_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTransformTimelineRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::TransformTimeline,
            SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
            decode_transform_timeline_record,
        )
    }

    pub fn node_transform_records<'a>(
        &self,
        container: &'a [u8],
        node: SceneBinaryNodeRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTransformTimelineRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::TransformTimeline,
            SCENE_BINARY_TRANSFORM_TIMELINE_RECORD_SIZE,
            node.first_transform,
            node.transform_count,
            decode_transform_timeline_record,
        )
    }

    pub fn transform_keyframe_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTransformKeyframeRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::TransformKeyframes,
            SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE,
            decode_transform_keyframe_record,
        )
    }

    pub fn transform_keyframe_record_range<'a>(
        &self,
        container: &'a [u8],
        transform: SceneBinaryTransformTimelineRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTransformKeyframeRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            if transform.first_keyframe == SCENE_BINARY_NONE_ID && transform.keyframe_count == 0 {
                (0, 0)
            } else {
                (transform.first_keyframe, transform.keyframe_count)
            };
        self.records_range(
            container,
            SceneBinaryChunkKind::TransformKeyframes,
            SCENE_BINARY_TRANSFORM_KEYFRAME_RECORD_SIZE,
            first_record,
            record_count,
            decode_transform_keyframe_record,
        )
    }

    pub fn geometry_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryGeometryRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::Geometry,
            SCENE_BINARY_GEOMETRY_RECORD_SIZE,
            decode_geometry_record,
        )
    }

    pub fn geometry_record_at(
        &self,
        container: &[u8],
        record_index: u32,
    ) -> Result<SceneBinaryGeometryRecord, SceneBinaryError> {
        self.record_at(
            container,
            SceneBinaryChunkKind::Geometry,
            SCENE_BINARY_GEOMETRY_RECORD_SIZE,
            record_index,
            decode_geometry_record,
        )
    }

    pub fn geometry_vertex_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryGeometryVertexRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::GeometryVertices,
            SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
            decode_geometry_vertex_record,
        )
    }

    pub fn geometry_index_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryGeometryIndexRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::GeometryIndices,
            SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
            decode_geometry_index_record,
        )
    }

    pub fn geometry_vertex_record_range<'a>(
        &self,
        container: &'a [u8],
        geometry: SceneBinaryGeometryRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryGeometryVertexRecord>, SceneBinaryError> {
        let (first_record, record_count) = if geometry.first_vertex == SCENE_BINARY_NONE_ID {
            (0, 0)
        } else {
            (geometry.first_vertex, geometry.vertex_count)
        };
        self.records_range(
            container,
            SceneBinaryChunkKind::GeometryVertices,
            SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
            first_record,
            record_count,
            decode_geometry_vertex_record,
        )
    }

    pub fn geometry_index_record_range<'a>(
        &self,
        container: &'a [u8],
        geometry: SceneBinaryGeometryRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryGeometryIndexRecord>, SceneBinaryError> {
        let (first_record, record_count) = if geometry.first_index == SCENE_BINARY_NONE_ID {
            (0, 0)
        } else {
            (geometry.first_index, geometry.index_count)
        };
        self.records_range(
            container,
            SceneBinaryChunkKind::GeometryIndices,
            SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
            first_record,
            record_count,
            decode_geometry_index_record,
        )
    }

    pub fn particle_emitter_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryParticleEmitterRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::ParticleEmitter,
            SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE,
            decode_particle_emitter_record,
        )
    }

    pub fn particle_emitter_record_at(
        &self,
        container: &[u8],
        record_index: u32,
    ) -> Result<SceneBinaryParticleEmitterRecord, SceneBinaryError> {
        self.record_at(
            container,
            SceneBinaryChunkKind::ParticleEmitter,
            SCENE_BINARY_PARTICLE_EMITTER_RECORD_SIZE,
            record_index,
            decode_particle_emitter_record,
        )
    }

    pub fn texture_slot_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTextureSlotRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::TextureSlots,
            SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
            decode_texture_slot_record,
        )
    }

    pub fn material_texture_slot_records<'a>(
        &self,
        container: &'a [u8],
        material: SceneBinaryMaterialPassRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTextureSlotRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::TextureSlots,
            SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
            material.first_texture_slot,
            material.texture_slot_count,
            decode_texture_slot_record,
        )
    }

    pub fn material_effect_pass_records<'a>(
        &self,
        container: &'a [u8],
        material: SceneBinaryMaterialPassRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectPassRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::EffectPass,
            self.required_record_size(SceneBinaryChunkKind::EffectPass)?,
            material.first_effect_pass,
            material.effect_pass_count,
            decode_effect_pass_record,
        )
    }

    pub fn material_pass_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryMaterialPassRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::MaterialPass,
            SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE,
            decode_material_pass_record,
        )
    }

    pub fn material_pass_record_at(
        &self,
        container: &[u8],
        record_index: u32,
    ) -> Result<SceneBinaryMaterialPassRecord, SceneBinaryError> {
        self.record_at(
            container,
            SceneBinaryChunkKind::MaterialPass,
            SCENE_BINARY_MATERIAL_PASS_RECORD_SIZE,
            record_index,
            decode_material_pass_record,
        )
    }

    pub fn effect_pass_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectPassRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::EffectPass,
            self.required_record_size(SceneBinaryChunkKind::EffectPass)?,
            decode_effect_pass_record,
        )
    }

    pub fn effect_uv_transform_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectUvTransformRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::EffectUvTransform,
            SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE,
            decode_effect_uv_transform_record,
        )
    }

    pub fn effect_parameter_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectParameterRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::EffectParameter,
            SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
            decode_effect_parameter_record,
        )
    }

    pub fn effect_texture_slot_records<'a>(
        &self,
        container: &'a [u8],
        effect_pass: SceneBinaryEffectPassRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryTextureSlotRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::TextureSlots,
            SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
            effect_pass.first_texture_slot,
            effect_pass.texture_slot_count,
            decode_texture_slot_record,
        )
    }

    pub fn effect_parameter_record_range<'a>(
        &self,
        container: &'a [u8],
        effect_pass: SceneBinaryEffectPassRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectParameterRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::EffectParameter,
            SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
            effect_pass.first_parameter,
            effect_pass.parameter_count,
            decode_effect_parameter_record,
        )
    }

    pub fn effect_uv_transform_record_range<'a>(
        &self,
        container: &'a [u8],
        effect_pass: SceneBinaryEffectPassRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectUvTransformRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::EffectUvTransform,
            SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE,
            effect_pass.first_effect_uv_transform,
            effect_pass.effect_uv_transform_count,
            decode_effect_uv_transform_record,
        )
    }

    pub fn flutter_parameter_records<'a>(
        &self,
        container: &'a [u8],
        flutter: SceneBinaryFlutterStateRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryEffectParameterRecord>, SceneBinaryError> {
        self.records_range(
            container,
            SceneBinaryChunkKind::EffectParameter,
            SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
            flutter.first_parameter,
            flutter.parameter_count,
            decode_effect_parameter_record,
        )
    }

    pub fn flutter_state_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryFlutterStateRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::FlutterState,
            SCENE_BINARY_FLUTTER_STATE_RECORD_SIZE,
            decode_flutter_state_record,
        )
    }

    pub fn puppet_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::Puppet,
            self.required_record_size(SceneBinaryChunkKind::Puppet)?,
            decode_puppet_record,
        )
    }

    pub fn puppet_record_at(
        &self,
        container: &[u8],
        record_index: u32,
    ) -> Result<SceneBinaryPuppetRecord, SceneBinaryError> {
        self.record_at(
            container,
            SceneBinaryChunkKind::Puppet,
            self.required_record_size(SceneBinaryChunkKind::Puppet)?,
            record_index,
            decode_puppet_record,
        )
    }

    pub fn puppet_skin_bone_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetSkinBoneRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::PuppetSkinBones,
            SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE,
            decode_puppet_skin_bone_record,
        )
    }

    pub fn puppet_skin_bone_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetSkinBoneRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_bone, puppet.bone_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetSkinBones,
            SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_skin_bone_record,
        )
    }

    pub fn puppet_skin_vertex_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetSkinVertexRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_skin_vertex, puppet.skin_vertex_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetSkinVertices,
            SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_skin_vertex_record,
        )
    }

    pub fn puppet_attachment_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetAttachmentRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_attachment, puppet.attachment_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetAttachments,
            SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_attachment_record,
        )
    }

    pub fn puppet_clip_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetClipRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_clip, puppet.clip_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetClips,
            SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_clip_record,
        )
    }

    pub fn puppet_frame_record_range<'a>(
        &self,
        container: &'a [u8],
        clip: SceneBinaryPuppetClipRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetFrameRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(clip.first_frame, clip.frame_record_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetFrames,
            SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_frame_record,
        )
    }

    pub fn puppet_layer_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetLayerRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_layer, puppet.animation_layer_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetLayers,
            SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_layer_record,
        )
    }

    pub fn puppet_clipping_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetClippingRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_clipping_record, puppet.clipping_record_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetClipping,
            SCENE_BINARY_PUPPET_CLIPPING_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_clipping_record,
        )
    }

    pub fn puppet_clipping_bone_record_range<'a>(
        &self,
        container: &'a [u8],
        record: SceneBinaryPuppetClippingRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetClippingBoneRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(record.first_bone, record.bone_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetClippingBones,
            SCENE_BINARY_PUPPET_CLIPPING_BONE_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_clipping_bone_record,
        )
    }

    pub fn puppet_clipping_frame_key_record_range<'a>(
        &self,
        container: &'a [u8],
        record: SceneBinaryPuppetClippingRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetClippingFrameKeyRecord>, SceneBinaryError>
    {
        let (first_record, record_count) =
            binary_range_start_count(record.first_frame_key, record.frame_key_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetClippingFrameKeys,
            SCENE_BINARY_PUPPET_CLIPPING_FRAME_KEY_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_clipping_frame_key_record,
        )
    }

    pub fn puppet_active_source_record_range<'a>(
        &self,
        container: &'a [u8],
        puppet: SceneBinaryPuppetRecord,
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryPuppetActiveSourceRecord>, SceneBinaryError> {
        let (first_record, record_count) =
            binary_range_start_count(puppet.first_active_source, puppet.active_source_count);
        self.records_range(
            container,
            SceneBinaryChunkKind::PuppetActiveSources,
            SCENE_BINARY_PUPPET_ACTIVE_SOURCE_RECORD_SIZE,
            first_record,
            record_count,
            decode_puppet_active_source_record,
        )
    }

    pub fn render_state_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryRenderStateRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::RenderState,
            SCENE_BINARY_RENDER_STATE_RECORD_SIZE,
            decode_render_state_record,
        )
    }

    pub fn retained_gpu_state_records<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryRecords<'a, SceneBinaryRetainedGpuStateRecord>, SceneBinaryError> {
        self.records(
            container,
            SceneBinaryChunkKind::RetainedGpuState,
            SCENE_BINARY_RETAINED_GPU_STATE_RECORD_SIZE,
            decode_retained_gpu_state_record,
        )
    }

    pub fn debug_names<'a>(
        &self,
        container: &'a [u8],
    ) -> Result<SceneBinaryDebugNames<'a>, SceneBinaryError> {
        let descriptor =
            self.chunk(SceneBinaryChunkKind::DebugNames)
                .ok_or(SceneBinaryError::MissingChunk {
                    kind: SceneBinaryChunkKind::DebugNames,
                })?;
        let payload = descriptor.payload(container)?;
        SceneBinaryDebugNames::new(descriptor.record_count, payload)
    }

    fn records<'a, T>(
        &self,
        container: &'a [u8],
        kind: SceneBinaryChunkKind,
        record_size: usize,
        decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
    ) -> Result<SceneBinaryRecords<'a, T>, SceneBinaryError> {
        let descriptor = self
            .chunk(kind)
            .ok_or(SceneBinaryError::MissingChunk { kind })?;
        let payload = descriptor.payload(container)?;
        let expected = usize::try_from(descriptor.record_count)
            .ok()
            .and_then(|count| count.checked_mul(record_size))
            .ok_or(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            })?;
        if payload.len() != expected {
            return Err(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            });
        }
        Ok(SceneBinaryRecords {
            bytes: payload,
            record_size,
            index: 0,
            record_count: descriptor.record_count as usize,
            decode,
        })
    }

    fn record_at<T>(
        &self,
        container: &[u8],
        kind: SceneBinaryChunkKind,
        record_size: usize,
        record_index: u32,
        decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
    ) -> Result<T, SceneBinaryError> {
        let descriptor = self
            .chunk(kind)
            .ok_or(SceneBinaryError::MissingChunk { kind })?;
        let payload = descriptor.payload(container)?;
        let expected = usize::try_from(descriptor.record_count)
            .ok()
            .and_then(|count| count.checked_mul(record_size))
            .ok_or(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            })?;
        if payload.len() != expected {
            return Err(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            });
        }
        if record_index >= descriptor.record_count {
            return Err(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record: record_index,
                record_count: 1,
                chunk_record_count: descriptor.record_count,
            });
        }
        let start = usize::try_from(record_index)
            .ok()
            .and_then(|index| index.checked_mul(record_size))
            .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record: record_index,
                record_count: 1,
                chunk_record_count: descriptor.record_count,
            })?;
        let end =
            start
                .checked_add(record_size)
                .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                    kind,
                    first_record: record_index,
                    record_count: 1,
                    chunk_record_count: descriptor.record_count,
                })?;
        decode(&payload[start..end])
    }

    fn records_range<'a, T>(
        &self,
        container: &'a [u8],
        kind: SceneBinaryChunkKind,
        record_size: usize,
        first_record: u32,
        record_count: u32,
        decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
    ) -> Result<SceneBinaryRecords<'a, T>, SceneBinaryError> {
        let descriptor = self
            .chunk(kind)
            .ok_or(SceneBinaryError::MissingChunk { kind })?;
        let payload = descriptor.payload(container)?;
        let expected = usize::try_from(descriptor.record_count)
            .ok()
            .and_then(|count| count.checked_mul(record_size))
            .ok_or(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            })?;
        if payload.len() != expected {
            return Err(SceneBinaryError::InvalidRecordPayload {
                kind,
                record_size,
                record_count: descriptor.record_count,
                length: payload.len(),
            });
        }
        if record_count == 0 {
            return Ok(SceneBinaryRecords {
                bytes: &payload[0..0],
                record_size,
                index: 0,
                record_count: 0,
                decode,
            });
        }
        let first = usize::try_from(first_record).map_err(|_| {
            SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            }
        })?;
        let count = usize::try_from(record_count).map_err(|_| {
            SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            }
        })?;
        let end_record =
            first
                .checked_add(count)
                .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                    kind,
                    first_record,
                    record_count,
                    chunk_record_count: descriptor.record_count,
                })?;
        if end_record > descriptor.record_count as usize {
            return Err(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            });
        }
        let start =
            first
                .checked_mul(record_size)
                .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                    kind,
                    first_record,
                    record_count,
                    chunk_record_count: descriptor.record_count,
                })?;
        let byte_len =
            count
                .checked_mul(record_size)
                .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                    kind,
                    first_record,
                    record_count,
                    chunk_record_count: descriptor.record_count,
                })?;
        let end = start
            .checked_add(byte_len)
            .ok_or(SceneBinaryError::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count: descriptor.record_count,
            })?;
        Ok(SceneBinaryRecords {
            bytes: &payload[start..end],
            record_size,
            index: 0,
            record_count: count,
            decode,
        })
    }
}

pub struct SceneBinaryRecords<'a, T> {
    bytes: &'a [u8],
    record_size: usize,
    index: usize,
    record_count: usize,
    decode: fn(&[u8]) -> Result<T, SceneBinaryError>,
}

impl<T> SceneBinaryRecords<'_, T> {
    pub fn len(&self) -> usize {
        self.record_count.saturating_sub(self.index)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Iterator for SceneBinaryRecords<'_, T> {
    type Item = Result<T, SceneBinaryError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.record_count {
            return None;
        }
        let start = self.index.checked_mul(self.record_size)?;
        let end = start.checked_add(self.record_size)?;
        self.index += 1;
        Some((self.decode)(&self.bytes[start..end]))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<T> ExactSizeIterator for SceneBinaryRecords<'_, T> {}

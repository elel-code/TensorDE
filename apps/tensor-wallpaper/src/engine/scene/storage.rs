//! Scene storage for the new engine binary.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/scene-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/material-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/effect-format.md`
//! - `references/tensor-wallpaper/godot/servers/rendering/storage/*`
//! - `references/tensor-wallpaper/godot/servers/rendering/rendering_server_default.*`

use std::fmt;
use std::io::Read;

use super::abi::*;
use super::binary::{
    SceneBinaryDocument, SceneBinaryError, read_scene_binary, read_scene_binary_bytes,
};

pub(crate) mod texture_sequence;
mod validation;

use validation::validate_document;

#[derive(Debug, Clone, PartialEq)]
pub struct SceneStorage {
    document: SceneBinaryDocument,
}

impl SceneStorage {
    pub fn from_binary_reader(reader: impl Read) -> Result<Self, SceneStorageError> {
        Self::from_document(read_scene_binary(reader)?)
    }

    pub fn from_binary_bytes(bytes: &[u8]) -> Result<Self, SceneStorageError> {
        Self::from_document(read_scene_binary_bytes(bytes)?)
    }

    pub fn from_document(document: SceneBinaryDocument) -> Result<Self, SceneStorageError> {
        validate_document(&document).map_err(|error| error.with_document_context(&document))?;
        Ok(Self { document })
    }

    pub fn document(&self) -> &SceneBinaryDocument {
        &self.document
    }

    pub fn project(&self) -> &SceneProjectRecord {
        &self.document.project
    }

    pub fn strings(&self) -> &[String] {
        &self.document.strings
    }

    pub fn string(&self, id: SceneStringId) -> Option<&str> {
        if !id.is_some() {
            return None;
        }
        self.document.strings.get(id.0 as usize).map(String::as_str)
    }

    pub fn resources(&self) -> &[SceneResourceRecord] {
        &self.document.resources
    }

    pub fn resource(&self, id: SceneResourceId) -> Option<&SceneResourceRecord> {
        if !id.is_some() {
            return None;
        }
        self.document
            .resources
            .iter()
            .find(|record| record.id == id)
    }

    pub fn resource_payload(&self, resource: &SceneResourceRecord) -> Option<&[u8]> {
        let start = usize::try_from(resource.payload_offset).ok()?;
        let len = usize::try_from(resource.payload_len).ok()?;
        let end = start.checked_add(len)?;
        self.document.resource_payload.get(start..end)
    }

    pub fn textures(&self) -> &[SceneTextureRecord] {
        &self.document.textures
    }

    pub fn texture(&self, resource: SceneResourceId) -> Option<&SceneTextureRecord> {
        self.document
            .textures
            .iter()
            .find(|texture| texture.resource == resource)
    }

    pub fn texture_mips(&self, texture: &SceneTextureRecord) -> &[SceneTextureMipRecord] {
        let start = texture.mip_start as usize;
        let end = start.saturating_add(texture.mip_count as usize);
        self.document
            .texture_mips
            .get(start..end)
            .expect("scene storage validates texture mip ranges")
    }

    pub fn texture_sequence_frames(
        &self,
        texture: &SceneTextureRecord,
    ) -> &[SceneTextureSequenceFrameRecord] {
        let start = texture.sequence_frame_start as usize;
        let end = start.saturating_add(texture.sequence_frame_count as usize);
        self.document
            .texture_sequence_frames
            .get(start..end)
            .expect("scene storage validates texture sequence frame ranges")
    }

    pub fn texture_payload(&self, texture: &SceneTextureRecord) -> &[u8] {
        let start = texture.payload_offset as usize;
        let end = start.saturating_add(texture.payload_len as usize);
        self.document
            .texture_payload
            .get(start..end)
            .expect("scene storage validates texture payload ranges")
    }

    pub fn texture_mip_payload(&self, mip: &SceneTextureMipRecord) -> &[u8] {
        let start = mip.payload_offset as usize;
        let end = start.saturating_add(mip.payload_len as usize);
        self.document
            .texture_payload
            .get(start..end)
            .expect("scene storage validates texture mip payload ranges")
    }

    pub fn objects(&self) -> &[SceneObjectRecord] {
        &self.document.objects
    }

    pub fn camera_parallax(&self) -> SceneCameraParallaxRecord {
        self.document.camera_parallax
    }

    pub fn object_parallax_depths(&self) -> &[SceneObjectParallaxDepthRecord] {
        &self.document.object_parallax_depths
    }

    pub fn object_animation_layers(&self) -> &[SceneObjectAnimationLayerRecord] {
        &self.document.object_animation_layers
    }

    pub fn object_transform_tracks(&self) -> &[SceneObjectTransformTrackRecord] {
        &self.document.object_transform_tracks
    }

    pub fn object_transform_channels(
        &self,
        track: &SceneObjectTransformTrackRecord,
    ) -> &[SceneObjectTransformChannelRecord] {
        let start = track.channel_start as usize;
        let end = start.saturating_add(track.channel_count as usize);
        self.document
            .object_transform_channels
            .get(start..end)
            .expect("scene storage validates object transform channel ranges")
    }

    pub fn object_transform_keyframes(
        &self,
        channel: &SceneObjectTransformChannelRecord,
    ) -> &[SceneObjectTransformKeyframeRecord] {
        let start = channel.keyframe_start as usize;
        let end = start.saturating_add(channel.keyframe_count as usize);
        self.document
            .object_transform_keyframes
            .get(start..end)
            .expect("scene storage validates object transform keyframe ranges")
    }

    pub fn puppet_animation_clips(&self) -> &[ScenePuppetAnimationClipRecord] {
        &self.document.puppet_animation_clips
    }

    pub fn puppet_animation_tracks(
        &self,
        clip: &ScenePuppetAnimationClipRecord,
    ) -> &[ScenePuppetAnimationTrackRecord] {
        let start = clip.track_start as usize;
        let end = start.saturating_add(clip.track_count as usize);
        self.document
            .puppet_animation_tracks
            .get(start..end)
            .expect("scene storage validates puppet animation track ranges")
    }

    pub fn puppet_animation_transform_samples(
        &self,
        track: &ScenePuppetAnimationTrackRecord,
    ) -> &[ScenePuppetAnimationTransformSampleRecord] {
        let start = track.sample_start as usize;
        let end = start.saturating_add(track.sample_count as usize);
        self.document
            .puppet_animation_transform_samples
            .get(start..end)
            .expect("scene storage validates puppet animation sample ranges")
    }

    pub fn puppet_animation_opacity_samples(
        &self,
        track: &ScenePuppetAnimationTrackRecord,
    ) -> &[f32] {
        let start = track.opacity_sample_start as usize;
        let end = start.saturating_add(track.opacity_sample_count as usize);
        self.document
            .puppet_animation_opacity_samples
            .get(start..end)
            .expect("scene storage validates puppet animation opacity sample ranges")
    }

    pub fn materials(&self) -> &[SceneMaterialRecord] {
        &self.document.materials
    }

    pub fn material(&self, handle: SceneMaterialHandle) -> Option<&SceneMaterialRecord> {
        if handle.0 == INVALID_MATERIAL_ID {
            return None;
        }
        self.document.materials.get(handle.0 as usize)
    }

    pub fn material_passes(&self, material: &SceneMaterialRecord) -> &[SceneMaterialPassRecord] {
        let start = material.pass_start as usize;
        let end = start.saturating_add(material.pass_count as usize);
        self.document
            .material_passes
            .get(start..end)
            .expect("scene storage validates material pass ranges")
    }

    pub fn material_pass_textures(
        &self,
        pass: &SceneMaterialPassRecord,
    ) -> &[SceneMaterialTextureRecord] {
        let start = pass.texture_start as usize;
        let end = start.saturating_add(pass.texture_count as usize);
        self.document
            .material_textures
            .get(start..end)
            .expect("scene storage validates material texture ranges")
    }

    pub fn effects(&self) -> &[SceneEffectRecord] {
        &self.document.effects
    }

    pub fn object_effects(&self) -> &[SceneObjectEffectRecord] {
        &self.document.object_effects
    }

    pub fn object_effects_for_object(
        &self,
        object: &SceneObjectRecord,
    ) -> &[SceneObjectEffectRecord] {
        if object.effect_count == 0 {
            return &[];
        }
        let start = object.effect_start as usize;
        let end = start.saturating_add(object.effect_count as usize);
        self.document
            .object_effects
            .get(start..end)
            .expect("scene storage validates object effect ranges")
    }

    pub fn meshes(&self) -> &[SceneMeshRecord] {
        &self.document.meshes
    }

    pub fn puppets(&self) -> &[ScenePuppetRecord] {
        &self.document.puppets
    }

    pub fn particles(&self) -> &[SceneParticleSystemRecord] {
        &self.document.particles
    }

    pub fn script_programs(&self) -> &[SceneScriptProgramRecord] {
        &self.document.script_programs
    }

    pub fn dynamic_texts(&self) -> &[SceneDynamicTextRecord] {
        &self.document.dynamic_texts
    }

    pub fn dynamic_text_for_object(
        &self,
        object: SceneObjectHandle,
    ) -> Option<&SceneDynamicTextRecord> {
        self.document
            .dynamic_texts
            .iter()
            .find(|text| text.object == object)
    }

    pub fn dynamic_text_glyphs(
        &self,
        text: &SceneDynamicTextRecord,
    ) -> &[SceneDynamicTextGlyphRecord] {
        let start = text.glyph_start as usize;
        let end = start.saturating_add(text.glyph_count as usize);
        self.document
            .dynamic_text_glyphs
            .get(start..end)
            .expect("scene storage validates dynamic text glyph ranges")
    }

    pub fn user_property_bindings(&self) -> &[SceneUserPropertyBindingRecord] {
        &self.document.user_property_bindings
    }

    pub fn particle(&self, particle_index: u32) -> Option<&SceneParticleSystemRecord> {
        self.document.particles.get(particle_index as usize)
    }

    pub fn puppet_attachments(&self, puppet: &ScenePuppetRecord) -> &[ScenePuppetAttachmentRecord] {
        let start = puppet.attachment_start as usize;
        let end = start.saturating_add(puppet.attachment_count as usize);
        self.document
            .puppet_attachments
            .get(start..end)
            .expect("scene storage validates puppet attachment ranges")
    }

    pub fn puppet_bones(&self, puppet: &ScenePuppetRecord) -> &[ScenePuppetBoneRecord] {
        let start = puppet.bone_start as usize;
        let end = start.saturating_add(puppet.bone_count as usize);
        self.document
            .puppet_bones
            .get(start..end)
            .expect("scene storage validates puppet bone ranges")
    }

    pub fn mesh_vertices(&self, mesh: &SceneMeshRecord) -> &[SceneMeshVertexRecord] {
        let start = mesh.vertex_start as usize;
        let end = start.saturating_add(mesh.vertex_count as usize);
        self.document
            .mesh_vertices
            .get(start..end)
            .expect("scene storage validates mesh vertex ranges")
    }

    pub fn mesh_indices(&self, mesh: &SceneMeshRecord) -> &[u32] {
        let start = mesh.index_start as usize;
        let end = start.saturating_add(mesh.index_count as usize);
        self.document
            .mesh_indices
            .get(start..end)
            .expect("scene storage validates mesh index ranges")
    }

    pub fn mesh_source_records(
        &self,
        mesh_index: u32,
    ) -> impl Iterator<Item = &SceneMeshSourceRecord> {
        self.document
            .mesh_source_records
            .iter()
            .filter(move |record| record.mesh == mesh_index)
    }

    pub fn mesh_clipping_subdraws(
        &self,
        mesh_index: u32,
    ) -> impl Iterator<Item = &SceneMeshClippingSubdrawRecord> {
        self.document
            .mesh_clipping_subdraws
            .iter()
            .filter(move |record| record.mesh == mesh_index)
    }

    pub fn mesh_clipping_slices(
        &self,
        mesh_index: u32,
    ) -> impl Iterator<Item = &SceneMeshClippingSliceRecord> {
        self.document
            .mesh_clipping_slices
            .iter()
            .filter(move |record| record.mesh == mesh_index)
    }

    pub fn mesh_clipping_target_ordinals(
        &self,
        subdraw: &SceneMeshClippingSubdrawRecord,
    ) -> &[u32] {
        let start = subdraw.target_source_start as usize;
        let end = start.saturating_add(subdraw.target_source_count as usize);
        self.document
            .mesh_clipping_source_ordinals
            .get(start..end)
            .expect("scene storage validates clipping target source ranges")
    }

    pub fn mesh_clipping_mask_ordinals(&self, subdraw: &SceneMeshClippingSubdrawRecord) -> &[u32] {
        let start = subdraw.mask_source_start as usize;
        let end = start.saturating_add(subdraw.mask_source_count as usize);
        self.document
            .mesh_clipping_source_ordinals
            .get(start..end)
            .expect("scene storage validates clipping mask source ranges")
    }

    pub fn render_graphs(&self) -> &[SceneRenderGraphRecord] {
        &self.document.render_graphs
    }

    pub fn render_graph_passes(&self, graph: &SceneRenderGraphRecord) -> &[SceneRenderPassRecord] {
        let start = graph.pass_start as usize;
        let end = start.saturating_add(graph.pass_count as usize);
        self.document
            .render_passes
            .get(start..end)
            .expect("scene storage validates render graph pass ranges")
    }

    pub fn render_pass_bindings(
        &self,
        pass: &SceneRenderPassRecord,
    ) -> &[SceneRenderBindingRecord] {
        let start = pass.binding_start as usize;
        let end = start.saturating_add(pass.binding_count as usize);
        self.document
            .render_bindings
            .get(start..end)
            .expect("scene storage validates render pass binding ranges")
    }

    pub fn shader_contracts(&self) -> &[SceneShaderContractRecord] {
        &self.document.shader_contracts
    }

    pub fn shader_programs(&self) -> &[SceneShaderProgramRecord] {
        &self.document.shader_programs
    }

    pub fn shader_program(
        &self,
        program_key: SceneStringId,
        stage: SceneShaderStage,
    ) -> Option<&SceneShaderProgramRecord> {
        self.document
            .shader_programs
            .iter()
            .find(|program| program.program_key == program_key && program.stage == stage)
    }

    pub fn shader_program_bindings(
        &self,
        program: &SceneShaderProgramRecord,
    ) -> &[SceneShaderBindingRecord] {
        let start = program.binding_start as usize;
        let end = start + program.binding_count as usize;
        &self.document.shader_bindings[start..end]
    }

    pub fn shader_program_stage_io(
        &self,
        program: &SceneShaderProgramRecord,
    ) -> &[SceneShaderStageIoRecord] {
        let start = program.stage_io_start as usize;
        let end = start + program.stage_io_count as usize;
        &self.document.shader_stage_io[start..end]
    }

    pub fn shader_program_uniform_buffers(
        &self,
        program: &SceneShaderProgramRecord,
    ) -> &[SceneShaderUniformBufferRecord] {
        let start = program.uniform_buffer_start as usize;
        let end = start + program.uniform_buffer_count as usize;
        &self.document.shader_uniform_buffers[start..end]
    }

    pub fn shader_uniform_buffer_members(
        &self,
        buffer: &SceneShaderUniformBufferRecord,
    ) -> &[SceneShaderUniformMemberRecord] {
        let start = buffer.member_start as usize;
        let end = start + buffer.member_count as usize;
        &self.document.shader_uniform_members[start..end]
    }

    pub fn shader_program_spirv(&self, program: &SceneShaderProgramRecord) -> &[u32] {
        let start = program.spirv_start as usize;
        let end = start + program.spirv_count as usize;
        &self.document.shader_spirv[start..end]
    }

    pub fn resource_payload_bytes(&self) -> usize {
        self.document.resource_payload.len()
    }

    pub fn release_parsed_resource_payload(&mut self) -> usize {
        let payload = std::mem::take(&mut self.document.resource_payload);
        let released_bytes = payload.len();
        for resource in &mut self.document.resources {
            resource.payload_offset = 0;
            resource.payload_len = 0;
        }
        drop(payload);
        released_bytes
    }

    pub fn texture_payload_bytes(&self) -> usize {
        self.document.texture_payload.len()
    }

    pub fn release_uploaded_texture_payload(&mut self) -> usize {
        let payload = std::mem::take(&mut self.document.texture_payload);
        let released_bytes = payload.len();
        for texture in &mut self.document.textures {
            texture.payload_offset = 0;
            texture.payload_len = 0;
        }
        for mip in &mut self.document.texture_mips {
            mip.payload_offset = 0;
            mip.payload_len = 0;
        }
        drop(payload);
        released_bytes
    }

    pub fn mesh_vertex_payload_bytes(&self) -> usize {
        self.document
            .mesh_vertices
            .len()
            .saturating_mul(std::mem::size_of::<SceneMeshVertexRecord>())
    }

    pub fn mesh_index_payload_bytes(&self) -> usize {
        self.document
            .mesh_indices
            .len()
            .saturating_mul(std::mem::size_of::<u32>())
    }

    pub fn release_uploaded_mesh_payload(&mut self) -> (usize, usize) {
        let vertices = std::mem::take(&mut self.document.mesh_vertices);
        let indices = std::mem::take(&mut self.document.mesh_indices);
        let released_vertex_bytes = vertices
            .len()
            .saturating_mul(std::mem::size_of::<SceneMeshVertexRecord>());
        let released_index_bytes = indices.len().saturating_mul(std::mem::size_of::<u32>());
        drop(vertices);
        drop(indices);
        (released_vertex_bytes, released_index_bytes)
    }
}

#[derive(Debug)]
pub enum SceneStorageError {
    Binary(SceneBinaryError),
    InvalidStringId {
        field: &'static str,
        id: SceneStringId,
    },
    InvalidResourceId {
        field: &'static str,
        id: SceneResourceId,
    },
    InvalidMaterialHandle {
        field: &'static str,
        handle: SceneMaterialHandle,
    },
    InvalidRange {
        field: &'static str,
        start: u32,
        count: u32,
        len: usize,
    },
    InvalidMeshIndex {
        mesh: usize,
        index: u32,
        vertex_count: u32,
    },
    InvalidPuppetBlendWeight {
        puppet: usize,
        mesh: usize,
        vertex: usize,
        slot: usize,
    },
    InvalidPuppetBlendIndex {
        puppet: usize,
        mesh: usize,
        vertex: usize,
        slot: usize,
        bone_index: u32,
        bone_count: u32,
    },
    InvalidPayloadRange {
        resource: SceneResourceId,
        offset: u64,
        len: u64,
        payload_len: usize,
    },
    InvalidTexturePayloadRange {
        texture: SceneResourceId,
        offset: u64,
        len: u64,
        payload_len: usize,
    },
    InvalidTextureSequence {
        resource: SceneResourceId,
        reason: &'static str,
    },
    InvalidScriptProgram {
        object: SceneObjectHandle,
        reason: &'static str,
    },
    InvalidUserPropertyBinding {
        object: SceneObjectHandle,
        reason: String,
    },
    InvalidPointerParallaxBinding {
        object: SceneObjectHandle,
        reason: &'static str,
    },
    InvalidCameraLayer {
        object: SceneObjectHandle,
        reason: &'static str,
    },
    InvalidShaderAccessMask {
        shader: SceneStringId,
        overlap: u32,
    },
    InvalidShaderProgram {
        program: SceneStringId,
        program_key: Option<String>,
        reason: &'static str,
        detail: Option<String>,
    },
}

impl SceneStorageError {
    fn with_document_context(mut self, document: &SceneBinaryDocument) -> Self {
        if let Self::InvalidShaderProgram {
            program,
            program_key,
            ..
        } = &mut self
        {
            *program_key = document.strings.get(program.0 as usize).cloned();
        }
        self
    }
}

impl fmt::Display for SceneStorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Binary(err) => write!(f, "{err}"),
            Self::InvalidStringId { field, id } => {
                write!(
                    f,
                    "scene storage {field} references invalid string id {}",
                    id.0
                )
            }
            Self::InvalidResourceId { field, id } => {
                write!(
                    f,
                    "scene storage {field} references invalid resource id {}",
                    id.0
                )
            }
            Self::InvalidMaterialHandle { field, handle } => write!(
                f,
                "scene storage {field} references invalid material handle {}",
                handle.0
            ),
            Self::InvalidRange {
                field,
                start,
                count,
                len,
            } => write!(
                f,
                "scene storage {field} range [{start}, {start}+{count}) exceeds length {len}"
            ),
            Self::InvalidMeshIndex {
                mesh,
                index,
                vertex_count,
            } => write!(
                f,
                "scene storage mesh {mesh} index {index} exceeds local vertex count {vertex_count}"
            ),
            Self::InvalidPuppetBlendWeight {
                puppet,
                mesh,
                vertex,
                slot,
            } => write!(
                f,
                "scene storage puppet {puppet} mesh {mesh} vertex {vertex} has an invalid blend weight in slot {slot}"
            ),
            Self::InvalidPuppetBlendIndex {
                puppet,
                mesh,
                vertex,
                slot,
                bone_index,
                bone_count,
            } => write!(
                f,
                "scene storage puppet {puppet} mesh {mesh} vertex {vertex} blend slot {slot} references bone {bone_index} outside {bone_count} bones"
            ),
            Self::InvalidPayloadRange {
                resource,
                offset,
                len,
                payload_len,
            } => write!(
                f,
                "scene storage resource {} payload range [{offset}, {offset}+{len}) exceeds payload chunk length {payload_len}",
                resource.0
            ),
            Self::InvalidTexturePayloadRange {
                texture,
                offset,
                len,
                payload_len,
            } => write!(
                f,
                "scene texture resource {} payload range [{offset}, {offset}+{len}) exceeds texture payload chunk length {payload_len}",
                texture.0
            ),
            Self::InvalidTextureSequence { resource, reason } => write!(
                f,
                "scene texture resource {} has an invalid sequence: {reason}",
                resource.0
            ),
            Self::InvalidScriptProgram { object, reason } => write!(
                f,
                "scene script program for object {} is invalid: {reason}",
                object.0
            ),
            Self::InvalidUserPropertyBinding { object, reason } => write!(
                f,
                "scene user property binding for object {} is invalid: {reason}",
                object.0
            ),
            Self::InvalidPointerParallaxBinding { object, reason } => write!(
                f,
                "scene pointer parallax binding for object {} is invalid: {reason}",
                object.0
            ),
            Self::InvalidCameraLayer { object, reason } => write!(
                f,
                "scene camera layer object {} is invalid: {reason}",
                object.0
            ),
            Self::InvalidShaderAccessMask { shader, overlap } => write!(
                f,
                "scene shader {} declares overlapping sampled/input-attachment slots: {overlap:#x}",
                shader.0
            ),
            Self::InvalidShaderProgram {
                program,
                program_key,
                reason,
                detail,
            } => {
                write!(f, "scene shader program {}", program.0)?;
                if let Some(program_key) = program_key {
                    write!(f, " ({program_key})")?;
                }
                write!(f, " is invalid: {reason}")?;
                if let Some(detail) = detail {
                    write!(f, ": {detail}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for SceneStorageError {}

impl From<SceneBinaryError> for SceneStorageError {
    fn from(value: SceneBinaryError) -> Self {
        Self::Binary(value)
    }
}

#[cfg(test)]
mod tests;

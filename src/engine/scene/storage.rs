//! Scene storage for the new engine binary.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/storage/*`
//! - `references/godot/servers/rendering/rendering_server_default.*`

use std::fmt;
use std::io::Read;

use super::abi::*;
use super::binary::{
    SceneBinaryDocument, SceneBinaryError, read_scene_binary, read_scene_binary_bytes,
};

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
        validate_document(&document)?;
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

    pub fn object_animation_layers(&self) -> &[SceneObjectAnimationLayerRecord] {
        &self.document.object_animation_layers
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

    pub fn resource_payload_bytes(&self) -> usize {
        self.document.resource_payload.len()
    }

    pub fn texture_payload_bytes(&self) -> usize {
        self.document.texture_payload.len()
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
        }
    }
}

impl std::error::Error for SceneStorageError {}

impl From<SceneBinaryError> for SceneStorageError {
    fn from(value: SceneBinaryError) -> Self {
        Self::Binary(value)
    }
}

fn validate_document(document: &SceneBinaryDocument) -> Result<(), SceneStorageError> {
    validate_project(document)?;
    for resource in &document.resources {
        validate_string(document, "resource.path", resource.path)?;
        validate_string(document, "resource.source", resource.source)?;
        validate_payload(document, resource)?;
    }
    for texture in &document.textures {
        validate_resource(document, "texture.resource", texture.resource)?;
        validate_string(document, "texture.texv_tag", texture.texv_tag)?;
        validate_string(document, "texture.texb_tag", texture.texb_tag)?;
        validate_range(
            "texture.mip_range",
            texture.mip_start,
            texture.mip_count,
            document.texture_mips.len(),
        )?;
        validate_texture_payload(
            document,
            texture.resource,
            texture.payload_offset,
            texture.payload_len,
        )?;
        for mip in document
            .texture_mips
            .iter()
            .skip(texture.mip_start as usize)
            .take(texture.mip_count as usize)
        {
            validate_texture_payload(
                document,
                texture.resource,
                mip.payload_offset,
                mip.payload_len,
            )?;
        }
    }
    for object in &document.objects {
        validate_string(document, "object.name", object.name)?;
        validate_optional_resource(document, "object.resource", object.resource)?;
        validate_optional_material(document, "object.material", object.material)?;
        validate_string(document, "object.attachment", object.attachment)?;
        validate_range(
            "object.effect_range",
            object.effect_start,
            object.effect_count,
            document.object_effects.len(),
        )?;
        validate_range(
            "object.render_graph",
            object.render_graph,
            u32::from(object.render_graph != u32::MAX),
            document.render_graphs.len(),
        )?;
    }
    for effect in &document.object_effects {
        validate_range(
            "object_effect.object",
            effect.object.0,
            1,
            document.objects.len(),
        )?;
        validate_range(
            "object_effect.effect",
            effect.effect.0,
            1,
            document.effects.len(),
        )?;
    }
    for layer in &document.object_animation_layers {
        validate_range(
            "object_animation_layer.object",
            layer.object.0,
            1,
            document.objects.len(),
        )?;
    }
    for (clip_index, clip) in document.puppet_animation_clips.iter().enumerate() {
        validate_range(
            "puppet_animation_clip.puppet",
            clip.puppet,
            1,
            document.puppets.len(),
        )?;
        validate_string(document, "puppet_animation_clip.name", clip.name)?;
        validate_string(document, "puppet_animation_clip.playback", clip.playback)?;
        validate_range(
            "puppet_animation_clip.track_range",
            clip.track_start,
            clip.track_count,
            document.puppet_animation_tracks.len(),
        )?;
        for track in document
            .puppet_animation_tracks
            .iter()
            .skip(clip.track_start as usize)
            .take(clip.track_count as usize)
        {
            if track.clip as usize != clip_index {
                return Err(SceneStorageError::InvalidRange {
                    field: "puppet_animation_track.clip_owner",
                    start: track.clip,
                    count: 1,
                    len: clip_index,
                });
            }
            validate_range(
                "puppet_animation_track.sample_range",
                track.sample_start,
                track.sample_count,
                document.puppet_animation_transform_samples.len(),
            )?;
            validate_range(
                "puppet_animation_track.opacity_sample_range",
                track.opacity_sample_start,
                track.opacity_sample_count,
                document.puppet_animation_opacity_samples.len(),
            )?;
        }
    }
    for material in &document.materials {
        validate_optional_resource(document, "material.resource", material.resource)?;
        validate_range(
            "material.pass_range",
            material.pass_start,
            material.pass_count,
            document.material_passes.len(),
        )?;
    }
    for pass in &document.material_passes {
        validate_string(document, "material_pass.shader_key", pass.shader_key)?;
        validate_string(document, "material_pass.target", pass.target)?;
        validate_string(document, "material_pass.alpha_writing", pass.alpha_writing)?;
        validate_range(
            "material_pass.texture_range",
            pass.texture_start,
            pass.texture_count,
            document.material_textures.len(),
        )?;
        validate_range(
            "material_pass.constant_range",
            pass.constant_start,
            pass.constant_count,
            document.material_constants.len(),
        )?;
    }
    for texture in &document.material_textures {
        validate_optional_resource(document, "material_texture.resource", texture.resource)?;
        validate_string(document, "material_texture.path", texture.path)?;
    }
    for constant in &document.material_constants {
        validate_string(document, "material_constant.name", constant.name)?;
        validate_string(
            document,
            "material_constant.value_json",
            constant.value_json,
        )?;
    }
    for (mesh_index, mesh) in document.meshes.iter().enumerate() {
        validate_range("mesh.object", mesh.object.0, 1, document.objects.len())?;
        validate_optional_material(document, "mesh.material", mesh.material)?;
        validate_range(
            "mesh.vertex_range",
            mesh.vertex_start,
            mesh.vertex_count,
            document.mesh_vertices.len(),
        )?;
        validate_range(
            "mesh.index_range",
            mesh.index_start,
            mesh.index_count,
            document.mesh_indices.len(),
        )?;
        for &index in document
            .mesh_indices
            .iter()
            .skip(mesh.index_start as usize)
            .take(mesh.index_count as usize)
        {
            if index >= mesh.vertex_count {
                return Err(SceneStorageError::InvalidMeshIndex {
                    mesh: mesh_index,
                    index,
                    vertex_count: mesh.vertex_count,
                });
            }
        }
    }
    for (puppet_index, puppet) in document.puppets.iter().enumerate() {
        validate_range("puppet.object", puppet.object.0, 1, document.objects.len())?;
        validate_optional_resource(document, "puppet.resource", puppet.resource)?;
        validate_range(
            "puppet.mesh_range",
            puppet.mesh_start,
            puppet.mesh_count,
            document.meshes.len(),
        )?;
        validate_range(
            "puppet.bone_range",
            puppet.bone_start,
            puppet.bone_count,
            document.puppet_bones.len(),
        )?;
        validate_range(
            "puppet.attachment_range",
            puppet.attachment_start,
            puppet.attachment_count,
            document.puppet_attachments.len(),
        )?;
        for (mesh_index, mesh) in document
            .meshes
            .iter()
            .enumerate()
            .skip(puppet.mesh_start as usize)
            .take(puppet.mesh_count as usize)
        {
            for (vertex_index, vertex) in document
                .mesh_vertices
                .iter()
                .skip(mesh.vertex_start as usize)
                .take(mesh.vertex_count as usize)
                .enumerate()
            {
                for slot in 0..4 {
                    let weight = vertex.blend_weights[slot];
                    if !weight.is_finite() || weight < 0.0 {
                        return Err(SceneStorageError::InvalidPuppetBlendWeight {
                            puppet: puppet_index,
                            mesh: mesh_index,
                            vertex: vertex_index,
                            slot,
                        });
                    }
                    let bone_index = vertex.blend_indices[slot];
                    if weight > 1.0e-6 && bone_index >= puppet.bone_count {
                        return Err(SceneStorageError::InvalidPuppetBlendIndex {
                            puppet: puppet_index,
                            mesh: mesh_index,
                            vertex: vertex_index,
                            slot,
                            bone_index,
                            bone_count: puppet.bone_count,
                        });
                    }
                }
            }
        }
        for bone in document
            .puppet_bones
            .iter()
            .skip(puppet.bone_start as usize)
            .take(puppet.bone_count as usize)
        {
            validate_range("puppet_bone.puppet", bone.puppet, 1, document.puppets.len())?;
            if bone.puppet as usize != puppet_index {
                return Err(SceneStorageError::InvalidRange {
                    field: "puppet_bone.puppet_owner",
                    start: bone.puppet,
                    count: 1,
                    len: puppet_index,
                });
            }
            validate_string(document, "puppet_bone.name", bone.name)?;
            validate_string(
                document,
                "puppet_bone.simulation_json",
                bone.simulation_json,
            )?;
        }
        for attachment in document
            .puppet_attachments
            .iter()
            .skip(puppet.attachment_start as usize)
            .take(puppet.attachment_count as usize)
        {
            validate_range(
                "puppet_attachment.puppet",
                attachment.puppet,
                1,
                document.puppets.len(),
            )?;
            if attachment.puppet as usize != puppet_index {
                return Err(SceneStorageError::InvalidRange {
                    field: "puppet_attachment.puppet_owner",
                    start: attachment.puppet,
                    count: 1,
                    len: puppet_index,
                });
            }
            validate_string(document, "puppet_attachment.name", attachment.name)?;
        }
    }
    for effect in &document.effects {
        validate_optional_resource(document, "effect.resource", effect.resource)?;
        validate_string(document, "effect.replacement_key", effect.replacement_key)?;
        validate_range(
            "effect.pass_range",
            effect.pass_start,
            effect.pass_count,
            document.effect_passes.len(),
        )?;
        validate_range(
            "effect.fbo_range",
            effect.fbo_start,
            effect.fbo_count,
            document.effect_fbos.len(),
        )?;
    }
    for pass in &document.effect_passes {
        validate_optional_material(document, "effect_pass.material", pass.material)?;
        validate_string(document, "effect_pass.command", pass.command)?;
        validate_string(document, "effect_pass.source", pass.source)?;
        validate_string(document, "effect_pass.target", pass.target)?;
        validate_range(
            "effect_pass.binding_range",
            pass.binding_start,
            pass.binding_count,
            document.effect_bindings.len(),
        )?;
        validate_range(
            "effect_pass.combo_range",
            pass.combo_start,
            pass.combo_count,
            document.effect_combos.len(),
        )?;
    }
    for graph in &document.render_graphs {
        validate_range(
            "render_graph.pass_range",
            graph.pass_start,
            graph.pass_count,
            document.render_passes.len(),
        )?;
        validate_range(
            "render_graph.unsupported_range",
            graph.unsupported_start,
            graph.unsupported_count,
            document.unsupported.len(),
        )?;
    }
    for pass in &document.render_passes {
        validate_optional_material(document, "render_pass.material", pass.material)?;
        validate_string(document, "render_pass.shader_key", pass.shader_key)?;
        validate_string(document, "render_pass.target_name", pass.target_name)?;
        validate_range(
            "render_pass.binding_range",
            pass.binding_start,
            pass.binding_count,
            document.render_bindings.len(),
        )?;
    }
    for binding in &document.render_bindings {
        validate_string(document, "render_binding.name", binding.name)?;
    }
    for target in &document.image_targets {
        validate_string(document, "image_target.name", target.name)?;
        validate_string(document, "image_target.format", target.format)?;
    }
    for contract in &document.shader_contracts {
        validate_string(document, "shader_contract.shader_key", contract.shader_key)?;
        validate_string(
            document,
            "shader_contract.pipeline_key",
            contract.pipeline_key,
        )?;
        validate_range(
            "shader_contract.constant_range",
            contract.constant_start,
            contract.constant_count,
            document.shader_constant_names.len(),
        )?;
    }
    for name in &document.shader_constant_names {
        validate_string(document, "shader_contract.constant_name", *name)?;
    }
    Ok(())
}

fn validate_project(document: &SceneBinaryDocument) -> Result<(), SceneStorageError> {
    let project = &document.project;
    validate_string(document, "project.title", project.title)?;
    validate_string(document, "project.wallpaper_type", project.wallpaper_type)?;
    validate_string(document, "project.scene_file", project.scene_file)?;
    validate_string(document, "project.preview", project.preview)?;
    validate_string(document, "project.properties_json", project.properties_json)
}

fn validate_string(
    document: &SceneBinaryDocument,
    field: &'static str,
    id: SceneStringId,
) -> Result<(), SceneStorageError> {
    if !id.is_some() || (id.0 as usize) < document.strings.len() {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidStringId { field, id })
    }
}

fn validate_resource(
    document: &SceneBinaryDocument,
    field: &'static str,
    id: SceneResourceId,
) -> Result<(), SceneStorageError> {
    if document.resources.iter().any(|resource| resource.id == id) {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidResourceId { field, id })
    }
}

fn validate_optional_resource(
    document: &SceneBinaryDocument,
    field: &'static str,
    id: SceneResourceId,
) -> Result<(), SceneStorageError> {
    if !id.is_some() {
        Ok(())
    } else {
        validate_resource(document, field, id)
    }
}

fn validate_optional_material(
    document: &SceneBinaryDocument,
    field: &'static str,
    handle: SceneMaterialHandle,
) -> Result<(), SceneStorageError> {
    if handle.0 == INVALID_MATERIAL_ID || (handle.0 as usize) < document.materials.len() {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidMaterialHandle { field, handle })
    }
}

fn validate_payload(
    document: &SceneBinaryDocument,
    resource: &SceneResourceRecord,
) -> Result<(), SceneStorageError> {
    let Ok(start) = usize::try_from(resource.payload_offset) else {
        return Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        });
    };
    let Ok(len) = usize::try_from(resource.payload_len) else {
        return Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        });
    };
    let Some(end) = start.checked_add(len) else {
        return Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        });
    };
    if end <= document.resource_payload.len() {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        })
    }
}

fn validate_texture_payload(
    document: &SceneBinaryDocument,
    texture: SceneResourceId,
    offset: u64,
    len: u64,
) -> Result<(), SceneStorageError> {
    let valid = usize::try_from(offset)
        .ok()
        .and_then(|start| {
            usize::try_from(len)
                .ok()
                .and_then(|len| start.checked_add(len))
        })
        .is_some_and(|end| end <= document.texture_payload.len());
    if valid {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidTexturePayloadRange {
            texture,
            offset,
            len,
            payload_len: document.texture_payload.len(),
        })
    }
}

fn validate_range(
    field: &'static str,
    start: u32,
    count: u32,
    len: usize,
) -> Result<(), SceneStorageError> {
    if start == u32::MAX && count == 0 {
        return Ok(());
    }
    let start_usize = start as usize;
    let count_usize = count as usize;
    let Some(end) = start_usize.checked_add(count_usize) else {
        return Err(SceneStorageError::InvalidRange {
            field,
            start,
            count,
            len,
        });
    };
    if end <= len {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidRange {
            field,
            start,
            count,
            len,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::binary::{SceneBinaryDocument, write_scene_binary};

    #[test]
    fn storage_borrows_resource_payload_slices() {
        let mut document = SceneBinaryDocument {
            strings: vec!["scene".to_owned(), "scene.json".to_owned()],
            resource_payload: vec![7, 8, 9],
            ..SceneBinaryDocument::default()
        };
        document.project.wallpaper_type = SceneStringId(0);
        document.project.scene_file = SceneStringId(1);
        document.resources.push(SceneResourceRecord {
            id: SceneResourceId(0),
            kind: SceneResourceKind::SceneJson,
            path: SceneStringId(1),
            source: SceneStringId(1),
            payload_offset: 0,
            payload_len: 3,
        });

        let mut bytes = Vec::new();
        write_scene_binary(&document, &mut bytes).expect("write");
        let storage = SceneStorage::from_binary_bytes(&bytes).expect("storage");
        let payload = storage
            .resource_payload(&storage.resources()[0])
            .expect("payload");

        assert_eq!(storage.string(SceneStringId(0)), Some("scene"));
        assert_eq!(payload, &[7, 8, 9]);
    }

    #[test]
    fn storage_rejects_invalid_material_handles() {
        let mut document = SceneBinaryDocument {
            strings: vec!["scene".to_owned(), "scene.json".to_owned()],
            ..SceneBinaryDocument::default()
        };
        document.project.wallpaper_type = SceneStringId(0);
        document.project.scene_file = SceneStringId(1);
        document.objects.push(SceneObjectRecord {
            id: SceneObjectHandle(0),
            we_id: 1,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(42),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3::default(),
            angles: SceneVec3::default(),
            scale: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: u32::MAX,
            effect_count: 0,
            render_graph: u32::MAX,
        });

        let err = SceneStorage::from_document(document).expect_err("invalid material");

        assert!(matches!(
            err,
            SceneStorageError::InvalidMaterialHandle {
                field: "object.material",
                handle: SceneMaterialHandle(42)
            }
        ));
    }

    #[test]
    fn storage_rejects_mesh_indices_outside_local_vertex_range() {
        let mut document = SceneBinaryDocument::default();
        document.objects.push(SceneObjectRecord {
            id: SceneObjectHandle(0),
            we_id: 1,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3::default(),
            angles: SceneVec3::default(),
            scale: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: u32::MAX,
            effect_count: 0,
            render_graph: u32::MAX,
        });
        document.meshes.push(SceneMeshRecord {
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 4,
            index_start: 0,
            index_count: 6,
            width: 64.0,
            height: 32.0,
            bounds_min: SceneVec3 {
                x: -32.0,
                y: -16.0,
                z: 0.0,
            },
            bounds_max: SceneVec3 {
                x: 32.0,
                y: 16.0,
                z: 0.0,
            },
        });
        document.mesh_vertices.resize(
            4,
            SceneMeshVertexRecord {
                position: SceneVec3::default(),
                uv: [0.0, 0.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
        );
        document.mesh_indices = vec![0, 1, 2, 0, 2, 4];

        let err = SceneStorage::from_document(document).expect_err("invalid mesh index");

        assert!(matches!(
            err,
            SceneStorageError::InvalidMeshIndex {
                mesh: 0,
                index: 4,
                vertex_count: 4
            }
        ));
    }
}

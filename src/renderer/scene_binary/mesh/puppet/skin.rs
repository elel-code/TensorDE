//! Puppet skin, bone, vertex and attachment decoding.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`

use crate::core::scene::binary::{
    SCENE_BINARY_NONE_ID, SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE,
    SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE, SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE,
    SceneBinaryChunkKind, SceneBinaryPuppetRecord, decode_puppet_attachment_record,
    decode_puppet_skin_bone_record, decode_puppet_skin_vertex_record,
};
use crate::core::scene::{
    SceneMeshSkin, SceneMeshSkinAttachment, SceneMeshSkinBone, SceneMeshSkinVertex,
};
use crate::renderer::RendererPlanError;

use super::super::super::facts::{BinarySceneNames, binary_name};
use super::super::super::reader::BinarySceneReader;

pub(in crate::renderer::scene_binary) fn binary_scene_puppet_skin(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    puppet: SceneBinaryPuppetRecord,
    include_vertices: bool,
) -> Result<SceneMeshSkin, RendererPlanError> {
    let bone_records = reader.record_range(
        SceneBinaryChunkKind::PuppetSkinBones,
        SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE,
        puppet.first_bone,
        puppet.bone_count,
        decode_puppet_skin_bone_record,
    )?;
    let mut bones = Vec::with_capacity(bone_records.len());
    for bone in bone_records {
        bones.push(SceneMeshSkinBone {
            parent: (bone.parent_index != SCENE_BINARY_NONE_ID)
                .then_some(bone.parent_index as usize),
            bind: bone.transform,
        });
    }

    let vertices = if include_vertices {
        let vertex_records = reader.record_range(
            SceneBinaryChunkKind::PuppetSkinVertices,
            SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE,
            puppet.first_skin_vertex,
            puppet.skin_vertex_count,
            decode_puppet_skin_vertex_record,
        )?;
        let mut vertices = Vec::with_capacity(vertex_records.len());
        for vertex in vertex_records {
            vertices.push(SceneMeshSkinVertex {
                bone_indices: [
                    vertex.bone_indices[0] as usize,
                    vertex.bone_indices[1] as usize,
                    vertex.bone_indices[2] as usize,
                    vertex.bone_indices[3] as usize,
                ],
                weights: [
                    f64::from(vertex.weights[0]),
                    f64::from(vertex.weights[1]),
                    f64::from(vertex.weights[2]),
                    f64::from(vertex.weights[3]),
                ],
            });
        }
        vertices
    } else {
        Vec::new()
    };

    let attachment_records = reader.record_range(
        SceneBinaryChunkKind::PuppetAttachments,
        SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE,
        puppet.first_attachment,
        puppet.attachment_count,
        decode_puppet_attachment_record,
    )?;
    let mut attachments = Vec::with_capacity(attachment_records.len());
    for attachment in attachment_records {
        let Some(name) = binary_name(names, attachment.name) else {
            continue;
        };
        attachments.push(SceneMeshSkinAttachment {
            name: name.to_owned(),
            bone_index: attachment.bone_index as usize,
            local_position: [
                f64::from(attachment.local_position[0]),
                f64::from(attachment.local_position[1]),
                f64::from(attachment.local_position[2]),
            ],
            bind_position: [
                f64::from(attachment.bind_position[0]),
                f64::from(attachment.bind_position[1]),
                f64::from(attachment.bind_position[2]),
            ],
        });
    }

    Ok(SceneMeshSkin {
        bones,
        vertices,
        attachments,
    })
}

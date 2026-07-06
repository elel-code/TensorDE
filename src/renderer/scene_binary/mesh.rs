//! `.gscn` mesh and puppet payload decoding.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::core::scene::binary::{
    SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE, SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH,
    SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY,
    SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE, SCENE_BINARY_NONE_ID,
    SCENE_BINARY_PUPPET_ATTACHMENT_RECORD_SIZE, SCENE_BINARY_PUPPET_CLIP_FLAG_LOOPING,
    SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE, SCENE_BINARY_PUPPET_CLIPPING_BONE_RECORD_SIZE,
    SCENE_BINARY_PUPPET_CLIPPING_FRAME_KEY_RECORD_SIZE, SCENE_BINARY_PUPPET_CLIPPING_RECORD_SIZE,
    SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE, SCENE_BINARY_PUPPET_LAYER_FLAG_ADDITIVE,
    SCENE_BINARY_PUPPET_LAYER_FLAG_LOCK_TRANSFORMS, SCENE_BINARY_PUPPET_LAYER_FLAG_VISIBLE,
    SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE, SCENE_BINARY_PUPPET_SKIN_BONE_RECORD_SIZE,
    SCENE_BINARY_PUPPET_SKIN_VERTEX_RECORD_SIZE, SceneBinaryChunkKind, SceneBinaryGeometryRecord,
    SceneBinaryPuppetRecord, decode_geometry_index_record, decode_geometry_vertex_record,
    decode_puppet_attachment_record, decode_puppet_clip_record, decode_puppet_clipping_bone_record,
    decode_puppet_clipping_frame_key_record, decode_puppet_clipping_record,
    decode_puppet_frame_record, decode_puppet_layer_record, decode_puppet_skin_bone_record,
    decode_puppet_skin_vertex_record,
};
use crate::core::scene::{
    SceneMesh, SceneMeshPuppetClippingRecord, SceneMeshSkin, SceneMeshSkinAttachment,
    SceneMeshSkinBone, SceneMeshSkinVertex, SceneMeshVertex, ScenePuppetAnimationBone,
    ScenePuppetAnimationClip, ScenePuppetAnimationLayer, ScenePuppetAttachmentDelta,
};
use crate::renderer::RendererPlanError;

use super::facts::{BinarySceneNames, binary_name, binary_scene_resource_path};
use super::reader::BinarySceneReader;

pub(super) fn binary_scene_geometry_is_mesh_payload(geometry: SceneBinaryGeometryRecord) -> bool {
    geometry.primitive_kind == SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH
        && geometry.vertex_layout == SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY
}

pub(super) fn binary_scene_mesh_vertices_indices(
    reader: &mut BinarySceneReader,
    geometry: SceneBinaryGeometryRecord,
) -> Result<(Vec<SceneMeshVertex>, Vec<u32>), RendererPlanError> {
    let vertex_records = reader.record_range(
        SceneBinaryChunkKind::GeometryVertices,
        SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE,
        geometry.first_vertex,
        geometry.vertex_count,
        decode_geometry_vertex_record,
    )?;
    let index_records = reader.record_range(
        SceneBinaryChunkKind::GeometryIndices,
        SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE,
        geometry.first_index,
        geometry.index_count,
        decode_geometry_index_record,
    )?;

    let mut vertices = Vec::with_capacity(vertex_records.len());
    for vertex in vertex_records {
        vertices.push(SceneMeshVertex {
            x: f64::from(vertex.x),
            y: f64::from(vertex.y),
            u: f64::from(vertex.u),
            v: f64::from(vertex.v),
            opacity: f64::from(vertex.opacity),
        });
    }

    let mut indices = Vec::with_capacity(index_records.len());
    for index in index_records {
        indices.push(index.index);
    }

    Ok((vertices, indices))
}

pub(super) fn binary_scene_mesh(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    geometry_index: u32,
    geometry: SceneBinaryGeometryRecord,
    puppet_index: u32,
) -> Result<Option<Arc<SceneMesh>>, RendererPlanError> {
    if !binary_scene_geometry_is_mesh_payload(geometry) {
        return Ok(None);
    }
    Ok(Some(binary_scene_base_mesh_cached(
        reader,
        names,
        geometry_index,
        geometry,
        puppet_index,
    )?))
}

fn binary_scene_base_mesh_cached(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    geometry_index: u32,
    geometry: SceneBinaryGeometryRecord,
    puppet_index: u32,
) -> Result<Arc<SceneMesh>, RendererPlanError> {
    let cache_key = (geometry_index, puppet_index);
    if let Some(mesh) = reader.geometry_mesh_cache.get(&cache_key) {
        return Ok(Arc::clone(mesh));
    }

    let (vertices, indices) = binary_scene_mesh_vertices_indices(reader, geometry)?;
    let mut mesh = SceneMesh {
        vertices,
        indices,
        skin: None,
        puppet_clips: Vec::new(),
        puppet_clipping_records: Vec::new(),
    };
    if puppet_index != SCENE_BINARY_NONE_ID {
        let puppet = reader.puppet_record_cached(puppet_index)?;
        if puppet.bone_count > 0 {
            mesh.skin = Some(binary_scene_puppet_skin(reader, names, puppet, true)?);
        }
        if puppet.clip_count > 0 {
            let clips = binary_scene_puppet_clips_cached(reader, puppet_index, puppet)?;
            mesh.puppet_clips = clips.as_ref().clone();
        }
        if puppet.clipping_record_count > 0 && mesh.skin.is_some() {
            mesh.puppet_clipping_records =
                binary_scene_puppet_clipping_records(reader, names, puppet)?;
        }
    }

    let mesh = Arc::new(mesh);
    reader
        .geometry_mesh_cache
        .insert(cache_key, Arc::clone(&mesh));
    Ok(mesh)
}

pub(super) fn binary_scene_puppet_attachment_deltas(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    puppet_index: u32,
    snapshot_time_ms: u64,
) -> Result<Option<BTreeMap<String, ScenePuppetAttachmentDelta>>, RendererPlanError> {
    if puppet_index == SCENE_BINARY_NONE_ID {
        return Ok(None);
    }
    let cache_key = (puppet_index, snapshot_time_ms);
    if let Some(cached) = reader.puppet_attachment_delta_cache.get(&cache_key) {
        return Ok(cached.as_ref().map(|deltas| (**deltas).clone()));
    }
    let puppet = reader.puppet_record_cached(puppet_index)?;
    if puppet.attachment_count == 0
        || puppet.animation_layer_count == 0
        || puppet.bone_count == 0
        || puppet.clip_count == 0
    {
        reader.puppet_attachment_delta_cache.insert(cache_key, None);
        return Ok(None);
    }
    let mesh = binary_scene_puppet_attachment_mesh_cached(reader, names, puppet_index, puppet)?;
    if mesh
        .skin
        .as_ref()
        .is_none_or(|skin| skin.attachments.is_empty())
    {
        reader.puppet_attachment_delta_cache.insert(cache_key, None);
        return Ok(None);
    }
    let layers = binary_scene_puppet_layers_cached(reader, puppet_index, puppet)?;
    let clips = binary_scene_puppet_clips_cached(reader, puppet_index, puppet)?;
    let deltas = mesh.sample_puppet_attachment_deltas_with_clips(
        clips.as_slice(),
        layers.as_slice(),
        snapshot_time_ms,
    );
    reader.puppet_attachment_delta_cache.insert(
        cache_key,
        deltas.as_ref().map(|deltas| Arc::new(deltas.clone())),
    );
    Ok(deltas)
}

fn binary_scene_puppet_attachment_mesh_cached(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    puppet_index: u32,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Arc<SceneMesh>, RendererPlanError> {
    if let Some(mesh) = reader.puppet_attachment_mesh_cache.get(&puppet_index) {
        return Ok(Arc::clone(mesh));
    }
    let mesh = Arc::new(SceneMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        skin: Some(binary_scene_puppet_skin(reader, names, puppet, false)?),
        puppet_clips: Vec::new(),
        puppet_clipping_records: Vec::new(),
    });
    reader
        .puppet_attachment_mesh_cache
        .insert(puppet_index, Arc::clone(&mesh));
    Ok(mesh)
}

pub(super) fn binary_scene_puppet_layers_cached(
    reader: &mut BinarySceneReader,
    puppet_index: u32,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Arc<Vec<ScenePuppetAnimationLayer>>, RendererPlanError> {
    if let Some(layers) = reader.puppet_layers_cache.get(&puppet_index) {
        return Ok(Arc::clone(layers));
    }
    let layers = Arc::new(binary_scene_puppet_layers(reader, puppet)?);
    reader
        .puppet_layers_cache
        .insert(puppet_index, Arc::clone(&layers));
    Ok(layers)
}

pub(super) fn binary_scene_puppet_clips_cached(
    reader: &mut BinarySceneReader,
    puppet_index: u32,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Arc<Vec<ScenePuppetAnimationClip>>, RendererPlanError> {
    if let Some(clips) = reader.puppet_clips_cache.get(&puppet_index) {
        return Ok(Arc::clone(clips));
    }
    let clips = Arc::new(binary_scene_puppet_clips(reader, puppet)?);
    reader
        .puppet_clips_cache
        .insert(puppet_index, Arc::clone(&clips));
    Ok(clips)
}

pub(super) fn binary_scene_puppet_skin(
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

pub(super) fn binary_scene_puppet_clips(
    reader: &mut BinarySceneReader,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Vec<ScenePuppetAnimationClip>, RendererPlanError> {
    let clip_records = reader.record_range(
        SceneBinaryChunkKind::PuppetClips,
        SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE,
        puppet.first_clip,
        puppet.clip_count,
        decode_puppet_clip_record,
    )?;
    let mut clips = Vec::with_capacity(clip_records.len());
    for clip in clip_records {
        let frame_records = reader.record_range(
            SceneBinaryChunkKind::PuppetFrames,
            SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE,
            clip.first_frame,
            clip.frame_record_count,
            decode_puppet_frame_record,
        )?;
        let mut bones = (0..clip.bone_count)
            .map(|_| ScenePuppetAnimationBone { frames: Vec::new() })
            .collect::<Vec<_>>();
        for frame in frame_records {
            if let Some(bone) = bones.get_mut(frame.bone_index as usize) {
                bone.frames.push(frame.transform);
            }
        }
        clips.push(ScenePuppetAnimationClip {
            id: clip.clip_id,
            name: None,
            fps: f64::from(clip.fps),
            frame_count: clip.frame_count,
            looping: clip.flags & SCENE_BINARY_PUPPET_CLIP_FLAG_LOOPING != 0,
            bones,
        });
    }
    Ok(clips)
}

pub(super) fn binary_scene_puppet_clipping_records(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Vec<SceneMeshPuppetClippingRecord>, RendererPlanError> {
    let clipping_records = reader.record_range(
        SceneBinaryChunkKind::PuppetClipping,
        SCENE_BINARY_PUPPET_CLIPPING_RECORD_SIZE,
        puppet.first_clipping_record,
        puppet.clipping_record_count,
        decode_puppet_clipping_record,
    )?;
    let mut records = Vec::with_capacity(clipping_records.len());
    for clipping in clipping_records {
        let Some(mask) = binary_name(names, clipping.mask_name) else {
            continue;
        };
        let bone_records = reader.record_range(
            SceneBinaryChunkKind::PuppetClippingBones,
            SCENE_BINARY_PUPPET_CLIPPING_BONE_RECORD_SIZE,
            clipping.first_bone,
            clipping.bone_count,
            decode_puppet_clipping_bone_record,
        )?;
        let frame_key_records = reader.record_range(
            SceneBinaryChunkKind::PuppetClippingFrameKeys,
            SCENE_BINARY_PUPPET_CLIPPING_FRAME_KEY_RECORD_SIZE,
            clipping.first_frame_key,
            clipping.frame_key_count,
            decode_puppet_clipping_frame_key_record,
        )?;
        records.push(SceneMeshPuppetClippingRecord {
            mask: mask.to_owned(),
            mask_resource: binary_scene_puppet_clipping_mask_resource(reader, mask),
            duration_frames: clipping.duration_frames,
            flags: clipping.flags,
            bones: bone_records
                .iter()
                .map(|bone| bone.bone_index as usize)
                .collect(),
            frame_keys: frame_key_records
                .iter()
                .map(|frame_key| frame_key.frame_key)
                .collect(),
        });
    }
    Ok(records)
}

fn binary_scene_puppet_clipping_mask_resource(
    reader: &BinarySceneReader,
    mask: &str,
) -> Option<String> {
    if Path::new(mask).is_absolute()
        || mask.ends_with(".gtex")
        || mask.starts_with("assets/")
        || mask.starts_with("assets\\")
    {
        Some(
            binary_scene_resource_path(&reader.package_root, mask)
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        None
    }
}

pub(super) fn binary_scene_puppet_layers(
    reader: &mut BinarySceneReader,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Vec<ScenePuppetAnimationLayer>, RendererPlanError> {
    let layer_records = reader.record_range(
        SceneBinaryChunkKind::PuppetLayers,
        SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE,
        puppet.first_layer,
        puppet.animation_layer_count,
        decode_puppet_layer_record,
    )?;
    let mut layers = Vec::with_capacity(layer_records.len());
    for layer in layer_records {
        layers.push(ScenePuppetAnimationLayer {
            clip_id: layer.clip_id,
            name: None,
            additive: layer.flags & SCENE_BINARY_PUPPET_LAYER_FLAG_ADDITIVE != 0,
            lock_transforms: layer.flags & SCENE_BINARY_PUPPET_LAYER_FLAG_LOCK_TRANSFORMS != 0,
            blend: f64::from(layer.blend),
            visible: layer.flags & SCENE_BINARY_PUPPET_LAYER_FLAG_VISIBLE != 0,
            rate: f64::from(layer.rate),
            initial_phase: f64::from(layer.initial_phase),
        });
    }
    Ok(layers)
}

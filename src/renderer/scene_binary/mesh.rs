//! `.gscn` mesh and puppet payload decoding.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`

use std::sync::Arc;

use crate::core::scene::binary::{
    SCENE_BINARY_GEOMETRY_INDEX_RECORD_SIZE, SCENE_BINARY_GEOMETRY_PRIMITIVE_MESH,
    SCENE_BINARY_GEOMETRY_VERTEX_LAYOUT_MESH_XY_UV_OPACITY,
    SCENE_BINARY_GEOMETRY_VERTEX_RECORD_SIZE, SCENE_BINARY_NONE_ID, SceneBinaryChunkKind,
    SceneBinaryGeometryRecord, decode_geometry_index_record, decode_geometry_vertex_record,
};
use crate::core::scene::{SceneMesh, SceneMeshVertex};
use crate::renderer::RendererPlanError;

use super::facts::BinarySceneNames;
use super::reader::BinarySceneReader;

mod puppet;

pub(super) use puppet::binary_scene_puppet_attachment_deltas;
use puppet::binary_scene_puppet_clips_cached;
pub(super) use puppet::{
    binary_scene_puppet_active_sources, binary_scene_puppet_clipping_records,
    binary_scene_puppet_clips, binary_scene_puppet_layers, binary_scene_puppet_skin,
};

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
        puppet_clipping_active_sources: Vec::new(),
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
        if puppet.active_source_count > 0 && mesh.skin.is_some() {
            mesh.puppet_clipping_active_sources =
                binary_scene_puppet_active_sources(reader, names, puppet)?;
        }
    }

    let mesh = Arc::new(mesh);
    reader
        .geometry_mesh_cache
        .insert(cache_key, Arc::clone(&mesh));
    Ok(mesh)
}

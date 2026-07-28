//! Retained scene and generated alpha-coverage mesh upload payloads.

use crate::engine::scene::{SceneMeshVertexRecord, SceneStorage};

use super::SCENE_MESH_VERTEX_STRIDE_BYTES;
use super::fullscreen_primitive::{
    append_fullscreen_triangle_indices, append_fullscreen_triangle_vertices,
};

pub(super) fn pack_scene_vertices(
    storage: &SceneStorage,
    include_fullscreen_utility: bool,
) -> Vec<u8> {
    let vertex_count = storage
        .document()
        .mesh_vertices
        .len()
        .saturating_add(usize::from(include_fullscreen_utility) * 3);
    let mut payload = Vec::with_capacity(vertex_count * SCENE_MESH_VERTEX_STRIDE_BYTES as usize);
    for vertex in &storage.document().mesh_vertices {
        append_vertex(&mut payload, vertex);
    }
    if include_fullscreen_utility {
        append_fullscreen_triangle_vertices(&mut payload);
    }
    payload
}

pub(super) fn pack_scene_indices(
    storage: &SceneStorage,
    include_fullscreen_utility: bool,
) -> Vec<u8> {
    let index_count = storage
        .document()
        .mesh_indices
        .len()
        .saturating_add(usize::from(include_fullscreen_utility) * 3);
    let mut payload = Vec::with_capacity(index_count * 4);
    for index in &storage.document().mesh_indices {
        payload.extend_from_slice(&index.to_le_bytes());
    }
    if include_fullscreen_utility {
        append_fullscreen_triangle_indices(&mut payload);
    }
    payload
}

fn append_vertex(payload: &mut Vec<u8>, vertex: &SceneMeshVertexRecord) {
    for value in [
        vertex.position.x,
        vertex.position.y,
        vertex.uv[0],
        vertex.uv[1],
        1.0,
    ] {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    for index in vertex.blend_indices {
        payload.extend_from_slice(&index.to_le_bytes());
    }
    for weight in vertex.blend_weights {
        payload.extend_from_slice(&weight.to_le_bytes());
    }
}

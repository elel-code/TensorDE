//! Retained scene and generated alpha-coverage mesh upload payloads.

use crate::engine::scene::{
    SceneMeshVertexRecord, SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan,
    SceneStorage,
};

use super::SCENE_MESH_VERTEX_STRIDE_BYTES;
use super::fullscreen_primitive::{
    append_fullscreen_triangle_indices, append_fullscreen_triangle_vertices,
};

pub(super) fn pack_scene_vertices(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
) -> Result<Vec<u8>, String> {
    let object_composite_vertex_count = graph
        .mesh_draws
        .iter()
        .filter(|draw| draw.uv_inset_texels > 0.0)
        .map(|draw| draw.vertex_count as usize)
        .sum::<usize>();
    let vertex_count = storage
        .document()
        .mesh_vertices
        .len()
        .saturating_add(graph.fullscreen_utility_draw_count() * 3)
        .saturating_add(object_composite_vertex_count);
    let mut payload = Vec::with_capacity(vertex_count * SCENE_MESH_VERTEX_STRIDE_BYTES as usize);
    for vertex in &storage.document().mesh_vertices {
        append_vertex(&mut payload, vertex);
    }
    for draw in graph
        .mesh_draws
        .iter()
        .filter(|draw| draw.primitive == SceneRenderingDeviceDrawPrimitive::FullscreenTriangle)
    {
        append_fullscreen_triangle_vertices(&mut payload, draw.authored_source_extent);
    }
    for draw in graph
        .mesh_draws
        .iter()
        .filter(|draw| draw.uv_inset_texels > 0.0)
    {
        append_object_composite_vertices(&mut payload, storage, draw)?;
    }
    Ok(payload)
}

pub(super) fn pack_scene_indices(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
) -> Vec<u8> {
    let index_count = storage
        .document()
        .mesh_indices
        .len()
        .saturating_add(graph.fullscreen_utility_draw_count() * 3);
    let mut payload = Vec::with_capacity(index_count * 4);
    for index in &storage.document().mesh_indices {
        payload.extend_from_slice(&index.to_le_bytes());
    }
    for _ in 0..graph.fullscreen_utility_draw_count() {
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

fn append_object_composite_vertices(
    payload: &mut Vec<u8>,
    storage: &SceneStorage,
    draw: &crate::engine::scene::SceneRenderingDeviceMeshDraw,
) -> Result<(), String> {
    if draw.primitive != SceneRenderingDeviceDrawPrimitive::ObjectMesh {
        return Err("object-composite UV inset requires an indexed ObjectMesh draw".to_owned());
    }
    let start = draw.vertex_start as usize;
    let end = start.saturating_add(draw.vertex_count as usize);
    let vertices = storage
        .document()
        .mesh_vertices
        .get(start..end)
        .ok_or_else(|| {
            format!(
                "object-composite mesh vertex range {start}..{end} exceeds retained vertex count {}",
                storage.document().mesh_vertices.len()
            )
        })?;
    for vertex in vertices {
        append_object_composite_vertex(
            payload,
            vertex,
            draw.authored_source_extent,
            draw.uv_inset_texels,
        )?;
    }
    Ok(())
}

fn append_object_composite_vertex(
    payload: &mut Vec<u8>,
    vertex: &SceneMeshVertexRecord,
    source_extent: [f32; 2],
    inset_texels: f32,
) -> Result<(), String> {
    if !inset_texels.is_finite() || inset_texels <= 0.0 {
        return Err(format!(
            "object-composite UV inset must be finite and positive, got {inset_texels}"
        ));
    }
    let mut inset = [0.0; 2];
    for axis in 0..2 {
        let extent = source_extent[axis];
        if !extent.is_finite() || extent <= inset_texels * 2.0 {
            return Err(format!(
                "object-composite source extent axis {axis} must exceed twice the UV inset, got {extent}"
            ));
        }
        inset[axis] = inset_texels / extent;
    }
    let mut padded = *vertex;
    padded.uv = [
        inset_uv_component(vertex.uv[0], inset[0]),
        inset_uv_component(vertex.uv[1], inset[1]),
    ];
    append_vertex(payload, &padded);
    Ok(())
}

fn inset_uv_component(value: f32, inset: f32) -> f32 {
    if value == 0.0 {
        inset
    } else if value == 1.0 {
        1.0 - inset
    } else {
        inset + value * ((1.0 - inset) - inset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::SceneVec3;

    #[test]
    fn object_composite_uv_matches_verified_vm_secondary_quad_bits() {
        let vertex = |uv| SceneMeshVertexRecord {
            position: SceneVec3 {
                x: 17.0,
                y: -23.0,
                z: 9.0,
            },
            uv,
            blend_indices: [1, 2, 3, 4],
            blend_weights: [0.1, 0.2, 0.3, 0.4],
        };
        let mut payload = Vec::new();
        append_object_composite_vertex(&mut payload, &vertex([0.0, 0.0]), [2560.0, 1152.0], 0.15)
            .expect("low UV");
        append_object_composite_vertex(&mut payload, &vertex([1.0, 1.0]), [2560.0, 1152.0], 0.15)
            .expect("high UV");
        append_object_composite_vertex(&mut payload, &vertex([0.0, 0.0]), [1579.0, 956.0], 0.15)
            .expect("second low UV");
        append_object_composite_vertex(&mut payload, &vertex([1.0, 1.0]), [1579.0, 956.0], 0.15)
            .expect("second high UV");

        let uv_bits = |vertex: usize, axis: usize| {
            let offset = vertex * SCENE_MESH_VERTEX_STRIDE_BYTES as usize + 8 + axis * 4;
            u32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
        };
        assert_eq!(uv_bits(0, 0), 0x3875_c290);
        assert_eq!(uv_bits(0, 1), 0x3908_8889);
        assert_eq!(uv_bits(1, 0), 0x3f7f_fc29);
        assert_eq!(uv_bits(1, 1), 0x3f7f_f777);
        assert_eq!(uv_bits(2, 0), 0x38c7_390a);
        assert_eq!(uv_bits(2, 1), 0x3924_8689);
        assert_eq!(uv_bits(3, 0), 0x3f7f_f9c6);
        assert_eq!(uv_bits(3, 1), 0x3f7f_f5b8);
        assert_eq!(f32_at(&payload, 0), 17.0);
        assert_eq!(f32_at(&payload, 4), -23.0);
    }

    fn f32_at(payload: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }
}

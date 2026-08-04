//! Mesh payload chunk encoding and decoding.

use super::*;

pub(in crate::engine::scene::binary) fn encode_meshes(
    meshes: &[SceneMeshRecord],
    vertices: &[SceneMeshVertexRecord],
    indices: &[u32],
    source_records: &[SceneMeshSourceRecord],
    clipping_subdraws: &[SceneMeshClippingSubdrawRecord],
    clipping_source_ordinals: &[u32],
    clipping_slices: &[SceneMeshClippingSliceRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(&mut out, checked_u32(meshes.len(), "mesh count")?);
    for record in meshes {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.material.0);
        put_u32(&mut out, record.vertex_start);
        put_u32(&mut out, record.vertex_count);
        put_u32(&mut out, record.index_start);
        put_u32(&mut out, record.index_count);
        put_f32(&mut out, record.width);
        put_f32(&mut out, record.height);
        put_vec3(&mut out, record.bounds_min);
        put_vec3(&mut out, record.bounds_max);
    }
    put_u32(&mut out, checked_u32(vertices.len(), "mesh vertex count")?);
    for vertex in vertices {
        put_vec3(&mut out, vertex.position);
        put_f32(&mut out, vertex.uv[0]);
        put_f32(&mut out, vertex.uv[1]);
        for index in vertex.blend_indices {
            put_u32(&mut out, index);
        }
        for weight in vertex.blend_weights {
            put_f32(&mut out, weight);
        }
    }
    put_u32(&mut out, checked_u32(indices.len(), "mesh index count")?);
    for index in indices {
        put_u32(&mut out, *index);
    }
    mesh_clipping::encode(
        &mut out,
        source_records,
        clipping_subdraws,
        clipping_source_ordinals,
        clipping_slices,
    )?;
    Ok(out)
}

pub(in crate::engine::scene::binary) fn decode_meshes(
    data: &[u8],
) -> Result<mesh_clipping::MeshDecode, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let mesh_count = decoder.u32()? as usize;
    let mut meshes = Vec::with_capacity(mesh_count);
    for _ in 0..mesh_count {
        meshes.push(SceneMeshRecord {
            object: SceneObjectHandle(decoder.u32()?),
            material: SceneMaterialHandle(decoder.u32()?),
            vertex_start: decoder.u32()?,
            vertex_count: decoder.u32()?,
            index_start: decoder.u32()?,
            index_count: decoder.u32()?,
            width: decoder.f32()?,
            height: decoder.f32()?,
            bounds_min: decoder.vec3()?,
            bounds_max: decoder.vec3()?,
        });
    }
    let vertex_count = decoder.u32()? as usize;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertices.push(SceneMeshVertexRecord {
            position: decoder.vec3()?,
            uv: [decoder.f32()?, decoder.f32()?],
            blend_indices: [
                decoder.u32()?,
                decoder.u32()?,
                decoder.u32()?,
                decoder.u32()?,
            ],
            blend_weights: [
                decoder.f32()?,
                decoder.f32()?,
                decoder.f32()?,
                decoder.f32()?,
            ],
        });
    }
    let index_count = decoder.u32()? as usize;
    let mut indices = Vec::with_capacity(index_count);
    for _ in 0..index_count {
        indices.push(decoder.u32()?);
    }
    let (source_records, clipping_subdraws, clipping_source_ordinals, clipping_slices) =
        mesh_clipping::decode(&mut decoder)?;
    Ok((
        meshes,
        vertices,
        indices,
        source_records,
        clipping_subdraws,
        clipping_source_ordinals,
        clipping_slices,
    ))
}

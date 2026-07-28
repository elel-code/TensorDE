//! MDLV source-record and clipping-subdraw binary payload.

use super::*;

pub(super) type MeshDecode = (
    Vec<SceneMeshRecord>,
    Vec<SceneMeshVertexRecord>,
    Vec<u32>,
    Vec<SceneMeshSourceRecord>,
    Vec<SceneMeshClippingSubdrawRecord>,
    Vec<u32>,
    Vec<SceneMeshClippingSliceRecord>,
);

pub(super) fn encode(
    out: &mut Vec<u8>,
    source_records: &[SceneMeshSourceRecord],
    clipping_subdraws: &[SceneMeshClippingSubdrawRecord],
    clipping_source_ordinals: &[u32],
    clipping_slices: &[SceneMeshClippingSliceRecord],
) -> Result<(), SceneBinaryError> {
    put_u32(
        out,
        checked_u32(source_records.len(), "mesh source record count")?,
    );
    for record in source_records {
        put_u32(out, record.mesh);
        put_u32(out, record.source_index);
        put_u32(out, record.local_index_offset);
        put_u32(out, record.index_start);
        put_u32(out, record.index_count);
    }
    put_u32(
        out,
        checked_u32(clipping_subdraws.len(), "mesh clipping subdraw count")?,
    );
    for record in clipping_subdraws {
        put_u32(out, record.mesh);
        put_u64(out, record.source_qword);
        put_string_id(out, record.mask);
        put_u32(out, record.mask_resource.0);
        put_u32(out, record.raw_flags);
        put_u32(out, record.target_source_start);
        put_u32(out, record.target_source_count);
        put_u32(out, record.mask_source_start);
        put_u32(out, record.mask_source_count);
    }
    put_u32(
        out,
        checked_u32(
            clipping_source_ordinals.len(),
            "mesh clipping source ordinal count",
        )?,
    );
    for ordinal in clipping_source_ordinals {
        put_u32(out, *ordinal);
    }
    put_u32(
        out,
        checked_u32(clipping_slices.len(), "mesh clipping slice count")?,
    );
    for slice in clipping_slices {
        put_u32(out, slice.mesh);
        put_u32(out, slice.subdraw);
        put_u32(out, slice.role.to_u32());
        put_u32(out, slice.index_start);
        put_u32(out, slice.index_count);
    }
    Ok(())
}

type DecodedMeshClipping = (
    Vec<SceneMeshSourceRecord>,
    Vec<SceneMeshClippingSubdrawRecord>,
    Vec<u32>,
    Vec<SceneMeshClippingSliceRecord>,
);

pub(super) fn decode(decoder: &mut Decoder<'_>) -> Result<DecodedMeshClipping, SceneBinaryError> {
    let source_record_count = decoder.u32()? as usize;
    let mut source_records = Vec::with_capacity(source_record_count);
    for _ in 0..source_record_count {
        source_records.push(SceneMeshSourceRecord {
            mesh: decoder.u32()?,
            source_index: decoder.u32()?,
            local_index_offset: decoder.u32()?,
            index_start: decoder.u32()?,
            index_count: decoder.u32()?,
        });
    }
    let subdraw_count = decoder.u32()? as usize;
    let mut subdraws = Vec::with_capacity(subdraw_count);
    for _ in 0..subdraw_count {
        subdraws.push(SceneMeshClippingSubdrawRecord {
            mesh: decoder.u32()?,
            source_qword: decoder.u64()?,
            mask: decoder.string_id()?,
            mask_resource: decoder.resource_id()?,
            raw_flags: decoder.u32()?,
            target_source_start: decoder.u32()?,
            target_source_count: decoder.u32()?,
            mask_source_start: decoder.u32()?,
            mask_source_count: decoder.u32()?,
        });
    }
    let ordinal_count = decoder.u32()? as usize;
    let mut ordinals = Vec::with_capacity(ordinal_count);
    for _ in 0..ordinal_count {
        ordinals.push(decoder.u32()?);
    }
    let slice_count = decoder.u32()? as usize;
    let mut slices = Vec::with_capacity(slice_count);
    for _ in 0..slice_count {
        let mesh = decoder.u32()?;
        let subdraw = decoder.u32()?;
        let role_raw = decoder.u32()?;
        let role = SceneMeshClippingSliceRole::from_u32(role_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("mesh clipping slice role", role_raw),
        )?;
        slices.push(SceneMeshClippingSliceRecord {
            mesh,
            subdraw,
            role,
            index_start: decoder.u32()?,
            index_count: decoder.u32()?,
        });
    }
    Ok((source_records, subdraws, ordinals, slices))
}

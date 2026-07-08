//! MDLV0023 raw entry geometry extraction for native scene resources.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`

use std::fs;
use std::path::Path;

use crate::engine::scene_engine::{
    SceneLayerAlphaMaskRtMethod8MdlvSourceRecord, SceneLayerAlphaMaskRtMethod8MdlvSubdraw,
};
use crate::renderer::RendererPlanError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BinarySceneMdlvEntryGeometry {
    pub(super) entry_owner_index: u32,
    pub(super) layout_key: u32,
    pub(super) vertex_stride_bytes: u32,
    pub(super) vertex_count: u32,
    pub(super) index_count: u32,
    pub(super) vertex_payload: Vec<u8>,
    pub(super) index_payload: Vec<u8>,
    pub(super) source_records: Vec<SceneLayerAlphaMaskRtMethod8MdlvSourceRecord>,
    pub(super) subdraws: Vec<SceneLayerAlphaMaskRtMethod8MdlvSubdraw>,
}

pub(super) fn binary_scene_mdlv_first_entry_geometry(
    path: &Path,
) -> Result<Option<BinarySceneMdlvEntryGeometry>, RendererPlanError> {
    let Ok(bytes) = fs::read(path) else {
        return Ok(None);
    };
    if !bytes.starts_with(b"MDLV") {
        return Ok(None);
    }
    if bytes.get(8) != Some(&0) {
        return Err(mdlv_error(path, "MDLV magic is not NUL-terminated"));
    }
    let version = std::str::from_utf8(bytes.get(4..8).unwrap_or_default())
        .ok()
        .and_then(|version| version.parse::<u32>().ok())
        .ok_or_else(|| mdlv_error(path, "MDLV version digits are invalid"))?;
    if version < 15 {
        return Err(mdlv_error(
            path,
            "MDLV version does not carry entry layout keys",
        ));
    }

    let mut cursor = BinarySceneMdlvCursor::new(&bytes, path);
    cursor.skip(9)?;
    let _file_layout_key = cursor.take_u32("MDLV file layout key")?;
    let material_count = cursor.take_u32("MDLV material count")?;
    let entry_count = cursor.take_u32("MDLV entry count")?;
    if material_count > 4096 || entry_count == 0 || entry_count > 4096 {
        return Err(mdlv_error(path, "MDLV header counts are unreasonable"));
    }
    for _ in 0..material_count {
        cursor.take_c_string("MDLV material path")?;
    }

    let entry_owner_index = 0;
    let entry_flags = if version >= 4 {
        let flags = cursor.take_u32("MDLV entry flags")?;
        if flags & 0x2 != 0 {
            cursor.take_u32("MDLV entry flags extra")?;
        }
        flags
    } else {
        0
    };
    if version >= 17 {
        cursor.skip(24)?;
    }
    let layout_key = cursor.take_u32("MDLV entry layout key")?;
    let vertex_bytes = cursor.take_u32("MDLV vertex bytes")?;
    let vertex_payload = cursor.take_bytes(vertex_bytes as usize, "MDLV vertex payload")?;
    let index_bytes = cursor.take_u32("MDLV index bytes")?;
    let index_payload = cursor.take_bytes(index_bytes as usize, "MDLV index payload")?;
    if index_bytes % 2 != 0 {
        return Err(mdlv_error(path, "MDLV index payload is not R16-aligned"));
    }
    let mut source_records = Vec::new();
    let mut subdraws = Vec::new();
    if version >= 21 {
        if cursor.take_u8("MDLV optional byte block flag A")? != 0 {
            cursor.take_u32("MDLV optional byte block A metadata")?;
            cursor.skip_byte_block("MDLV optional byte block A")?;
        }
        if cursor.take_u8("MDLV optional byte block flag B")? != 0 {
            let records = cursor.take_byte_block("MDLV optional byte block B")?;
            if records.len() % 16 != 0 {
                return Err(mdlv_error(
                    path,
                    "MDLV optional byte block B is not aligned to 16-byte source records",
                ));
            }
            source_records.reserve(records.len() / 16);
            for record in records.chunks_exact(16) {
                source_records.push(SceneLayerAlphaMaskRtMethod8MdlvSourceRecord {
                    source_index: u32::from_le_bytes(record[0..4].try_into().unwrap()),
                    local_offset: u32::from_le_bytes(record[4..8].try_into().unwrap()),
                    index_span_offset: u32::from_le_bytes(record[8..12].try_into().unwrap()),
                    index_span_count: u32::from_le_bytes(record[12..16].try_into().unwrap()),
                });
            }
        }
    }
    if version >= 23 {
        let subdraw_count = cursor.take_u32("MDLV clipping subdraw count")?;
        if subdraw_count > 1_000_000 {
            return Err(mdlv_error(
                path,
                "MDLV clipping subdraw count is unreasonable",
            ));
        }
        for _ in 0..subdraw_count {
            let source_qword = cursor.take_u64("MDLV subdraw source qword")?;
            let mask_resource = cursor.take_c_string("MDLV subdraw mask")?.to_owned();
            let raw_flags = cursor.take_u32("MDLV subdraw flags")?;
            let first_indices = cursor.take_u32_list("MDLV subdraw first index list")?;
            let second_indices = cursor.take_u32_list("MDLV subdraw second index list")?;
            validate_mdlv_subdraw_indices(path, &source_records, &first_indices)?;
            validate_mdlv_subdraw_indices(path, &source_records, &second_indices)?;
            subdraws.push(SceneLayerAlphaMaskRtMethod8MdlvSubdraw {
                source_qword,
                mask_resource,
                raw_flags,
                first_indices,
                second_indices,
                link: u32::MAX,
            });
        }
        assign_mdlv_subdraw_links(&mut subdraws);
    }

    let vertex_stride_bytes = mdlv_layout_stride_bytes(layout_key).ok_or_else(|| {
        mdlv_error(
            path,
            &format!("MDLV entry layout key 0x{layout_key:08x} has unknown stride"),
        )
    })?;
    if vertex_stride_bytes == 0 || vertex_bytes % vertex_stride_bytes != 0 {
        return Err(mdlv_error(
            path,
            "MDLV vertex payload is not aligned to recovered layout stride",
        ));
    }

    let _usage_flags = entry_flags;
    Ok(Some(BinarySceneMdlvEntryGeometry {
        entry_owner_index,
        layout_key,
        vertex_stride_bytes,
        vertex_count: vertex_bytes / vertex_stride_bytes,
        index_count: index_bytes / 2,
        vertex_payload,
        index_payload,
        source_records,
        subdraws,
    }))
}

fn validate_mdlv_subdraw_indices(
    path: &Path,
    source_records: &[SceneLayerAlphaMaskRtMethod8MdlvSourceRecord],
    indices: &[u32],
) -> Result<(), RendererPlanError> {
    for index in indices {
        let Some(index) = usize::try_from(*index).ok() else {
            return Err(mdlv_error(
                path,
                "MDLV subdraw index references outside optional-B source records",
            ));
        };
        if index >= source_records.len() {
            return Err(mdlv_error(
                path,
                "MDLV subdraw index references outside optional-B source records",
            ));
        }
    }
    Ok(())
}

fn assign_mdlv_subdraw_links(subdraws: &mut [SceneLayerAlphaMaskRtMethod8MdlvSubdraw]) {
    for current in 0..subdraws.len() {
        if subdraws[current].second_indices.is_empty() {
            continue;
        }
        if let Some(prior) = (0..current).find(|prior| {
            subdraws[current]
                .second_indices
                .iter()
                .all(|index| subdraws[*prior].first_indices.contains(index))
        }) {
            subdraws[current].link = prior as u32;
        }
    }
}

fn mdlv_layout_stride_bytes(layout_key: u32) -> Option<u32> {
    let mut stride = 0u32;
    let attributes = [
        (0x0000_0001, 12),
        (0x0000_0002, 12),
        (0x0000_0004, 16),
        (0x0000_0008, 8),
        (0x0000_0010, 12),
        (0x0080_0000, 16),
        (0x0100_0000, 16),
        (0x0200_0000, 12),
        (0x0001_0000, 16),
    ];
    let mut remaining = layout_key;
    for (bit, bytes) in attributes {
        if layout_key & bit != 0 {
            stride = stride.checked_add(bytes)?;
            remaining &= !bit;
        }
    }
    (remaining == 0).then_some(stride)
}

struct BinarySceneMdlvCursor<'a> {
    bytes: &'a [u8],
    path: &'a Path,
    position: usize,
}

impl<'a> BinarySceneMdlvCursor<'a> {
    fn new(bytes: &'a [u8], path: &'a Path) -> Self {
        Self {
            bytes,
            path,
            position: 0,
        }
    }

    fn skip(&mut self, bytes: usize) -> Result<(), RendererPlanError> {
        self.position = self
            .position
            .checked_add(bytes)
            .ok_or_else(|| mdlv_error(self.path, "MDLV cursor overflow"))?;
        if self.position > self.bytes.len() {
            return Err(mdlv_error(self.path, "MDLV cursor moved past EOF"));
        }
        Ok(())
    }

    fn take_u32(&mut self, field: &str) -> Result<u32, RendererPlanError> {
        let end = self
            .position
            .checked_add(4)
            .ok_or_else(|| mdlv_error(self.path, "MDLV u32 cursor overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is out of bounds")))?;
        self.position = end;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn take_u64(&mut self, field: &str) -> Result<u64, RendererPlanError> {
        let end = self
            .position
            .checked_add(8)
            .ok_or_else(|| mdlv_error(self.path, "MDLV u64 cursor overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is out of bounds")))?;
        self.position = end;
        Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn take_u8(&mut self, field: &str) -> Result<u8, RendererPlanError> {
        let byte = self
            .bytes
            .get(self.position)
            .copied()
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is out of bounds")))?;
        self.position += 1;
        Ok(byte)
    }

    fn take_bytes(&mut self, count: usize, field: &str) -> Result<Vec<u8>, RendererPlanError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| mdlv_error(self.path, "MDLV byte block cursor overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is out of bounds")))?;
        self.position = end;
        Ok(bytes.to_vec())
    }

    fn take_byte_block(&mut self, field: &str) -> Result<&'a [u8], RendererPlanError> {
        let count = self.take_u32(field)? as usize;
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| mdlv_error(self.path, "MDLV byte block cursor overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is out of bounds")))?;
        self.position = end;
        Ok(bytes)
    }

    fn skip_byte_block(&mut self, field: &str) -> Result<(), RendererPlanError> {
        let count = self.take_u32(field)? as usize;
        self.skip(count)
    }

    fn take_u32_list(&mut self, field: &str) -> Result<Vec<u32>, RendererPlanError> {
        let count = self.take_u32(field)? as usize;
        if count > 1_000_000 {
            return Err(mdlv_error(
                self.path,
                &format!("{field} count is unreasonable"),
            ));
        }
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.take_u32(field)?);
        }
        Ok(values)
    }

    fn take_c_string(&mut self, field: &str) -> Result<&'a str, RendererPlanError> {
        let relative_end = self.bytes[self.position..]
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is not NUL-terminated")))?;
        let end = self.position + relative_end;
        let value = std::str::from_utf8(&self.bytes[self.position..end])
            .map_err(|_| mdlv_error(self.path, &format!("{field} is not valid UTF-8")))?;
        self.position = end + 1;
        Ok(value)
    }
}

fn mdlv_error(path: &Path, message: &str) -> RendererPlanError {
    RendererPlanError::PackageLoad(format!(
        "failed to read MDLV raw scene geometry {}: {message}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdlv_layout_stride_matches_recovered_eye_layout() {
        assert_eq!(mdlv_layout_stride_bytes(0x0180_000f), Some(80));
        assert_eq!(mdlv_layout_stride_bytes(0x0000_0009), Some(20));
        assert_eq!(mdlv_layout_stride_bytes(0x8000_0000), None);
    }
}

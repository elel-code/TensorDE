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

mod cursor;
mod error;
mod layout;
mod source_records;
mod subdraw;

use cursor::BinarySceneMdlvCursor;
use error::mdlv_error;
use layout::mdlv_layout_stride_bytes;
use source_records::binary_scene_mdlv_v21_source_records;
use subdraw::binary_scene_mdlv_v23_subdraws;

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
    let source_records = if version >= 21 {
        binary_scene_mdlv_v21_source_records(&mut cursor, path)?
    } else {
        Vec::new()
    };
    let subdraws = if version >= 23 {
        binary_scene_mdlv_v23_subdraws(&mut cursor, path, &source_records)?
    } else {
        Vec::new()
    };

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

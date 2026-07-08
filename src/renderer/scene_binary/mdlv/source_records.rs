//! MDLV v21 optional-B source record extraction.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`

use std::path::Path;

use crate::engine::scene_engine::SceneLayerAlphaMaskRtMethod8MdlvSourceRecord;
use crate::renderer::RendererPlanError;

use super::cursor::BinarySceneMdlvCursor;
use super::error::mdlv_error;

const MDLV_SOURCE_RECORD_BYTES: usize = 16;

pub(super) fn binary_scene_mdlv_v21_source_records(
    cursor: &mut BinarySceneMdlvCursor<'_>,
    path: &Path,
) -> Result<Vec<SceneLayerAlphaMaskRtMethod8MdlvSourceRecord>, RendererPlanError> {
    if cursor.take_u8("MDLV optional byte block flag A")? != 0 {
        cursor.take_u32("MDLV optional byte block A metadata")?;
        cursor.skip_byte_block("MDLV optional byte block A")?;
    }
    if cursor.take_u8("MDLV optional byte block flag B")? == 0 {
        return Ok(Vec::new());
    }
    let records = cursor.take_byte_block("MDLV optional byte block B")?;
    binary_scene_mdlv_source_records_from_bytes(path, records)
}

fn binary_scene_mdlv_source_records_from_bytes(
    path: &Path,
    records: &[u8],
) -> Result<Vec<SceneLayerAlphaMaskRtMethod8MdlvSourceRecord>, RendererPlanError> {
    if records.len() % MDLV_SOURCE_RECORD_BYTES != 0 {
        return Err(mdlv_error(
            path,
            "MDLV optional byte block B is not aligned to 16-byte source records",
        ));
    }
    let mut source_records = Vec::with_capacity(records.len() / MDLV_SOURCE_RECORD_BYTES);
    for record in records.chunks_exact(MDLV_SOURCE_RECORD_BYTES) {
        source_records.push(SceneLayerAlphaMaskRtMethod8MdlvSourceRecord {
            source_index: u32::from_le_bytes(record[0..4].try_into().unwrap()),
            local_offset: u32::from_le_bytes(record[4..8].try_into().unwrap()),
            index_span_offset: u32::from_le_bytes(record[8..12].try_into().unwrap()),
            index_span_count: u32::from_le_bytes(record[12..16].try_into().unwrap()),
        });
    }
    Ok(source_records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_records_parse_16_byte_records() {
        let bytes = [
            7u32.to_le_bytes(),
            8u32.to_le_bytes(),
            9u32.to_le_bytes(),
            10u32.to_le_bytes(),
        ]
        .concat();
        let records =
            binary_scene_mdlv_source_records_from_bytes(Path::new("unit.mdl"), &bytes).unwrap();
        assert_eq!(
            records,
            vec![SceneLayerAlphaMaskRtMethod8MdlvSourceRecord {
                source_index: 7,
                local_offset: 8,
                index_span_offset: 9,
                index_span_count: 10,
            }]
        );
    }
}

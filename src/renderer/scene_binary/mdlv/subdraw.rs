//! MDLV v23 clipping subdraw extraction.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`

use std::path::Path;

use crate::engine::scene_engine::{
    SceneLayerAlphaMaskRtMethod8MdlvSourceRecord, SceneLayerAlphaMaskRtMethod8MdlvSubdraw,
};
use crate::renderer::RendererPlanError;

use super::cursor::BinarySceneMdlvCursor;
use super::error::mdlv_error;

pub(super) fn binary_scene_mdlv_v23_subdraws(
    cursor: &mut BinarySceneMdlvCursor<'_>,
    path: &Path,
    source_records: &[SceneLayerAlphaMaskRtMethod8MdlvSourceRecord],
) -> Result<Vec<SceneLayerAlphaMaskRtMethod8MdlvSubdraw>, RendererPlanError> {
    let subdraw_count = cursor.take_u32("MDLV clipping subdraw count")?;
    if subdraw_count > 1_000_000 {
        return Err(mdlv_error(
            path,
            "MDLV clipping subdraw count is unreasonable",
        ));
    }
    let mut subdraws = Vec::with_capacity(subdraw_count as usize);
    for _ in 0..subdraw_count {
        let source_qword = cursor.take_u64("MDLV subdraw source qword")?;
        let mask_resource = cursor.take_c_string("MDLV subdraw mask")?.to_owned();
        let raw_flags = cursor.take_u32("MDLV subdraw flags")?;
        let first_indices = cursor.take_u32_list("MDLV subdraw first index list")?;
        let second_indices = cursor.take_u32_list("MDLV subdraw second index list")?;
        validate_mdlv_subdraw_indices(path, source_records, &first_indices)?;
        validate_mdlv_subdraw_indices(path, source_records, &second_indices)?;
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
    Ok(subdraws)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn subdraw(
        first_indices: &[u32],
        second_indices: &[u32],
    ) -> SceneLayerAlphaMaskRtMethod8MdlvSubdraw {
        SceneLayerAlphaMaskRtMethod8MdlvSubdraw {
            source_qword: 0,
            mask_resource: String::new(),
            raw_flags: 0,
            first_indices: first_indices.to_vec(),
            second_indices: second_indices.to_vec(),
            link: u32::MAX,
        }
    }

    #[test]
    fn subdraw_link_uses_prior_first_list_containment() {
        let mut subdraws = vec![subdraw(&[1, 2, 3], &[]), subdraw(&[4], &[1, 3])];
        assign_mdlv_subdraw_links(&mut subdraws);
        assert_eq!(subdraws[0].link, u32::MAX);
        assert_eq!(subdraws[1].link, 0);
    }
}

//! Typed render-target extent resolution.

use super::*;

pub(super) fn authored_texture_space_target_extent(
    storage: &SceneStorage,
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> Option<[u32; 2]> {
    (target_extent_domain(storage, graph_index, target, target_name)
        == SceneTargetExtentDomain::OwnerAuthored)
        .then(|| {
            let base = authored_graph_extent(storage, graph_index)?;
            Some(
                image_target(storage, target, target_name)
                    .map(|target| target.scaled_extent(base))
                    .unwrap_or(base),
            )
        })
        .flatten()
}

pub(super) fn image_target(
    storage: &SceneStorage,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> Option<&SceneImageTargetRecord> {
    storage
        .document()
        .image_targets
        .iter()
        .find(|record| record.role == target && record.name == target_name)
}

pub(super) fn target_extent_domain(
    storage: &SceneStorage,
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> SceneTargetExtentDomain {
    let declared = image_target(storage, target, target_name)
        .map(|record| record.extent_domain)
        .unwrap_or_else(|| match target {
            SceneRenderTargetKind::ImageLocalMain | SceneRenderTargetKind::ImageLocalSub => {
                SceneTargetExtentDomain::GraphSource
            }
            _ => SceneTargetExtentDomain::PhysicalSurface,
        });
    if declared != SceneTargetExtentDomain::GraphSource {
        return declared;
    }
    match storage
        .render_graphs()
        .get(graph_index as usize)
        .map(|graph| graph.source_extent_domain)
        .unwrap_or(SceneRenderSourceExtentDomain::OwnerAuthored)
    {
        SceneRenderSourceExtentDomain::PhysicalSurface => SceneTargetExtentDomain::PhysicalSurface,
        SceneRenderSourceExtentDomain::OwnerAuthored => SceneTargetExtentDomain::OwnerAuthored,
    }
}

fn authored_graph_extent(storage: &SceneStorage, graph_index: u32) -> Option<[u32; 2]> {
    let graph = storage.render_graphs().get(graph_index as usize)?;
    let [width, height] = authored_source_extent(storage, graph.object);
    (width.is_finite() && height.is_finite() && width >= 1.0 && height >= 1.0)
        .then_some([width.round() as u32, height.round() as u32])
}

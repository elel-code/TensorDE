use std::collections::BTreeSet;

use super::*;

pub(super) fn rows_from_column_major(matrix: [f32; 16]) -> [[f32; 4]; 4] {
    [
        [matrix[0], matrix[4], matrix[8], matrix[12]],
        [matrix[1], matrix[5], matrix[9], matrix[13]],
        [matrix[2], matrix[6], matrix[10], matrix[14]],
        [matrix[3], matrix[7], matrix[11], matrix[15]],
    ]
}

pub(super) fn skinning_palette_start(
    palettes: &[SceneRenderingDevicePuppetBonePalette],
    object: SceneObjectHandle,
) -> u32 {
    palettes
        .iter()
        .find(|palette| palette.object == object && palette.resolved_visible)
        .map(|palette| palette.bone_matrix_start)
        .unwrap_or(INVALID_OBJECT_ID)
}

pub(super) fn skinning_palette_count(
    palettes: &[SceneRenderingDevicePuppetBonePalette],
    object: SceneObjectHandle,
) -> u32 {
    palettes
        .iter()
        .find(|palette| palette.object == object && palette.resolved_visible)
        .map(|palette| palette.bone_matrix_count)
        .unwrap_or(0)
}

pub(super) fn pass_draw_material(
    pass: &SceneRenderPassRecord,
    mesh_material: SceneMaterialHandle,
) -> SceneMaterialHandle {
    if pass.material.0 == INVALID_MATERIAL_ID {
        mesh_material
    } else {
        pass.material
    }
}

pub(super) fn material_sampled_bindings(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
) -> Vec<SceneRenderingDeviceMaterialSampledBinding> {
    let mut bindings = Vec::new();
    for (draw_index, draw) in draws.iter().enumerate() {
        let Some(material) = storage.material(draw.material) else {
            continue;
        };
        let Some(pass) = storage.material_passes(material).first() else {
            continue;
        };
        bindings.extend(
            storage
                .material_pass_textures(pass)
                .iter()
                .filter(|texture| storage.texture(texture.resource).is_some())
                .map(|texture| SceneRenderingDeviceMaterialSampledBinding {
                    draw_index: draw_index as u32,
                    slot: texture.slot,
                    resource: texture.resource,
                }),
        );
    }
    bindings
}

pub(super) fn sampled_binding(
    storage: &SceneStorage,
    graph_index: u32,
    pass_node_index: u32,
    mesh_draw_start: u32,
    mesh_draw_count: u32,
    binding: &SceneRenderBindingRecord,
) -> Option<SceneRenderingDeviceSampledBinding> {
    if !matches!(
        binding.kind,
        SceneRenderBindingKind::SourceTexture
            | SceneRenderBindingKind::TextureSlot
            | SceneRenderBindingKind::AlphaTextureSlot
            | SceneRenderBindingKind::PreviousGraphTarget
            | SceneRenderBindingKind::GraphTarget
            | SceneRenderBindingKind::NamedFboBind
            | SceneRenderBindingKind::EffectTarget
            | SceneRenderBindingKind::VideoFrame
    ) {
        return None;
    }
    let producer_graph = if binding.kind == SceneRenderBindingKind::EffectTarget {
        render_texture_producer_graph(storage, binding.target, binding.name).unwrap_or(graph_index)
    } else {
        graph_index
    };
    Some(SceneRenderingDeviceSampledBinding {
        pass_node_index,
        graph_index: producer_graph,
        mesh_draw_start,
        mesh_draw_count,
        kind: binding.kind,
        slot: binding.slot,
        target: binding.target,
        target_name: binding.name,
    })
}

pub(super) fn render_texture_producer_graphs(storage: &SceneStorage) -> BTreeSet<u32> {
    storage
        .render_graphs()
        .iter()
        .flat_map(|graph| storage.render_graph_passes(graph))
        .flat_map(|pass| storage.render_pass_bindings(pass))
        .filter(|binding| binding.kind == SceneRenderBindingKind::EffectTarget)
        .filter_map(|binding| render_texture_producer_graph(storage, binding.target, binding.name))
        .collect()
}

fn render_texture_producer_graph(
    storage: &SceneStorage,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> Option<u32> {
    storage
        .render_graphs()
        .iter()
        .enumerate()
        .find(|(_, graph)| {
            storage
                .render_graph_passes(graph)
                .iter()
                .any(|pass| pass.target == target && pass.target_name == target_name)
        })
        .map(|(index, _)| index as u32)
}

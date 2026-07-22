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
        render_texture_producer_graph(storage, graph_index, binding.target, binding.name)
            .unwrap_or(graph_index)
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
        access: SceneRenderingDeviceImageAccess::SampledImage,
    })
}

pub(super) fn image_binding_access(
    storage: &SceneStorage,
    pass: &SceneRenderPassRecord,
    slot: u32,
) -> SceneRenderingDeviceImageAccess {
    let input_attachment = storage
        .shader_contracts()
        .iter()
        .find(|contract| contract.shader_key == pass.shader_key)
        .is_some_and(|contract| {
            slot < u32::BITS && contract.input_attachment_slot_mask & (1 << slot) != 0
        });
    if input_attachment {
        SceneRenderingDeviceImageAccess::InputAttachment
    } else {
        SceneRenderingDeviceImageAccess::SampledImage
    }
}

fn render_texture_producer_graph(
    storage: &SceneStorage,
    preferred_graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> Option<u32> {
    let graph_writes_target = |graph: &SceneRenderGraphRecord| {
        storage
            .render_graph_passes(graph)
            .iter()
            .any(|pass| pass.target == target && pass.target_name == target_name)
    };
    if storage
        .render_graphs()
        .get(preferred_graph_index as usize)
        .is_some_and(graph_writes_target)
    {
        return Some(preferred_graph_index);
    }
    storage
        .render_graphs()
        .iter()
        .enumerate()
        .find(|(_, graph)| graph_writes_target(graph))
        .map(|(index, _)| index as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::SceneBinaryDocument;

    #[test]
    fn same_named_effect_target_prefers_the_current_graph_producer() {
        let target_name = SceneStringId(0);
        let target_pass = |id| SceneRenderPassRecord {
            id,
            role: SceneRenderPassKind::CopyTarget,
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            pass_index: 0,
            shader_key: SceneStringId::NONE,
            target: SceneRenderTargetKind::FirstClassEffectTarget,
            target_name,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            pipeline_blend: ScenePipelineBlend::Normal,
            scene_blend: SceneCompositeBlend::Alpha,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            color_write_mask: SceneColorWriteMask::Rgba,
            clear_target: false,
        };
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["_rt_FullFrameBuffer".to_owned()],
            render_graphs: vec![
                SceneRenderGraphRecord {
                    object: SceneObjectHandle(INVALID_OBJECT_ID),
                    activation_policy: SceneRenderGraphActivationPolicy::Always,
                    pass_start: 0,
                    pass_count: 1,
                    unsupported_start: 0,
                    unsupported_count: 0,
                },
                SceneRenderGraphRecord {
                    object: SceneObjectHandle(INVALID_OBJECT_ID),
                    activation_policy: SceneRenderGraphActivationPolicy::Always,
                    pass_start: 1,
                    pass_count: 1,
                    unsupported_start: 0,
                    unsupported_count: 0,
                },
            ],
            render_passes: vec![target_pass(0), target_pass(0)],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let binding = SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::EffectTarget,
            slot: 2,
            target: SceneRenderTargetKind::FirstClassEffectTarget,
            name: target_name,
        };

        let sampled = sampled_binding(&storage, 1, 3, 7, 1, &binding).expect("sampled binding");

        assert_eq!(sampled.graph_index, 1);
        assert_eq!(sampled.pass_node_index, 3);
        assert_eq!(sampled.mesh_draw_start, 7);
        assert_eq!(sampled.slot, 2);
    }
}

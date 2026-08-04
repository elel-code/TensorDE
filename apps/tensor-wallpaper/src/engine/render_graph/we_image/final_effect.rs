use super::*;

pub(super) fn append(graph: &mut RenderGraph, contract: &WeImageGraphContract) -> bool {
    if contract.puppet_skinning_after_effects {
        return false;
    }
    let Some(final_effect) = &contract.final_effect_material else {
        return false;
    };
    if let Some(prepass) = &final_effect.prepass {
        append_prepass(graph, contract, prepass);
    }
    if let Some(intermediate) = &final_effect.intermediate {
        append_intermediate(graph, contract, final_effect, intermediate);
    }
    let effect_visibility = material_stage_range_visibility(
        &contract.effect_passes,
        final_effect.effect_stage_index,
        final_effect.effect_stage_count,
    )
    .expect("typed final effect requires contiguous effect bindings");
    let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
    graph.passes.push(RenderPassNode {
        id: pass_id,
        role: RenderPassRole::SceneComposite,
        draw_primitive: final_effect.draw_primitive,
        object_index: Some(contract.object_index),
        material_index: Some(final_effect.material_index),
        pass_index: 0,
        shader: Some(final_effect.shader.clone()),
        target: RenderTargetRole::SceneColor,
        target_name: None,
        target_extent: None,
        target_format: None,
        bindings: final_effect
            .prepass
            .as_ref()
            .map(|_| vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }])
            .unwrap_or_default(),
        effect_visibility,
        state: PassState {
            pipeline_blend: final_pipeline_blend(contract),
            scene_blend: contract.final_scene_blend,
            color_write_mask: if final_effect.prepass.is_some() {
                ColorWriteMask::Rgb
            } else {
                ColorWriteMask::Rgba
            },
            ..PassState::default()
        },
    });
    true
}

fn append_prepass(
    graph: &mut RenderGraph,
    contract: &WeImageGraphContract,
    prepass: &WeFinalEffectPrepass,
) {
    let effect = contract
        .effect_passes
        .get(prepass.effect_stage_index)
        .expect("typed final-effect prepass references a missing effect stage");
    let (role, draw_primitive, bindings, effect_visibility) = match prepass.input {
        WeFinalEffectPrepassInput::FramebufferSnapshot => {
            let snapshot = contract
                .framebuffer_snapshot
                .as_ref()
                .expect("framebuffer final-effect prepass requires a framebuffer snapshot");
            graph.passes.push(RenderPassNode {
                id: 0,
                role: RenderPassRole::CopyTarget,
                draw_primitive: RenderPassDrawPrimitive::None,
                object_index: Some(contract.object_index),
                material_index: None,
                pass_index: 0,
                shader: None,
                target: RenderTargetRole::FirstClassEffectTarget,
                target_name: Some(snapshot.target_name.clone()),
                target_extent: None,
                target_format: Some("rgba_backbuffer".to_owned()),
                bindings: vec![TextureBindingRole::GraphTarget {
                    slot: snapshot.texture_slot,
                    role: RenderTargetRole::SceneColor,
                    name: None,
                }],
                effect_visibility: RenderPassEffectVisibility::NONE,
                state: PassState::default(),
            });
            (
                RenderPassRole::EffectMaterial,
                RenderPassDrawPrimitive::FullscreenTriangle,
                vec![TextureBindingRole::EffectTarget {
                    slot: 0,
                    name: snapshot.target_name.clone(),
                }],
                single_effect_visibility(effect, RenderPassEffectVisibility::passthrough),
            )
        }
        WeFinalEffectPrepassInput::ObjectSource => (
            RenderPassRole::ObjectLocalSource,
            RenderPassDrawPrimitive::ObjectUvSupportQuad,
            vec![TextureBindingRole::SourceTexture],
            RenderPassEffectVisibility::NONE,
        ),
    };
    let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
    graph.passes.push(RenderPassNode {
        id: pass_id,
        role,
        draw_primitive,
        object_index: Some(contract.object_index),
        material_index: Some(prepass.material_index),
        pass_index: effect.pass_index,
        shader: Some(prepass.shader.clone()),
        target: RenderTargetRole::ImageLocalMain,
        target_name: None,
        target_extent: None,
        target_format: Some("rgba8".to_owned()),
        bindings,
        effect_visibility,
        state: PassState {
            pipeline_blend: PipelineBlendMode::Normal,
            scene_blend: SceneBlendMode::Normal,
            ..PassState::default()
        },
    });
}

fn append_intermediate(
    graph: &mut RenderGraph,
    contract: &WeImageGraphContract,
    final_effect: &WeFinalEffectMaterial,
    intermediate: &WeFinalEffectIntermediate,
) {
    assert!(
        final_effect.prepass.is_some(),
        "typed final-effect intermediate requires an earlier prepass"
    );
    let effect = contract
        .effect_passes
        .get(intermediate.effect_stage_index)
        .expect("typed final-effect intermediate references a missing effect stage");
    let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
    graph.passes.push(RenderPassNode {
        id: pass_id,
        role: RenderPassRole::EffectMaterial,
        draw_primitive: RenderPassDrawPrimitive::FullscreenTriangle,
        object_index: Some(contract.object_index),
        material_index: Some(intermediate.material_index),
        pass_index: effect.pass_index,
        shader: Some(intermediate.shader.clone()),
        target: RenderTargetRole::ImageLocalSub,
        target_name: None,
        target_extent: None,
        target_format: Some("rgba8".to_owned()),
        bindings: vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }],
        effect_visibility: material_stage_range_visibility(
            &contract.effect_passes,
            intermediate.effect_stage_index,
            intermediate.effect_stage_count,
        )
        .expect("typed final-effect intermediate requires contiguous effect bindings"),
        state: PassState {
            pipeline_blend: PipelineBlendMode::Normal,
            scene_blend: SceneBlendMode::Normal,
            ..PassState::default()
        },
    });
}

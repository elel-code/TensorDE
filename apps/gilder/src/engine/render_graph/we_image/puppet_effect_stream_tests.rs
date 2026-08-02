use std::collections::BTreeMap;

use super::*;

#[test]
fn authored_puppet_waterwaves_and_opacity_keep_twelve_draws_before_terminal_skinning() {
    let mut effects = (0..9)
        .map(|binding| effect(binding, "effects/waterwaves__SLOTS_1"))
        .collect::<Vec<_>>();
    effects.push(effect(9, "effects/opacity__SLOTS_1"));

    let graph = we_image_graph(&puppet_contract(effects));

    assert_eq!(graph.passes.len(), 12);
    assert_eq!(graph.passes[0].role, RenderPassRole::ObjectLocalSource);
    assert_eq!(
        graph.passes[0].draw_primitive,
        RenderPassDrawPrimitive::ObjectUvSupportQuad
    );
    assert_eq!(
        graph.passes[0].shader.as_deref(),
        Some("we/image-effect-source")
    );
    assert!(graph.passes[1..11].iter().all(|pass| {
        pass.role == RenderPassRole::EffectMaterial
            && pass.draw_primitive == RenderPassDrawPrimitive::FullscreenTriangle
    }));
    assert_eq!(graph.passes[11].role, RenderPassRole::SceneComposite);
    assert_eq!(
        graph.passes[11].draw_primitive,
        RenderPassDrawPrimitive::ObjectMesh
    );
    assert_eq!(
        graph.passes[11].shader.as_deref(),
        Some("we/puppet-effect-composite")
    );
    assert_puppet_source_and_terminal_state(&graph);
}

#[test]
fn authored_puppet_two_stage_waterwaves_keep_four_draws() {
    let graph = we_image_graph(&puppet_contract(vec![
        effect(0, "effects/waterwaves__SLOTS_1"),
        effect(1, "effects/waterwaves__SLOTS_3"),
    ]));

    assert_eq!(graph.passes.len(), 4);
    assert_eq!(
        graph
            .passes
            .iter()
            .map(|pass| pass.role)
            .collect::<Vec<_>>(),
        [
            RenderPassRole::ObjectLocalSource,
            RenderPassRole::EffectMaterial,
            RenderPassRole::EffectMaterial,
            RenderPassRole::SceneComposite,
        ]
    );
    assert_puppet_source_and_terminal_state(&graph);
}

fn assert_puppet_source_and_terminal_state(graph: &RenderGraph) {
    let source = graph
        .passes
        .first()
        .expect("typed puppet effect graph requires a local source");
    assert_eq!(source.state.pipeline_blend, PipelineBlendMode::Normal);
    assert_eq!(source.state.color_write_mask, ColorWriteMask::Rgba);

    let terminal = graph
        .passes
        .last()
        .expect("typed puppet effect graph requires a terminal composite");
    assert_eq!(
        terminal.state.pipeline_blend,
        PipelineBlendMode::Translucent
    );
    assert_eq!(terminal.state.color_write_mask, ColorWriteMask::Rgb);
}

fn puppet_contract(effect_passes: Vec<WeEffectPassContract>) -> WeImageGraphContract {
    WeImageGraphContract {
        object_index: 7,
        base_material_index: Some(3),
        base_shader: Some("we/genericimage4__PUPPETSKINNING_1".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![0],
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: None,
        final_scene_blend: SceneBlendMode::Alpha,
        static_black_output: false,
        effects_in_authored_texture_space: true,
        puppet_skinning_after_effects: true,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: None,
        effect_passes,
    }
}

fn effect(effect_binding_start: u32, shader: &str) -> WeEffectPassContract {
    WeEffectPassContract {
        object_index: 7,
        effect_binding_start,
        effect_binding_count: 1,
        runtime_visibility: true,
        material_index: Some(effect_binding_start as usize + 4),
        effect_file: format!("{shader}.json"),
        pass_index: 0,
        command: None,
        shader: Some(shader.to_owned()),
        source: None,
        target: None,
        binds: BTreeMap::from([(0, "previous".to_owned())]),
        pass_constants: Vec::new(),
        material_blending: Some("normal".to_owned()),
        depthtest: None,
        depthwrite: None,
        cullmode: None,
        combos: BTreeMap::new(),
    }
}

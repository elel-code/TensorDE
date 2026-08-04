use super::*;

#[test]
fn direct_generic_multiply_uses_the_premultiplied_fixed_blend_shader() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 7,
        base_material_index: Some(3),
        base_shader: Some("genericimage4".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![0],
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: None,
        final_scene_blend: SceneBlendMode::Multiply,
        static_black_output: false,
        effects_in_authored_texture_space: true,
        puppet_skinning_after_effects: false,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: None,
        effect_passes: Vec::new(),
    });

    assert_eq!(graph.passes.len(), 1);
    assert_eq!(
        graph.passes[0].shader.as_deref(),
        Some("we/genericimage4-multiply-composite")
    );
    assert_eq!(graph.passes[0].target, RenderTargetRole::SceneColor);
}

#[test]
fn offscreen_static_black_multiply_uses_typed_composite_shader() {
    let effect = WeEffectPassContract {
        object_index: 7,
        effect_binding_start: 0,
        effect_binding_count: 2,
        runtime_visibility: false,
        material_index: Some(4),
        effect_file: "tensor-wallpaper/typed/waterwaves-effect-run".to_owned(),
        pass_index: 0,
        command: None,
        shader: Some("we/effect-waterwaves-direct__STAGES_2".to_owned()),
        source: None,
        target: None,
        binds: [(0, "previous".to_owned())].into_iter().collect(),
        pass_constants: Vec::new(),
        material_blending: Some("normal".to_owned()),
        depthtest: Some("disabled".to_owned()),
        depthwrite: Some("disabled".to_owned()),
        cullmode: Some("nocull".to_owned()),
        combos: Default::default(),
    };
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 7,
        base_material_index: Some(3),
        base_shader: Some("genericimage4".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![0],
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: None,
        final_scene_blend: SceneBlendMode::Multiply,
        static_black_output: true,
        effects_in_authored_texture_space: true,
        puppet_skinning_after_effects: false,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: None,
        effect_passes: vec![effect],
    });

    assert_eq!(graph.passes.len(), 3);
    assert_eq!(
        graph.passes[2].shader.as_deref(),
        Some("we/image-effect-composite__STATIC_BLACK_1")
    );
    assert_eq!(graph.passes[2].state.scene_blend, SceneBlendMode::Multiply);
}

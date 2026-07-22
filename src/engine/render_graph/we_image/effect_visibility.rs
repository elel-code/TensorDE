use super::super::*;

#[test]
fn fused_eye_draw_owns_both_contiguous_material_visibility_stages() {
    let effect_passes = [
        (
            17,
            "effects/iris/effect.json",
            "effects/iris__SLOTS_3__MASK_1",
        ),
        (
            18,
            "effects/waterripple/effect.json",
            "effects/waterripple__SLOTS_7",
        ),
    ]
    .into_iter()
    .map(
        |(effect_binding_start, effect_file, shader)| WeEffectPassContract {
            object_index: 3,
            effect_binding_start,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(4),
            effect_file: effect_file.to_owned(),
            pass_index: 0,
            command: None,
            shader: Some(shader.to_owned()),
            source: None,
            target: None,
            binds: [(0, "previous".to_owned())].into_iter().collect(),
            pass_constants: Vec::new(),
            material_blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            combos: BTreeMap::new(),
        },
    )
    .collect();
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 3,
        base_material_index: Some(1),
        base_shader: Some("genericimage4".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![0],
        base_pass_constants: Vec::new(),
        framebuffer_snapshot: None,
        final_scene_blend: SceneBlendMode::Alpha,
        static_black_output: false,
        effects_in_authored_texture_space: true,
        puppet_skinning_after_effects: true,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        ripple_flow_material_indices: None,
        final_effect_material: Some(WeFinalEffectMaterial {
            material_index: 9,
            shader: "we/puppet-iris-waterripple-final".to_owned(),
        }),
        effect_passes,
    });

    assert_eq!(graph.passes.len(), 1);
    assert_eq!(
        graph.passes[0].effect_visibility,
        RenderPassEffectVisibility::material_stages(17, 2)
    );
}

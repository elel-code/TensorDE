use super::*;

fn rounded_contract(shader: &str, binds: BTreeMap<u32, String>) -> WeImageGraphContract {
    WeImageGraphContract {
        object_index: 7,
        base_material_index: Some(3),
        base_shader: Some("we/flat".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: Vec::new(),
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: None,
        final_scene_blend: SceneBlendMode::Alpha,
        static_black_output: false,
        effects_in_authored_texture_space: true,
        puppet_skinning_after_effects: false,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: None,
        effect_passes: vec![WeEffectPassContract {
            object_index: 7,
            effect_binding_start: 0,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(4),
            effect_file: "effects/rounded_mask/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some(shader.to_owned()),
            source: None,
            target: None,
            binds,
            pass_constants: Vec::new(),
            material_blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            combos: BTreeMap::from([
                ("B_SQUARE".to_owned(), 0),
                ("C_ALPHA_ONLY".to_owned(), 0),
                ("SOFT".to_owned(), 1),
            ]),
        }],
    }
}

#[test]
fn flat_authored_effects_composite_from_object_uv_over_full_scene() {
    let mut contract = rounded_contract(
        "effects/rounded_mask",
        [(0, "previous".to_owned())].into_iter().collect(),
    );
    let graph = we_image_graph(&contract);

    assert_eq!(graph.passes.len(), 1);
    assert_eq!(
        graph.passes[0].shader.as_deref(),
        Some("we/flat-rounded-mask-composite")
    );
    assert_eq!(graph.passes[0].material_index, Some(4));

    contract.final_scene_blend = SceneBlendMode::HslColor;
    assert!(we_image_graph_requires_generated_scene_snapshot(&contract));
    contract.framebuffer_snapshot = Some(WeFramebufferSnapshotContract {
        target_name: "_rt_FullFrameBuffer".to_owned(),
        texture_slot: 0,
        composite_to_object_mesh: false,
        usage: WeFramebufferSnapshotUsage::ObjectSource,
    });
    let graph = we_image_graph(&contract);
    assert_eq!(graph.activation_policy, RenderGraphActivationPolicy::Always);
    assert_eq!(graph.passes.len(), 2);
    assert_eq!(graph.passes[0].role, RenderPassRole::CopyTarget);
    assert_eq!(
        graph.passes[1].shader.as_deref(),
        Some("we/flat-rounded-hsl-source")
    );
    assert_eq!(graph.passes[1].target, RenderTargetRole::SceneColor);
    assert_eq!(graph.passes[1].state.scene_blend, SceneBlendMode::Normal);
}

#[test]
fn flat_rounded_mask_accepts_typed_shader_variants_with_sparse_effect_metadata() {
    let mut contract = rounded_contract(
        "effects/rounded_mask__SLOTS_1__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1",
        BTreeMap::new(),
    );
    contract.effect_passes[0].combos.clear();
    let graph = we_image_graph(&contract);

    assert_eq!(graph.passes.len(), 1);
    assert_eq!(
        graph.passes[0].shader.as_deref(),
        Some("we/flat-rounded-mask-composite")
    );
}

#[test]
fn flat_rounded_mask_ignores_a_stale_binding_when_opacity_mask_is_inactive() {
    let mut contract = rounded_contract(
        "effects/rounded_mask__SLOTS_5__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1",
        [(0, "previous".to_owned()), (2, "util/white".to_owned())]
            .into_iter()
            .collect(),
    );
    contract.color_blend_mode = 28;
    contract.final_scene_blend = SceneBlendMode::HslColor;
    contract.effect_passes[0].depthtest = Some("disabled".to_owned());
    contract.effect_passes[0].depthwrite = Some("disabled".to_owned());
    contract.effect_passes[0].cullmode = Some("nocull".to_owned());

    assert!(we_image_graph_requires_generated_scene_snapshot(&contract));
    contract.framebuffer_snapshot = Some(WeFramebufferSnapshotContract {
        target_name: "_rt_FullFrameBuffer".to_owned(),
        texture_slot: 0,
        composite_to_object_mesh: false,
        usage: WeFramebufferSnapshotUsage::ObjectSource,
    });
    let graph = we_image_graph(&contract);
    assert_eq!(graph.passes.len(), 2);
    assert_eq!(graph.passes[0].role, RenderPassRole::CopyTarget);
    assert_eq!(
        graph.passes[1].shader.as_deref(),
        Some("we/flat-rounded-hsl-source")
    );
    assert!(
        graph.passes[1]
            .bindings
            .iter()
            .all(|binding| !matches!(binding, TextureBindingRole::TextureSlot { slot: 2 }))
    );

    contract.framebuffer_snapshot = None;
    contract.effect_passes[0]
        .combos
        .insert("OPACITYMASK".to_owned(), 1);
    assert!(!we_image_graph_requires_generated_scene_snapshot(&contract));
    let graph = we_image_graph(&contract);
    assert!(graph.passes.len() >= 3);
    assert!(
        graph
            .passes
            .iter()
            .any(|pass| { pass.shader.as_deref() == contract.effect_passes[0].shader.as_deref() })
    );
}

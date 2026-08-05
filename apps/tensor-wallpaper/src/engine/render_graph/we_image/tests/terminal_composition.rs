// Terminal-composition and scene-snapshot graph contracts.

#[test]
fn effect_only_terminal_keeps_local_source_and_declared_scene_snapshot_slots() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 12,
        base_material_index: Some(5),
        base_shader: Some("composelayer".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![0],
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: Some(WeFramebufferSnapshotContract {
            target_name: "_rt_FullFrameBuffer".to_owned(),
            texture_slot: 0,
            composite_to_object_mesh: true,
            usage: WeFramebufferSnapshotUsage::EffectOnlyLayer,
        }),
        final_scene_blend: SceneBlendMode::Alpha,
        static_black_output: false,
        effects_in_authored_texture_space: false,
        puppet_skinning_after_effects: false,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: None,
        effect_passes: vec![WeEffectPassContract {
            object_index: 12,
            effect_binding_start: 0,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(6),
            effect_file: "effects/oscilloscope/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some(
                "effects/audio_responsive_oscilloscope__SLOTS_5__RESOLUTION_16".to_owned(),
            ),
            source: None,
            target: None,
            binds: [
                (0, "previous".to_owned()),
                (2, "_rt_FullFrameBuffer".to_owned()),
            ]
            .into_iter()
            .collect(),
            pass_constants: Vec::new(),
            material_blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            combos: [("RESOLUTION".to_owned(), 16)].into_iter().collect(),
        }],
    });

    let terminal = graph.passes.last().expect("terminal pass");
    assert_eq!(terminal.role, RenderPassRole::SceneComposite);
    assert!(
        terminal
            .bindings
            .contains(&TextureBindingRole::PreviousGraphTarget { slot: 0 })
    );
    assert!(
        terminal
            .bindings
            .contains(&TextureBindingRole::EffectTarget {
                slot: 2,
                name: "_rt_FullFrameBuffer".to_owned(),
            })
    );
}

#[test]
fn static_effect_only_terminal_keeps_the_authored_last_effect() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 12,
        base_material_index: Some(5),
        base_shader: Some("composelayer".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![0],
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: Some(WeFramebufferSnapshotContract {
            target_name: "_rt_FullFrameBuffer".to_owned(),
            texture_slot: 0,
            composite_to_object_mesh: true,
            usage: WeFramebufferSnapshotUsage::EffectOnlyLayer,
        }),
        final_scene_blend: SceneBlendMode::Alpha,
        static_black_output: false,
        effects_in_authored_texture_space: false,
        puppet_skinning_after_effects: false,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: None,
        effect_passes: vec![
            static_effect(
                0,
                "effects/simple_audio_bars__SLOTS_1__SHAPE_7",
                &[(0, "previous")],
            ),
            static_effect(1, "effects/skew__SLOTS_1", &[(0, "previous")]),
            static_effect(
                2,
                "effects/opacity__SLOTS_3__MASK_1",
                &[(0, "previous"), (1, "mask")],
            ),
        ],
    });

    assert_eq!(graph.passes.len(), 5);
    let terminal = graph.passes.last().expect("authored terminal");
    assert_eq!(terminal.role, RenderPassRole::SceneComposite);
    assert_eq!(
        terminal.shader.as_deref(),
        Some("effects/opacity__SLOTS_3__MASK_1")
    );
    assert_eq!(terminal.target, RenderTargetRole::SceneColor);
    assert_eq!(
        terminal.state.pipeline_blend,
        PipelineBlendMode::Translucent
    );
    assert_eq!(terminal.state.color_write_mask, ColorWriteMask::Rgb);
}

#[test]
fn shader_only_scene_snapshot_is_not_injected_into_the_base_pass() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 9,
        base_material_index: Some(2),
        base_shader: Some("genericimage4".to_owned()),
        base_material_blending: Some("normal".to_owned()),
        base_texture_slots: vec![0],
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: Some(WeFramebufferSnapshotContract {
            target_name: "_rt_FullFrameBuffer".to_owned(),
            texture_slot: 0,
            composite_to_object_mesh: false,
            usage: WeFramebufferSnapshotUsage::EffectShaderInput,
        }),
        final_scene_blend: SceneBlendMode::Alpha,
        static_black_output: false,
        effects_in_authored_texture_space: false,
        puppet_skinning_after_effects: false,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: None,
        effect_passes: vec![WeEffectPassContract {
            object_index: 9,
            effect_binding_start: 0,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(3),
            effect_file: "effects/framebuffer/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some("effects/framebuffer__SLOTS_5".to_owned()),
            source: None,
            target: Some("_rt_output".to_owned()),
            binds: [
                (0, "previous".to_owned()),
                (2, "_rt_FullFrameBuffer".to_owned()),
            ]
            .into_iter()
            .collect(),
            pass_constants: Vec::new(),
            material_blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            combos: BTreeMap::new(),
        }],
    });

    assert_eq!(graph.passes[0].role, RenderPassRole::CopyTarget);
    let base = graph
        .passes
        .iter()
        .find(|pass| pass.role == RenderPassRole::BaseMaterial)
        .expect("base pass");
    assert!(
        !base
            .bindings
            .iter()
            .any(|binding| texture_binding_uses_slot(binding, 2))
    );
    let effect = graph
        .passes
        .iter()
        .find(|pass| pass.role == RenderPassRole::EffectMaterial)
        .expect("effect pass");
    assert!(effect.bindings.contains(&TextureBindingRole::EffectTarget {
        slot: 2,
        name: "_rt_FullFrameBuffer".to_owned(),
    }));
}

#[test]
fn effect_only_explicit_final_target_keeps_a_separate_scene_composite() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 12,
        base_material_index: Some(5),
        base_shader: Some("composelayer".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![0],
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: Some(WeFramebufferSnapshotContract {
            target_name: "_rt_FullFrameBuffer".to_owned(),
            texture_slot: 0,
            composite_to_object_mesh: true,
            usage: WeFramebufferSnapshotUsage::EffectOnlyLayer,
        }),
        final_scene_blend: SceneBlendMode::Alpha,
        static_black_output: false,
        effects_in_authored_texture_space: false,
        puppet_skinning_after_effects: false,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: None,
        effect_passes: vec![WeEffectPassContract {
            object_index: 12,
            effect_binding_start: 0,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(6),
            effect_file: "effects/opacity/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some("effects/opacity__SLOTS_1".to_owned()),
            source: None,
            target: Some("_rt_authored_output".to_owned()),
            binds: [(0, "previous".to_owned())].into_iter().collect(),
            pass_constants: vec!["alpha".to_owned()],
            material_blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            combos: BTreeMap::new(),
        }],
    });

    assert_eq!(graph.passes.len(), 4);
    assert_eq!(
        graph.passes[2].target,
        RenderTargetRole::FirstClassEffectTarget
    );
    assert_eq!(
        graph.passes[2].target_name.as_deref(),
        Some("_rt_authored_output")
    );
    assert_eq!(graph.passes[3].role, RenderPassRole::SceneComposite);
    assert_eq!(
        graph.passes[3].draw_primitive,
        RenderPassDrawPrimitive::FullscreenTriangle
    );
    assert_eq!(
        graph.passes[3].shader.as_deref(),
        Some("we/objectcomposite")
    );
}

#[test]
fn singleton_final_effect_draw_owns_its_material_visibility_stage() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 3,
        base_material_index: Some(1),
        base_shader: Some("genericimage4".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![0],
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
        final_effect_material: Some(WeFinalEffectMaterial {
            material_index: 9,
            shader: "we/image-scroll-final".to_owned(),
            draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
            effect_stage_index: 0,
            effect_stage_count: 1,
            prepass: None,
            intermediate: None,
        }),
        effect_passes: vec![WeEffectPassContract {
            object_index: 3,
            effect_binding_start: 17,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(4),
            effect_file: "effects/scroll/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some("effects/scroll__SLOTS_1".to_owned()),
            source: None,
            target: None,
            binds: [(0, "previous".to_owned())].into_iter().collect(),
            pass_constants: Vec::new(),
            material_blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            combos: BTreeMap::new(),
        }],
    });

    assert_eq!(graph.passes.len(), 1);
    assert_eq!(
        graph.passes[0].shader.as_deref(),
        Some("we/image-scroll-final")
    );
    assert_eq!(
        graph.passes[0].effect_visibility,
        RenderPassEffectVisibility::material_stages(17, 1)
    );
}

fn static_effect(
    binding_start: u32,
    shader: &str,
    bindings: &[(u32, &str)],
) -> WeEffectPassContract {
    WeEffectPassContract {
        object_index: 12,
        effect_binding_start: binding_start,
        effect_binding_count: 1,
        runtime_visibility: false,
        material_index: Some(6 + binding_start as usize),
        effect_file: "effects/test/effect.json".to_owned(),
        pass_index: 0,
        command: None,
        shader: Some(shader.to_owned()),
        source: None,
        target: None,
        binds: bindings
            .iter()
            .map(|(slot, source)| (*slot, (*source).to_owned()))
            .collect(),
        pass_constants: Vec::new(),
        material_blending: Some("normal".to_owned()),
        depthtest: Some("disabled".to_owned()),
        depthwrite: Some("disabled".to_owned()),
        cullmode: Some("nocull".to_owned()),
        combos: BTreeMap::new(),
    }
}

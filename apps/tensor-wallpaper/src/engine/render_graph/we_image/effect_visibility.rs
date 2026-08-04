use super::super::*;

#[test]
fn runtime_disabled_single_image_effect_selects_exact_direct_base_branch() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 5,
        base_material_index: Some(15),
        base_shader: Some("we/genericimage4".to_owned()),
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
        final_effect_material: None,
        effect_passes: vec![WeEffectPassContract {
            object_index: 5,
            effect_binding_start: 7,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(16),
            effect_file: "effects/tint/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some("effects/tint__SLOTS_1".to_owned()),
            source: None,
            target: None,
            binds: [(0, "previous".to_owned())].into_iter().collect(),
            pass_constants: Vec::new(),
            material_blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            combos: BTreeMap::new(),
        }],
    });

    assert_eq!(graph.passes.len(), 3);
    assert!(
        graph.passes[..2].iter().all(|pass| {
            pass.effect_visibility == RenderPassEffectVisibility::any_visible(7, 1)
        })
    );
    assert_eq!(graph.passes[1].role, RenderPassRole::SceneComposite);
    assert_eq!(
        graph.passes[1].draw_primitive,
        RenderPassDrawPrimitive::ObjectMesh
    );
    assert_eq!(graph.passes[1].target, RenderTargetRole::SceneColor);
    let bypass = &graph.passes[2];
    assert_eq!(bypass.role, RenderPassRole::BaseMaterial);
    assert_eq!(bypass.draw_primitive, RenderPassDrawPrimitive::ObjectMesh);
    assert_eq!(bypass.shader.as_deref(), Some("we/genericimage4"));
    assert_eq!(bypass.target, RenderTargetRole::SceneColor);
    assert_eq!(
        bypass.bindings,
        [
            TextureBindingRole::SourceTexture,
            TextureBindingRole::TextureSlot { slot: 0 },
        ]
    );
    assert_eq!(
        bypass.effect_visibility,
        RenderPassEffectVisibility::none_visible(7, 1)
    );
    assert_eq!(bypass.state.pipeline_blend, PipelineBlendMode::Translucent);
}

#[test]
fn runtime_hidden_terminal_effect_promotes_previous_stable_stage_to_scene_color() {
    let effect = |binding, runtime_visibility, shader: &str| WeEffectPassContract {
        object_index: 5,
        effect_binding_start: binding,
        effect_binding_count: 1,
        runtime_visibility,
        material_index: Some(20 + binding as usize),
        effect_file: format!("effects/{shader}/effect.json"),
        pass_index: 0,
        command: None,
        shader: Some(shader.to_owned()),
        source: None,
        target: None,
        binds: [(0, "previous".to_owned())].into_iter().collect(),
        pass_constants: Vec::new(),
        material_blending: Some("normal".to_owned()),
        depthtest: Some("disabled".to_owned()),
        depthwrite: Some("disabled".to_owned()),
        cullmode: Some("nocull".to_owned()),
        combos: BTreeMap::new(),
    };
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 5,
        base_material_index: Some(15),
        base_shader: Some("we/flat".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: Vec::new(),
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: None,
        final_scene_blend: SceneBlendMode::Alpha,
        static_black_output: false,
        effects_in_authored_texture_space: false,
        puppet_skinning_after_effects: false,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: None,
        effect_passes: vec![
            effect(7, false, "external-source"),
            effect(8, false, "stable-wave"),
            effect(9, true, "dynamic-tint"),
        ],
    });

    assert_eq!(graph.passes.len(), 5);
    assert_eq!(graph.passes[2].shader.as_deref(), Some("stable-wave"));
    assert_eq!(
        graph.passes[2].effect_visibility,
        RenderPassEffectVisibility::any_visible(9, 1)
    );
    assert_eq!(graph.passes[3].shader.as_deref(), Some("dynamic-tint"));
    assert_eq!(graph.passes[3].role, RenderPassRole::SceneComposite);
    assert_eq!(graph.passes[3].target, RenderTargetRole::SceneColor);
    assert_eq!(
        graph.passes[3].effect_visibility,
        RenderPassEffectVisibility::any_visible(9, 1)
    );
    assert_eq!(graph.passes[4].shader.as_deref(), Some("stable-wave"));
    assert_eq!(graph.passes[4].role, RenderPassRole::SceneComposite);
    assert_eq!(graph.passes[4].target, RenderTargetRole::SceneColor);
    assert_eq!(
        graph.passes[4].effect_visibility,
        RenderPassEffectVisibility::none_visible(9, 1)
    );
    assert_eq!(
        graph.passes[4].bindings,
        [TextureBindingRole::GraphTarget {
            slot: 0,
            role: RenderTargetRole::ImageLocalSub,
            name: None,
        }]
    );
}

#[test]
fn hidden_internal_puppet_effect_rebuilds_the_stable_suffix_without_a_passthrough_draw() {
    let effect = |binding, runtime_visibility, shader: &str| WeEffectPassContract {
        object_index: 5,
        effect_binding_start: binding,
        effect_binding_count: 1,
        runtime_visibility,
        material_index: Some(20 + binding as usize),
        effect_file: format!("effects/{shader}/effect.json"),
        pass_index: 0,
        command: None,
        shader: Some(shader.to_owned()),
        source: None,
        target: None,
        binds: [(0, "previous".to_owned())].into_iter().collect(),
        pass_constants: Vec::new(),
        material_blending: Some("normal".to_owned()),
        depthtest: Some("disabled".to_owned()),
        depthwrite: Some("disabled".to_owned()),
        cullmode: Some("nocull".to_owned()),
        combos: BTreeMap::new(),
    };
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 5,
        base_material_index: Some(15),
        base_shader: Some("we/genericimage4".to_owned()),
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
        effect_passes: vec![
            effect(7, true, "dynamic-tint"),
            effect(8, false, "stable-iris"),
            effect(9, false, "stable-water"),
        ],
    });

    assert_eq!(graph.passes.len(), 8);
    assert_eq!(graph.passes[0].role, RenderPassRole::ObjectLocalSource);
    assert!(
        graph.passes[1..5].iter().all(|pass| {
            pass.effect_visibility == RenderPassEffectVisibility::any_visible(7, 1)
        })
    );
    assert!(
        graph.passes[5..].iter().all(|pass| {
            pass.effect_visibility == RenderPassEffectVisibility::none_visible(7, 1)
        })
    );
    assert_eq!(graph.passes[5].shader.as_deref(), Some("stable-iris"));
    assert_eq!(graph.passes[5].target, RenderTargetRole::ImageLocalSub);
    assert_eq!(graph.passes[6].shader.as_deref(), Some("stable-water"));
    assert_eq!(graph.passes[6].target, RenderTargetRole::ImageLocalMain);
    assert_eq!(
        graph.passes[7].shader.as_deref(),
        Some("we/puppet-effect-composite")
    );
    assert_eq!(graph.passes[7].target, RenderTargetRole::SceneColor);
    assert_eq!(
        graph.passes[7].bindings,
        [TextureBindingRole::GraphTarget {
            slot: 0,
            role: RenderTargetRole::ImageLocalMain,
            name: None,
        }]
    );
}

#[test]
fn hidden_internal_unskinned_effect_keeps_the_static_terminal_direct() {
    let effect = |binding, runtime_visibility, shader: &str| WeEffectPassContract {
        object_index: 5,
        effect_binding_start: binding,
        effect_binding_count: 1,
        runtime_visibility,
        material_index: Some(20 + binding as usize),
        effect_file: format!("effects/{shader}/effect.json"),
        pass_index: 0,
        command: None,
        shader: Some(shader.to_owned()),
        source: None,
        target: None,
        binds: [(0, "previous".to_owned())].into_iter().collect(),
        pass_constants: Vec::new(),
        material_blending: Some("normal".to_owned()),
        depthtest: Some("disabled".to_owned()),
        depthwrite: Some("disabled".to_owned()),
        cullmode: Some("nocull".to_owned()),
        combos: BTreeMap::new(),
    };
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 5,
        base_material_index: Some(15),
        base_shader: Some("we/genericimage4".to_owned()),
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
        final_effect_material: None,
        effect_passes: vec![
            effect(7, true, "dynamic-tint"),
            effect(8, false, "stable-wave"),
        ],
    });

    assert_eq!(graph.passes.len(), 4);
    assert_eq!(graph.passes[2].role, RenderPassRole::SceneComposite);
    assert_eq!(graph.passes[2].shader.as_deref(), Some("stable-wave"));
    assert_eq!(
        graph.passes[2].effect_visibility,
        RenderPassEffectVisibility::any_visible(7, 1)
    );
    assert_eq!(graph.passes[3].role, RenderPassRole::SceneComposite);
    assert_eq!(graph.passes[3].shader.as_deref(), Some("stable-wave"));
    assert_eq!(
        graph.passes[3].effect_visibility,
        RenderPassEffectVisibility::none_visible(7, 1)
    );
    assert_eq!(
        graph.passes[3].bindings,
        [TextureBindingRole::GraphTarget {
            slot: 0,
            role: RenderTargetRole::ImageLocalMain,
            name: None,
        }]
    );
}

#[test]
fn static_terminal_effect_writes_scene_color_without_an_extra_composite() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 5,
        base_material_index: Some(15),
        base_shader: Some("we/flat".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: Vec::new(),
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: None,
        final_scene_blend: SceneBlendMode::Alpha,
        static_black_output: false,
        effects_in_authored_texture_space: false,
        puppet_skinning_after_effects: false,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: None,
        effect_passes: vec![WeEffectPassContract {
            object_index: 5,
            effect_binding_start: 7,
            effect_binding_count: 1,
            runtime_visibility: false,
            material_index: Some(20),
            effect_file: "effects/pulse/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some("effects/pulse".to_owned()),
            source: None,
            target: None,
            binds: [(0, "previous".to_owned())].into_iter().collect(),
            pass_constants: Vec::new(),
            material_blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            combos: BTreeMap::new(),
        }],
    });

    assert_eq!(graph.passes.len(), 2);
    assert_eq!(graph.passes[1].shader.as_deref(), Some("effects/pulse"));
    assert_eq!(graph.passes[1].role, RenderPassRole::SceneComposite);
    assert_eq!(graph.passes[1].target, RenderTargetRole::SceneColor);
    assert_eq!(
        graph.passes[1].effect_visibility,
        RenderPassEffectVisibility::NONE
    );
}

#[test]
fn authored_puppet_eye_effects_reject_final_draw_fusion() {
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
        color_blend_mode: 0,
        framebuffer_snapshot: None,
        final_scene_blend: SceneBlendMode::Alpha,
        static_black_output: false,
        effects_in_authored_texture_space: true,
        puppet_skinning_after_effects: true,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: Some(WeFinalEffectMaterial {
            material_index: 9,
            shader: "we/puppet-iris-waterripple-final".to_owned(),
            draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
            effect_stage_index: 0,
            effect_stage_count: 2,
            prepass: None,
            intermediate: None,
        }),
        effect_passes,
    });

    assert_eq!(graph.passes.len(), 5);
    assert_eq!(
        graph.passes[0].draw_primitive,
        RenderPassDrawPrimitive::ObjectUvSupportQuad
    );
    assert_eq!(
        graph.passes[1].effect_visibility,
        RenderPassEffectVisibility::passthrough(17, 1)
    );
    assert_eq!(
        graph.passes[2].effect_visibility,
        RenderPassEffectVisibility::any_visible(18, 1)
    );
    assert_eq!(
        graph.passes[3].shader.as_deref(),
        Some("we/puppet-effect-composite")
    );
    assert_eq!(
        graph.passes[3].effect_visibility,
        RenderPassEffectVisibility::any_visible(18, 1)
    );
    assert_eq!(
        graph.passes[3].bindings,
        [TextureBindingRole::GraphTarget {
            slot: 0,
            role: RenderTargetRole::ImageLocalMain,
            name: None,
        }]
    );
    assert_eq!(
        graph.passes[4].effect_visibility,
        RenderPassEffectVisibility::none_visible(18, 1)
    );
    assert_eq!(
        graph.passes[4].bindings,
        [TextureBindingRole::GraphTarget {
            slot: 0,
            role: RenderTargetRole::ImageLocalSub,
            name: None,
        }]
    );
}

#[test]
fn waterripple_keeps_rgba8_object_source_prepass_before_final_composite() {
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
            shader: "we/image-waterripple-final".to_owned(),
            draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
            effect_stage_index: 0,
            effect_stage_count: 1,
            prepass: Some(WeFinalEffectPrepass {
                material_index: 1,
                shader: "we/image-effect-source".to_owned(),
                effect_stage_index: 0,
                input: WeFinalEffectPrepassInput::ObjectSource,
            }),
            intermediate: None,
        }),
        effect_passes: vec![WeEffectPassContract {
            object_index: 3,
            effect_binding_start: 17,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(4),
            effect_file: "effects/waterripple/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some("effects/waterripple__SLOTS_5".to_owned()),
            source: None,
            target: None,
            binds: [(0, "previous".to_owned()), (2, "texture".to_owned())]
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

    assert_eq!(graph.passes.len(), 2);
    assert_eq!(graph.passes[0].role, RenderPassRole::ObjectLocalSource);
    assert_eq!(
        graph.passes[0].draw_primitive,
        RenderPassDrawPrimitive::ObjectUvSupportQuad
    );
    assert_eq!(graph.passes[0].target, RenderTargetRole::ImageLocalMain);
    assert_eq!(graph.passes[0].target_format.as_deref(), Some("rgba8"));
    assert_eq!(
        graph.passes[0].bindings,
        vec![TextureBindingRole::SourceTexture]
    );
    assert_eq!(
        graph.passes[0].effect_visibility,
        RenderPassEffectVisibility::NONE
    );
    assert_eq!(graph.passes[1].role, RenderPassRole::SceneComposite);
    assert_eq!(
        graph.passes[1].bindings,
        vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }]
    );
    assert_eq!(
        graph.passes[1].effect_visibility,
        RenderPassEffectVisibility::material_stages(17, 1)
    );
    assert_eq!(graph.passes[1].state.color_write_mask, ColorWriteMask::Rgb);
}

#[test]
fn framebuffer_water_graph_keeps_prepass_and_final_visibility_independent() {
    let effect_passes = [
        (
            17,
            "effects/caustics__SLOTS_3d__BLENDMODE_6",
            &[0, 2, 3, 4, 5][..],
        ),
        (18, "effects/waterwaves__SLOTS_1", &[0][..]),
        (19, "effects/opacity__SLOTS_1", &[0][..]),
        (20, "effects/shake__SLOTS_3", &[0, 1][..]),
    ]
    .into_iter()
    .enumerate()
    .map(
        |(pass_index, (effect_binding_start, shader, slots))| WeEffectPassContract {
            object_index: 3,
            effect_binding_start,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(4 + pass_index),
            effect_file: format!("effects/{pass_index}/effect.json"),
            pass_index: u32::try_from(pass_index).expect("four-stage pass index fits u32"),
            command: None,
            shader: Some(shader.to_owned()),
            source: None,
            target: None,
            binds: slots
                .iter()
                .copied()
                .map(|slot| {
                    (
                        slot,
                        if slot == 0 { "previous" } else { "texture" }.to_owned(),
                    )
                })
                .collect(),
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
        base_shader: Some("we/composelayer".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![0],
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: Some(WeFramebufferSnapshotContract {
            target_name: "_rt_FullFrameBuffer".to_owned(),
            texture_slot: 0,
            composite_to_object_mesh: false,
            usage: WeFramebufferSnapshotUsage::EffectOnlyLayer,
        }),
        final_scene_blend: SceneBlendMode::Alpha,
        static_black_output: false,
        effects_in_authored_texture_space: false,
        puppet_skinning_after_effects: false,
        waterwaves_uv_field_material_index: None,
        waterwaves_direct_material: None,
        foliage_ripple_material: None,
        final_effect_material: Some(WeFinalEffectMaterial {
            material_index: 9,
            shader: "we/framebuffer-water-quantized-shake-final".to_owned(),
            draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
            effect_stage_index: 3,
            effect_stage_count: 1,
            prepass: Some(WeFinalEffectPrepass {
                material_index: 8,
                shader: "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1".to_owned(),
                effect_stage_index: 0,
                input: WeFinalEffectPrepassInput::FramebufferSnapshot,
            }),
            intermediate: Some(WeFinalEffectIntermediate {
                material_index: 10,
                shader: "we/framebuffer-water-quantized-water-opacity".to_owned(),
                effect_stage_index: 1,
                effect_stage_count: 2,
            }),
        }),
        effect_passes,
    });

    assert_eq!(
        graph.activation_policy,
        RenderGraphActivationPolicy::AnyEffectVisible
    );
    assert_eq!(graph.passes.len(), 4);
    assert_eq!(graph.passes[0].role, RenderPassRole::CopyTarget);
    assert_eq!(
        graph.passes[0].draw_primitive,
        RenderPassDrawPrimitive::None
    );
    assert_eq!(
        graph.passes[0].target,
        RenderTargetRole::FirstClassEffectTarget
    );
    assert_eq!(
        graph.passes[0].target_format.as_deref(),
        Some("rgba_backbuffer")
    );
    assert_eq!(graph.passes[1].role, RenderPassRole::EffectMaterial);
    assert_eq!(
        graph.passes[1].draw_primitive,
        RenderPassDrawPrimitive::FullscreenTriangle
    );
    assert_eq!(graph.passes[1].target, RenderTargetRole::ImageLocalMain);
    assert_eq!(graph.passes[1].target_format.as_deref(), Some("rgba8"));
    assert_eq!(
        graph.passes[1].effect_visibility,
        RenderPassEffectVisibility::passthrough(17, 1)
    );
    assert_eq!(
        graph.passes[1].bindings,
        vec![TextureBindingRole::EffectTarget {
            slot: 0,
            name: "_rt_FullFrameBuffer".to_owned(),
        }]
    );
    assert_eq!(graph.passes[2].role, RenderPassRole::EffectMaterial);
    assert_eq!(
        graph.passes[2].draw_primitive,
        RenderPassDrawPrimitive::FullscreenTriangle
    );
    assert_eq!(graph.passes[2].target, RenderTargetRole::ImageLocalSub);
    assert_eq!(
        graph.passes[2].effect_visibility,
        RenderPassEffectVisibility::material_stages(18, 2)
    );
    assert_eq!(
        graph.passes[2].bindings,
        vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }]
    );
    assert_eq!(
        graph.passes[2].state.pipeline_blend,
        PipelineBlendMode::Normal
    );
    assert_eq!(graph.passes[3].role, RenderPassRole::SceneComposite);
    assert_eq!(
        graph.passes[3].draw_primitive,
        RenderPassDrawPrimitive::ObjectMesh
    );
    assert_eq!(graph.passes[3].target, RenderTargetRole::SceneColor);
    assert_eq!(
        graph.passes[3].effect_visibility,
        RenderPassEffectVisibility::material_stages(20, 1)
    );
    assert_eq!(
        graph.passes[3].bindings,
        vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }]
    );
    assert_eq!(
        graph.passes[3].state.pipeline_blend,
        PipelineBlendMode::Translucent
    );
    assert_eq!(graph.passes[3].state.color_write_mask, ColorWriteMask::Rgb);
}

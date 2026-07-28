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
            draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
            effect_stage_index: 0,
            effect_stage_count: 2,
            prepass: None,
            intermediate: None,
        }),
        effect_passes,
    });

    assert_eq!(graph.passes.len(), 1);
    assert_eq!(
        graph.passes[0].effect_visibility,
        RenderPassEffectVisibility::material_stages(17, 2)
    );
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
        ripple_flow_material_indices: None,
        final_effect_material: Some(WeFinalEffectMaterial {
            material_index: 9,
            shader: "we/framebuffer-water-quantized-shake-final".to_owned(),
            draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
            effect_stage_index: 3,
            effect_stage_count: 1,
            prepass: Some(WeFinalEffectPrepass {
                material_index: 8,
                shader: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1".to_owned(),
                effect_stage_index: 0,
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

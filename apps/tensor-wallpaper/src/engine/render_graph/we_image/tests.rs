use super::*;

mod blend_lowering;
mod effect_visibility;

#[test]
fn generated_temporary_target_names_keep_transient_lifetime_semantics() {
    assert_eq!(
        we_render_target_role("_tmp_TensorWallpaperFramebufferCaustics"),
        RenderTargetRole::Temporary
    );
    assert_eq!(
        we_binding_role(0, "_tmp_TensorWallpaperFramebufferCaustics"),
        TextureBindingRole::EffectTarget {
            slot: 0,
            name: "_tmp_TensorWallpaperFramebufferCaustics".to_owned(),
        }
    );
}

#[test]
fn reverse_engineered_material_blend_strings_map_to_pipeline_state() {
    assert_eq!(
        PipelineBlendMode::from_we_material_blending("normal"),
        PipelineBlendMode::Normal
    );
    assert_eq!(
        PipelineBlendMode::from_we_material_blending("translucent"),
        PipelineBlendMode::Translucent
    );
    assert_eq!(
        PipelineBlendMode::from_we_material_blending("additive"),
        PipelineBlendMode::Additive
    );
    assert_eq!(
        PipelineBlendMode::from_we_material_blending("alphatocoverage"),
        PipelineBlendMode::AlphaToCoverage
    );
}

#[test]
fn reverse_engineered_shader_blendmode_table_keeps_special_modes() {
    assert_eq!(
        ShaderBlendMode::from_we_blendmode(28),
        ShaderBlendMode::HslColor
    );
    assert_eq!(
        ShaderBlendMode::from_we_blendmode(30),
        ShaderBlendMode::Tint
    );
    assert_eq!(
        ShaderBlendMode::from_we_blendmode(31),
        ShaderBlendMode::LinearDodge
    );
    assert_eq!(
        ShaderBlendMode::from_we_blendmode(32),
        ShaderBlendMode::Modulate
    );
    assert!(ShaderBlendMode::from_we_blendmode(28).requires_framebuffer_sample());
}

#[test]
fn we_image_graph_keeps_pass_targets_and_derives_barriers() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 7,
        base_material_index: Some(3),
        base_shader: Some("genericimage4".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![1],
        base_pass_constants: vec!["tint".to_owned()],
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
            WeEffectPassContract {
                object_index: 7,
                effect_binding_start: 0,
                effect_binding_count: 1,
                runtime_visibility: true,
                material_index: Some(4),
                effect_file: "effects/waterflow/effect.json".to_owned(),
                pass_index: 1,
                command: None,
                shader: Some("effects/waterflow".to_owned()),
                source: None,
                target: Some("fbo_velocity".to_owned()),
                binds: [(1, "previous".to_owned())].into_iter().collect(),
                pass_constants: vec!["speed".to_owned()],
                material_blending: Some("normal".to_owned()),
                depthtest: Some("disabled".to_owned()),
                depthwrite: Some("disabled".to_owned()),
                cullmode: Some("nocull".to_owned()),
                combos: BTreeMap::new(),
            },
            WeEffectPassContract {
                object_index: 7,
                effect_binding_start: 1,
                effect_binding_count: 1,
                runtime_visibility: true,
                material_index: Some(5),
                effect_file: "materials/util/effectpassthrough.json".to_owned(),
                pass_index: 2,
                command: None,
                shader: Some("util/effectpassthrough".to_owned()),
                source: None,
                target: None,
                binds: [(1, "fbo_velocity".to_owned())].into_iter().collect(),
                pass_constants: Vec::new(),
                material_blending: Some("normal".to_owned()),
                depthtest: Some("disabled".to_owned()),
                depthwrite: Some("disabled".to_owned()),
                cullmode: Some("nocull".to_owned()),
                combos: [("BLENDMODE".to_owned(), 28)].into_iter().collect(),
            },
        ],
    });

    assert_eq!(graph.passes.len(), 4);
    assert_eq!(
        graph.passes[0].state.pipeline_blend,
        PipelineBlendMode::Normal
    );
    assert_eq!(graph.passes[0].material_index, Some(3));
    assert!(
        graph.passes[0]
            .bindings
            .contains(&TextureBindingRole::PassConstant {
                name: "tint".to_owned()
            })
    );
    assert_eq!(graph.passes[1].material_index, Some(4));
    assert!(
        graph.passes[1]
            .bindings
            .contains(&TextureBindingRole::PreviousGraphTarget { slot: 1 })
    );
    assert!(
        graph.passes[1]
            .bindings
            .contains(&TextureBindingRole::PassConstant {
                name: "speed".to_owned()
            })
    );
    assert_eq!(graph.passes[1].target, RenderTargetRole::NamedFbo);
    assert_eq!(graph.passes[2].role, RenderPassRole::ColorBlendPassthrough);
    assert_eq!(graph.passes[2].target, RenderTargetRole::ImageLocalMain);
    assert_eq!(
        graph.passes[2].state.pipeline_blend,
        PipelineBlendMode::Normal
    );
    assert!(
        graph.passes[2]
            .bindings
            .contains(&TextureBindingRole::NamedFboBind {
                slot: 1,
                name: "fbo_velocity".to_owned(),
            })
    );
    assert_eq!(graph.passes[3].role, RenderPassRole::SceneComposite);
    assert_eq!(graph.passes[3].target, RenderTargetRole::SceneColor);
    assert_eq!(
        graph.passes[3].shader.as_deref(),
        Some("we/objectcomposite")
    );
    assert!(
        graph
            .resource_uses()
            .iter()
            .any(|use_| use_.resource_key == "target:named-fbo:fbo_velocity")
    );
    assert!(
        graph
            .derived_barriers()
            .iter()
            .any(|barrier| barrier.resource_key == "target:named-fbo:fbo_velocity")
    );
}

#[test]
fn effect_target_base_replaces_the_offscreen_source_before_terminal_blending() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 7,
        base_material_index: Some(3),
        base_shader: Some("genericimage4".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![0],
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
            object_index: 7,
            effect_binding_start: 0,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(4),
            effect_file: "effects/waterwaves/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some("effects/waterwaves".to_owned()),
            source: None,
            target: None,
            binds: BTreeMap::new(),
            pass_constants: Vec::new(),
            material_blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            combos: BTreeMap::new(),
        }],
    });

    assert_eq!(
        graph.passes[0].state.pipeline_blend,
        PipelineBlendMode::Normal
    );
    assert_eq!(graph.passes[0].target, RenderTargetRole::ImageLocalMain);
    assert_eq!(graph.passes[1].target, RenderTargetRole::ImageLocalSub);
    assert_eq!(graph.passes[2].role, RenderPassRole::SceneComposite);
    assert_eq!(graph.passes[2].target, RenderTargetRole::SceneColor);
}

#[test]
fn authored_modulate_effect_composite_premultiplies_layer_alpha() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 7,
        base_material_index: Some(3),
        base_shader: Some("genericimage4".to_owned()),
        base_material_blending: Some("translucent".to_owned()),
        base_texture_slots: vec![0],
        base_pass_constants: Vec::new(),
        color_blend_mode: 0,
        framebuffer_snapshot: None,
        final_scene_blend: SceneBlendMode::Modulate,
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
            effect_file: "effects/waterripple/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some("effects/waterripple__SLOTS_5".to_owned()),
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

    assert_eq!(
        graph.passes[2].shader.as_deref(),
        Some("we/image-effect-modulate-composite")
    );
    assert_eq!(graph.passes[2].state.scene_blend, SceneBlendMode::Modulate);
}

#[test]
fn puppet_image_effects_run_before_skinning_composite() {
    let graph = we_image_graph(&WeImageGraphContract {
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
        effect_passes: vec![WeEffectPassContract {
            object_index: 7,
            effect_binding_start: 0,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(4),
            effect_file: "effects/waterwaves/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some("effects/waterwaves__SLOTS_3".to_owned()),
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

    assert_eq!(graph.passes.len(), 4);
    assert_eq!(graph.passes[0].role, RenderPassRole::ObjectLocalSource);
    assert_eq!(
        graph.passes[0].draw_primitive,
        RenderPassDrawPrimitive::ObjectUvSupportQuad
    );
    assert_eq!(
        graph.passes[0].shader.as_deref(),
        Some("we/image-effect-source")
    );
    assert_eq!(
        graph.passes[2].shader.as_deref(),
        Some("we/puppet-effect-composite")
    );
    assert_eq!(
        graph.passes[2].effect_visibility,
        RenderPassEffectVisibility::any_visible(0, 1)
    );
    assert_eq!(
        graph.passes[3].shader.as_deref(),
        Some("we/puppet-effect-composite")
    );
    assert_eq!(
        graph.passes[3].effect_visibility,
        RenderPassEffectVisibility::none_visible(0, 1)
    );
}

#[test]
fn we_image_graph_keeps_effect_copy_and_swap_command_passes() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 9,
        base_material_index: None,
        base_shader: Some("genericimage4".to_owned()),
        base_material_blending: None,
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
            WeEffectPassContract {
                object_index: 9,
                effect_binding_start: 0,
                effect_binding_count: 1,
                runtime_visibility: true,
                material_index: None,
                effect_file: "effects/fluid/effect.json".to_owned(),
                pass_index: 1,
                command: Some("copy".to_owned()),
                shader: None,
                source: Some("fbo_src".to_owned()),
                target: Some("fbo_dst".to_owned()),
                binds: BTreeMap::new(),
                pass_constants: Vec::new(),
                material_blending: None,
                depthtest: None,
                depthwrite: None,
                cullmode: None,
                combos: BTreeMap::new(),
            },
            WeEffectPassContract {
                object_index: 9,
                effect_binding_start: 0,
                effect_binding_count: 1,
                runtime_visibility: true,
                material_index: None,
                effect_file: "effects/fluid/effect.json".to_owned(),
                pass_index: 2,
                command: Some("swap".to_owned()),
                shader: None,
                source: Some("fbo_a".to_owned()),
                target: Some("fbo_b".to_owned()),
                binds: BTreeMap::new(),
                pass_constants: Vec::new(),
                material_blending: None,
                depthtest: None,
                depthwrite: None,
                cullmode: None,
                combos: BTreeMap::new(),
            },
        ],
    });

    assert_eq!(graph.passes[1].role, RenderPassRole::CopyTarget);
    assert_eq!(graph.passes[2].role, RenderPassRole::SwapTargetReferences);
    assert!(graph.unsupported.is_empty());
    assert!(
        graph
            .resource_uses()
            .iter()
            .any(|use_| use_.resource_key == "target:named-fbo:fbo_src")
    );
}

#[test]
fn effect_only_framebuffer_layer_without_effects_records_no_passes() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 11,
        base_material_index: Some(4),
        base_shader: Some("passthrough".to_owned()),
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
        final_effect_material: None,
        effect_passes: Vec::new(),
    });

    assert_eq!(graph.activation_policy, RenderGraphActivationPolicy::Always);
    assert!(graph.passes.is_empty());
}

#[test]
fn framebuffer_cloudmotion_final_material_composites_directly_to_scene() {
    let graph = we_image_graph(&WeImageGraphContract {
        object_index: 7,
        base_material_index: Some(27),
        base_shader: Some("passthrough".to_owned()),
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
        final_effect_material: None,
        effect_passes: vec![WeEffectPassContract {
            object_index: 7,
            effect_binding_start: 0,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(28),
            effect_file: "effects/cloudmotion/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some("effects/cloudmotion__SLOTS_5".to_owned()),
            source: None,
            target: None,
            binds: [(0, "previous".to_owned()), (2, "cloudnoise".to_owned())]
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

    assert_eq!(
        graph.activation_policy,
        RenderGraphActivationPolicy::AnyEffectVisible
    );
    assert_eq!(graph.passes.len(), 2);
    assert_eq!(graph.passes[0].role, RenderPassRole::CopyTarget);
    assert_eq!(graph.passes[1].role, RenderPassRole::SceneComposite);
    assert_eq!(
        graph.passes[1].draw_primitive,
        RenderPassDrawPrimitive::FullscreenTriangle
    );
    assert_eq!(graph.passes[1].target, RenderTargetRole::SceneColor);
    assert_eq!(
        graph.passes[1].state.pipeline_blend,
        PipelineBlendMode::Translucent
    );
    assert_eq!(graph.passes[1].state.color_write_mask, ColorWriteMask::Rgb);
    assert!(
        graph.passes[1]
            .bindings
            .contains(&TextureBindingRole::EffectTarget {
                slot: 0,
                name: "_rt_FullFrameBuffer".to_owned(),
            })
    );
    assert!(
        !graph
            .passes
            .iter()
            .any(|pass| pass.role == RenderPassRole::BaseMaterial)
    );
}

#[test]
fn composelayer_final_effect_material_composites_directly_to_scene() {
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
            target: None,
            binds: [(0, "previous".to_owned())].into_iter().collect(),
            pass_constants: vec!["alpha".to_owned()],
            material_blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            combos: BTreeMap::new(),
        }],
    });

    assert_eq!(
        graph.activation_policy,
        RenderGraphActivationPolicy::AnyEffectVisible
    );
    assert_eq!(graph.passes.len(), 3);
    assert_eq!(
        graph.passes[1].draw_primitive,
        RenderPassDrawPrimitive::FramebufferCompositeMesh
    );
    assert_eq!(
        graph.passes[1].state.pipeline_blend,
        PipelineBlendMode::Normal
    );
    assert_eq!(graph.passes[2].role, RenderPassRole::SceneComposite);
    assert_eq!(
        graph.passes[2].draw_primitive,
        RenderPassDrawPrimitive::ObjectMesh
    );
    assert_eq!(
        graph.passes[2].shader.as_deref(),
        Some("effects/opacity__SLOTS_1")
    );
    assert_eq!(graph.passes[2].target, RenderTargetRole::SceneColor);
    assert_eq!(
        graph.passes[2].state.pipeline_blend,
        PipelineBlendMode::Translucent
    );
    assert_eq!(graph.passes[2].state.color_write_mask, ColorWriteMask::Rgb);
    assert!(
        !graph.passes[2]
            .bindings
            .iter()
            .any(|binding| texture_binding_uses_slot(binding, 2))
    );
}

include!("tests/terminal_composition.rs");

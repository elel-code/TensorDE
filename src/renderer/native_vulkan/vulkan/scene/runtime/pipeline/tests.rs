    use super::*;
    use crate::engine::scene::{
        SceneBinaryDocument, SceneRenderPassKind, SceneRenderPassRecord, SceneRenderTargetKind,
        SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMeshDraw, SceneRenderingDevicePassNode,
        SceneRenderingDeviceTargetAllocation, SceneShaderContractRecord,
    };

    #[test]
    fn pipeline_indices_follow_drawn_pass_shader_and_blend_order() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                "genericimage4".to_owned(),
                "effects/opacity__SLOTS_1".to_owned(),
                "generic-pipeline".to_owned(),
                "opacity-pipeline".to_owned(),
            ],
            shader_contracts: vec![
                SceneShaderContractRecord {
                    shader_key: SceneStringId(0),
                    pipeline_key: SceneStringId(2),
                    texture_slot_mask: 0b1,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 2,
                    sampler_heap_count: 1,
                },
                SceneShaderContractRecord {
                    shader_key: SceneStringId(1),
                    pipeline_key: SceneStringId(3),
                    texture_slot_mask: 0b1,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 1,
                    sampler_heap_count: 1,
                },
            ],
            render_passes: vec![
                render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal),
                render_pass(1, SceneStringId(1), ScenePipelineBlend::Additive),
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let graph = graph_with_passes(vec![pass_node(0, 0, 1), pass_node(1, 1, 1)]);

        let layout = scene_pipeline_descriptor_layout(&storage, &graph).expect("layout");
        let indices =
            scene_pipeline_indices_for_draws(
                &storage,
                &graph,
                vk::Format::B8G8R8A8_UNORM,
                &[],
                false,
            )
            .expect("indices");

        assert_eq!(layout.sampled_slots, vec![0]);
        assert!(layout.material_uniform_enabled);
        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn pipeline_indices_include_dynamic_rendering_target_format() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["effects/opacity__SLOTS_1".to_owned(), "pipeline".to_owned()],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 1,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 1,
                sampler_heap_count: 1,
            }],
            render_passes: vec![
                render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal),
                render_pass(1, SceneStringId(0), ScenePipelineBlend::Normal),
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let mut offscreen_pass = pass_node(1, 1, 1);
        offscreen_pass.target = SceneRenderTargetKind::NamedFbo;
        offscreen_pass.target_name = SceneStringId(7);
        let mut graph = graph_with_passes(vec![pass_node(0, 0, 1), offscreen_pass]);
        graph.target_allocations = vec![SceneRenderingDeviceTargetAllocation {
            graph_index: 0,
            target: SceneRenderTargetKind::NamedFbo,
            target_name: SceneStringId(7),
            first_write_pass_id: 1,
            last_use_pass_id: 1,
            physical_slot: 3,
            width: 0,
            height: 0,
        }];
        let target_plans = vec![SceneEffectTargetImagePlan {
            physical_slot: 3,
            graph_index: 0,
            target: SceneRenderTargetKind::NamedFbo,
            target_name: SceneStringId(7),
            format: vk::Format::R16G16B16A16_SFLOAT,
            width: 960,
            height: 540,
            batch_field_count: 1,
            batch_atlas_columns: 1,
            batch_atlas_rows: 1,
            persistent_across_frames: true,
            aliased_logical_target_count: 1,
        }];

        let indices = scene_pipeline_indices_for_draws(
            &storage,
            &graph,
            vk::Format::B8G8R8A8_UNORM,
            &target_plans,
            false,
        )
        .expect("indices");

        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn pipeline_indices_keep_scene_color_msaa_separate_from_single_sample_effect_targets() {
        let mut scene_pass = render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal);
        scene_pass.scene_blend = SceneCompositeBlend::Normal;
        let mut effect_pass = render_pass(1, SceneStringId(0), ScenePipelineBlend::Normal);
        effect_pass.scene_blend = SceneCompositeBlend::Normal;
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["effects/opacity__SLOTS_1".to_owned(), "pipeline".to_owned()],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 1,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 1,
                sampler_heap_count: 1,
            }],
            render_passes: vec![scene_pass, effect_pass],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let mut offscreen_pass = pass_node(1, 1, 1);
        offscreen_pass.target = SceneRenderTargetKind::NamedFbo;
        offscreen_pass.target_name = SceneStringId(7);
        let mut graph = graph_with_passes(vec![pass_node(0, 0, 1), offscreen_pass]);
        graph.target_allocations = vec![SceneRenderingDeviceTargetAllocation {
            graph_index: 0,
            target: SceneRenderTargetKind::NamedFbo,
            target_name: SceneStringId(7),
            first_write_pass_id: 1,
            last_use_pass_id: 1,
            physical_slot: 3,
            width: 0,
            height: 0,
        }];
        let target_plans = vec![SceneEffectTargetImagePlan {
            physical_slot: 3,
            graph_index: 0,
            target: SceneRenderTargetKind::NamedFbo,
            target_name: SceneStringId(7),
            format: vk::Format::B8G8R8A8_UNORM,
            width: 960,
            height: 540,
            batch_field_count: 1,
            batch_atlas_columns: 1,
            batch_atlas_rows: 1,
            persistent_across_frames: true,
            aliased_logical_target_count: 1,
        }];

        let single_sample = scene_pipeline_indices_for_draws(
            &storage,
            &graph,
            vk::Format::B8G8R8A8_UNORM,
            &target_plans,
            false,
        )
        .expect("single-sample indices");
        let scene_msaa = scene_pipeline_indices_for_draws(
            &storage,
            &graph,
            vk::Format::B8G8R8A8_UNORM,
            &target_plans,
            true,
        )
        .expect("scene MSAA indices");

        assert_eq!(single_sample, vec![0, 0]);
        assert_eq!(scene_msaa, vec![0, 1]);
    }

    #[test]
    fn final_target_pipeline_keys_include_scene_composite_blend() {
        let mut alpha = render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal);
        alpha.scene_blend = SceneCompositeBlend::Alpha;
        let mut multiply = render_pass(1, SceneStringId(0), ScenePipelineBlend::Normal);
        multiply.scene_blend = SceneCompositeBlend::Multiply;
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["genericimage4".to_owned(), "pipeline".to_owned()],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 1,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 2,
                sampler_heap_count: 1,
            }],
            render_passes: vec![alpha, multiply],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let graph = graph_with_passes(vec![pass_node(0, 0, 1), pass_node(1, 1, 1)]);

        let indices =
            scene_pipeline_indices_for_draws(
                &storage,
                &graph,
                vk::Format::B8G8R8A8_UNORM,
                &[],
                false,
            )
            .expect("indices");

        assert_eq!(indices, vec![0, 1]);
        assert_eq!(
            scene_gpu_blend(
                &storage,
                &storage.document().render_passes[0],
                SceneRenderTargetKind::SceneColor
            ),
            SceneGpuBlend::Alpha
        );
        assert_eq!(
            scene_gpu_blend(
                &storage,
                &storage.document().render_passes[1],
                SceneRenderTargetKind::SceneColor
            ),
            SceneGpuBlend::Multiply
        );
        assert_eq!(
            scene_gpu_blend(
                &storage,
                &storage.document().render_passes[1],
                SceneRenderTargetKind::NamedFbo
            ),
            SceneGpuBlend::Replace
        );
    }

    #[test]
    fn foliage_screen_variant_uses_standard_premultiplied_screen_blend() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                "we/image-foliage-ripple-screen-composite".to_owned(),
                "we/image-foliage-ripple-screen-composite__GILDER_FOLIAGE_POWER_TWO_1"
                    .to_owned(),
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let mut pass = render_pass(0, SceneStringId(0), ScenePipelineBlend::Translucent);
        pass.scene_blend = SceneCompositeBlend::Screen;

        assert_eq!(
            scene_gpu_blend(&storage, &pass, SceneRenderTargetKind::SceneColor),
            SceneGpuBlend::ScreenPremultiplied
        );
        pass.shader_key = SceneStringId(1);
        assert_eq!(
            scene_gpu_blend(&storage, &pass, SceneRenderTargetKind::SceneColor),
            SceneGpuBlend::ScreenPremultiplied
        );
        let attachment = scene_color_blend_attachment(SceneGpuBlend::ScreenPremultiplied);
        assert_eq!(attachment.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(attachment.src_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(
            attachment.dst_color_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_COLOR
        );
        assert_eq!(attachment.src_alpha_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(
            attachment.dst_alpha_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
    }

    #[test]
    fn typed_multiply_variant_uses_standard_premultiplied_multiply_blend() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["we/image-waterwaves-multiply-composite".to_owned()],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let mut pass = render_pass(0, SceneStringId(0), ScenePipelineBlend::Translucent);
        pass.scene_blend = SceneCompositeBlend::Multiply;

        assert_eq!(
            scene_gpu_blend(&storage, &pass, SceneRenderTargetKind::SceneColor),
            SceneGpuBlend::MultiplyPremultiplied
        );
        let attachment = scene_color_blend_attachment(SceneGpuBlend::MultiplyPremultiplied);
        assert_eq!(attachment.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(
            attachment.src_color_blend_factor,
            vk::BlendFactor::DST_COLOR
        );
        assert_eq!(
            attachment.dst_color_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
        assert_eq!(attachment.src_alpha_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(
            attachment.dst_alpha_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
    }

    #[test]
    fn rounded_hsl_quad_declares_disjoint_advanced_blend_coverage() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["we/flat-rounded-mask-composite".to_owned()],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let mut pass = render_pass(0, SceneStringId(0), ScenePipelineBlend::Translucent);
        pass.scene_blend = SceneCompositeBlend::HslColor;

        assert_eq!(
            advanced_blend_overlap(&storage, &pass),
            vk::BlendOverlapEXT::DISJOINT
        );
    }

    #[test]
    fn material_normal_replaces_while_translucent_alpha_blends() {
        assert_eq!(
            pipeline_gpu_blend(ScenePipelineBlend::Normal),
            SceneGpuBlend::Replace
        );
        assert_eq!(
            pipeline_gpu_blend(ScenePipelineBlend::Disabled),
            SceneGpuBlend::Replace
        );
        assert_eq!(
            pipeline_gpu_blend(ScenePipelineBlend::Translucent),
            SceneGpuBlend::Alpha
        );

        let replace = scene_color_blend_attachment(SceneGpuBlend::Replace);
        let translucent = scene_color_blend_attachment(SceneGpuBlend::Alpha);
        assert_eq!(replace.blend_enable, vk::FALSE);
        assert_eq!(translucent.blend_enable, vk::TRUE);
        assert_eq!(
            translucent.src_color_blend_factor,
            vk::BlendFactor::SRC_ALPHA
        );
        assert_eq!(
            translucent.dst_color_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
    }

    #[test]
    fn advanced_blend_marks_only_effect_output_as_premultiplied() {
        let mut effect = render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal);
        effect.scene_blend = SceneCompositeBlend::Multiply;
        assert!(advanced_source_is_premultiplied(&effect));

        effect.role = SceneRenderPassKind::BaseMaterial;
        assert!(!advanced_source_is_premultiplied(&effect));

        effect.role = SceneRenderPassKind::EffectMaterial;
        effect.scene_blend = SceneCompositeBlend::Alpha;
        assert!(!advanced_source_is_premultiplied(&effect));
    }

    #[test]
    fn gpu_blend_attachments_match_we_composite_equations() {
        let multiply = scene_color_blend_attachment(SceneGpuBlend::Multiply);
        assert_eq!(multiply.color_blend_op, vk::BlendOp::MULTIPLY_EXT);
        assert_eq!(multiply.alpha_blend_op, vk::BlendOp::MULTIPLY_EXT);

        let modulate = scene_color_blend_attachment(SceneGpuBlend::Modulate);
        assert_eq!(modulate.src_color_blend_factor, vk::BlendFactor::DST_COLOR);
        assert_eq!(modulate.dst_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(modulate.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(modulate.src_alpha_blend_factor, vk::BlendFactor::ZERO);
        assert_eq!(modulate.dst_alpha_blend_factor, vk::BlendFactor::ONE);
    }

    fn graph_with_passes(
        pass_nodes: Vec<SceneRenderingDevicePassNode>,
    ) -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes,
            mesh_draws: vec![draw(), draw()],
            target_allocations: Vec::new(),
            effect_batches: Vec::new(),
            effect_batch_instances: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: Vec::new(),
            resolved_object_count: 0,
            resolved_visible_object_count: 0,
            resolved_attachment_link_count: 0,
            resolved_visible_effect_instance_count: 0,
            resolved_visible_effect_pass_count: 0,
            resolved_visible_effect_fbo_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 0,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 0,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        }
    }

    fn pass_node(
        pass_record_index: u32,
        mesh_draw_start: u32,
        mesh_draw_count: u32,
    ) -> SceneRenderingDevicePassNode {
        SceneRenderingDevicePassNode {
            graph_index: 0,
            pass_record_index,
            pass_id: pass_record_index,
            role: SceneRenderPassKind::EffectMaterial,
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            mesh_draw_start,
            mesh_draw_count,
        }
    }

    fn render_pass(
        id: u32,
        shader_key: SceneStringId,
        pipeline_blend: ScenePipelineBlend,
    ) -> SceneRenderPassRecord {
        SceneRenderPassRecord {
            id,
            role: SceneRenderPassKind::EffectMaterial,
            object: crate::engine::scene::SceneObjectHandle(
                crate::engine::scene::INVALID_OBJECT_ID,
            ),
            material: crate::engine::scene::SceneMaterialHandle(
                crate::engine::scene::INVALID_MATERIAL_ID,
            ),
            pass_index: id,
            shader_key,
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            pipeline_blend,
            scene_blend: crate::engine::scene::SceneCompositeBlend::Alpha,
            depth_test: crate::engine::scene::SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: crate::engine::scene::SceneCullMode::None,
        }
    }

    fn draw() -> SceneRenderingDeviceMeshDraw {
        SceneRenderingDeviceMeshDraw {
            primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            mesh_index: crate::engine::scene::INVALID_OBJECT_ID,
            resolved_object_index: crate::engine::scene::INVALID_OBJECT_ID,
            clip_transform: [[0.0; 4]; 4],
            authored_source_extent: [0.0; 2],
            skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
            skinning_palette_count: 0,
            resolved_color: crate::engine::scene::SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            resolved_alpha: 1.0,
            apply_resolved_visual: true,
            effect_batch_atlas_tile: u32::MAX,
            effect_batch_atlas_grid: [0; 2],
            object: crate::engine::scene::SceneObjectHandle(
                crate::engine::scene::INVALID_OBJECT_ID,
            ),
            material: crate::engine::scene::SceneMaterialHandle(
                crate::engine::scene::INVALID_MATERIAL_ID,
            ),
            vertex_start: 0,
            vertex_count: 3,
            index_start: 0,
            index_count: 3,
            instance_count: 1,
        }
    }

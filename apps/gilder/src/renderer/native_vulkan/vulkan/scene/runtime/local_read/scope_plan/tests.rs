    use super::*;
    use crate::engine::scene::{
        SceneBinaryDocument, SceneColorWriteMask, SceneCompositeBlend, SceneCullMode,
        SceneDepthTest, SceneMaterialHandle, SceneObjectHandle, ScenePipelineBlend,
        SceneRenderEffectVisibilityPolicy, SceneRenderPassKind, SceneRenderPassRecord,
        SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceMeshDraw,
        SceneRenderingDeviceSampledBinding, SceneRenderingDeviceTargetAllocation,
        SceneShaderContractRecord, SceneVec3, INVALID_MATERIAL_ID, INVALID_OBJECT_ID,
    };

    #[test]
    fn planner_keeps_adjacent_exact_pixel_passes_in_one_typed_scope() {
        let storage = local_read_storage("we/passthrough", 1);
        let graph = local_read_graph();
        let plans = effect_target_plans(true, [64, 64], [64, 64]);

        let scopes = scene_local_read_scope_plans(&storage, &graph, &plans)
            .expect("typed local-read scope");

        assert_eq!(scopes.len(), 1);
        let scope = &scopes[0];
        assert_eq!(scope.graph_index, 0);
        assert_eq!(scope.pass_role(0), Some(SceneLocalReadScopePassRole::Producer));
        assert_eq!(scope.pass_role(1), Some(SceneLocalReadScopePassRole::Consumer));
        assert_eq!(scope.extent.width, 64);
        assert_eq!(scope.color_attachment_formats, [vk::Format::R8G8B8A8_UNORM; 2]);
        assert_eq!(scope.input_slot, 0);
        assert_eq!(scope.input_attachment_index, 0);

        let producer_access =
            scene_pipeline_shader_descriptor_access(&storage, SceneStringId(0))
                .expect("producer descriptor access");
        let producer_metadata = scope
            .pipeline_metadata(
                SceneLocalReadScopePassRole::Producer,
                &producer_access,
                None,
                SceneLocalReadDeviceLimits::new(8, 8),
            )
            .expect("producer pipeline metadata");
        assert_eq!(producer_metadata.local_read_fragment_spirv(), None);

        let consumer_access =
            scene_pipeline_shader_descriptor_access(&storage, SceneStringId(1))
                .expect("consumer descriptor access");
        let consumer_shader = native_vulkan_scene_shader_for_key("we/passthrough")
            .expect("passthrough")
            .local_read_shader
            .expect("local-read variant");
        let consumer_metadata = scope
            .pipeline_metadata(
                SceneLocalReadScopePassRole::Consumer,
                &consumer_access,
                Some(&consumer_shader),
                SceneLocalReadDeviceLimits::new(8, 8),
            )
            .expect("consumer pipeline metadata");
        assert_eq!(
            consumer_metadata.local_read_fragment_spirv(),
            Some(consumer_shader.fragment_spirv)
        );
}
    #[test]
    fn planner_rejects_scope_without_usage_extent_or_explicit_shader_contract() {
        let graph = local_read_graph();
        let storage = local_read_storage("we/passthrough", 1);
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(false, false, [64, 64], [64, 64]),
        )
        .expect_err("input usage is required");
        assert!(error.contains("lacks input-attachment image usage"));

        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [32, 64]),
        )
        .expect_err("matching extents are required");
        assert!(error.contains("mismatched extents"));

        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, false, [64, 64], [64, 64]),
        )
        .expect_err("both attached images require input usage");
        assert!(error.contains("destination physical slot"));

        let storage = local_read_storage("we/genericimage4", 1);
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("explicit subpassInput shader is required");
        assert!(error.contains("no explicit subpassInput variant"));
    }

    #[test]
    fn planner_rejects_non_adjacent_or_aliased_producer_instead_of_sampling() {
        let storage = local_read_storage("we/passthrough", 1);
        let mut graph = local_read_graph();
        graph.pass_nodes.insert(1, SceneRenderingDevicePassNode {
            graph_index: 0,
            graph_activation_policy: crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
            pass_record_index: 0,
            pass_id: 7,
            role: SceneRenderPassKind::CopyTarget,
            target: SceneRenderTargetKind::Temporary,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            mesh_draw_start: 1,
            mesh_draw_count: 0,
        });
        graph.sampled_bindings[0].pass_node_index = 2;
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("copy boundary cannot be crossed");
        assert!(error.contains("must contain authored draws"));

        let mut graph = local_read_graph();
        graph.target_allocations[1].physical_slot = 0;
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("live targets cannot alias");
        assert!(error.contains("alias physical target"));

        let mut graph = local_read_graph();
        graph.pass_nodes[0].role = SceneRenderPassKind::CopyTarget;
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("copy pass cannot become a local-read producer");
        assert!(error.contains("cannot cross copy or swap-reference passes"));

        let mut graph = local_read_graph();
        graph.sampled_bindings.push(SceneRenderingDeviceSampledBinding {
            pass_node_index: 1,
            graph_index: 0,
            mesh_draw_start: 1,
            mesh_draw_count: 1,
            kind: crate::engine::scene::SceneRenderBindingKind::EffectTarget,
            slot: 2,
            target: SceneRenderTargetKind::ImageLocalMain,
            target_name: SceneStringId::NONE,
            access: SceneRenderingDeviceImageAccess::SampledImage,
        });
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("attached target cannot use sampled access");
        assert!(error.contains("attached targets cannot retain sampled-image layout"));

        let mut graph = local_read_graph();
        graph.sampled_bindings[0].mesh_draw_start = 0;
        let error = scene_local_read_scope_plans(
            &storage,
            &graph,
            &effect_target_plans_with_usage(true, true, [64, 64], [64, 64]),
        )
        .expect_err("input binding draw range must match consumer");
        assert!(error.contains("does not match consumer draw range"));
    }

    fn local_read_storage(consumer_shader: &str, consumer_input_mask: u32) -> SceneStorage {
        SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["we/genericimage4".to_owned(), consumer_shader.to_owned()],
            shader_contracts: vec![
                SceneShaderContractRecord {
                    shader_key: SceneStringId(0),
                    pipeline_key: SceneStringId(0),
                    texture_slot_mask: 0,
                    input_attachment_slot_mask: 0,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 2,
                    sampler_heap_count: 0,
                },
                SceneShaderContractRecord {
                    shader_key: SceneStringId(1),
                    pipeline_key: SceneStringId(1),
                    texture_slot_mask: 0,
                    input_attachment_slot_mask: consumer_input_mask,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 1,
                    sampler_heap_count: 0,
                },
            ],
            render_passes: vec![
                render_pass(0, SceneStringId(0), SceneRenderTargetKind::ImageLocalMain),
                render_pass(1, SceneStringId(1), SceneRenderTargetKind::ImageLocalSub),
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("local-read storage")
    }

    fn render_pass(
        id: u32,
        shader_key: SceneStringId,
        target: SceneRenderTargetKind,
    ) -> SceneRenderPassRecord {
        SceneRenderPassRecord {
            id,
            role: if id == 0 {
                SceneRenderPassKind::BaseMaterial
            } else {
                SceneRenderPassKind::EffectMaterial
            },
            draw_primitive: if id == 0 {
                crate::engine::scene::SceneRenderPassDrawPrimitive::ObjectMesh
            } else {
                crate::engine::scene::SceneRenderPassDrawPrimitive::FullscreenTriangle
            },
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            pass_index: id,
            shader_key,
            target,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            pipeline_blend: ScenePipelineBlend::Normal,
            scene_blend: SceneCompositeBlend::Alpha,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            color_write_mask: SceneColorWriteMask::Rgba,
            clear_target: false,
        }
    }

    fn local_read_graph() -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![
                pass_node(0, 0, SceneRenderTargetKind::ImageLocalMain),
                pass_node(1, 1, SceneRenderTargetKind::ImageLocalSub),
            ],
            target_allocations: vec![
                target_allocation(SceneRenderTargetKind::ImageLocalMain, 0),
                target_allocation(SceneRenderTargetKind::ImageLocalSub, 1),
            ],
            sampled_bindings: vec![SceneRenderingDeviceSampledBinding {
                pass_node_index: 1,
                graph_index: 0,
                mesh_draw_start: 1,
                mesh_draw_count: 1,
                kind: crate::engine::scene::SceneRenderBindingKind::PreviousGraphTarget,
                slot: 0,
                target: SceneRenderTargetKind::ImageLocalMain,
                target_name: SceneStringId::NONE,
                access: SceneRenderingDeviceImageAccess::InputAttachment,
            }],
            mesh_draws: vec![draw(SceneStringId(0)), draw(SceneStringId(1))],
            graph_physical_target_count: 2,
            descriptor_heap_required: true,
            fifo_latest_ready_present_required: true,
            ..SceneRenderingDeviceGraphPlan {
                pass_nodes: Vec::new(),
                target_allocations: Vec::new(),
                effect_batches: Vec::new(),
                effect_batch_instances: Vec::new(),
                sampled_bindings: Vec::new(),
                material_sampled_bindings: Vec::new(),
                mesh_draws: Vec::new(),
                puppet_bone_palettes: Vec::new(),
                puppet_bone_matrices: Vec::new(),
                particle_gpu_emitters: Vec::new(),
                resolved_object_count: 0,
                resolved_visible_object_count: 0,
                resolved_attachment_link_count: 0,
                resolved_visible_effect_instance_count: 0,
                resolved_visible_effect_pass_count: 0,
                resolved_visible_effect_fbo_count: 0,
                descriptor_heap_required: false,
                descriptor_heap_resource_count: 0,
                descriptor_heap_sampled_image_count: 0,
                descriptor_heap_uniform_buffer_count: 0,
                descriptor_heap_storage_buffer_count: 0,
                descriptor_heap_sampler_count: 0,
                graph_physical_target_count: 0,
                graph_aliased_target_count: 0,
                fifo_latest_ready_present_required: false,
            }
        }
    }

    fn pass_node(
        pass_id: u32,
        draw_start: u32,
        target: SceneRenderTargetKind,
    ) -> SceneRenderingDevicePassNode {
        SceneRenderingDevicePassNode {
            graph_index: 0,
            graph_activation_policy: crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
            pass_record_index: pass_id,
            pass_id,
            role: if pass_id == 0 {
                SceneRenderPassKind::BaseMaterial
            } else {
                SceneRenderPassKind::EffectMaterial
            },
            target,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            mesh_draw_start: draw_start,
            mesh_draw_count: 1,
        }
    }

    fn target_allocation(
        target: SceneRenderTargetKind,
        physical_slot: u32,
    ) -> SceneRenderingDeviceTargetAllocation {
        SceneRenderingDeviceTargetAllocation {
            graph_index: 0,
            target,
            target_name: SceneStringId::NONE,
            first_write_pass_id: physical_slot,
            last_use_pass_id: physical_slot + 1,
            physical_slot,
            width: 64,
            height: 64,
        }
    }

    fn draw(shader_key: SceneStringId) -> SceneRenderingDeviceMeshDraw {
        SceneRenderingDeviceMeshDraw {
            primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            shader_key,
            mesh_index: INVALID_OBJECT_ID,
            resolved_object_index: INVALID_OBJECT_ID,
            render_world_matrix: [[0.0; 4]; 4],
            clip_transform: [[0.0; 4]; 4],
            effect_model_view_projection_matrix: [[0.0; 4]; 4],
            authored_source_extent: [64.0, 64.0],
            skinning_palette_start: 0,
            skinning_palette_count: 0,
            resolved_color: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            resolved_alpha: 1.0,
            apply_resolved_visual: false,
            effect_batch_atlas_tile: INVALID_OBJECT_ID,
            effect_batch_atlas_grid: [0; 2],
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            resolved_effect_visibility_mask: 0,
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 3,
            index_start: 0,
            index_count: 3,
            instance_count: 1,
        }
    }

    fn effect_target_plans(
        source_input_usage: bool,
        source_extent: [u32; 2],
        destination_extent: [u32; 2],
    ) -> Vec<SceneEffectTargetImagePlan> {
        effect_target_plans_with_usage(
            source_input_usage,
            source_input_usage,
            source_extent,
            destination_extent,
        )
    }

    fn effect_target_plans_with_usage(
        source_input_usage: bool,
        destination_input_usage: bool,
        source_extent: [u32; 2],
        destination_extent: [u32; 2],
    ) -> Vec<SceneEffectTargetImagePlan> {
        vec![
            effect_target_plan(0, SceneRenderTargetKind::ImageLocalMain, source_extent, source_input_usage),
            effect_target_plan(1, SceneRenderTargetKind::ImageLocalSub, destination_extent, destination_input_usage),
        ]
    }

    fn effect_target_plan(
        physical_slot: u32,
        target: SceneRenderTargetKind,
        extent: [u32; 2],
        input_attachment_required: bool,
    ) -> SceneEffectTargetImagePlan {
        SceneEffectTargetImagePlan {
            physical_slot,
            graph_index: 0,
            target,
            target_name: SceneStringId::NONE,
            format: vk::Format::R8G8B8A8_UNORM,
            width: extent[0],
            height: extent[1],
            batch_field_count: 1,
            batch_atlas_columns: 1,
            batch_atlas_rows: 1,
            persistent_across_frames: false,
            aliased_logical_target_count: 1,
            input_attachment_required,
        }
    }

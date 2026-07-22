use super::*;

#[test]
fn procedural_particle_capacity_omits_permanently_inactive_slots() {
    let mut particle = SceneParticleSystemRecord::unsupported(
        SceneObjectHandle(0),
        SceneResourceId(0),
        SceneMaterialHandle(0),
        0,
        3000,
        1.0,
        0.0,
        1.0,
    );
    particle.rate = 200.0;
    particle.lifetime_max = 8.0;

    assert_eq!(procedural_particle_instance_capacity(&particle), 1600);
    particle.max_count = 500;
    assert_eq!(procedural_particle_instance_capacity(&particle), 500);
}

#[test]
fn particle_gpu_plan_selects_profiles_and_stable_indices() {
    let mut analytic = SceneParticleSystemRecord::unsupported(
        SceneObjectHandle(4),
        SceneResourceId(0),
        SceneMaterialHandle(0),
        0,
        300,
        1.0,
        0.0,
        1.0,
    );
    analytic.simulation = SceneParticleSimulationKind::FallingLeaves;
    analytic.rate = 20.0;
    analytic.lifetime_max = 5.0;
    let retained = SceneParticleSystemRecord::unsupported(
        SceneObjectHandle(9),
        SceneResourceId(0),
        SceneMaterialHandle(0),
        0,
        12,
        1.0,
        0.0,
        1.0,
    );

    let plans = particle_gpu_emitter_plans(&[analytic, retained]);
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].profile, SceneParticleGpuProfile::AnalyticBillboard);
    assert_eq!(plans[0].capacity, 100);
    assert_eq!(plans[0].state_index, 0);
    assert_eq!(plans[1].profile, SceneParticleGpuProfile::RetainedState);
    assert_eq!(plans[1].state_index, 1);
}
use crate::engine::scene::{RenderingServer, SceneStorage};
use crate::engine::scene::{
    SceneBinaryDocument, SceneEffectHandle, SceneEffectRecord, SceneMaterialHandle,
    SceneMaterialRecord, SceneMeshRecord, SceneMeshVertexRecord, SceneObjectEffectRecord,
    SceneObjectHandle, SceneObjectKind, SceneObjectRecord, ScenePuppetBoneRecord,
    ScenePuppetRecord, SceneRenderBindingKind, SceneRenderBindingRecord, SceneRenderGraphRecord,
    SceneRenderPassRecord, SceneResourceId, SceneResourceKind, SceneResourceRecord,
    SceneShaderContractRecord, SceneStringId, SceneTextureFormat, SceneTextureRecord, SceneVec3,
};

#[test]
fn authored_targets_only_alias_images_with_identical_extents() {
    let base = TargetAllocationCompatibility {
        format: SceneStringId(3),
        width_divisor_milli: 1_000,
        height_divisor_milli: 1_000,
        authored_width: 1_571,
        authored_height: 2_621,
        authored_texture_space: true,
    };
    assert!(target_allocations_are_compatible(base, base));
    assert!(!target_allocations_are_compatible(
        base,
        TargetAllocationCompatibility {
            authored_width: 2_318,
            authored_height: 1_794,
            ..base
        }
    ));
}

#[test]
fn scene_projection_maps_authored_bounds_to_vulkan_ndc() {
    let mut project = SceneBinaryDocument::default().project;
    project.logical_width = 3840;
    project.logical_height = 2160;
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];

    let transform = scene_clip_transform(&project, identity);

    assert_eq!(transform[0], [2.0 / 3840.0, 0.0, 0.0, -1.0]);
    assert_eq!(transform[1], [0.0, -2.0 / 2160.0, 0.0, 1.0]);
    assert_eq!(transform[3], [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn rendering_device_graph_plans_mesh_draws_and_heap_counts() {
    let document = SceneBinaryDocument {
        strings: vec!["shader".to_owned(), "pipeline".to_owned()],
        objects: vec![
            SceneObjectRecord {
                id: SceneObjectHandle(0),
                we_id: 7,
                name: SceneStringId::NONE,
                kind: SceneObjectKind::Puppet,
                resource: SceneResourceId::NONE,
                material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                parent_we_id: INVALID_OBJECT_ID,
                attachment: SceneStringId::NONE,
                origin: SceneVec3::default(),
                angles: SceneVec3::default(),
                scale: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                color: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                alpha: 1.0,
                visible: true,
                color_blend_mode: 0,
                sort_order: 0,
                effect_start: 0,
                effect_count: 1,
                render_graph: 0,
            },
            SceneObjectRecord {
                id: SceneObjectHandle(1),
                we_id: 8,
                name: SceneStringId::NONE,
                kind: SceneObjectKind::Image,
                resource: SceneResourceId::NONE,
                material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                parent_we_id: INVALID_OBJECT_ID,
                attachment: SceneStringId::NONE,
                origin: SceneVec3::default(),
                angles: SceneVec3::default(),
                scale: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                color: SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                alpha: 1.0,
                visible: false,
                color_blend_mode: 0,
                sort_order: 0,
                effect_start: u32::MAX,
                effect_count: 0,
                render_graph: 1,
            },
        ],
        meshes: vec![
            SceneMeshRecord {
                object: SceneObjectHandle(0),
                material: SceneMaterialHandle(0),
                vertex_start: 0,
                vertex_count: 4,
                index_start: 0,
                index_count: 6,
                width: 64.0,
                height: 32.0,
                bounds_min: SceneVec3 {
                    x: -32.0,
                    y: -16.0,
                    z: 0.0,
                },
                bounds_max: SceneVec3 {
                    x: 32.0,
                    y: 16.0,
                    z: 0.0,
                },
            },
            SceneMeshRecord {
                object: SceneObjectHandle(1),
                material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                vertex_start: 4,
                vertex_count: 4,
                index_start: 6,
                index_count: 6,
                width: 64.0,
                height: 32.0,
                bounds_min: SceneVec3 {
                    x: -32.0,
                    y: -16.0,
                    z: 0.0,
                },
                bounds_max: SceneVec3 {
                    x: 32.0,
                    y: 16.0,
                    z: 0.0,
                },
            },
        ],
        mesh_vertices: vec![
            SceneMeshVertexRecord {
                position: SceneVec3 {
                    x: -32.0,
                    y: -16.0,
                    z: 0.0,
                },
                uv: [0.0, 1.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            };
            8
        ],
        mesh_indices: vec![0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3],
        puppets: vec![ScenePuppetRecord {
            object: SceneObjectHandle(0),
            resource: SceneResourceId::NONE,
            mesh_start: 0,
            mesh_count: 1,
            bone_start: 0,
            bone_count: 1,
            attachment_start: 0,
            attachment_count: 0,
        }],
        puppet_bones: vec![ScenePuppetBoneRecord {
            puppet: 0,
            bone_index: 41,
            name: SceneStringId::NONE,
            simulation_type: 0,
            parent_index: -1,
            local_bind_matrix: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            simulation_json: SceneStringId::NONE,
        }],
        materials: vec![
            SceneMaterialRecord {
                id: SceneMaterialHandle(0),
                resource: SceneResourceId::NONE,
                pass_start: 0,
                pass_count: 0,
            },
            SceneMaterialRecord {
                id: SceneMaterialHandle(1),
                resource: SceneResourceId::NONE,
                pass_start: 0,
                pass_count: 0,
            },
        ],
        effects: vec![SceneEffectRecord {
            id: SceneEffectHandle(0),
            resource: SceneResourceId::NONE,
            replacement_key: SceneStringId::NONE,
            pass_start: 0,
            pass_count: 0,
            fbo_start: 0,
            fbo_count: 0,
        }],
        object_effects: vec![SceneObjectEffectRecord {
            object: SceneObjectHandle(0),
            effect: SceneEffectHandle(0),
            name: SceneStringId::NONE,
            instance_id: 0,
            visible: false,
        }],
        render_graphs: vec![
            SceneRenderGraphRecord {
                object: SceneObjectHandle(0),
                activation_policy: SceneRenderGraphActivationPolicy::AnyEffectVisible,
                pass_start: 0,
                pass_count: 1,
                unsupported_start: 0,
                unsupported_count: 0,
            },
            SceneRenderGraphRecord {
                object: SceneObjectHandle(1),
                activation_policy: SceneRenderGraphActivationPolicy::Always,
                pass_start: 1,
                pass_count: 1,
                unsupported_start: 0,
                unsupported_count: 0,
            },
        ],
        render_passes: vec![
            SceneRenderPassRecord {
                id: 9,
                role: SceneRenderPassKind::BaseMaterial,
                object: SceneObjectHandle(0),
                material: SceneMaterialHandle(1),
                pass_index: 0,
                shader_key: SceneStringId(0),
                target: SceneRenderTargetKind::SceneColor,
                target_name: SceneStringId::NONE,
                binding_start: 0,
                binding_count: 0,
                effect_binding_start: 0,
                effect_binding_count: 1,
                effect_visibility_policy: SceneRenderEffectVisibilityPolicy::MaterialStages,
                pipeline_blend: ScenePipelineBlend::Normal,
                scene_blend: SceneCompositeBlend::Alpha,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
                color_write_mask: SceneColorWriteMask::Rgba,
                clear_target: false,
            },
            SceneRenderPassRecord {
                id: 10,
                role: SceneRenderPassKind::BaseMaterial,
                object: SceneObjectHandle(1),
                material: SceneMaterialHandle(INVALID_MATERIAL_ID),
                pass_index: 0,
                shader_key: SceneStringId(0),
                target: SceneRenderTargetKind::SceneColor,
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
            },
        ],
        shader_contracts: vec![SceneShaderContractRecord {
            shader_key: SceneStringId(0),
            pipeline_key: SceneStringId(1),
            texture_slot_mask: 0b1,
            input_attachment_slot_mask: 0,
            constant_start: 0,
            constant_count: 0,
            resource_heap_count: 2,
            sampler_heap_count: 1,
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

    assert_eq!(graph.pass_nodes.len(), 2);
    assert_eq!(graph.pass_nodes[0].pass_id, 9);
    assert_eq!(
        graph.pass_nodes[0].graph_activation_policy,
        SceneRenderGraphActivationPolicy::AnyEffectVisible
    );
    assert_eq!(graph.pass_nodes[0].mesh_draw_count, 1);
    assert_eq!(graph.pass_nodes[1].pass_id, 10);
    assert_eq!(graph.pass_nodes[1].mesh_draw_count, 1);
    assert_eq!(graph.mesh_draws.len(), 2);
    assert_eq!(graph.mesh_draws[0].resolved_object_index, 0);
    assert_eq!(graph.mesh_draws[0].clip_transform[0][0], 2.0);
    assert_eq!(graph.mesh_draws[0].clip_transform[1][1], -2.0);
    assert_eq!(graph.mesh_draws[0].skinning_palette_start, 0);
    assert_eq!(graph.mesh_draws[0].skinning_palette_count, 1);
    assert_eq!(graph.mesh_draws[0].material, SceneMaterialHandle(1));
    assert_eq!(graph.mesh_draws[0].vertex_count, 4);
    assert_eq!(graph.mesh_draws[0].index_count, 6);
    assert_eq!(graph.mesh_draws[0].effect_binding_start, 0);
    assert_eq!(graph.mesh_draws[0].effect_binding_count, 1);
    assert_eq!(graph.mesh_draws[0].resolved_effect_visibility_mask, 0);
    assert_eq!(graph.mesh_draws[1].object, SceneObjectHandle(1));
    assert_eq!(graph.puppet_bone_palettes.len(), 1);
    assert_eq!(graph.puppet_bone_matrices.len(), 1);
    assert_eq!(graph.puppet_bone_matrices[0].bone_index, 41);
    assert_eq!(graph.resolved_object_count, 2);
    assert_eq!(graph.resolved_visible_object_count, 1);
    assert_eq!(graph.descriptor_heap_resource_count, 3);
    assert_eq!(graph.descriptor_heap_sampled_image_count, 1);
    assert_eq!(graph.descriptor_heap_uniform_buffer_count, 1);
    assert_eq!(graph.descriptor_heap_storage_buffer_count, 1);
    assert!(graph.fifo_latest_ready_present_required);
}

#[test]
fn rendering_device_graph_allocates_named_effect_targets_from_pass_bindings() {
    let document = SceneBinaryDocument {
        strings: vec!["fbo_a".to_owned(), "fbo_b".to_owned()],
        render_graphs: vec![SceneRenderGraphRecord {
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_start: 0,
            pass_count: 3,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        render_passes: vec![
            named_fbo_pass(1, 0, SceneStringId(0), 0, 0),
            named_fbo_pass(2, 1, SceneStringId(1), 0, 1),
            scene_color_pass_reading_fbo(3, 1, 1),
        ],
        render_bindings: vec![
            named_fbo_binding(SceneStringId(0), 0),
            named_fbo_binding(SceneStringId(1), 2),
        ],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

    assert_eq!(graph.target_allocations.len(), 2);
    assert_eq!(
        graph.target_allocations[0].target,
        SceneRenderTargetKind::NamedFbo
    );
    assert_eq!(graph.target_allocations[0].target_name, SceneStringId(0));
    assert_eq!(graph.target_allocations[0].last_use_pass_id, 2);
    assert_eq!(graph.target_allocations[1].target_name, SceneStringId(1));
    assert_eq!(graph.target_allocations[1].last_use_pass_id, 3);
    assert_eq!(graph.graph_physical_target_count, 2);
    assert_eq!(graph.graph_aliased_target_count, 0);
    assert_eq!(graph.sampled_bindings.len(), 2);
    assert_eq!(graph.sampled_bindings[0].pass_node_index, 1);
    assert_eq!(graph.sampled_bindings[0].slot, 0);
    assert_eq!(graph.sampled_bindings[1].pass_node_index, 2);
    assert_eq!(graph.sampled_bindings[1].slot, 2);
    assert_eq!(
        graph.sampled_bindings[1].logical_target(),
        Some((0, SceneRenderTargetKind::NamedFbo, SceneStringId(1)))
    );
}

#[test]
fn rendering_device_graph_preserves_explicit_input_attachment_access() {
    let mut consumer = scene_color_pass_reading_fbo(3, 1, 1);
    consumer.shader_key = SceneStringId(2);
    let document = SceneBinaryDocument {
        strings: vec![
            "fbo_a".to_owned(),
            "fbo_b".to_owned(),
            "typed/local-read".to_owned(),
            "pipeline".to_owned(),
        ],
        render_graphs: vec![SceneRenderGraphRecord {
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_start: 0,
            pass_count: 3,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        render_passes: vec![
            named_fbo_pass(1, 0, SceneStringId(0), 0, 0),
            named_fbo_pass(2, 1, SceneStringId(1), 0, 1),
            consumer,
        ],
        render_bindings: vec![
            named_fbo_binding(SceneStringId(0), 0),
            named_fbo_binding(SceneStringId(1), 2),
        ],
        shader_contracts: vec![SceneShaderContractRecord {
            shader_key: SceneStringId(2),
            pipeline_key: SceneStringId(3),
            texture_slot_mask: 0,
            input_attachment_slot_mask: 1 << 2,
            constant_start: 0,
            constant_count: 0,
            resource_heap_count: 1,
            sampler_heap_count: 0,
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

    assert_eq!(
        graph.sampled_bindings[0].access,
        SceneRenderingDeviceImageAccess::SampledImage
    );
    assert_eq!(
        graph.sampled_bindings[1].access,
        SceneRenderingDeviceImageAccess::InputAttachment
    );
}

#[test]
fn rendering_device_graph_does_not_alias_incompatible_effect_target_images() {
    let document = SceneBinaryDocument {
        strings: vec![
            "fbo_a".to_owned(),
            "fbo_b".to_owned(),
            "rgba8".to_owned(),
            "rgba16f".to_owned(),
        ],
        render_graphs: vec![SceneRenderGraphRecord {
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_start: 0,
            pass_count: 4,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        render_passes: vec![
            named_fbo_pass(1, 0, SceneStringId(0), 0, 0),
            scene_color_pass_reading_fbo(2, 0, 1),
            named_fbo_pass(3, 2, SceneStringId(1), 1, 0),
            scene_color_pass_reading_fbo(4, 1, 1),
        ],
        render_bindings: vec![
            named_fbo_binding(SceneStringId(0), 0),
            named_fbo_binding(SceneStringId(1), 0),
        ],
        image_targets: vec![
            SceneImageTargetRecord {
                name: SceneStringId(0),
                role: SceneRenderTargetKind::NamedFbo,
                format: SceneStringId(2),
                width_divisor_milli: 1_000,
                height_divisor_milli: 1_000,
            },
            SceneImageTargetRecord {
                name: SceneStringId(1),
                role: SceneRenderTargetKind::NamedFbo,
                format: SceneStringId(3),
                width_divisor_milli: 2_000,
                height_divisor_milli: 2_000,
            },
        ],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

    assert_eq!(graph.target_allocations.len(), 2);
    assert_eq!(graph.graph_physical_target_count, 2);
    assert_eq!(graph.graph_aliased_target_count, 0);
    assert_eq!(graph.target_allocations[0].physical_slot, 0);
    assert_eq!(graph.target_allocations[1].physical_slot, 1);
}

#[test]
fn same_named_fbo_in_distinct_graphs_keeps_graph_scoped_identity() {
    let document = SceneBinaryDocument {
        strings: vec!["fbo_shared".to_owned()],
        render_graphs: vec![
            SceneRenderGraphRecord {
                object: SceneObjectHandle(INVALID_OBJECT_ID),
                activation_policy: SceneRenderGraphActivationPolicy::Always,
                pass_start: 0,
                pass_count: 2,
                unsupported_start: 0,
                unsupported_count: 0,
            },
            SceneRenderGraphRecord {
                object: SceneObjectHandle(INVALID_OBJECT_ID),
                activation_policy: SceneRenderGraphActivationPolicy::Always,
                pass_start: 2,
                pass_count: 2,
                unsupported_start: 0,
                unsupported_count: 0,
            },
        ],
        render_passes: vec![
            named_fbo_pass(1, 0, SceneStringId(0), 0, 0),
            scene_color_pass_reading_fbo(2, 0, 1),
            named_fbo_pass(1, 0, SceneStringId(0), 1, 0),
            scene_color_pass_reading_fbo(2, 1, 1),
        ],
        render_bindings: vec![
            named_fbo_binding(SceneStringId(0), 0),
            named_fbo_binding(SceneStringId(0), 0),
        ],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

    assert_eq!(graph.target_allocations.len(), 2);
    assert_eq!(graph.target_allocations[0].graph_index, 0);
    assert_eq!(graph.target_allocations[1].graph_index, 1);
    assert_ne!(
        graph.target_allocations[0].physical_slot,
        graph.target_allocations[1].physical_slot
    );
    assert_eq!(
        graph.sampled_bindings[0].logical_target(),
        Some((0, SceneRenderTargetKind::NamedFbo, SceneStringId(0)))
    );
    assert_eq!(
        graph.sampled_bindings[1].logical_target(),
        Some((1, SceneRenderTargetKind::NamedFbo, SceneStringId(0)))
    );
}

#[test]
fn rendering_device_graph_uses_fullscreen_utility_for_effect_pass_without_object_mesh() {
    let document = SceneBinaryDocument {
        strings: vec!["effects/opacity__SLOTS_1".to_owned(), "fbo_a".to_owned()],
        render_graphs: vec![SceneRenderGraphRecord {
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_start: 0,
            pass_count: 1,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        render_passes: vec![SceneRenderPassRecord {
            id: 5,
            role: SceneRenderPassKind::EffectMaterial,
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            pass_index: 0,
            shader_key: SceneStringId(0),
            target: SceneRenderTargetKind::NamedFbo,
            target_name: SceneStringId(1),
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
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();

    assert_eq!(graph.pass_nodes[0].mesh_draw_start, 0);
    assert_eq!(graph.pass_nodes[0].mesh_draw_count, 1);
    assert_eq!(graph.mesh_draws.len(), 1);
    assert_eq!(
        graph.mesh_draws[0].primitive,
        SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
    );
    assert_eq!(graph.mesh_draws[0].vertex_count, 3);
    assert_eq!(graph.mesh_draws[0].index_count, 3);
    assert_eq!(
        graph.mesh_draws[0].clip_transform,
        identity_clip_transform()
    );
    assert_eq!(graph.mesh_draws[0].authored_source_extent, [0.0; 2]);
}

#[test]
fn direct_flat_rounded_mask_uses_object_uv_support_quad() {
    assert_eq!(
        shader_utility_primitive("we/flat-rounded-mask-composite"),
        Some(SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad)
    );
}

#[test]
fn aggregated_waterwaves_effect_run_uses_fullscreen_triangle() {
    assert_eq!(
        shader_utility_primitive("we/effect-waterwaves-direct__STAGES_6"),
        Some(SceneRenderingDeviceDrawPrimitive::FullscreenTriangle)
    );
}

#[test]
fn object_effect_utility_retains_semantic_transform_and_authored_source_extent() {
    let mut project = SceneBinaryDocument::default().project;
    project.logical_width = 200;
    project.logical_height = 100;
    let source_resource = SceneResourceId(7);
    let document = SceneBinaryDocument {
        project,
        strings: vec!["effects/waterwaves__SLOTS_1".to_owned()],
        resources: vec![SceneResourceRecord {
            id: source_resource,
            kind: SceneResourceKind::TextureTex,
            path: SceneStringId::NONE,
            source: SceneStringId::NONE,
            payload_offset: 0,
            payload_len: 0,
        }],
        textures: vec![SceneTextureRecord {
            resource: source_resource,
            format: SceneTextureFormat::Bc7UnormBlock,
            source_runtime_format: 0,
            payload_format: 0,
            sampler_flags: 0,
            width: 1571,
            height: 2621,
            storage_width: 1572,
            storage_height: 2624,
            mip_start: 0,
            mip_count: 0,
            texv_tag: SceneStringId::NONE,
            texb_tag: SceneStringId::NONE,
            payload_offset: 0,
            payload_len: 0,
            alpha_coverage_rows: [u32::MAX;
                crate::engine::scene::SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
        }],
        objects: vec![SceneObjectRecord {
            id: SceneObjectHandle(0),
            we_id: 937,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Puppet,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(0),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3 {
                x: 50.0,
                y: 20.0,
                z: 0.0,
            },
            angles: SceneVec3::default(),
            scale: SceneVec3::ONE,
            color: SceneVec3::ONE,
            alpha: 1.0,
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: u32::MAX,
            effect_count: 0,
            render_graph: 0,
        }],
        materials: vec![SceneMaterialRecord {
            id: SceneMaterialHandle(0),
            resource: SceneResourceId::NONE,
            pass_start: 0,
            pass_count: 1,
        }],
        material_passes: vec![SceneMaterialPassRecord {
            material: SceneMaterialHandle(0),
            shader_key: SceneStringId(0),
            target: SceneStringId::NONE,
            texture_start: 0,
            texture_count: 1,
            constant_start: 0,
            constant_count: 0,
            pipeline_blend: ScenePipelineBlend::Normal,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_writing: SceneStringId::NONE,
            clear_target: false,
        }],
        material_textures: vec![SceneMaterialTextureRecord {
            slot: 0,
            resource: source_resource,
            path: SceneStringId::NONE,
        }],
        render_graphs: vec![SceneRenderGraphRecord {
            object: SceneObjectHandle(0),
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_start: 0,
            pass_count: 1,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        render_passes: vec![SceneRenderPassRecord {
            id: 1,
            role: SceneRenderPassKind::EffectMaterial,
            object: SceneObjectHandle(0),
            material: SceneMaterialHandle(0),
            pass_index: 0,
            shader_key: SceneStringId(0),
            target: SceneRenderTargetKind::SceneColor,
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
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let graph = RenderingServer::new(&storage).rendering_device_graph_plan();
    let draw = graph.mesh_draws.first().expect("fullscreen effect draw");

    assert_eq!(
        draw.primitive,
        SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
    );
    assert_eq!(draw.authored_source_extent, [1571.0, 2621.0]);
    assert_eq!(draw.clip_transform[0], [0.01, 0.0, 0.0, -0.5]);
    assert_eq!(draw.clip_transform[1], [0.0, -0.02, 0.0, 0.6]);
}

#[test]
fn textureless_solid_layer_uses_authored_mesh_extent_for_local_effects() {
    let object = SceneObjectHandle(0);
    let document = SceneBinaryDocument {
        objects: vec![SceneObjectRecord {
            id: object,
            we_id: 1416,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3::default(),
            angles: SceneVec3::default(),
            scale: SceneVec3::ONE,
            color: SceneVec3::ONE,
            alpha: 1.0,
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: 0,
            effect_count: 0,
            render_graph: INVALID_OBJECT_ID,
        }],
        meshes: vec![SceneMeshRecord {
            object,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 0,
            index_start: 0,
            index_count: 0,
            width: 550.0,
            height: 3300.0,
            bounds_min: SceneVec3::default(),
            bounds_max: SceneVec3::default(),
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");

    assert_eq!(authored_source_extent(&storage, object), [550.0, 3300.0]);
}

#[test]
fn textureless_composite_layer_allocates_image_local_targets_at_authored_extent() {
    let object = SceneObjectHandle(0);
    let document = SceneBinaryDocument {
        objects: vec![SceneObjectRecord {
            id: object,
            we_id: 1212,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Image,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3::default(),
            angles: SceneVec3::default(),
            scale: SceneVec3::ONE,
            color: SceneVec3::ONE,
            alpha: 1.0,
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: 0,
            effect_count: 0,
            render_graph: 0,
        }],
        meshes: vec![SceneMeshRecord {
            object,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 0,
            index_start: 0,
            index_count: 0,
            width: 1760.0,
            height: 500.0,
            bounds_min: SceneVec3::default(),
            bounds_max: SceneVec3::default(),
        }],
        render_graphs: vec![SceneRenderGraphRecord {
            object,
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_start: 0,
            pass_count: 0,
            unsupported_start: 0,
            unsupported_count: 0,
        }],
        ..SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("storage");
    let compatibility = target_allocation_compatibility(
        &storage,
        TargetAllocationState {
            graph_index: 0,
            target: SceneRenderTargetKind::ImageLocalMain,
            target_name: SceneStringId::NONE,
            first_write_pass_id: 0,
            last_use_pass_id: 0,
            first_write_order: 0,
            last_use_order: 0,
        },
    );

    assert_eq!(compatibility.authored_width, 1760);
    assert_eq!(compatibility.authored_height, 500);
    assert!(compatibility.authored_texture_space);
}

#[test]
fn only_base_material_pass_draws_authored_object_mesh() {
    let storage =
        SceneStorage::from_document(SceneBinaryDocument::default()).expect("empty storage");
    let mut pass = named_fbo_pass(5, 0, SceneStringId(1), 0, 0);
    pass.object = SceneObjectHandle(7);
    assert!(!pass_draws_object_mesh(&storage, &pass));

    pass.role = SceneRenderPassKind::ColorBlendPassthrough;
    assert!(!pass_draws_object_mesh(&storage, &pass));

    pass.role = SceneRenderPassKind::BaseMaterial;
    assert!(pass_draws_object_mesh(&storage, &pass));
}

fn named_fbo_pass(
    id: u32,
    pass_index: u32,
    target_name: SceneStringId,
    binding_start: u32,
    binding_count: u32,
) -> SceneRenderPassRecord {
    SceneRenderPassRecord {
        id,
        role: SceneRenderPassKind::EffectMaterial,
        object: SceneObjectHandle(INVALID_OBJECT_ID),
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        pass_index,
        shader_key: SceneStringId::NONE,
        target: SceneRenderTargetKind::NamedFbo,
        target_name,
        binding_start,
        binding_count,
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

fn scene_color_pass_reading_fbo(
    id: u32,
    binding_start: u32,
    binding_count: u32,
) -> SceneRenderPassRecord {
    SceneRenderPassRecord {
        target: SceneRenderTargetKind::SceneColor,
        target_name: SceneStringId::NONE,
        ..named_fbo_pass(id, 2, SceneStringId::NONE, binding_start, binding_count)
    }
}

fn named_fbo_binding(name: SceneStringId, slot: u32) -> SceneRenderBindingRecord {
    SceneRenderBindingRecord {
        kind: SceneRenderBindingKind::NamedFboBind,
        slot,
        target: SceneRenderTargetKind::NamedFbo,
        name,
    }
}

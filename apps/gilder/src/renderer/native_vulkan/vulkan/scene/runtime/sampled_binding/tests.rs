use super::*;
use crate::engine::scene::{
    SceneBinaryDocument, SceneRenderBindingKind, SceneRenderingDeviceGraphPlan,
    SceneRenderingDeviceMaterialSampledBinding, SceneRenderingDevicePassNode,
    SceneRenderingDeviceSampledBinding, SceneStorage, SceneTargetExtentDomain,
};

#[test]
fn sampled_binding_plan_preserves_nonzero_slots_and_swap_rewrites() {
    let graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![
            pass_node(
                0,
                SceneRenderPassKind::EffectMaterial,
                SceneStringId(0),
                0,
                1,
            ),
            pass_node(
                1,
                SceneRenderPassKind::SwapTargetReferences,
                SceneStringId(1),
                1,
                0,
            ),
            pass_node(
                2,
                SceneRenderPassKind::EffectMaterial,
                SceneStringId(2),
                1,
                1,
            ),
        ],
        target_allocations: vec![
            allocation(SceneStringId(0), 0),
            allocation(SceneStringId(1), 1),
            allocation(SceneStringId(2), 2),
        ],
        sampled_bindings: vec![
            sampled_binding(0, 2, SceneStringId(1), 0, 1),
            sampled_binding(1, 0, SceneStringId(0), 1, 0),
            sampled_binding(2, 2, SceneStringId(0), 1, 1),
        ],
        material_sampled_bindings: vec![
            SceneRenderingDeviceMaterialSampledBinding {
                draw_index: 0,
                slot: 0,
                resource: SceneResourceId(7),
            },
            SceneRenderingDeviceMaterialSampledBinding {
                draw_index: 1,
                slot: 2,
                resource: SceneResourceId(8),
            },
        ],
        mesh_draws: vec![draw(), draw()],
        ..empty_graph_plan()
    };

    let plan = scene_sampled_image_binding_plan(&graph, &[0, 2], &[]).expect("binding plan");
    let cycle = scene_sampled_image_binding_cycle(&graph, &[0, 2], &[]).expect("binding cycle");

    assert_eq!(plan.effect_target_descriptor_count, 2);
    assert_eq!(plan.scene_texture_descriptor_count, 1);
    assert_eq!(plan.fallback_descriptor_count, 1);
    assert_eq!(cycle.len(), 2);
    assert_eq!(cycle[0].initial_reference_physical_slots, vec![0, 1, 2]);
    assert_eq!(cycle[1].initial_reference_physical_slots, vec![1, 0, 2]);
    assert_eq!(
        plan.source(0, 0),
        Some(SceneSampledImageSource::SceneTexture {
            resource: SceneResourceId(7)
        })
    );
    assert_eq!(
        plan.source(0, 1),
        Some(SceneSampledImageSource::EffectTarget {
            physical_slot: 1,
            batch_atlas_tile: 0,
        })
    );
    assert_eq!(
        plan.source(1, 1),
        Some(SceneSampledImageSource::EffectTarget {
            physical_slot: 1,
            batch_atlas_tile: 0,
        })
    );
    assert_eq!(
        cycle[1].source(1, 1),
        Some(SceneSampledImageSource::EffectTarget {
            physical_slot: 0,
            batch_atlas_tile: 0,
        })
    );
}
#[test]
fn sampled_binding_plan_rejects_unowned_input_attachment_access() {
    let target_name = SceneStringId(9);
    let mut binding = sampled_binding(0, 0, target_name, 0, 1);
    binding.access = SceneRenderingDeviceImageAccess::InputAttachment;
    let graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![pass_node(
            0,
            SceneRenderPassKind::EffectMaterial,
            SceneStringId::NONE,
            0,
            1,
        )],
        target_allocations: vec![allocation(target_name, 0)],
        sampled_bindings: vec![binding],
        mesh_draws: vec![draw()],
        ..empty_graph_plan()
    };

    let error = scene_sampled_image_binding_plan(&graph, &[0], &[])
        .expect_err("input attachments must not be sampled-image lowered");
    assert!(error.contains("absent from input-attachment shader contracts"));
}

#[test]
fn input_attachment_binding_plan_keeps_target_source_out_of_sampled_lane() {
    let target_name = SceneStringId(9);
    let mut binding = sampled_binding(0, 0, target_name, 0, 1);
    binding.access = SceneRenderingDeviceImageAccess::InputAttachment;
    let mut graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![pass_node(
            0,
            SceneRenderPassKind::EffectMaterial,
            target_name,
            0,
            1,
        )],
        target_allocations: vec![allocation(target_name, 4)],
        sampled_bindings: vec![binding],
        mesh_draws: vec![draw()],
        ..empty_graph_plan()
    };
    graph.mesh_draws[0].shader_key = SceneStringId(0);
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["effects/opacity__SLOTS_1".to_owned(), "pipeline".to_owned()],
        shader_contracts: vec![crate::engine::scene::SceneShaderContractRecord {
            shader_key: SceneStringId(0),
            pipeline_key: SceneStringId(1),
            texture_slot_mask: 0,
            input_attachment_slot_mask: 1,
            constant_start: 0,
            constant_count: 0,
            resource_heap_count: 1,
            sampler_heap_count: 0,
        }],
        ..SceneBinaryDocument::default()
    })
    .expect("input storage");
    let sampled = scene_sampled_image_binding_plan(&graph, &[], &[0]).expect("sampled lane");
    let input = super::super::input_attachment_binding::scene_input_attachment_binding_cycle(
        &storage,
        &graph,
        &[0],
        std::slice::from_ref(&sampled),
    )
    .expect("input lane");

    assert_eq!(sampled.sampled_slot_count, 0);
    assert_eq!(input[0].input_attachment_slot_count, 1);
    assert_eq!(
        input[0].source(0, 0),
        Some(
            super::super::input_attachment_binding::SceneInputAttachmentSource::EffectTarget {
                physical_slot: 4,
                batch_atlas_tile: 0,
            }
        )
    );
}

#[test]
fn sampled_binding_plan_follows_lowered_ping_pong_previous_targets() {
    let graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![
            pass_node(
                0,
                SceneRenderPassKind::BaseMaterial,
                SceneStringId::NONE,
                0,
                1,
            ),
            pass_node(
                1,
                SceneRenderPassKind::EffectMaterial,
                SceneStringId::NONE,
                1,
                1,
            ),
            pass_node(
                2,
                SceneRenderPassKind::EffectMaterial,
                SceneStringId::NONE,
                2,
                1,
            ),
            pass_node(
                3,
                SceneRenderPassKind::EffectMaterial,
                SceneStringId::NONE,
                3,
                1,
            ),
        ],
        target_allocations: vec![
            SceneRenderingDeviceTargetAllocation {
                graph_index: 0,
                target: SceneRenderTargetKind::ImageLocalMain,
                target_name: SceneStringId::NONE,
                first_write_pass_id: 0,
                last_use_pass_id: 3,
                physical_slot: 0,
                width: 64,
                height: 64,
                extent_domain: SceneTargetExtentDomain::OwnerAuthored,
            },
            SceneRenderingDeviceTargetAllocation {
                graph_index: 0,
                target: SceneRenderTargetKind::ImageLocalSub,
                target_name: SceneStringId::NONE,
                first_write_pass_id: 1,
                last_use_pass_id: 2,
                physical_slot: 1,
                width: 64,
                height: 64,
                extent_domain: SceneTargetExtentDomain::OwnerAuthored,
            },
        ],
        sampled_bindings: vec![
            previous_target_binding(1, 1, SceneRenderTargetKind::ImageLocalMain),
            previous_target_binding(2, 2, SceneRenderTargetKind::ImageLocalSub),
            previous_target_binding(3, 3, SceneRenderTargetKind::ImageLocalMain),
        ],
        mesh_draws: vec![draw(), draw(), draw(), draw()],
        ..empty_graph_plan()
    };

    let plan = scene_sampled_image_binding_plan(&graph, &[0], &[]).expect("ping-pong plan");

    assert_eq!(
        plan.source(1, 0),
        Some(SceneSampledImageSource::EffectTarget {
            physical_slot: 0,
            batch_atlas_tile: 0,
        })
    );
    assert_eq!(
        plan.source(2, 0),
        Some(SceneSampledImageSource::EffectTarget {
            physical_slot: 1,
            batch_atlas_tile: 0,
        })
    );
    assert_eq!(
        plan.source(3, 0),
        Some(SceneSampledImageSource::EffectTarget {
            physical_slot: 0,
            batch_atlas_tile: 0,
        })
    );
}

#[test]
fn direct_scene_snapshot_requires_consumption_before_scene_color_rendering() {
    let snapshot_name = SceneStringId(7);
    let mut copy = pass_node(0, SceneRenderPassKind::CopyTarget, snapshot_name, 0, 0);
    copy.target = SceneRenderTargetKind::FirstClassEffectTarget;
    let mut consumer = pass_node(
        1,
        SceneRenderPassKind::EffectMaterial,
        SceneStringId::NONE,
        0,
        1,
    );
    consumer.target = SceneRenderTargetKind::ImageLocalMain;
    let copy_source = SceneRenderingDeviceSampledBinding {
        pass_node_index: 0,
        graph_index: 0,
        mesh_draw_start: 0,
        mesh_draw_count: 0,
        kind: SceneRenderBindingKind::GraphTarget,
        slot: 0,
        target: SceneRenderTargetKind::SceneColor,
        target_name: SceneStringId::NONE,
        access: crate::engine::scene::SceneRenderingDeviceImageAccess::SampledImage,
    };
    let snapshot_consumer = SceneRenderingDeviceSampledBinding {
        pass_node_index: 1,
        graph_index: 0,
        mesh_draw_start: 0,
        mesh_draw_count: 1,
        kind: SceneRenderBindingKind::EffectTarget,
        slot: 2,
        target: SceneRenderTargetKind::FirstClassEffectTarget,
        target_name: snapshot_name,
        access: crate::engine::scene::SceneRenderingDeviceImageAccess::SampledImage,
    };
    let mut graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![copy, consumer],
        sampled_bindings: vec![copy_source, snapshot_consumer],
        mesh_draws: vec![draw()],
        ..empty_graph_plan()
    };

    assert!(target_is_direct_scene_color_snapshot(
        &graph,
        0,
        SceneRenderTargetKind::FirstClassEffectTarget,
        snapshot_name,
    ));

    graph.pass_nodes[1].target = SceneRenderTargetKind::SceneColor;
    assert!(!target_is_direct_scene_color_snapshot(
        &graph,
        0,
        SceneRenderTargetKind::FirstClassEffectTarget,
        snapshot_name,
    ));

    graph.sampled_bindings.pop();
    assert!(!target_is_direct_scene_color_snapshot(
        &graph,
        0,
        SceneRenderTargetKind::FirstClassEffectTarget,
        snapshot_name,
    ));
}

#[test]
fn sampled_binding_plan_expands_external_video_into_y_and_uv_planes() {
    let graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![pass_node(
            0,
            SceneRenderPassKind::VideoSample,
            SceneStringId::NONE,
            0,
            1,
        )],
        sampled_bindings: vec![SceneRenderingDeviceSampledBinding {
            pass_node_index: 0,
            graph_index: 0,
            mesh_draw_start: 0,
            mesh_draw_count: 1,
            kind: SceneRenderBindingKind::VideoFrame,
            slot: 3,
            target: SceneRenderTargetKind::VideoExternalImage,
            target_name: SceneStringId::NONE,
            access: crate::engine::scene::SceneRenderingDeviceImageAccess::SampledImage,
        }],
        mesh_draws: vec![draw()],
        ..empty_graph_plan()
    };

    let plan = scene_sampled_image_binding_plan(&graph, &[0, 1], &[]).expect("video frame plan");

    assert_eq!(plan.fallback_descriptor_count, 0);
    assert_eq!(plan.video_frame_descriptor_count, 2);
    assert_eq!(
        plan.source(0, 0),
        Some(SceneSampledImageSource::VideoFramePlane {
            media_instance: 3,
            plane: SceneVideoPlane::Y,
        })
    );
    assert_eq!(
        plan.source(0, 1),
        Some(SceneSampledImageSource::VideoFramePlane {
            media_instance: 3,
            plane: SceneVideoPlane::Uv,
        })
    );
}

#[test]
fn sampled_binding_plan_rejects_video_without_both_plane_slots() {
    let graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![pass_node(
            0,
            SceneRenderPassKind::VideoSample,
            SceneStringId::NONE,
            0,
            1,
        )],
        sampled_bindings: vec![SceneRenderingDeviceSampledBinding {
            pass_node_index: 0,
            graph_index: 0,
            mesh_draw_start: 0,
            mesh_draw_count: 1,
            kind: SceneRenderBindingKind::VideoFrame,
            slot: 7,
            target: SceneRenderTargetKind::VideoExternalImage,
            target_name: SceneStringId::NONE,
            access: crate::engine::scene::SceneRenderingDeviceImageAccess::SampledImage,
        }],
        mesh_draws: vec![draw()],
        ..empty_graph_plan()
    };

    assert!(scene_sampled_image_binding_plan(&graph, &[0], &[]).is_err());
}

fn pass_node(
    pass_record_index: u32,
    role: SceneRenderPassKind,
    target_name: SceneStringId,
    mesh_draw_start: u32,
    mesh_draw_count: u32,
) -> SceneRenderingDevicePassNode {
    SceneRenderingDevicePassNode {
        graph_index: 0,
        graph_activation_policy: crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
        pass_record_index,
        pass_id: pass_record_index,
        role,
        target: SceneRenderTargetKind::NamedFbo,
        target_name,
        binding_start: 0,
        binding_count: 0,
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        mesh_draw_start,
        mesh_draw_count,
    }
}

fn allocation(
    target_name: SceneStringId,
    physical_slot: u32,
) -> SceneRenderingDeviceTargetAllocation {
    SceneRenderingDeviceTargetAllocation {
        graph_index: 0,
        target: SceneRenderTargetKind::NamedFbo,
        target_name,
        first_write_pass_id: 0,
        last_use_pass_id: 2,
        physical_slot,
        width: 0,
        height: 0,
        extent_domain: SceneTargetExtentDomain::PhysicalSurface,
    }
}

fn sampled_binding(
    pass_node_index: u32,
    slot: u32,
    target_name: SceneStringId,
    mesh_draw_start: u32,
    mesh_draw_count: u32,
) -> SceneRenderingDeviceSampledBinding {
    SceneRenderingDeviceSampledBinding {
        pass_node_index,
        graph_index: 0,
        mesh_draw_start,
        mesh_draw_count,
        kind: SceneRenderBindingKind::NamedFboBind,
        slot,
        target: SceneRenderTargetKind::NamedFbo,
        target_name,
        access: SceneRenderingDeviceImageAccess::SampledImage,
    }
}

fn previous_target_binding(
    pass_node_index: u32,
    draw_index: u32,
    target: SceneRenderTargetKind,
) -> SceneRenderingDeviceSampledBinding {
    SceneRenderingDeviceSampledBinding {
        pass_node_index,
        graph_index: 0,
        mesh_draw_start: draw_index,
        mesh_draw_count: 1,
        kind: crate::engine::scene::SceneRenderBindingKind::PreviousGraphTarget,
        slot: 0,
        target,
        target_name: SceneStringId::NONE,
        access: SceneRenderingDeviceImageAccess::SampledImage,
    }
}

fn draw() -> crate::engine::scene::SceneRenderingDeviceMeshDraw {
    crate::engine::scene::SceneRenderingDeviceMeshDraw {
        primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
        projection_domain: crate::engine::scene::SceneRenderingDeviceProjectionDomain::Scene,
        shader_key: crate::engine::scene::SceneStringId::NONE,
        mesh_index: crate::engine::scene::INVALID_OBJECT_ID,
        resolved_object_index: crate::engine::scene::INVALID_OBJECT_ID,
        render_world_matrix: [[0.0; 4]; 4],
        clip_transform: [[0.0; 4]; 4],
        effect_model_view_projection_matrix: [[0.0; 4]; 4],
        authored_source_extent: [0.0; 2],
        uv_inset_texels: 0.0,
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
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        resolved_effect_visibility_mask: 0,
        object: crate::engine::scene::SceneObjectHandle(crate::engine::scene::INVALID_OBJECT_ID),
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

fn empty_graph_plan() -> SceneRenderingDeviceGraphPlan {
    SceneRenderingDeviceGraphPlan {
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

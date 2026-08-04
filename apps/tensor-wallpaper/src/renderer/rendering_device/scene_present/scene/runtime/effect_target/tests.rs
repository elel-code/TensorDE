use super::super::input_attachment_binding::{
    SceneInputAttachmentBindingPlan, SceneInputAttachmentSource,
};
use super::super::sampled_binding::SceneSampledImageBindingPlan;
use super::scene_color_copy::{graph_copies_scene_color, graph_uses_direct_scene_color_snapshot};
use super::*;
use crate::engine::scene::{
    INVALID_MATERIAL_ID, INVALID_OBJECT_ID, SceneBinaryDocument, SceneColorWriteMask,
    SceneCompositeBlend, SceneCullMode, SceneDepthTest, SceneMaterialHandle, SceneObjectHandle,
    ScenePipelineBlend, SceneRenderEffectVisibilityPolicy, SceneRenderPassRecord,
    SceneRenderingDeviceEffectBatch, SceneRenderingDeviceEffectBatchFamily,
    SceneRenderingDeviceGraphPlan, SceneRenderingDeviceTargetAllocation, SceneTargetExtentDomain,
};

#[test]
fn effect_target_image_plan_scales_and_aliases_physical_slots() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["rt_a".to_owned(), "rgba8".to_owned(), "fbo_b".to_owned()],
        image_targets: vec![
            SceneImageTargetRecord {
                name: SceneStringId(0),
                role: SceneRenderTargetKind::FirstClassEffectTarget,
                format: SceneStringId(1),
                extent_domain: SceneTargetExtentDomain::PhysicalSurface,
                width_divisor_milli: 2_000,
                height_divisor_milli: 4_000,
            },
            SceneImageTargetRecord {
                name: SceneStringId(2),
                role: SceneRenderTargetKind::NamedFbo,
                format: SceneStringId(1),
                extent_domain: SceneTargetExtentDomain::PhysicalSurface,
                width_divisor_milli: 2_000,
                height_divisor_milli: 4_000,
            },
        ],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let graph = graph_with_allocations(vec![
        allocation(
            2,
            SceneRenderTargetKind::FirstClassEffectTarget,
            SceneStringId(0),
        ),
        allocation(2, SceneRenderTargetKind::NamedFbo, SceneStringId(2)),
    ]);

    let plans = scene_effect_target_image_plan(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        Extent2D::new(1920, 1080),
    )
    .expect("effect target plan");

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].physical_slot, 2);
    assert_eq!(plans[0].format, TextureFormat::Rgba8Unorm);
    assert_eq!(plans[0].extent, Extent2D::new(960, 270));
    assert!(plans[0].persistent_across_frames);
    assert_eq!(plans[0].aliased_logical_target_count, 2);
}

#[test]
fn effect_target_image_plan_honors_non_zero_plan_allocation_extent() {
    // Authored multipass local targets carry plan width/height (e.g. WE 2318×1794).
    // Runtime must not expand them via projected mesh AABB (historical 2542×1968 bug).
    let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
    let mut alloc = allocation(
        0,
        SceneRenderTargetKind::ImageLocalMain,
        SceneStringId::NONE,
    );
    alloc.width = 2318;
    alloc.height = 1794;
    alloc.extent_domain = SceneTargetExtentDomain::OwnerAuthored;
    let graph = graph_with_allocations(vec![alloc]);

    let plans = scene_effect_target_image_plan(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        Extent2D::new(3856, 2199),
    )
    .expect("effect target plan");

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].extent, Extent2D::new(2318, 1794));
}

#[test]
fn effect_target_image_plan_keeps_4k_owner_quarter_target_on_lower_physical_surface() {
    let target_name = SceneStringId(0);
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec![
            "_rt_QuarterCompoBuffer1".to_owned(),
            "rgba_backbuffer".to_owned(),
        ],
        image_targets: vec![SceneImageTargetRecord {
            name: target_name,
            role: SceneRenderTargetKind::FirstClassEffectTarget,
            format: SceneStringId(1),
            extent_domain: SceneTargetExtentDomain::OwnerAuthored,
            width_divisor_milli: 4_000,
            height_divisor_milli: 4_000,
        }],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let mut owner_target = allocation(
        0,
        SceneRenderTargetKind::FirstClassEffectTarget,
        target_name,
    );
    // This is resolved cold from the 3840×2160 graph owner divided by four.
    owner_target.extent_domain = SceneTargetExtentDomain::OwnerAuthored;
    owner_target.width = 960;
    owner_target.height = 540;
    let graph = graph_with_allocations(vec![owner_target]);

    let plans = scene_effect_target_image_plan(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        Extent2D::new(2560, 1600),
    )
    .expect("owner target plan");

    assert_eq!(plans[0].extent, Extent2D::new(960, 540));
}

#[test]
fn effect_target_image_plan_keeps_scene_snapshot_in_the_physical_surface_domain() {
    let target_name = SceneStringId(0);
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["_rt_FullFrameBuffer".to_owned()],
        image_targets: vec![SceneImageTargetRecord {
            name: target_name,
            role: SceneRenderTargetKind::FirstClassEffectTarget,
            format: SceneStringId::NONE,
            extent_domain: SceneTargetExtentDomain::PhysicalSurface,
            width_divisor_milli: 1_000,
            height_divisor_milli: 1_000,
        }],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let graph = graph_with_allocations(vec![allocation(
        0,
        SceneRenderTargetKind::FirstClassEffectTarget,
        target_name,
    )]);

    let plans = scene_effect_target_image_plan(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        Extent2D::new(2560, 1600),
    )
    .expect("physical snapshot target plan");

    assert_eq!(plans[0].extent, Extent2D::new(2560, 1600));
}

#[test]
fn effect_target_image_plan_rejects_unresolved_owner_target_extent() {
    let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
    let mut owner_target = allocation(
        0,
        SceneRenderTargetKind::FirstClassEffectTarget,
        SceneStringId(9),
    );
    owner_target.extent_domain = SceneTargetExtentDomain::OwnerAuthored;
    let graph = graph_with_allocations(vec![owner_target]);

    let error = scene_effect_target_image_plan(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        Extent2D::new(2560, 1600),
    )
    .expect_err("owner targets must not fall back to the physical surface");

    assert!(error.contains("owner-authored scene target"));
    assert!(error.contains("has no resolved extent"));
}

#[test]
fn effect_batch_atlas_applies_its_declared_field_resolution_once() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["waterwaves_uv".to_owned(), "rg16f".to_owned()],
        image_targets: vec![SceneImageTargetRecord {
            name: SceneStringId(0),
            role: SceneRenderTargetKind::Temporary,
            format: SceneStringId(1),
            extent_domain: SceneTargetExtentDomain::PhysicalSurface,
            width_divisor_milli: 4_000,
            height_divisor_milli: 4_000,
        }],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let mut graph = graph_with_allocations(vec![allocation(
        0,
        SceneRenderTargetKind::Temporary,
        SceneStringId(0),
    )]);
    graph.effect_batches.push(SceneRenderingDeviceEffectBatch {
        family: SceneRenderingDeviceEffectBatchFamily::WaterWavesUvField,
        physical_slot: 0,
        instance_start: 0,
        instance_count: 22,
        layer_count: 11,
        atlas_columns: 4,
        atlas_rows: 3,
        field_extent_divisor: 4,
    });

    let plans = scene_effect_target_image_plan(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        Extent2D::new(2560, 1600),
    )
    .expect("effect atlas plan");

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].format, TextureFormat::Rg16Float);
    assert_eq!(plans[0].extent, Extent2D::new(640, 300));
    assert_eq!(plans[0].batch_field_count, 11);
}

#[test]
fn effect_target_image_plan_uses_backbuffer_format_for_missing_target_records() {
    let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
    let graph = graph_with_allocations(vec![allocation(
        0,
        SceneRenderTargetKind::NamedFbo,
        SceneStringId(5),
    )]);

    let plans = scene_effect_target_image_plan(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        Extent2D::new(1280, 720),
    )
    .expect("effect target plan");

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].format, TextureFormat::Bgra8Unorm);
    assert_eq!(plans[0].extent, Extent2D::new(1280, 720));
}

#[test]
fn input_attachment_usage_covers_every_physical_slot_in_the_reference_cycle() {
    let mut plans = vec![
        SceneEffectTargetImagePlan {
            physical_slot: 0,
            graph_index: 0,
            target: SceneRenderTargetKind::ImageLocalMain,
            target_name: SceneStringId::NONE,
            format: TextureFormat::Rgba8Unorm,
            extent: Extent2D::new(64, 64),
            batch_field_count: 1,
            batch_atlas_columns: 1,
            batch_atlas_rows: 1,
            persistent_across_frames: false,
            aliased_logical_target_count: 1,
            input_attachment_required: false,
        },
        SceneEffectTargetImagePlan {
            physical_slot: 1,
            graph_index: 0,
            target: SceneRenderTargetKind::ImageLocalSub,
            target_name: SceneStringId::NONE,
            format: TextureFormat::Rgba8Unorm,
            extent: Extent2D::new(64, 64),
            batch_field_count: 1,
            batch_atlas_columns: 1,
            batch_atlas_rows: 1,
            persistent_across_frames: false,
            aliased_logical_target_count: 1,
            input_attachment_required: false,
        },
    ];
    let cycle = vec![
        SceneInputAttachmentBindingPlan {
            input_attachment_slot_count: 1,
            sources: vec![Some(SceneInputAttachmentSource::EffectTarget {
                physical_slot: 0,
                batch_atlas_tile: 0,
            })],
            effect_target_descriptor_count: 1,
        },
        SceneInputAttachmentBindingPlan {
            input_attachment_slot_count: 1,
            sources: vec![Some(SceneInputAttachmentSource::EffectTarget {
                physical_slot: 1,
                batch_atlas_tile: 0,
            })],
            effect_target_descriptor_count: 1,
        },
    ];

    apply_scene_effect_target_input_attachment_usage(&mut plans, &cycle).expect("input usage plan");

    assert!(plans.iter().all(|plan| plan.input_attachment_required));
}

#[test]
fn local_read_candidate_usage_covers_destination_and_reference_permutations() {
    let target_plan = |physical_slot, target_name| SceneEffectTargetImagePlan {
        physical_slot,
        graph_index: 0,
        target: SceneRenderTargetKind::NamedFbo,
        target_name,
        format: TextureFormat::Rgba8Unorm,
        extent: Extent2D::new(64, 64),
        batch_field_count: 1,
        batch_atlas_columns: 1,
        batch_atlas_rows: 1,
        persistent_across_frames: false,
        aliased_logical_target_count: 1,
        input_attachment_required: false,
    };
    let mut producer = pass_node(0, SceneRenderPassKind::BaseMaterial, SceneStringId(0), 0);
    producer.mesh_draw_count = 1;
    let mut consumer = pass_node(1, SceneRenderPassKind::EffectMaterial, SceneStringId(1), 0);
    consumer.mesh_draw_start = 1;
    consumer.mesh_draw_count = 1;
    let graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![producer, consumer],
        target_allocations: vec![
            allocation(0, SceneRenderTargetKind::NamedFbo, SceneStringId(0)),
            allocation(1, SceneRenderTargetKind::NamedFbo, SceneStringId(1)),
            allocation(2, SceneRenderTargetKind::NamedFbo, SceneStringId(2)),
        ],
        sampled_bindings: vec![crate::engine::scene::SceneRenderingDeviceSampledBinding {
            pass_node_index: 1,
            graph_index: 0,
            mesh_draw_start: 1,
            mesh_draw_count: 1,
            kind: crate::engine::scene::SceneRenderBindingKind::PreviousGraphTarget,
            slot: 0,
            target: SceneRenderTargetKind::NamedFbo,
            target_name: SceneStringId(0),
            access: SceneRenderingDeviceImageAccess::InputAttachment,
        }],
        graph_physical_target_count: 3,
        ..empty_graph_plan()
    };
    let sampled_phase = |slots| SceneSampledImageBindingPlan {
        sampled_slot_count: 0,
        sources: Vec::new(),
        initial_reference_physical_slots: slots,
        fallback_descriptor_count: 0,
        scene_texture_descriptor_count: 0,
        scene_color_snapshot_descriptor_count: 0,
        effect_target_descriptor_count: 0,
        video_frame_descriptor_count: 0,
    };
    let sampled_cycle = vec![sampled_phase(vec![0, 1, 2]), sampled_phase(vec![2, 0, 1])];
    let mut plans = vec![
        target_plan(0, SceneStringId(0)),
        target_plan(1, SceneStringId(1)),
        target_plan(2, SceneStringId(2)),
    ];

    apply_scene_effect_target_local_read_candidate_usage(&mut plans, &graph, &sampled_cycle)
        .expect("candidate local-read usage");

    assert!(plans.iter().all(|plan| plan.input_attachment_required));
}

#[test]
fn effect_target_image_plan_rejects_incompatible_manual_aliases() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec![
            "fbo_a".to_owned(),
            "rgba8".to_owned(),
            "fbo_b".to_owned(),
            "rgba16f".to_owned(),
        ],
        image_targets: vec![
            SceneImageTargetRecord {
                name: SceneStringId(0),
                role: SceneRenderTargetKind::NamedFbo,
                format: SceneStringId(1),
                extent_domain: SceneTargetExtentDomain::PhysicalSurface,
                width_divisor_milli: 1_000,
                height_divisor_milli: 1_000,
            },
            SceneImageTargetRecord {
                name: SceneStringId(2),
                role: SceneRenderTargetKind::NamedFbo,
                format: SceneStringId(3),
                extent_domain: SceneTargetExtentDomain::PhysicalSurface,
                width_divisor_milli: 2_000,
                height_divisor_milli: 2_000,
            },
        ],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let graph = graph_with_allocations(vec![
        allocation(0, SceneRenderTargetKind::NamedFbo, SceneStringId(0)),
        allocation(0, SceneRenderTargetKind::NamedFbo, SceneStringId(2)),
    ]);

    let error = scene_effect_target_image_plan(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        Extent2D::new(1920, 1080),
    )
    .expect_err("incompatible alias must fail");

    assert!(error.contains("aliases incompatible images"));
    assert_eq!(divided_axis(1920, 4_000), 480);
    assert_eq!(divided_axis(1080, 4_000), 270);
    // Half-scale integer floor matches WE `full / scale_divisor` (not div_ceil).
    assert_eq!(divided_axis(3856, 2_000), 1928);
    assert_eq!(divided_axis(2199, 2_000), 1099);
    assert_eq!(divided_axis(3840, 2_000), 1920);
    assert_eq!(divided_axis(2160, 2_000), 1080);
    assert_eq!(divided_axis(1, 2_000), 2);
    assert_eq!(
        target_format("r16f", TextureFormat::Bgra8Unorm).expect("r16f"),
        TextureFormat::R16Float
    );
    assert_eq!(
        target_format("rg1616f", TextureFormat::Bgra8Unorm).expect("rg1616f"),
        TextureFormat::Rg16Float
    );
    assert_eq!(
        target_format("rgba8888", TextureFormat::Bgra8Unorm).expect("rgba8888"),
        TextureFormat::Rgba8Unorm
    );
}

#[test]
fn effect_target_commands_track_copy_swap_and_dynamic_passes() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["fbo_a".to_owned(), "fbo_b".to_owned()],
        render_bindings: vec![
            named_fbo_binding(SceneStringId(0)),
            named_fbo_binding(SceneStringId(0)),
        ],
        render_passes: vec![render_pass_record(true)],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let mut effect_pass = pass_node(3, SceneRenderPassKind::EffectMaterial, SceneStringId(1), 2);
    effect_pass.mesh_draw_start = 4;
    effect_pass.mesh_draw_count = 2;
    let graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![
            pass_node(1, SceneRenderPassKind::CopyTarget, SceneStringId(1), 0),
            pass_node(
                2,
                SceneRenderPassKind::SwapTargetReferences,
                SceneStringId(1),
                1,
            ),
            effect_pass,
        ],
        target_allocations: vec![
            allocation(0, SceneRenderTargetKind::NamedFbo, SceneStringId(0)),
            allocation(1, SceneRenderTargetKind::NamedFbo, SceneStringId(1)),
        ],
        graph_physical_target_count: 2,
        ..empty_graph_plan()
    };

    let commands = scene_effect_target_commands(&storage, &graph);
    let plan = scene_effect_target_command_plan(&commands, &graph);
    let mut references = logical_target_references(&graph.target_allocations);

    assert_eq!(commands.len(), 3);
    assert_eq!(plan.copy_command_count, 1);
    assert_eq!(plan.swap_reference_command_count, 1);
    assert_eq!(plan.dynamic_rendering_pass_count, 1);
    assert_eq!(plan.mesh_draw_count, 2);
    assert_eq!(plan.discard_load_count, 0);
    assert_eq!(commands[2].mesh_draw_start, 4);
    assert_eq!(commands[2].mesh_draw_count, 2);
    assert!(commands[2].clear_before_draw);
    let timing_commands = scene_effect_target_timing_commands(&commands, &[0]);
    assert_eq!(timing_commands.len(), 2);
    assert_eq!(timing_commands[0].source_position, 0);
    assert_eq!(timing_commands[0].graph_command_index, 0);
    assert_eq!(timing_commands[0].command_kind, "copy");
    assert_eq!(timing_commands[1].source_position, 2);
    assert_eq!(timing_commands[1].graph_command_index, 1);
    assert_eq!(timing_commands[1].command_kind, "dynamic-render");

    swap_logical_references(commands[1], &mut references).expect("swap refs");
    assert_eq!(
        references
            .iter()
            .find(|reference| reference.key.name == SceneStringId(0))
            .expect("fbo_a")
            .physical_slot,
        1
    );
    assert_eq!(
        references
            .iter()
            .find(|reference| reference.key.name == SceneStringId(1))
            .expect("fbo_b")
            .physical_slot,
        0
    );
}

#[test]
fn scene_color_consumer_keeps_copied_snapshot_image_and_command() {
    let snapshot_name = SceneStringId(0);
    let mut copy_record = render_pass_record(false);
    copy_record.role = SceneRenderPassKind::CopyTarget;
    copy_record.target = SceneRenderTargetKind::FirstClassEffectTarget;
    copy_record.target_name = snapshot_name;
    copy_record.binding_count = 1;
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["scene_snapshot".to_owned(), "rgba8".to_owned()],
        image_targets: vec![SceneImageTargetRecord {
            name: snapshot_name,
            role: SceneRenderTargetKind::FirstClassEffectTarget,
            format: SceneStringId(1),
            extent_domain: SceneTargetExtentDomain::PhysicalSurface,
            width_divisor_milli: 1_000,
            height_divisor_milli: 1_000,
        }],
        render_bindings: vec![crate::engine::scene::SceneRenderBindingRecord {
            kind: crate::engine::scene::SceneRenderBindingKind::GraphTarget,
            slot: 0,
            target: SceneRenderTargetKind::SceneColor,
            name: SceneStringId::NONE,
        }],
        render_passes: vec![copy_record],
        ..SceneBinaryDocument::default()
    })
    .expect("storage");
    let mut copy = pass_node(0, SceneRenderPassKind::CopyTarget, snapshot_name, 0);
    copy.target = SceneRenderTargetKind::FirstClassEffectTarget;
    let mut consumer = pass_node(
        1,
        SceneRenderPassKind::EffectMaterial,
        SceneStringId::NONE,
        1,
    );
    consumer.target = SceneRenderTargetKind::SceneColor;
    let graph = SceneRenderingDeviceGraphPlan {
        pass_nodes: vec![copy, consumer],
        target_allocations: vec![allocation(
            4,
            SceneRenderTargetKind::FirstClassEffectTarget,
            snapshot_name,
        )],
        sampled_bindings: vec![
            crate::engine::scene::SceneRenderingDeviceSampledBinding {
                pass_node_index: 0,
                graph_index: 0,
                mesh_draw_start: 0,
                mesh_draw_count: 0,
                kind: crate::engine::scene::SceneRenderBindingKind::GraphTarget,
                slot: 0,
                target: SceneRenderTargetKind::SceneColor,
                target_name: SceneStringId::NONE,
                access: crate::engine::scene::SceneRenderingDeviceImageAccess::SampledImage,
            },
            crate::engine::scene::SceneRenderingDeviceSampledBinding {
                pass_node_index: 1,
                graph_index: 0,
                mesh_draw_start: 0,
                mesh_draw_count: 1,
                kind: crate::engine::scene::SceneRenderBindingKind::EffectTarget,
                slot: 2,
                target: SceneRenderTargetKind::FirstClassEffectTarget,
                target_name: snapshot_name,
                access: crate::engine::scene::SceneRenderingDeviceImageAccess::SampledImage,
            },
        ],
        graph_physical_target_count: 1,
        ..empty_graph_plan()
    };

    let images = scene_effect_target_image_plan(
        &storage,
        &graph,
        TextureFormat::Bgra8Unorm,
        Extent2D::new(3840, 2160),
    )
    .expect("effect target plan");
    let commands = scene_effect_target_commands(&storage, &graph);
    let command_plan = scene_effect_target_command_plan(&commands, &graph);

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].physical_slot, 4);
    assert_eq!(images[0].extent, Extent2D::new(3840, 2160));
    assert_eq!(commands.len(), 1);
    assert!(!commands[0].direct_scene_color_snapshot);
    assert_eq!(command_plan.copy_command_count, 1);
    assert!(graph_copies_scene_color(&commands, 0));
    assert!(!graph_uses_direct_scene_color_snapshot(&commands, 0));
}

fn graph_with_allocations(
    target_allocations: Vec<SceneRenderingDeviceTargetAllocation>,
) -> SceneRenderingDeviceGraphPlan {
    SceneRenderingDeviceGraphPlan {
        target_allocations,
        graph_physical_target_count: 1,
        ..empty_graph_plan()
    }
}

fn pass_node(
    pass_id: u32,
    role: SceneRenderPassKind,
    target_name: SceneStringId,
    binding_start: u32,
) -> SceneRenderingDevicePassNode {
    SceneRenderingDevicePassNode {
        graph_index: 0,
        graph_activation_policy: crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
        pass_record_index: 0,
        pass_id,
        role,
        target: SceneRenderTargetKind::NamedFbo,
        target_name,
        binding_start,
        binding_count: u32::from(binding_start < 2),
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        mesh_draw_start: 0,
        mesh_draw_count: 0,
    }
}

fn render_pass_record(clear_target: bool) -> SceneRenderPassRecord {
    SceneRenderPassRecord {
        id: 0,
        role: SceneRenderPassKind::EffectMaterial,
        draw_primitive: crate::engine::scene::SceneRenderPassDrawPrimitive::FullscreenTriangle,
        object: SceneObjectHandle(INVALID_OBJECT_ID),
        material: SceneMaterialHandle(INVALID_MATERIAL_ID),
        pass_index: 0,
        shader_key: SceneStringId::NONE,
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
        clear_target,
    }
}

fn allocation(
    physical_slot: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> SceneRenderingDeviceTargetAllocation {
    SceneRenderingDeviceTargetAllocation {
        graph_index: 0,
        target,
        target_name,
        first_write_pass_id: 1,
        last_use_pass_id: 2,
        physical_slot,
        width: 0,
        height: 0,
        extent_domain: SceneTargetExtentDomain::PhysicalSurface,
    }
}

fn named_fbo_binding(name: SceneStringId) -> crate::engine::scene::SceneRenderBindingRecord {
    crate::engine::scene::SceneRenderBindingRecord {
        kind: crate::engine::scene::SceneRenderBindingKind::NamedFboBind,
        slot: 0,
        target: SceneRenderTargetKind::NamedFbo,
        name,
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

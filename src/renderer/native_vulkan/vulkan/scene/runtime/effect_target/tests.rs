use super::*;
use crate::engine::scene::{
    INVALID_MATERIAL_ID, INVALID_OBJECT_ID, SceneBinaryDocument, SceneColorWriteMask,
    SceneCompositeBlend, SceneCullMode, SceneDepthTest, SceneMaterialHandle, SceneObjectHandle,
    ScenePipelineBlend, SceneRenderEffectVisibilityPolicy, SceneRenderPassRecord,
    SceneRenderingDeviceEffectBatch, SceneRenderingDeviceEffectBatchFamily,
    SceneRenderingDeviceGraphPlan, SceneRenderingDeviceTargetAllocation,
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
                width_divisor_milli: 2_000,
                height_divisor_milli: 4_000,
            },
            SceneImageTargetRecord {
                name: SceneStringId(2),
                role: SceneRenderTargetKind::NamedFbo,
                format: SceneStringId(1),
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
        vk::Format::B8G8R8A8_UNORM,
        vk::Extent2D {
            width: 1920,
            height: 1080,
        },
    )
    .expect("effect target plan");

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].physical_slot, 2);
    assert_eq!(plans[0].format, vk::Format::R8G8B8A8_UNORM);
    assert_eq!(plans[0].width, 960);
    assert_eq!(plans[0].height, 270);
    assert!(plans[0].persistent_across_frames);
    assert_eq!(plans[0].aliased_logical_target_count, 2);
}

#[test]
fn effect_batch_atlas_applies_its_declared_field_resolution_once() {
    let storage = SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["waterwaves_uv".to_owned(), "rg16f".to_owned()],
        image_targets: vec![SceneImageTargetRecord {
            name: SceneStringId(0),
            role: SceneRenderTargetKind::Temporary,
            format: SceneStringId(1),
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
        vk::Format::B8G8R8A8_UNORM,
        vk::Extent2D {
            width: 2560,
            height: 1600,
        },
    )
    .expect("effect atlas plan");

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].format, vk::Format::R16G16_SFLOAT);
    assert_eq!(plans[0].width, 640);
    assert_eq!(plans[0].height, 300);
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
        vk::Format::B8G8R8A8_UNORM,
        vk::Extent2D {
            width: 1280,
            height: 720,
        },
    )
    .expect("effect target plan");

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].format, vk::Format::B8G8R8A8_UNORM);
    assert_eq!(plans[0].width, 1280);
    assert_eq!(plans[0].height, 720);
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
                width_divisor_milli: 1_000,
                height_divisor_milli: 1_000,
            },
            SceneImageTargetRecord {
                name: SceneStringId(2),
                role: SceneRenderTargetKind::NamedFbo,
                format: SceneStringId(3),
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
        vk::Format::B8G8R8A8_UNORM,
        vk::Extent2D {
            width: 1920,
            height: 1080,
        },
    )
    .expect_err("incompatible alias must fail");

    assert!(error.contains("aliases incompatible images"));
    assert_eq!(divided_axis(1920, 4_000), 480);
    assert_eq!(divided_axis(1080, 4_000), 270);
    assert_eq!(
        target_format("r16f", vk::Format::B8G8R8A8_UNORM).expect("r16f"),
        vk::Format::R16_SFLOAT
    );
    assert_eq!(
        target_format("rg1616f", vk::Format::B8G8R8A8_UNORM).expect("rg1616f"),
        vk::Format::R16G16_SFLOAT
    );
    assert_eq!(
        target_format("rgba8888", vk::Format::B8G8R8A8_UNORM).expect("rgba8888"),
        vk::Format::R8G8B8A8_UNORM
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
fn repeated_effect_target_passes_load_after_the_initial_clear() {
    let mut initialized = Vec::new();

    assert_eq!(
        effect_target_load_op(&initialized, 4, false, false, false),
        vk::AttachmentLoadOp::CLEAR
    );
    mark_target_initialized(&mut initialized, 4);
    assert_eq!(
        effect_target_load_op(&initialized, 4, true, false, false),
        vk::AttachmentLoadOp::LOAD
    );
    assert_eq!(
        effect_target_load_op(&initialized, 4, true, true, false),
        vk::AttachmentLoadOp::CLEAR
    );
    assert_eq!(
        effect_target_load_op(&initialized, 4, true, false, true),
        vk::AttachmentLoadOp::DONT_CARE
    );
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
        graph_activation_policy:
            crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
        pass_record_index: 0,
        pass_id,
        role,
        target: SceneRenderTargetKind::NamedFbo,
        target_name,
        binding_start,
        binding_count: u32::from(binding_start < 2),
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy:
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        mesh_draw_start: 0,
        mesh_draw_count: 0,
    }
}

fn render_pass_record(clear_target: bool) -> SceneRenderPassRecord {
    SceneRenderPassRecord {
        id: 0,
        role: SceneRenderPassKind::EffectMaterial,
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

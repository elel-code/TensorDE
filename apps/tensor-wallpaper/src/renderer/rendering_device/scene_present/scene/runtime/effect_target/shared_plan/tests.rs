use super::*;

#[test]
fn phase_reference_slots_select_exact_physical_targets() {
    let target_a = target(0);
    let target_b = target(1);
    let commands = [
        dynamic_render(target_a, 4, 2),
        dynamic_render(target_b, 6, 1),
    ];
    let plans = shared_scene_effect_execution_plans(
        &commands,
        &[allocation(target_a, 0), allocation(target_b, 1)],
        &[image_plan(0, false), image_plan(1, false)],
        &[phase(&[0, 1]), phase(&[1, 0])],
        &[],
    )
    .expect("phase-aware plans");

    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].reference_phase, 0);
    assert_eq!(plans[1].reference_phase, 1);
    assert_eq!(dynamic_target_slots(&plans[0]), vec![0, 1]);
    assert_eq!(dynamic_target_slots(&plans[1]), vec![1, 0]);
}

#[test]
fn swap_changes_the_physical_source_used_by_following_copy() {
    let source = target(0);
    let destination = target(1);
    let commands = [
        swap(source, destination),
        copy_logical(source, destination),
        dynamic_render(source, 0, 1),
    ];
    let plans = shared_scene_effect_execution_plans(
        &commands,
        &[allocation(source, 3), allocation(destination, 7)],
        &[image_plan(3, false), image_plan(7, false)],
        &[phase(&[3, 7])],
        &[],
    )
    .expect("swapped plan");

    assert_eq!(
        plans[0].commands[0].kind,
        SharedSceneEffectCommandKind::SwapReferences {
            source_before: 3,
            destination_before: 7,
        }
    );
    assert_eq!(
        plans[0].commands[1].kind,
        SharedSceneEffectCommandKind::Copy {
            source: SharedSceneEffectCopySource::PhysicalSlot(7),
            destination_physical_slot: Some(3),
            direct_scene_color_snapshot: false,
            coverage: SceneColorCopyCoverage::FullTarget,
        }
    );
    assert_eq!(dynamic_target_slots(&plans[0]), vec![7]);
}

#[test]
fn direct_scene_color_snapshot_does_not_invent_an_effect_target() {
    let destination = target(0);
    let commands = [SceneEffectTargetCommand {
        kind: SceneEffectTargetCommandKind::Copy,
        pass_record_index: 9,
        target: destination,
        source: Some(SceneEffectTargetCommandSource::SceneColor),
        mesh_draw_start: 0,
        mesh_draw_count: 0,
        clear_before_draw: false,
        fully_overwrites_target: false,
        direct_scene_color_snapshot: true,
        scene_color_copy_coverage: SceneColorCopyCoverage::ConsumerDrawScissors {
            draw_start: 2,
            draw_count: 3,
        },
        batch_physical_slot: None,
        batch_atlas_tile: None,
    }];
    let plans = shared_scene_effect_execution_plans(
        &commands,
        &[allocation(destination, 5)],
        &[],
        &[phase(&[5])],
        &[],
    )
    .expect("direct SceneColor snapshot plan");

    assert_eq!(
        plans[0].commands[0].kind,
        SharedSceneEffectCommandKind::Copy {
            source: SharedSceneEffectCopySource::SceneColor,
            destination_physical_slot: None,
            direct_scene_color_snapshot: true,
            coverage: SceneColorCopyCoverage::ConsumerDrawScissors {
                draw_start: 2,
                draw_count: 3,
            },
        }
    );
}

#[test]
fn local_read_pair_must_be_adjacent_and_producer_first() {
    let target = target(0);
    let producer = compiled_dynamic_render(target, 0, SceneLocalReadScopePassRole::Producer);
    let consumer = compiled_dynamic_render(target, 1, SceneLocalReadScopePassRole::Consumer);
    validate_local_read_pairs(&[producer, consumer], 1).expect("adjacent local-read pair");

    let unrelated = SharedSceneEffectCommand {
        source_position: 1,
        graph_index: 0,
        pass_record_index: 0,
        kind: SharedSceneEffectCommandKind::SwapReferences {
            source_before: 0,
            destination_before: 1,
        },
    };
    let delayed_consumer = SharedSceneEffectCommand {
        source_position: 2,
        ..consumer
    };
    let error = validate_local_read_pairs(&[producer, unrelated, delayed_consumer], 1)
        .expect_err("non-adjacent local read must fail");
    assert!(error.contains("adjacent producer/consumer"));

    let error = validate_local_read_pairs(&[consumer, producer], 1)
        .expect_err("consumer-first local read must fail");
    assert!(error.contains("adjacent producer/consumer"));
}

#[test]
fn load_policy_is_compiled_from_persistence_and_authored_overwrite_facts() {
    let persistent = target(0);
    let transient = target(1);
    let mut overwrite = dynamic_render(transient, 2, 1);
    overwrite.fully_overwrites_target = true;
    let commands = [
        dynamic_render(persistent, 0, 1),
        dynamic_render(transient, 1, 1),
        overwrite,
        dynamic_render(transient, 3, 1),
    ];
    let plans = shared_scene_effect_execution_plans(
        &commands,
        &[allocation(persistent, 0), allocation(transient, 1)],
        &[image_plan(0, true), image_plan(1, false)],
        &[phase(&[0, 1])],
        &[],
    )
    .expect("load policy plan");
    let loads = plans[0]
        .commands
        .iter()
        .map(|command| match command.kind {
            SharedSceneEffectCommandKind::DynamicRender { load_op, .. } => load_op,
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();

    assert_eq!(
        loads,
        vec![
            SharedSceneEffectLoadOp::Load,
            SharedSceneEffectLoadOp::Clear,
            SharedSceneEffectLoadOp::Discard,
            SharedSceneEffectLoadOp::Load,
        ]
    );
}

fn target(name: u32) -> LogicalEffectTargetKey {
    LogicalEffectTargetKey {
        graph_index: 0,
        target: SceneRenderTargetKind::NamedFbo,
        name: SceneStringId(name),
    }
}

fn allocation(
    target: LogicalEffectTargetKey,
    physical_slot: u32,
) -> SceneRenderingDeviceTargetAllocation {
    SceneRenderingDeviceTargetAllocation {
        graph_index: target.graph_index,
        target: target.target,
        target_name: target.name,
        first_write_pass_id: 0,
        last_use_pass_id: 1,
        physical_slot,
        width: 0,
        height: 0,
        extent_domain: SceneTargetExtentDomain::PhysicalSurface,
    }
}

fn phase(initial_reference_physical_slots: &[u32]) -> SceneSampledImageBindingPlan {
    SceneSampledImageBindingPlan {
        sampled_slot_count: 0,
        sources: Vec::new(),
        initial_reference_physical_slots: initial_reference_physical_slots.to_vec(),
        fallback_descriptor_count: 0,
        scene_texture_descriptor_count: 0,
        scene_color_snapshot_descriptor_count: 0,
        effect_target_descriptor_count: 0,
        video_frame_descriptor_count: 0,
    }
}

fn image_plan(physical_slot: u32, persistent_across_frames: bool) -> SceneEffectTargetImagePlan {
    SceneEffectTargetImagePlan {
        physical_slot,
        graph_index: 0,
        target: SceneRenderTargetKind::NamedFbo,
        target_name: SceneStringId(physical_slot),
        format: vulkan_renderer::TextureFormat::Rgba8Unorm,
        extent: vulkan_renderer::Extent2D::new(64, 64),
        batch_field_count: 1,
        batch_atlas_columns: 1,
        batch_atlas_rows: 1,
        persistent_across_frames,
        aliased_logical_target_count: 1,
        input_attachment_required: false,
    }
}

fn dynamic_render(
    target: LogicalEffectTargetKey,
    mesh_draw_start: u32,
    mesh_draw_count: u32,
) -> SceneEffectTargetCommand {
    SceneEffectTargetCommand {
        kind: SceneEffectTargetCommandKind::DynamicRender,
        pass_record_index: mesh_draw_start,
        target,
        source: None,
        mesh_draw_start,
        mesh_draw_count,
        clear_before_draw: false,
        fully_overwrites_target: false,
        direct_scene_color_snapshot: false,
        scene_color_copy_coverage: SceneColorCopyCoverage::FullTarget,
        batch_physical_slot: None,
        batch_atlas_tile: None,
    }
}

fn swap(
    source: LogicalEffectTargetKey,
    destination: LogicalEffectTargetKey,
) -> SceneEffectTargetCommand {
    SceneEffectTargetCommand {
        kind: SceneEffectTargetCommandKind::SwapReferences,
        source: Some(SceneEffectTargetCommandSource::LogicalTarget(source)),
        ..dynamic_render(destination, 0, 0)
    }
}

fn copy_logical(
    source: LogicalEffectTargetKey,
    destination: LogicalEffectTargetKey,
) -> SceneEffectTargetCommand {
    SceneEffectTargetCommand {
        kind: SceneEffectTargetCommandKind::Copy,
        source: Some(SceneEffectTargetCommandSource::LogicalTarget(source)),
        ..dynamic_render(destination, 0, 0)
    }
}

fn dynamic_target_slots(plan: &SharedSceneEffectExecutionPlan) -> Vec<u32> {
    plan.commands
        .iter()
        .filter_map(|command| match command.kind {
            SharedSceneEffectCommandKind::DynamicRender {
                target_physical_slot,
                ..
            } => Some(target_physical_slot),
            _ => None,
        })
        .collect()
}

fn compiled_dynamic_render(
    target: LogicalEffectTargetKey,
    source_position: usize,
    role: SceneLocalReadScopePassRole,
) -> SharedSceneEffectCommand {
    SharedSceneEffectCommand {
        source_position,
        graph_index: target.graph_index,
        pass_record_index: source_position as u32,
        kind: SharedSceneEffectCommandKind::DynamicRender {
            target_physical_slot: 0,
            draw_start: 0,
            draw_count: 1,
            load_op: SharedSceneEffectLoadOp::Clear,
            batch_physical_slot: None,
            batch_atlas_tile: None,
            local_read: Some((0, role)),
        },
    }
}

use std::collections::BTreeMap;

use super::{
    VulkanaliaSceneOrderedDrawPipeline, VulkanaliaSceneOrderedDrawStep,
    VulkanaliaSceneSampledImageDescriptorBinding, VulkanaliaSceneSampledImageDrawCommand,
    VulkanaliaSceneSampledImageRenderTarget, VulkanaliaSceneSolidQuadDrawCommand,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct VulkanaliaSceneOrderedDrawTargetStats {
    pub(super) ordered_draw_step_count: u32,
    pub(super) ordered_target_run_count: u32,
    pub(super) ordered_swapchain_target_run_count: u32,
    pub(super) ordered_effect_target_run_count: u32,
    pub(super) ordered_max_target_run_draw_count: u32,
    pub(super) target_switch_swapchain_to_effect_target_count: u32,
    pub(super) target_switch_effect_target_to_swapchain_count: u32,
    pub(super) target_switch_effect_target_to_effect_target_count: u32,
    pub(super) repeated_effect_target_run_count: u32,
    pub(super) max_effect_target_run_count: u32,
}

pub(super) fn native_vulkan_vulkanalia_scene_ordered_draw_steps(
    solid_commands: &[VulkanaliaSceneSolidQuadDrawCommand],
    sampled_commands: &[VulkanaliaSceneSampledImageDrawCommand],
    effect_target_resource_base_index: usize,
    effect_target_resource_count: usize,
) -> Vec<VulkanaliaSceneOrderedDrawStep> {
    let mut scene_steps =
        Vec::with_capacity(solid_commands.len().saturating_add(sampled_commands.len()));
    for (command_index, command) in solid_commands.iter().enumerate() {
        scene_steps.push(SceneOrderedDrawSceneStep {
            draw: VulkanaliaSceneOrderedDrawStep {
                layer_index: command.layer_index,
                pipeline: VulkanaliaSceneOrderedDrawPipeline::SolidQuad,
                command_index,
            },
            effect_target_reads: Vec::new(),
        });
    }
    let mut offscreen_steps = Vec::new();
    for (command_index, command) in sampled_commands.iter().enumerate() {
        let draw = VulkanaliaSceneOrderedDrawStep {
            layer_index: command.layer_index,
            pipeline: VulkanaliaSceneOrderedDrawPipeline::SampledImage,
            command_index,
        };
        match command.render_target {
            VulkanaliaSceneSampledImageRenderTarget::Swapchain => {
                scene_steps.push(SceneOrderedDrawSceneStep {
                    draw,
                    effect_target_reads: scene_sampled_image_draw_command_effect_target_reads(
                        command,
                        effect_target_resource_base_index,
                        effect_target_resource_count,
                    ),
                });
            }
            VulkanaliaSceneSampledImageRenderTarget::EffectTarget { target_index, .. } => {
                let effect_target_reads = scene_sampled_image_draw_command_effect_target_reads(
                    command,
                    effect_target_resource_base_index,
                    effect_target_resource_count,
                );
                offscreen_steps.push(SceneOrderedDrawOffscreenStep {
                    draw,
                    write_target_index: target_index,
                    effect_target_reads,
                    earliest_scene_gap: 0,
                });
            }
        }
    }
    scene_steps.sort_by(|left, right| scene_ordered_draw_step_cmp(&left.draw, &right.draw));
    offscreen_steps.sort_by(|left, right| left.draw.command_index.cmp(&right.draw.command_index));

    if offscreen_steps.is_empty() {
        return scene_steps.into_iter().map(|step| step.draw).collect();
    }

    let sampled_scene_positions_by_layer =
        scene_sampled_image_scene_positions_by_layer(&scene_steps);
    let mut last_gap_by_layer = BTreeMap::<usize, usize>::new();
    for offscreen_step in &mut offscreen_steps {
        let final_scene_position = sampled_scene_positions_by_layer
            .get(&offscreen_step.draw.layer_index)
            .and_then(|positions| {
                positions
                    .iter()
                    .find(|(command_index, _)| *command_index > offscreen_step.draw.command_index)
            })
            .map(|(_, position)| *position)
            .unwrap_or(scene_steps.len());
        let mut earliest_gap = 0usize;
        for (scene_position, scene_step) in
            scene_steps.iter().enumerate().take(final_scene_position)
        {
            if scene_step
                .effect_target_reads
                .iter()
                .any(|target_index| *target_index == offscreen_step.write_target_index)
            {
                earliest_gap = earliest_gap.max(scene_position.saturating_add(1));
            }
        }
        if let Some(last_gap) = last_gap_by_layer.get(&offscreen_step.draw.layer_index) {
            earliest_gap = earliest_gap.max(*last_gap);
        }
        offscreen_step.earliest_scene_gap = earliest_gap;
        last_gap_by_layer.insert(offscreen_step.draw.layer_index, earliest_gap);
    }
    offscreen_steps.sort_by(|left, right| {
        left.earliest_scene_gap
            .cmp(&right.earliest_scene_gap)
            .then(left.draw.command_index.cmp(&right.draw.command_index))
    });
    let offscreen_steps = scene_ordered_draw_schedule_offscreen_steps(&offscreen_steps);

    let mut ordered =
        Vec::with_capacity(solid_commands.len().saturating_add(sampled_commands.len()));
    let mut offscreen_index = 0usize;
    for scene_gap in 0..=scene_steps.len() {
        while offscreen_index < offscreen_steps.len()
            && offscreen_steps[offscreen_index].earliest_scene_gap == scene_gap
        {
            ordered.push(offscreen_steps[offscreen_index].draw);
            offscreen_index += 1;
        }
        if let Some(scene_step) = scene_steps.get(scene_gap) {
            ordered.push(scene_step.draw);
        }
    }
    ordered
}

pub(super) fn native_vulkan_vulkanalia_scene_ordered_draw_target_stats(
    ordered_draws: &[VulkanaliaSceneOrderedDrawStep],
    sampled_commands: &[VulkanaliaSceneSampledImageDrawCommand],
    effect_target_resource_count: usize,
) -> VulkanaliaSceneOrderedDrawTargetStats {
    let mut stats = VulkanaliaSceneOrderedDrawTargetStats {
        ordered_draw_step_count: saturating_u32(ordered_draws.len()),
        ..VulkanaliaSceneOrderedDrawTargetStats::default()
    };
    let mut previous_target = None;
    let mut current_run_draw_count = 0usize;
    let mut effect_target_run_counts = vec![0usize; effect_target_resource_count];

    for draw in ordered_draws {
        let target = ordered_draw_target(draw, sampled_commands);
        if previous_target == Some(target) {
            current_run_draw_count = current_run_draw_count.saturating_add(1);
            continue;
        }

        stats.ordered_max_target_run_draw_count = stats
            .ordered_max_target_run_draw_count
            .max(saturating_u32(current_run_draw_count));
        if let Some(previous) = previous_target {
            match (previous, target) {
                (OrderedDrawTarget::Swapchain, OrderedDrawTarget::EffectTarget(_)) => {
                    stats.target_switch_swapchain_to_effect_target_count = stats
                        .target_switch_swapchain_to_effect_target_count
                        .saturating_add(1);
                }
                (OrderedDrawTarget::EffectTarget(_), OrderedDrawTarget::Swapchain) => {
                    stats.target_switch_effect_target_to_swapchain_count = stats
                        .target_switch_effect_target_to_swapchain_count
                        .saturating_add(1);
                }
                (OrderedDrawTarget::EffectTarget(_), OrderedDrawTarget::EffectTarget(_)) => {
                    stats.target_switch_effect_target_to_effect_target_count = stats
                        .target_switch_effect_target_to_effect_target_count
                        .saturating_add(1);
                }
                (OrderedDrawTarget::Swapchain, OrderedDrawTarget::Swapchain) => {}
            }
        }

        stats.ordered_target_run_count = stats.ordered_target_run_count.saturating_add(1);
        match target {
            OrderedDrawTarget::Swapchain => {
                stats.ordered_swapchain_target_run_count =
                    stats.ordered_swapchain_target_run_count.saturating_add(1);
            }
            OrderedDrawTarget::EffectTarget(target_index) => {
                stats.ordered_effect_target_run_count =
                    stats.ordered_effect_target_run_count.saturating_add(1);
                if let Some(count) = effect_target_run_counts.get_mut(target_index as usize) {
                    *count = count.saturating_add(1);
                }
            }
        }
        previous_target = Some(target);
        current_run_draw_count = 1;
    }
    stats.ordered_max_target_run_draw_count = stats
        .ordered_max_target_run_draw_count
        .max(saturating_u32(current_run_draw_count));
    stats.repeated_effect_target_run_count = saturating_u32(
        effect_target_run_counts
            .iter()
            .map(|count| count.saturating_sub(1))
            .sum::<usize>(),
    );
    stats.max_effect_target_run_count = saturating_u32(
        effect_target_run_counts
            .iter()
            .copied()
            .max()
            .unwrap_or_default(),
    );
    stats
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneOrderedDrawSceneStep {
    draw: VulkanaliaSceneOrderedDrawStep,
    effect_target_reads: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneOrderedDrawOffscreenStep {
    draw: VulkanaliaSceneOrderedDrawStep,
    write_target_index: u32,
    effect_target_reads: Vec<u32>,
    earliest_scene_gap: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrderedDrawTarget {
    Swapchain,
    EffectTarget(u32),
}

fn ordered_draw_target(
    draw: &VulkanaliaSceneOrderedDrawStep,
    sampled_commands: &[VulkanaliaSceneSampledImageDrawCommand],
) -> OrderedDrawTarget {
    match draw.pipeline {
        VulkanaliaSceneOrderedDrawPipeline::SolidQuad => OrderedDrawTarget::Swapchain,
        VulkanaliaSceneOrderedDrawPipeline::SampledImage => {
            match sampled_commands[draw.command_index].render_target {
                VulkanaliaSceneSampledImageRenderTarget::Swapchain => OrderedDrawTarget::Swapchain,
                VulkanaliaSceneSampledImageRenderTarget::EffectTarget { target_index, .. } => {
                    OrderedDrawTarget::EffectTarget(target_index)
                }
            }
        }
    }
}

fn scene_ordered_draw_schedule_offscreen_steps(
    offscreen_steps: &[SceneOrderedDrawOffscreenStep],
) -> Vec<SceneOrderedDrawOffscreenStep> {
    let mut scheduled = Vec::with_capacity(offscreen_steps.len());
    let mut group_start = 0usize;
    while group_start < offscreen_steps.len() {
        let scene_gap = offscreen_steps[group_start].earliest_scene_gap;
        let group_end = offscreen_steps[group_start..]
            .iter()
            .position(|step| step.earliest_scene_gap != scene_gap)
            .map(|offset| group_start.saturating_add(offset))
            .unwrap_or(offscreen_steps.len());
        scheduled.extend(scene_ordered_draw_schedule_offscreen_group(
            &offscreen_steps[group_start..group_end],
        ));
        group_start = group_end;
    }
    scheduled
}

fn scene_ordered_draw_schedule_offscreen_group(
    group: &[SceneOrderedDrawOffscreenStep],
) -> Vec<SceneOrderedDrawOffscreenStep> {
    if group.len() < 2 {
        return group.to_vec();
    }

    let mut successors = vec![Vec::<usize>::new(); group.len()];
    let mut indegrees = vec![0usize; group.len()];
    for before_index in 0..group.len() {
        for after_index in before_index.saturating_add(1)..group.len() {
            if !scene_ordered_draw_offscreen_dependency(&group[before_index], &group[after_index]) {
                continue;
            }
            successors[before_index].push(after_index);
            indegrees[after_index] = indegrees[after_index].saturating_add(1);
        }
    }

    let mut emitted = vec![false; group.len()];
    let mut ordered_indices = Vec::with_capacity(group.len());
    let mut last_target = None;
    while ordered_indices.len() < group.len() {
        let Some(index) =
            scene_ordered_draw_next_ready_offscreen_step(group, &indegrees, &emitted, last_target)
        else {
            return group.to_vec();
        };
        emitted[index] = true;
        ordered_indices.push(index);
        last_target = Some(group[index].write_target_index);
        for successor in &successors[index] {
            indegrees[*successor] = indegrees[*successor].saturating_sub(1);
        }
    }

    ordered_indices
        .into_iter()
        .map(|index| group[index].clone())
        .collect()
}

fn scene_ordered_draw_offscreen_dependency(
    before: &SceneOrderedDrawOffscreenStep,
    after: &SceneOrderedDrawOffscreenStep,
) -> bool {
    before.draw.layer_index == after.draw.layer_index
        || before.write_target_index == after.write_target_index
        || after
            .effect_target_reads
            .contains(&before.write_target_index)
        || before
            .effect_target_reads
            .contains(&after.write_target_index)
}

fn scene_ordered_draw_next_ready_offscreen_step(
    group: &[SceneOrderedDrawOffscreenStep],
    indegrees: &[usize],
    emitted: &[bool],
    last_target: Option<u32>,
) -> Option<usize> {
    let ready = group
        .iter()
        .enumerate()
        .filter(|(index, _)| !emitted[*index] && indegrees[*index] == 0);
    if let Some(last_target) = last_target
        && let Some((index, _)) = ready
            .clone()
            .filter(|(_, step)| step.write_target_index == last_target)
            .min_by(|(_, left), (_, right)| {
                left.draw
                    .command_index
                    .cmp(&right.draw.command_index)
                    .then(left.write_target_index.cmp(&right.write_target_index))
            })
    {
        return Some(index);
    }
    ready
        .min_by(|(_, left), (_, right)| {
            left.write_target_index
                .cmp(&right.write_target_index)
                .then(left.draw.command_index.cmp(&right.draw.command_index))
        })
        .map(|(index, _)| index)
}

fn scene_ordered_draw_step_cmp(
    left: &VulkanaliaSceneOrderedDrawStep,
    right: &VulkanaliaSceneOrderedDrawStep,
) -> std::cmp::Ordering {
    left.layer_index
        .cmp(&right.layer_index)
        .then(left.pipeline.sort_rank().cmp(&right.pipeline.sort_rank()))
        .then(left.command_index.cmp(&right.command_index))
}

fn scene_sampled_image_scene_positions_by_layer(
    scene_steps: &[SceneOrderedDrawSceneStep],
) -> BTreeMap<usize, Vec<(usize, usize)>> {
    let mut positions = BTreeMap::<usize, Vec<(usize, usize)>>::new();
    for (position, scene_step) in scene_steps.iter().enumerate() {
        if scene_step.draw.pipeline != VulkanaliaSceneOrderedDrawPipeline::SampledImage {
            continue;
        }
        positions
            .entry(scene_step.draw.layer_index)
            .or_default()
            .push((scene_step.draw.command_index, position));
    }
    positions
}

pub(super) fn scene_sampled_image_draw_command_effect_target_reads(
    command: &VulkanaliaSceneSampledImageDrawCommand,
    effect_target_resource_base_index: usize,
    effect_target_resource_count: usize,
) -> Vec<u32> {
    if effect_target_resource_count == 0 {
        return Vec::new();
    }
    let effect_target_resource_end =
        effect_target_resource_base_index.saturating_add(effect_target_resource_count);
    let VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
        texture_slot_bindings,
        ..
    } = &command.descriptor_binding;
    let mut reads = Vec::new();
    for binding in texture_slot_bindings {
        let resource_index = binding.resource_index as usize;
        if resource_index < effect_target_resource_base_index
            || resource_index >= effect_target_resource_end
        {
            continue;
        }
        let target_index =
            (resource_index - effect_target_resource_base_index).min(u32::MAX as usize) as u32;
        if !reads.contains(&target_index) {
            reads.push(target_index);
        }
    }
    reads
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

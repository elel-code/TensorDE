use std::collections::BTreeMap;

use super::{
    VulkanaliaSceneOrderedDrawPipeline, VulkanaliaSceneOrderedDrawStep,
    VulkanaliaSceneSampledImageDescriptorBinding, VulkanaliaSceneSampledImageDrawCommand,
    VulkanaliaSceneSampledImageRenderTarget, VulkanaliaSceneSolidQuadDrawCommand,
};

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
                offscreen_steps.push(SceneOrderedDrawOffscreenStep {
                    draw,
                    write_target_index: target_index,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneOrderedDrawSceneStep {
    draw: VulkanaliaSceneOrderedDrawStep,
    effect_target_reads: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneOrderedDrawOffscreenStep {
    draw: VulkanaliaSceneOrderedDrawStep,
    write_target_index: u32,
    earliest_scene_gap: usize,
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

fn scene_sampled_image_draw_command_effect_target_reads(
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

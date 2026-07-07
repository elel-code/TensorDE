//! Scene effect runtime command graph executor.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

mod command_sequence;
mod copy_command;
mod material_command;
mod target_access;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::SceneEffectPassGraphPlan;

pub(in crate::renderer::native_vulkan) use self::command_sequence::NativeVulkanSceneEffectRuntimeCommandSequencePlan;
use self::command_sequence::{SceneEffectGraphCommand, ordered_effect_graph_commands};
use self::copy_command::{
    NativeVulkanSceneEffectCopyCommandPlan, NativeVulkanSceneEffectSwapCommandPlan,
    record_effect_copy_command,
};
use self::material_command::{
    NativeVulkanSceneEffectMaterialRuntimeCommandPlan, record_effect_material_pass,
};
use self::target_access::{
    effect_access_initial_clear_count, effect_access_transition_count, effect_target_format,
};
use super::effect_pipeline_warmup::NativeVulkanSceneEffectPipelineWarmupPlan;
use super::frame_resources::NativeVulkanSceneFrameResources;
use super::target_formats::NativeVulkanSceneGraphTargetFormatPlan;

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectRuntimeFrameContext<'a> {
    pub device: &'a Device,
    pub command_buffer: vk::CommandBuffer,
    pub target_formats: &'a NativeVulkanSceneGraphTargetFormatPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectRuntimeFramePlan<'a> {
    pub command_sequence: NativeVulkanSceneEffectRuntimeCommandSequencePlan,
    pub pipeline_warmup: NativeVulkanSceneEffectPipelineWarmupPlan,
    pub command_count: usize,
    pub material_pass_count: usize,
    pub copy_command_count: usize,
    pub swap_command_count: usize,
    pub target_transition_count: usize,
    pub target_initial_clear_count: usize,
    pub target_scope_count: usize,
    pub fullscreen_draw_count: usize,
    pub copy_image_count: usize,
    pub commands: Vec<NativeVulkanSceneEffectRuntimeCommandPlan<'a>>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneEffectRuntimeCommandPlan<'a> {
    MaterialPass(NativeVulkanSceneEffectMaterialRuntimeCommandPlan<'a>),
    Copy(NativeVulkanSceneEffectCopyCommandPlan),
    Swap(NativeVulkanSceneEffectSwapCommandPlan),
}

impl<'a> NativeVulkanSceneEffectRuntimeFramePlan<'a> {
    fn empty(
        command_sequence: NativeVulkanSceneEffectRuntimeCommandSequencePlan,
        pipeline_warmup: NativeVulkanSceneEffectPipelineWarmupPlan,
    ) -> Self {
        Self {
            command_sequence,
            pipeline_warmup,
            command_count: 0,
            material_pass_count: 0,
            copy_command_count: 0,
            swap_command_count: 0,
            target_transition_count: 0,
            target_initial_clear_count: 0,
            target_scope_count: 0,
            fullscreen_draw_count: 0,
            copy_image_count: 0,
            commands: Vec::new(),
            command_order: [
                "validate_effect_command_sequence",
                "require_warmed_effect_pipelines",
                "transition_effect_inputs",
                "record_effect_material_passes",
                "record_effect_copy_commands",
                "preserve_effect_swap_alias_commands",
            ],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_effect_runtime_frame<'a>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneEffectRuntimeFrameContext<'_>,
    graph: &'a SceneEffectPassGraphPlan,
) -> Result<NativeVulkanSceneEffectRuntimeFramePlan<'a>, String> {
    let command_sequence =
        NativeVulkanSceneEffectRuntimeCommandSequencePlan::from_effect_pass_graph(graph)?;
    let pipeline_warmup =
        NativeVulkanSceneEffectPipelineWarmupPlan::from_effect_pass_graph_with_target_formats(
            graph,
            |target| effect_target_format(frame_resources, context.target_formats, target),
        )?;
    for key in pipeline_warmup.cache_keys() {
        frame_resources.cached_effect_pipeline(key).map_err(|err| {
            format!(
                "{err}; scene effect runtime requires pipeline warmup before present-frame recording"
            )
        })?;
    }
    if command_sequence.command_count == 0 {
        return Ok(NativeVulkanSceneEffectRuntimeFramePlan::empty(
            command_sequence,
            pipeline_warmup,
        ));
    }

    let commands = ordered_effect_graph_commands(graph)?;
    let mut written_targets = BTreeSet::new();
    let mut runtime_commands = Vec::with_capacity(commands.len());
    let mut target_transition_count = 0usize;
    let mut target_initial_clear_count = 0usize;
    let mut target_scope_count = 0usize;
    let mut fullscreen_draw_count = 0usize;
    let mut copy_image_count = 0usize;

    for command in commands {
        match command {
            SceneEffectGraphCommand::Material(pass) => {
                let plan = record_effect_material_pass(
                    frame_resources,
                    &context,
                    pass,
                    &mut written_targets,
                )?;
                target_transition_count = target_transition_count
                    .saturating_add(effect_access_transition_count(&plan.input_accesses))
                    .saturating_add(plan.output_transition_count);
                target_initial_clear_count = target_initial_clear_count
                    .saturating_add(effect_access_initial_clear_count(&plan.input_accesses));
                target_scope_count = target_scope_count.saturating_add(1);
                fullscreen_draw_count =
                    fullscreen_draw_count.saturating_add(plan.pass.fullscreen_draw_count);
                runtime_commands.push(NativeVulkanSceneEffectRuntimeCommandPlan::MaterialPass(
                    plan,
                ));
            }
            SceneEffectGraphCommand::Copy(copy) => {
                let plan = record_effect_copy_command(
                    frame_resources,
                    context.device,
                    context.command_buffer,
                    copy,
                    &mut written_targets,
                )?;
                target_transition_count = target_transition_count
                    .saturating_add(plan.source_access.iter().count())
                    .saturating_add(plan.target_access.iter().count());
                copy_image_count = copy_image_count.saturating_add(plan.copy_image_count);
                runtime_commands.push(NativeVulkanSceneEffectRuntimeCommandPlan::Copy(plan));
            }
            SceneEffectGraphCommand::Swap(swap) => {
                runtime_commands.push(NativeVulkanSceneEffectRuntimeCommandPlan::Swap(
                    NativeVulkanSceneEffectSwapCommandPlan::from_graph_swap(swap),
                ));
            }
        }
    }

    Ok(NativeVulkanSceneEffectRuntimeFramePlan {
        command_count: runtime_commands.len(),
        material_pass_count: graph.material_pass_count,
        copy_command_count: graph.copy_command_count,
        swap_command_count: graph.swap_command_count,
        target_transition_count,
        target_initial_clear_count,
        target_scope_count,
        fullscreen_draw_count,
        copy_image_count,
        commands: runtime_commands,
        command_sequence,
        pipeline_warmup,
        command_order: [
            "validate_effect_command_sequence",
            "require_warmed_effect_pipelines",
            "transition_effect_inputs",
            "record_effect_material_passes",
            "record_effect_copy_commands",
            "preserve_effect_swap_alias_commands",
        ],
    })
}

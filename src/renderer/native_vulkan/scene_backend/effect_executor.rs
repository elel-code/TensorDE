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
pub(in crate::renderer::native_vulkan::scene_backend) mod target_access;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneEffectPassGraphOutput, SceneEffectPassGraphPlan, SceneGraphTarget, SceneObjectId,
};

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
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectRuntimePreflightPlan {
    pub command_sequence: NativeVulkanSceneEffectRuntimeCommandSequencePlan,
    pub pipeline_warmup: NativeVulkanSceneEffectPipelineWarmupPlan,
    pub command_order: [&'static str; 2],
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectRuntimeCommandCounts {
    pub command_count: usize,
    pub material_pass_count: usize,
    pub copy_command_count: usize,
    pub swap_command_count: usize,
    pub target_transition_count: usize,
    pub target_initial_clear_count: usize,
    pub target_scope_count: usize,
    pub fullscreen_draw_count: usize,
    pub copy_image_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneEffectObjectCommandKind {
    Material,
    Copy,
    Swap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectObjectCommandStreamPlan {
    pub stream_count: usize,
    pub command_count: usize,
    pub material_pass_count: usize,
    pub copy_command_count: usize,
    pub swap_command_count: usize,
    pub layer_final_pass_count: usize,
    pub streams: Vec<NativeVulkanSceneEffectObjectCommandStream>,
    pub entries: Vec<NativeVulkanSceneEffectObjectCommandStreamEntry>,
    pub command_order: [&'static str; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectObjectCommandStream {
    pub object: SceneObjectId,
    pub entry_index_start: usize,
    pub entry_index_end: usize,
    pub command_count: usize,
    pub material_pass_count: usize,
    pub copy_command_count: usize,
    pub swap_command_count: usize,
    pub layer_final_pass_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectObjectCommandStreamEntry {
    pub graph_command_index: usize,
    pub object: SceneObjectId,
    pub kind: NativeVulkanSceneEffectObjectCommandKind,
    pub graph_vector_index: usize,
}

impl NativeVulkanSceneEffectRuntimePreflightPlan {
    fn from_parts(
        command_sequence: NativeVulkanSceneEffectRuntimeCommandSequencePlan,
        pipeline_warmup: NativeVulkanSceneEffectPipelineWarmupPlan,
    ) -> Self {
        Self {
            command_sequence,
            pipeline_warmup,
            command_order: [
                "validate_effect_command_sequence",
                "require_warmed_effect_pipelines",
            ],
        }
    }
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_count_scene_effect_runtime_commands(
    commands: &[NativeVulkanSceneEffectRuntimeCommandPlan<'_>],
) -> NativeVulkanSceneEffectRuntimeCommandCounts {
    let mut counts = NativeVulkanSceneEffectRuntimeCommandCounts {
        command_count: commands.len(),
        material_pass_count: 0,
        copy_command_count: 0,
        swap_command_count: 0,
        target_transition_count: 0,
        target_initial_clear_count: 0,
        target_scope_count: 0,
        fullscreen_draw_count: 0,
        copy_image_count: 0,
    };
    for command in commands {
        match command {
            NativeVulkanSceneEffectRuntimeCommandPlan::MaterialPass(pass) => {
                counts.material_pass_count = counts.material_pass_count.saturating_add(1);
                counts.target_transition_count = counts
                    .target_transition_count
                    .saturating_add(effect_access_transition_count(&pass.input_accesses))
                    .saturating_add(pass.output_transition_count);
                counts.target_initial_clear_count = counts
                    .target_initial_clear_count
                    .saturating_add(effect_access_initial_clear_count(&pass.input_accesses));
                counts.target_scope_count = counts.target_scope_count.saturating_add(1);
                counts.fullscreen_draw_count = counts
                    .fullscreen_draw_count
                    .saturating_add(pass.pass.fullscreen_draw_count);
            }
            NativeVulkanSceneEffectRuntimeCommandPlan::Copy(copy) => {
                counts.copy_command_count = counts.copy_command_count.saturating_add(1);
                counts.target_transition_count = counts
                    .target_transition_count
                    .saturating_add(copy.source_access.iter().count())
                    .saturating_add(copy.target_access.iter().count());
                counts.copy_image_count = counts
                    .copy_image_count
                    .saturating_add(copy.copy_image_count);
            }
            NativeVulkanSceneEffectRuntimeCommandPlan::Swap(_) => {
                counts.swap_command_count = counts.swap_command_count.saturating_add(1);
            }
        }
    }
    counts
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_effect_runtime_preflight(
    frame_resources: &NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneEffectRuntimeFrameContext<'_>,
    graph: &SceneEffectPassGraphPlan,
) -> Result<NativeVulkanSceneEffectRuntimePreflightPlan, String> {
    let command_sequence =
        NativeVulkanSceneEffectRuntimeCommandSequencePlan::from_effect_pass_graph(graph)?;
    let pipeline_warmup = if graph.material_pass_count == 0 {
        NativeVulkanSceneEffectPipelineWarmupPlan::from_effect_pass_graph_with_target_formats(
            graph,
            |target| effect_target_format(frame_resources, context.target_formats, target),
        )?
    } else {
        let effect_resource_heap = frame_resources
            .current_effect_resource_heap_frame_plan()
            .ok_or_else(|| {
                "scene effect runtime preflight requires current effect resource heap frame plan"
                    .to_owned()
            })?;
        NativeVulkanSceneEffectPipelineWarmupPlan::from_effect_pass_graph_with_target_formats_and_resource_heap(
            graph,
            |target| effect_target_format(frame_resources, context.target_formats, target),
            effect_resource_heap,
        )?
    };
    for key in pipeline_warmup.cache_keys() {
        frame_resources.cached_effect_pipeline(key).map_err(|err| {
            format!(
                "{err}; scene effect runtime requires pipeline warmup before present-frame recording"
            )
        })?;
    }
    Ok(NativeVulkanSceneEffectRuntimePreflightPlan::from_parts(
        command_sequence,
        pipeline_warmup,
    ))
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_effect_runtime_frame<'a>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneEffectRuntimeFrameContext<'_>,
    graph: &'a SceneEffectPassGraphPlan,
) -> Result<NativeVulkanSceneEffectRuntimeFramePlan<'a>, String> {
    let preflight =
        native_vulkan_plan_scene_effect_runtime_preflight(frame_resources, &context, graph)?;
    if preflight.command_sequence.command_count == 0 {
        return Ok(NativeVulkanSceneEffectRuntimeFramePlan::empty(
            preflight.command_sequence,
            preflight.pipeline_warmup,
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
        command_sequence: preflight.command_sequence,
        pipeline_warmup: preflight.pipeline_warmup,
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_effect_object_command_streams(
    graph: &SceneEffectPassGraphPlan,
) -> Result<NativeVulkanSceneEffectObjectCommandStreamPlan, String> {
    let mut entries = Vec::with_capacity(
        graph
            .material_pass_count
            .saturating_add(graph.copy_command_count)
            .saturating_add(graph.swap_command_count),
    );
    entries.extend(graph.passes.iter().enumerate().map(|(index, pass)| {
        NativeVulkanSceneEffectObjectCommandStreamEntry {
            graph_command_index: pass.graph_command_index,
            object: pass.object,
            kind: NativeVulkanSceneEffectObjectCommandKind::Material,
            graph_vector_index: index,
        }
    }));
    entries.extend(graph.copies.iter().enumerate().map(|(index, copy)| {
        NativeVulkanSceneEffectObjectCommandStreamEntry {
            graph_command_index: copy.graph_command_index,
            object: copy.object,
            kind: NativeVulkanSceneEffectObjectCommandKind::Copy,
            graph_vector_index: index,
        }
    }));
    entries.extend(graph.swaps.iter().enumerate().map(|(index, swap)| {
        NativeVulkanSceneEffectObjectCommandStreamEntry {
            graph_command_index: swap.graph_command_index,
            object: swap.object,
            kind: NativeVulkanSceneEffectObjectCommandKind::Swap,
            graph_vector_index: index,
        }
    }));
    entries.sort_by_key(|entry| entry.graph_command_index);
    for (expected, entry) in entries.iter().enumerate() {
        if entry.graph_command_index != expected {
            return Err(format!(
                "scene effect object command stream must be dense and ordered; expected command index {expected}, got {}",
                entry.graph_command_index
            ));
        }
    }

    let mut streams = Vec::new();
    let mut entry_index = 0usize;
    while entry_index < entries.len() {
        let object = entries[entry_index].object;
        let start = entry_index;
        let mut material_pass_count = 0usize;
        let mut copy_command_count = 0usize;
        let mut swap_command_count = 0usize;
        let mut layer_final_pass_count = 0usize;
        while entry_index < entries.len() && entries[entry_index].object == object {
            match entries[entry_index].kind {
                NativeVulkanSceneEffectObjectCommandKind::Material => {
                    material_pass_count = material_pass_count.saturating_add(1);
                    let pass = graph.passes.get(entries[entry_index].graph_vector_index).ok_or_else(|| {
                        format!(
                            "scene effect object command stream entry {} points outside material pass list",
                            entries[entry_index].graph_command_index
                        )
                    })?;
                    if effect_pass_is_layer_final_output(graph, object, &pass.output) {
                        layer_final_pass_count = layer_final_pass_count.saturating_add(1);
                    }
                }
                NativeVulkanSceneEffectObjectCommandKind::Copy => {
                    copy_command_count = copy_command_count.saturating_add(1);
                }
                NativeVulkanSceneEffectObjectCommandKind::Swap => {
                    swap_command_count = swap_command_count.saturating_add(1);
                }
            }
            entry_index = entry_index.saturating_add(1);
        }
        let command_count = entry_index.saturating_sub(start);
        streams.push(NativeVulkanSceneEffectObjectCommandStream {
            object,
            entry_index_start: start,
            entry_index_end: entry_index,
            command_count,
            material_pass_count,
            copy_command_count,
            swap_command_count,
            layer_final_pass_count,
        });
    }

    Ok(NativeVulkanSceneEffectObjectCommandStreamPlan {
        stream_count: streams.len(),
        command_count: entries.len(),
        material_pass_count: graph.material_pass_count,
        copy_command_count: graph.copy_command_count,
        swap_command_count: graph.swap_command_count,
        layer_final_pass_count: streams
            .iter()
            .map(|stream| stream.layer_final_pass_count)
            .sum(),
        streams,
        entries,
        command_order: [
            "merge_effect_material_copy_swap_commands",
            "sort_by_scene_effect_graph_command_index",
            "partition_contiguous_commands_by_object",
            "count_layer_final_outputs_per_stream",
        ],
    })
}

fn effect_pass_is_layer_final_output(
    graph: &SceneEffectPassGraphPlan,
    object: SceneObjectId,
    output: &SceneEffectPassGraphOutput,
) -> bool {
    match output {
        SceneEffectPassGraphOutput::ObjectFinal(output_object) => *output_object == object,
        SceneEffectPassGraphOutput::GraphTarget(target) => {
            graph.image_layer_targets.iter().any(|image_layer| {
                image_layer.object == object && image_layer.final_source_target == *target
            })
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_effect_layer_final_command_stream<
    'a,
>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneEffectRuntimeFrameContext<'_>,
    graph: &'a SceneEffectPassGraphPlan,
    stream_plan: &NativeVulkanSceneEffectObjectCommandStreamPlan,
    object: SceneObjectId,
    written_targets: &mut BTreeSet<SceneGraphTarget>,
    runtime_commands: &mut Vec<NativeVulkanSceneEffectRuntimeCommandPlan<'a>>,
) -> Result<usize, String> {
    let mut matches = stream_plan
        .streams
        .iter()
        .filter(|stream| stream.object == object);
    let stream = matches.next().ok_or_else(|| {
        format!("scene effect layer-final block for object {object:?} has no effect command stream")
    })?;
    if matches.next().is_some() {
        return Err(format!(
            "scene effect layer-final block for object {object:?} has non-contiguous effect command streams"
        ));
    }
    if stream.layer_final_pass_count != 1 {
        return Err(format!(
            "scene effect layer-final block for object {object:?} requires exactly one layer-final material pass, got {}",
            stream.layer_final_pass_count
        ));
    }
    for entry in stream_plan
        .entries
        .get(stream.entry_index_start..stream.entry_index_end)
        .ok_or_else(|| {
            format!(
                "scene effect layer-final stream for object {object:?} has invalid entry range {}..{}",
                stream.entry_index_start, stream.entry_index_end
            )
        })?
    {
        match entry.kind {
            NativeVulkanSceneEffectObjectCommandKind::Material => {
                let pass = graph.passes.get(entry.graph_vector_index).ok_or_else(|| {
                    format!(
                        "scene effect material stream entry {} points outside material pass list",
                        entry.graph_command_index
                    )
                })?;
                runtime_commands.push(NativeVulkanSceneEffectRuntimeCommandPlan::MaterialPass(
                    record_effect_material_pass(frame_resources, context, pass, written_targets)?,
                ));
            }
            NativeVulkanSceneEffectObjectCommandKind::Copy => {
                let copy = graph.copies.get(entry.graph_vector_index).ok_or_else(|| {
                    format!(
                        "scene effect copy stream entry {} points outside copy command list",
                        entry.graph_command_index
                    )
                })?;
                runtime_commands.push(NativeVulkanSceneEffectRuntimeCommandPlan::Copy(
                    record_effect_copy_command(
                        frame_resources,
                        context.device,
                        context.command_buffer,
                        copy,
                        written_targets,
                    )?,
                ));
            }
            NativeVulkanSceneEffectObjectCommandKind::Swap => {
                let swap = graph.swaps.get(entry.graph_vector_index).ok_or_else(|| {
                    format!(
                        "scene effect swap stream entry {} points outside swap command list",
                        entry.graph_command_index
                    )
                })?;
                runtime_commands.push(NativeVulkanSceneEffectRuntimeCommandPlan::Swap(
                    NativeVulkanSceneEffectSwapCommandPlan::from_graph_swap(swap),
                ));
            }
        }
    }
    Ok(stream.command_count)
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_effect_runtime_frame_from_recorded_commands<
    'a,
>(
    _graph: &SceneEffectPassGraphPlan,
    preflight: NativeVulkanSceneEffectRuntimePreflightPlan,
    commands: Vec<NativeVulkanSceneEffectRuntimeCommandPlan<'a>>,
) -> NativeVulkanSceneEffectRuntimeFramePlan<'a> {
    let counts = native_vulkan_count_scene_effect_runtime_commands(&commands);
    NativeVulkanSceneEffectRuntimeFramePlan {
        command_count: counts.command_count,
        material_pass_count: counts.material_pass_count,
        copy_command_count: counts.copy_command_count,
        swap_command_count: counts.swap_command_count,
        target_transition_count: counts.target_transition_count,
        target_initial_clear_count: counts.target_initial_clear_count,
        target_scope_count: counts.target_scope_count,
        fullscreen_draw_count: counts.fullscreen_draw_count,
        copy_image_count: counts.copy_image_count,
        commands,
        command_sequence: preflight.command_sequence,
        pipeline_warmup: preflight.pipeline_warmup,
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

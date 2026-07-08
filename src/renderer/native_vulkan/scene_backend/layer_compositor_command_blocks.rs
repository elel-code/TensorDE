//! Command-block recorder for WE layer compositor schedules.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneEffectPassGraphPlan, SceneFramePlan, SceneGraphExecutionPass, SceneGraphExecutionPlan,
    SceneGraphTarget,
};

use super::effect_executor::{
    NativeVulkanSceneEffectObjectCommandStreamPlan, NativeVulkanSceneEffectRuntimeCommandCounts,
    NativeVulkanSceneEffectRuntimeCommandPlan, NativeVulkanSceneEffectRuntimeFrameContext,
    native_vulkan_count_scene_effect_runtime_commands,
    native_vulkan_record_scene_effect_object_final_command_stream,
};
use super::frame_resources::NativeVulkanSceneFrameResources;
use super::graph_executor::{
    NativeVulkanSceneGraphFrameCommandPlan, NativeVulkanSceneGraphPassCommandPlan,
    NativeVulkanSceneGraphRuntimeFrameContext,
};
use super::layer_alpha_mask_executor::token_draw_list::{
    NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordContext,
    NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan,
    native_vulkan_record_scene_layer_alpha_mask_token_draw_list_step,
};
use super::layer_alpha_mask_executor::{
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
    NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
    NativeVulkanSceneLayerAlphaMaskTokenRecordingKind,
    NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
};
use super::layer_compositor_scheduler::{
    NativeVulkanSceneLayerCompositorRecordingBlockKind,
    NativeVulkanSceneLayerCompositorSchedulePlan,
};
use super::pass_command::{
    NativeVulkanSceneMeshPassCommand, NativeVulkanSceneMeshPassCommandPlan,
    NativeVulkanSceneMeshPassDrawSpanCounts, NativeVulkanSceneMeshPassDrawSpanState,
    native_vulkan_record_scene_mesh_pass_draw_span_commands,
};
use super::pipeline::NativeVulkanScenePipelineCacheKey;
use super::render_target::{
    NativeVulkanSceneRenderTarget, native_vulkan_record_scene_render_target_begin,
    native_vulkan_record_scene_render_target_end,
};

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerCompositorAlphaTokenBlockInputs<
    'a,
> {
    pub token_recording: &'a NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
    pub producer_targets: &'a NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    pub producer_pipelines: &'a NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
    pub generated_commands: &'a NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    pub generated_pipelines: &'a NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    pub rt_method8_commands: &'a NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
    pub resource_binds: &'a NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerCompositorEffectBlockInputs<
    'graph,
    'context,
    'streams,
> {
    pub context: NativeVulkanSceneEffectRuntimeFrameContext<'context>,
    pub graph: &'graph SceneEffectPassGraphPlan,
    pub command_streams: &'streams NativeVulkanSceneEffectObjectCommandStreamPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerCompositorCommandBlockRecordPlan
{
    pub block_count: usize,
    pub mesh_span_block_count: usize,
    pub object_final_effect_block_count: usize,
    pub alpha_mask_token_block_count: usize,
    pub no_draw_marker_block_count: usize,
    pub mesh_target_scope_count: usize,
    pub object_final_effect_target_scope_count: usize,
    pub alpha_mask_target_scope_count: usize,
    pub object_final_effect_recorded_command_count: usize,
    pub object_final_effect_material_pass_count: usize,
    pub object_final_effect_copy_command_count: usize,
    pub object_final_effect_swap_command_count: usize,
    pub object_final_effect_target_transition_count: usize,
    pub object_final_effect_target_initial_clear_count: usize,
    pub object_final_effect_fullscreen_draw_count: usize,
    pub object_final_effect_copy_image_count: usize,
    pub alpha_mask_recorded_step_count: usize,
    pub alpha_mask_token_draw_list: NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan,
    pub command_order: [&'static str; 7],
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerCompositorCommandBlockRecordOutput<
    'a,
> {
    pub mesh_frame: NativeVulkanSceneGraphFrameCommandPlan<'a>,
    pub command_blocks: NativeVulkanSceneLayerCompositorCommandBlockRecordPlan,
    pub effect_commands: Vec<NativeVulkanSceneEffectRuntimeCommandPlan<'a>>,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_layer_compositor_command_blocks<
    'a,
>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneGraphRuntimeFrameContext<'_>,
    frame: &'a SceneFramePlan,
    graph_execution: &SceneGraphExecutionPlan,
    schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
    alpha_inputs: NativeVulkanSceneLayerCompositorAlphaTokenBlockInputs<'_>,
    effect_inputs: NativeVulkanSceneLayerCompositorEffectBlockInputs<'a, '_, '_>,
) -> Result<Option<NativeVulkanSceneLayerCompositorCommandBlockRecordOutput<'a>>, String> {
    if schedule.recording_block_count == 0 && graph_execution.pass_count == 0 {
        return Ok(Some(
            NativeVulkanSceneLayerCompositorCommandBlockRecordOutput {
                mesh_frame: empty_mesh_block_frame_plan(
                    context.target_formats.target_format_count(),
                ),
                command_blocks: NativeVulkanSceneLayerCompositorCommandBlockRecordPlan::empty(),
                effect_commands: Vec::new(),
            },
        ));
    }
    if !schedule_is_command_block_recordable(schedule) {
        return Ok(None);
    }

    let execution_pass = if schedule.mesh_graph_draw_span_block_count == 0 {
        if graph_execution.pass_count != 0 {
            return Ok(None);
        }
        None
    } else {
        if graph_execution.pass_count != 1 {
            return Ok(None);
        }
        let execution_pass = &graph_execution.passes[0];
        if execution_pass.input.is_some() || execution_pass.output != SceneGraphTarget::Swapchain {
            return Ok(None);
        }
        if !mesh_blocks_cover_execution_pass(schedule, execution_pass)? {
            return Ok(None);
        }
        Some(execution_pass)
    };
    let graph_pass = execution_pass
        .map(|execution_pass| {
            frame
                .graph
                .passes
                .get(execution_pass.pass_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer compositor command-block recorder pass {} is outside graph",
                        execution_pass.pass_index
                    )
                })
        })
        .transpose()?;
    let pass_target_format = context.target_formats.format(SceneGraphTarget::Swapchain)?;
    let last_swapchain_writer_block = last_swapchain_writer_block_index(schedule, &alpha_inputs)?;
    let mut commands = Vec::with_capacity(
        graph_pass
            .map(|pass| pass.draws.len())
            .unwrap_or_default()
            .saturating_mul(2)
            .saturating_add(schedule.recording_blocks.len())
            .saturating_add(2),
    );
    let mut first_mesh_target_scope = None;
    let mut mesh_target_scope_count = 0usize;
    let mut counts = NativeVulkanSceneMeshPassDrawSpanCounts::default();
    let mut alpha_steps = Vec::new();
    let mut effect_commands = Vec::new();
    let mut written_effect_targets = std::collections::BTreeSet::new();
    let mut swapchain_current_layout = context.swapchain_target.initial_layout;
    let mut swapchain_was_written = false;

    for block in &schedule.recording_blocks {
        match block.kind {
            NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan => {
                let execution_pass = execution_pass.ok_or_else(|| {
                    format!(
                        "scene layer compositor command-block recorder block {} needs a graph execution pass",
                        block.block_index
                    )
                })?;
                let graph_pass = graph_pass.ok_or_else(|| {
                    format!(
                        "scene layer compositor command-block recorder block {} needs a graph pass",
                        block.block_index
                    )
                })?;
                let draw_index_start = block.graph_draw_index_start.ok_or_else(|| {
                    format!(
                        "scene layer compositor command-block recorder block {} has no draw start",
                        block.block_index
                    )
                })?;
                let draw_index_end = block.graph_draw_index_end.ok_or_else(|| {
                    format!(
                        "scene layer compositor command-block recorder block {} has no draw end",
                        block.block_index
                    )
                })?;
                let mut swapchain = context.swapchain_target;
                swapchain.initial_layout = swapchain_current_layout;
                swapchain.final_layout =
                    swapchain_final_layout(block.block_index, last_swapchain_writer_block);
                let render_target = NativeVulkanSceneRenderTarget::Swapchain(swapchain);
                let clear_color = (!swapchain_was_written)
                    .then_some(context.clear_color)
                    .flatten();
                let target_scope = native_vulkan_record_scene_render_target_begin(
                    context.device,
                    context.command_buffer,
                    render_target,
                    clear_color,
                )?;
                first_mesh_target_scope.get_or_insert(target_scope);
                mesh_target_scope_count = mesh_target_scope_count.saturating_add(1);
                commands.push(NativeVulkanSceneMeshPassCommand::BeginPass {
                    name: graph_pass.name.as_str(),
                    input: graph_pass.input,
                    output: graph_pass.output,
                    draw_index_start,
                });
                let mut draw_span_state = NativeVulkanSceneMeshPassDrawSpanState::default();
                let span_counts = {
                    let resources = &*frame_resources;
                    native_vulkan_record_scene_mesh_pass_draw_span_commands(
                        context.device,
                        context.command_buffer,
                        graph_pass,
                        execution_pass.draw_index_start,
                        draw_index_start,
                        draw_index_end,
                        &mut draw_span_state,
                        &mut commands,
                        |key| {
                            let cache_key = NativeVulkanScenePipelineCacheKey::from_bind_key(
                                key,
                                pass_target_format,
                            )?;
                            Ok(resources.cached_mesh_pipeline(&cache_key)?.pipeline)
                        },
                        |draw_index| resources.resource_heap_draw_bind_info_for_draw(draw_index),
                        |geometry| resources.mesh_draw_buffers(geometry),
                    )?
                };
                counts.pipeline_bind_count = counts
                    .pipeline_bind_count
                    .saturating_add(span_counts.pipeline_bind_count);
                counts.resource_heap_bind_count = counts
                    .resource_heap_bind_count
                    .saturating_add(span_counts.resource_heap_bind_count);
                counts.indexed_draw_count = counts
                    .indexed_draw_count
                    .saturating_add(span_counts.indexed_draw_count);
                commands.push(NativeVulkanSceneMeshPassCommand::EndPass);
                native_vulkan_record_scene_render_target_end(
                    context.device,
                    context.command_buffer,
                    render_target,
                    clear_color,
                )?;
                swapchain_current_layout = swapchain.final_layout;
                swapchain_was_written = true;
            }
            NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker => {
                commands.push(NativeVulkanSceneMeshPassCommand::LayerCompositorNoDrawMarker {
                    block_index: block.block_index,
                    step_index_start: block.step_index_start,
                    step_index_end: block.step_index_end,
                });
            }
            NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep => {
                let token_recording_step_index = block.token_recording_step_index.ok_or_else(|| {
                    format!(
                        "scene layer compositor alpha-mask block {} has no token recording step index",
                        block.block_index
                    )
                })?;
                let mut swapchain = context.swapchain_target;
                swapchain.initial_layout = swapchain_current_layout;
                let generated_swapchain_final_layout =
                    swapchain_final_layout(block.block_index, last_swapchain_writer_block);
                swapchain.final_layout = generated_swapchain_final_layout;
                let step = native_vulkan_record_scene_layer_alpha_mask_token_draw_list_step(
                    frame_resources,
                    NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordContext {
                        device: context.device,
                        command_buffer: context.command_buffer,
                        swapchain_target: swapchain,
                        generated_swapchain_final_layout,
                    },
                    alpha_inputs.token_recording,
                    token_recording_step_index,
                    alpha_inputs.producer_targets,
                    alpha_inputs.producer_pipelines,
                    alpha_inputs.generated_commands,
                    alpha_inputs.generated_pipelines,
                    alpha_inputs.rt_method8_commands,
                    alpha_inputs.resource_binds,
                )?;
                if step.target == Some(SceneGraphTarget::Swapchain) {
                    swapchain_current_layout = generated_swapchain_final_layout;
                    swapchain_was_written = true;
                }
                alpha_steps.push(step);
            }
            NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime => {
                let object = schedule
                    .steps
                    .get(block.step_index_start)
                    .ok_or_else(|| {
                        format!(
                            "scene layer compositor ObjectFinal block {} has no schedule step",
                            block.block_index
                        )
                    })?
                    .object;
                native_vulkan_record_scene_effect_object_final_command_stream(
                    frame_resources,
                    &effect_inputs.context,
                    effect_inputs.graph,
                    effect_inputs.command_streams,
                    object,
                    &mut written_effect_targets,
                    &mut effect_commands,
                )?;
            }
            NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired => {
                return Ok(None);
            }
        }
    }

    let passes = if let (Some(graph_pass), Some(execution_pass), Some(target_scope)) =
        (graph_pass, execution_pass, first_mesh_target_scope)
    {
        vec![NativeVulkanSceneGraphPassCommandPlan {
            target: SceneGraphTarget::Swapchain,
            target_scope,
            pass: NativeVulkanSceneMeshPassCommandPlan {
                name: graph_pass.name.as_str(),
                input: graph_pass.input,
                output: graph_pass.output,
                draw_index_start: execution_pass.draw_index_start,
                draw_index_end: execution_pass.draw_index_end,
                draw_count: counts.indexed_draw_count,
                pipeline_bind_count: counts.pipeline_bind_count,
                resource_heap_bind_count: counts.resource_heap_bind_count,
                indexed_draw_count: counts.indexed_draw_count,
                commands,
            },
        }]
    } else {
        Vec::new()
    };
    let mesh_frame = NativeVulkanSceneGraphFrameCommandPlan {
        pass_count: passes.len(),
        target_barrier_count: 0,
        target_format_count: context.target_formats.target_format_count(),
        passes,
        target_barriers: Vec::new(),
        command_order: [
            "resolve_scene_graph_target_formats",
            "record_layer_compositor_mesh_block_target",
            "record_mesh_and_alpha_draw_spans_from_compositor_blocks",
            "record_scene_graph_target_barriers",
        ],
    };
    let alpha_mask_token_draw_list =
        NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan::from_steps(alpha_steps);
    let effect_counts = native_vulkan_count_scene_effect_runtime_commands(&effect_commands);
    let command_blocks =
        NativeVulkanSceneLayerCompositorCommandBlockRecordPlan::from_recorded_parts(
            schedule,
            mesh_target_scope_count,
            effect_counts,
            alpha_mask_token_draw_list,
        );
    Ok(Some(
        NativeVulkanSceneLayerCompositorCommandBlockRecordOutput {
            mesh_frame,
            command_blocks,
            effect_commands,
        },
    ))
}

impl NativeVulkanSceneLayerCompositorCommandBlockRecordPlan {
    fn empty() -> Self {
        Self {
            block_count: 0,
            mesh_span_block_count: 0,
            object_final_effect_block_count: 0,
            alpha_mask_token_block_count: 0,
            no_draw_marker_block_count: 0,
            mesh_target_scope_count: 0,
            object_final_effect_target_scope_count: 0,
            alpha_mask_target_scope_count: 0,
            object_final_effect_recorded_command_count: 0,
            object_final_effect_material_pass_count: 0,
            object_final_effect_copy_command_count: 0,
            object_final_effect_swap_command_count: 0,
            object_final_effect_target_transition_count: 0,
            object_final_effect_target_initial_clear_count: 0,
            object_final_effect_fullscreen_draw_count: 0,
            object_final_effect_copy_image_count: 0,
            alpha_mask_recorded_step_count: 0,
            alpha_mask_token_draw_list:
                NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan::empty(),
            command_order: command_block_record_order(),
        }
    }

    fn from_recorded_parts(
        schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
        mesh_target_scope_count: usize,
        effect_counts: NativeVulkanSceneEffectRuntimeCommandCounts,
        alpha_mask_token_draw_list: NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan,
    ) -> Self {
        Self {
            block_count: schedule.recording_block_count,
            mesh_span_block_count: schedule.mesh_graph_draw_span_block_count,
            object_final_effect_block_count: schedule.object_final_producer_command_count,
            alpha_mask_token_block_count: schedule.alpha_mask_token_recording_block_count,
            no_draw_marker_block_count: schedule.no_draw_marker_block_count,
            mesh_target_scope_count,
            object_final_effect_target_scope_count: effect_counts.target_scope_count,
            alpha_mask_target_scope_count: alpha_mask_token_draw_list.target_scope_count,
            object_final_effect_recorded_command_count: effect_counts.command_count,
            object_final_effect_material_pass_count: effect_counts.material_pass_count,
            object_final_effect_copy_command_count: effect_counts.copy_command_count,
            object_final_effect_swap_command_count: effect_counts.swap_command_count,
            object_final_effect_target_transition_count: effect_counts.target_transition_count,
            object_final_effect_target_initial_clear_count: effect_counts
                .target_initial_clear_count,
            object_final_effect_fullscreen_draw_count: effect_counts.fullscreen_draw_count,
            object_final_effect_copy_image_count: effect_counts.copy_image_count,
            alpha_mask_recorded_step_count: alpha_mask_token_draw_list.scheduled_step_count,
            alpha_mask_token_draw_list,
            command_order: command_block_record_order(),
        }
    }
}

fn command_block_record_order() -> [&'static str; 7] {
    [
        "read_layer_compositor_recording_blocks",
        "record_mesh_spans_in_schedule_order",
        "record_object_final_effects_in_schedule_order",
        "preserve_no_draw_layer_markers",
        "record_alpha_mask_token_steps_in_schedule_order",
        "keep_descriptor_heap_binding_model",
        "leave_effect_and_clear_blocks_for_full_compositor_recorder",
    ]
}

fn schedule_is_command_block_recordable(
    schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
) -> bool {
    schedule.recording_block_count
        == schedule
            .mesh_graph_draw_span_block_count
            .saturating_add(schedule.no_draw_marker_block_count)
            .saturating_add(schedule.object_final_producer_command_count)
            .saturating_add(schedule.alpha_mask_token_recording_block_count)
        && schedule.recording_blocks.iter().all(|block| {
            matches!(
                block.kind,
                    NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan
                        | NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker
                        | NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime
                        | NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep
            )
        })
}

fn last_swapchain_writer_block_index(
    schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
    alpha_inputs: &NativeVulkanSceneLayerCompositorAlphaTokenBlockInputs<'_>,
) -> Result<Option<usize>, String> {
    let mut last = None;
    for block in &schedule.recording_blocks {
        match block.kind {
            NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan => {
                last = Some(block.block_index);
            }
            NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep => {
                if alpha_token_block_writes_swapchain(block.token_recording_step_index, alpha_inputs)?
                {
                    last = Some(block.block_index);
                }
            }
            NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker
            | NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime => {}
            NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired => {
                return Ok(None);
            }
        }
    }
    Ok(last)
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_layer_compositor_effect_blocks_are_recordable(
    schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
    graph: &SceneEffectPassGraphPlan,
    command_streams: &NativeVulkanSceneEffectObjectCommandStreamPlan,
) -> bool {
    if schedule.object_final_producer_command_count == 0 {
        return false;
    }
    if command_streams.command_count
        != graph
            .material_pass_count
            .saturating_add(graph.copy_command_count)
            .saturating_add(graph.swap_command_count)
    {
        return false;
    }
    if command_streams.object_final_pass_count != schedule.object_final_producer_command_count {
        return false;
    }
    let objects = schedule
        .steps
        .iter()
        .filter(|step| {
            step.scheduled_kind
                == super::layer_compositor_scheduler::NativeVulkanSceneLayerCompositorScheduledKind::ObjectFinalProducerEffectRuntime
        })
        .map(|step| step.object)
        .collect::<Vec<_>>();
    if objects.len() != schedule.object_final_producer_command_count {
        return false;
    }
    let unique_objects = objects
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if unique_objects.len() != objects.len() {
        return false;
    }
    if command_streams.stream_count != objects.len() {
        return false;
    }
    command_streams
        .streams
        .iter()
        .map(|stream| stream.object)
        .eq(objects)
        && command_streams
            .streams
            .iter()
            .all(|stream| stream.object_final_pass_count == 1)
}

fn alpha_token_block_writes_swapchain(
    token_recording_step_index: Option<usize>,
    alpha_inputs: &NativeVulkanSceneLayerCompositorAlphaTokenBlockInputs<'_>,
) -> Result<bool, String> {
    let Some(step_index) = token_recording_step_index else {
        return Err(
            "scene layer compositor alpha-mask block has no token recording step index".to_owned(),
        );
    };
    let step = alpha_inputs
        .token_recording
        .steps
        .get(step_index)
        .ok_or_else(|| {
            format!(
                "scene layer compositor alpha-mask block token recording step {step_index} is outside recording plan"
            )
        })?;
    if step.recording_kind
        != NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::GeneratedClippingTargetRtMethod8
    {
        return Ok(false);
    }
    let command = alpha_inputs
        .generated_commands
        .commands
        .iter()
        .find(|command| {
            command.command_index == step.command_index && command.object == step.object
        })
        .ok_or_else(|| {
            format!(
                "scene layer compositor generated CLIPPINGTARGET command {} has no runtime command",
                step.command_index
            )
        })?;
    Ok(command.color_target == SceneGraphTarget::Swapchain)
}

fn swapchain_final_layout(
    block_index: usize,
    last_swapchain_writer: Option<usize>,
) -> vk::ImageLayout {
    if Some(block_index) == last_swapchain_writer {
        vk::ImageLayout::PRESENT_SRC_KHR
    } else {
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
    }
}

fn mesh_blocks_cover_execution_pass(
    schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
    execution_pass: &SceneGraphExecutionPass,
) -> Result<bool, String> {
    let mut next_draw_index = execution_pass.draw_index_start;
    for block in &schedule.recording_blocks {
        match block.kind {
            NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan => {
                if block.graph_pass_index != Some(execution_pass.pass_index) {
                    return Ok(false);
                }
                let draw_start = block.graph_draw_index_start.ok_or_else(|| {
                    format!(
                        "scene layer compositor command-block recorder block {} has no draw start",
                        block.block_index
                    )
                })?;
                let draw_end = block.graph_draw_index_end.ok_or_else(|| {
                    format!(
                        "scene layer compositor command-block recorder block {} has no draw end",
                        block.block_index
                    )
                })?;
                if draw_start != next_draw_index || draw_end <= draw_start {
                    return Ok(false);
                }
                next_draw_index = draw_end;
            }
            NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker
            | NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep
            | NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime => {}
            NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired => {
                return Ok(false);
            }
        }
    }
    Ok(next_draw_index == execution_pass.draw_index_end)
}

fn empty_mesh_block_frame_plan(
    target_format_count: usize,
) -> NativeVulkanSceneGraphFrameCommandPlan<'static> {
    NativeVulkanSceneGraphFrameCommandPlan {
        pass_count: 0,
        target_barrier_count: 0,
        target_format_count,
        passes: Vec::new(),
        target_barriers: Vec::new(),
        command_order: [
            "resolve_scene_graph_target_formats",
            "record_layer_compositor_mesh_block_target",
            "record_mesh_draw_spans_from_compositor_blocks",
            "record_scene_graph_target_barriers",
        ],
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::engine::scene_engine::{
        SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
        SceneEffectPassGraphCopy, SceneEffectPassGraphMaterialPass, SceneEffectPassGraphOutput,
        SceneEffectPassGraphSwap, SceneLayerCompositorEntry, SceneLayerCompositorOperation,
        SceneLayerCompositorRoute, SceneObjectId, we::WeEffectKind,
    };

    use super::super::layer_compositor_scheduler::{
        NativeVulkanSceneLayerCompositorRecordingBlock,
        NativeVulkanSceneLayerCompositorSchedulePlan, NativeVulkanSceneLayerCompositorScheduleStep,
        NativeVulkanSceneLayerCompositorScheduledKind,
    };
    use super::*;

    #[test]
    fn mesh_block_recorder_accepts_mesh_spans_and_no_draw_markers() {
        let schedule = schedule(vec![
            block(
                0,
                NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan,
            ),
            block(
                1,
                NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker,
            ),
            block(
                2,
                NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan,
            ),
        ]);

        assert!(schedule_is_command_block_recordable(&schedule));
    }

    #[test]
    fn mesh_block_recorder_accepts_alpha_mask_token_blocks() {
        let schedule = schedule(vec![
            block(
                0,
                NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan,
            ),
            block(
                1,
                NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep,
            ),
        ]);

        assert!(schedule_is_command_block_recordable(&schedule));
    }

    #[test]
    fn mesh_block_recorder_accepts_object_final_effect_blocks() {
        let effect = schedule(vec![block(
            0,
            NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime,
        )]);

        assert!(schedule_is_command_block_recordable(&effect));
    }

    #[test]
    fn mesh_block_recorder_rejects_clear_blocks() {
        let clear = schedule(vec![block(
            0,
            NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired,
        )]);

        assert!(!schedule_is_command_block_recordable(&clear));
    }

    #[test]
    fn object_final_effect_blocks_require_object_final_only_graph() {
        let object = SceneObjectId(7);
        let schedule = object_final_schedule(object);
        let graph = object_final_graph(object, 0);
        let command_streams =
            super::super::effect_executor::native_vulkan_plan_scene_effect_object_command_streams(
                &graph,
            )
            .expect("object stream plan");

        assert!(
            native_vulkan_scene_layer_compositor_effect_blocks_are_recordable(
                &schedule,
                &graph,
                &command_streams
            )
        );

        let graph_with_copy_and_swap = object_final_graph_with_copy_and_swap(object);
        let command_streams =
            super::super::effect_executor::native_vulkan_plan_scene_effect_object_command_streams(
                &graph_with_copy_and_swap,
            )
            .expect("copy/swap object stream plan");
        assert!(
            native_vulkan_scene_layer_compositor_effect_blocks_are_recordable(
                &schedule,
                &graph_with_copy_and_swap,
                &command_streams
            )
        );

        let mismatched_graph = object_final_graph(SceneObjectId(8), 0);
        let command_streams =
            super::super::effect_executor::native_vulkan_plan_scene_effect_object_command_streams(
                &mismatched_graph,
            )
            .expect("mismatched object stream plan");
        assert!(
            !native_vulkan_scene_layer_compositor_effect_blocks_are_recordable(
                &schedule,
                &mismatched_graph,
                &command_streams
            )
        );
    }

    #[test]
    fn object_final_effect_blocks_reject_interleaved_object_streams() {
        let first = SceneObjectId(7);
        let second = SceneObjectId(8);
        let schedule = object_final_pair_schedule(first, second);
        let graph = SceneEffectPassGraphPlan {
            object_program_count: 2,
            material_pass_count: 3,
            passes: vec![
                effect_pass(first, 0, 0, SceneGraphTarget::NamedFbo(1)),
                effect_pass(second, 1, 1, SceneGraphTarget::ObjectFinal(second)),
                effect_pass(first, 2, 2, SceneGraphTarget::ObjectFinal(first)),
            ],
            ..SceneEffectPassGraphPlan::empty()
        };
        let command_streams =
            super::super::effect_executor::native_vulkan_plan_scene_effect_object_command_streams(
                &graph,
            )
            .expect("interleaved stream plan");

        assert!(
            !native_vulkan_scene_layer_compositor_effect_blocks_are_recordable(
                &schedule,
                &graph,
                &command_streams
            )
        );
    }

    #[test]
    fn object_final_effect_blocks_ignore_frames_without_object_final_producers() {
        let schedule = schedule(vec![block(
            0,
            NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan,
        )]);

        assert!(
            !native_vulkan_scene_layer_compositor_effect_blocks_are_recordable(
                &schedule,
                &SceneEffectPassGraphPlan::empty(),
                &super::super::effect_executor::native_vulkan_plan_scene_effect_object_command_streams(
                    &SceneEffectPassGraphPlan::empty(),
                )
                .expect("empty object stream plan")
            )
        );
    }

    #[test]
    fn command_block_record_plan_counts_object_final_effect_stream_shapes() {
        let schedule = object_final_schedule(SceneObjectId(7));
        let plan = NativeVulkanSceneLayerCompositorCommandBlockRecordPlan::from_recorded_parts(
            &schedule,
            0,
            NativeVulkanSceneEffectRuntimeCommandCounts {
                command_count: 4,
                material_pass_count: 2,
                copy_command_count: 1,
                swap_command_count: 1,
                target_transition_count: 3,
                target_initial_clear_count: 1,
                target_scope_count: 2,
                fullscreen_draw_count: 2,
                copy_image_count: 1,
            },
            NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan::empty(),
        );

        assert_eq!(plan.object_final_effect_block_count, 1);
        assert_eq!(plan.object_final_effect_recorded_command_count, 4);
        assert_eq!(plan.object_final_effect_material_pass_count, 2);
        assert_eq!(plan.object_final_effect_copy_command_count, 1);
        assert_eq!(plan.object_final_effect_swap_command_count, 1);
        assert_eq!(plan.object_final_effect_target_scope_count, 2);
        assert_eq!(plan.object_final_effect_target_transition_count, 3);
        assert_eq!(plan.object_final_effect_target_initial_clear_count, 1);
        assert_eq!(plan.object_final_effect_fullscreen_draw_count, 2);
        assert_eq!(plan.object_final_effect_copy_image_count, 1);
    }

    #[test]
    fn mesh_block_recorder_requires_contiguous_pass_coverage() {
        let mut schedule = schedule(vec![
            block(
                0,
                NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan,
            ),
            block(
                1,
                NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker,
            ),
            block(
                2,
                NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan,
            ),
        ]);
        schedule.recording_blocks[0].graph_draw_index_start = Some(0);
        schedule.recording_blocks[0].graph_draw_index_end = Some(1);
        schedule.recording_blocks[2].graph_draw_index_start = Some(1);
        schedule.recording_blocks[2].graph_draw_index_end = Some(2);

        assert!(
            mesh_blocks_cover_execution_pass(&schedule, &execution_pass(0, 2))
                .expect("coverage check")
        );
        schedule.recording_blocks[2].graph_draw_index_start = Some(3);
        schedule.recording_blocks[2].graph_draw_index_end = Some(4);
        assert!(
            !mesh_blocks_cover_execution_pass(&schedule, &execution_pass(0, 4)).expect("gap check")
        );
    }

    fn schedule(
        recording_blocks: Vec<NativeVulkanSceneLayerCompositorRecordingBlock>,
    ) -> NativeVulkanSceneLayerCompositorSchedulePlan {
        NativeVulkanSceneLayerCompositorSchedulePlan {
            layer_count: 1,
            command_count: recording_blocks.len(),
            direct_mesh_graph_command_count: recording_blocks
                .iter()
                .filter(|block| {
                    block.kind
                        == NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan
                })
                .count(),
            object_final_producer_command_count: recording_blocks
                .iter()
                .filter(|block| {
                    block.kind
                        == NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime
                })
                .count(),
            object_final_composite_command_count: 0,
            alpha_mask_token_draw_list_command_count: recording_blocks
                .iter()
                .filter(|block| {
                    block.kind
                        == NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep
                })
                .count(),
            token_program_no_draw_count: 0,
            clear_prep_recorder_required_count: 0,
            recording_block_count: recording_blocks.len(),
            mesh_graph_draw_span_block_count: recording_blocks
                .iter()
                .filter(|block| {
                    block.kind
                        == NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan
                })
                .count(),
            alpha_mask_token_recording_block_count: recording_blocks
                .iter()
                .filter(|block| {
                    block.kind
                        == NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep
                })
                .count(),
            no_draw_marker_block_count: recording_blocks
                .iter()
                .filter(|block| {
                    block.kind == NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker
                })
                .count(),
            all_alpha_mask_commands_recordable: true,
            steps: Vec::new(),
            recording_blocks,
            command_order: [
                "read_scene_layer_compositor_order",
                "join_direct_layers_to_mesh_graph_draws",
                "join_object_final_producers_to_effect_runtime",
                "join_object_final_composites_to_graph_passes",
                "join_tokenized_commands_to_alpha_mask_token_recording",
                "coalesce_consecutive_mesh_graph_draws_into_recording_blocks",
                "reject_missing_alpha_mask_token_draw_list_steps",
                "emit_schedule_for_present_frame_recorder",
            ],
        }
    }

    fn object_final_schedule(
        object: SceneObjectId,
    ) -> NativeVulkanSceneLayerCompositorSchedulePlan {
        let mut plan = schedule(vec![block(
            0,
            NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime,
        )]);
        plan.steps = vec![NativeVulkanSceneLayerCompositorScheduleStep {
            global_command_index: 0,
            layer_index: 0,
            layer_command_index: 0,
            object,
            route: SceneLayerCompositorRoute::ObjectFinalMeshComposite,
            entry: SceneLayerCompositorEntry::NormalRenderEntry32,
            operation: SceneLayerCompositorOperation::NormalRender,
            scheduled_kind:
                NativeVulkanSceneLayerCompositorScheduledKind::ObjectFinalProducerEffectRuntime,
            graph_pass_index: None,
            graph_draw_index: None,
            token_recording_step_index: None,
            command_order: vec!["test_object_final_producer"],
        }];
        plan
    }

    fn object_final_pair_schedule(
        first: SceneObjectId,
        second: SceneObjectId,
    ) -> NativeVulkanSceneLayerCompositorSchedulePlan {
        let mut plan = schedule(vec![
            block(
                0,
                NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime,
            ),
            block(
                1,
                NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime,
            ),
        ]);
        plan.steps = vec![object_final_step(0, first), object_final_step(1, second)];
        plan
    }

    fn object_final_step(
        global_command_index: usize,
        object: SceneObjectId,
    ) -> NativeVulkanSceneLayerCompositorScheduleStep {
        NativeVulkanSceneLayerCompositorScheduleStep {
            global_command_index,
            layer_index: global_command_index,
            layer_command_index: 0,
            object,
            route: SceneLayerCompositorRoute::ObjectFinalMeshComposite,
            entry: SceneLayerCompositorEntry::NormalRenderEntry32,
            operation: SceneLayerCompositorOperation::NormalRender,
            scheduled_kind:
                NativeVulkanSceneLayerCompositorScheduledKind::ObjectFinalProducerEffectRuntime,
            graph_pass_index: None,
            graph_draw_index: None,
            token_recording_step_index: None,
            command_order: vec!["test_object_final_producer"],
        }
    }

    fn object_final_graph(
        object: SceneObjectId,
        graph_command_index: usize,
    ) -> SceneEffectPassGraphPlan {
        SceneEffectPassGraphPlan {
            object_program_count: 1,
            material_pass_count: 1,
            passes: vec![effect_pass(
                object,
                graph_command_index,
                graph_command_index,
                SceneGraphTarget::ObjectFinal(object),
            )],
            ..SceneEffectPassGraphPlan::empty()
        }
    }

    fn object_final_graph_with_copy_and_swap(object: SceneObjectId) -> SceneEffectPassGraphPlan {
        SceneEffectPassGraphPlan {
            object_program_count: 1,
            material_pass_count: 2,
            copy_command_count: 1,
            swap_command_count: 1,
            passes: vec![
                effect_pass(object, 0, 0, SceneGraphTarget::NamedFbo(1)),
                effect_pass(object, 3, 3, SceneGraphTarget::ObjectFinal(object)),
            ],
            copies: vec![SceneEffectPassGraphCopy {
                graph_command_index: 1,
                object,
                program_index: 0,
                pass_index: 1,
                source: SceneGraphTarget::NamedFbo(1),
                target: SceneGraphTarget::NamedFbo(2),
            }],
            swaps: vec![SceneEffectPassGraphSwap {
                graph_command_index: 2,
                object,
                program_index: 0,
                pass_index: 2,
                a: SceneGraphTarget::NamedFbo(1),
                b: SceneGraphTarget::NamedFbo(2),
            }],
            ..SceneEffectPassGraphPlan::empty()
        }
    }

    fn effect_pass(
        object: SceneObjectId,
        graph_command_index: usize,
        graph_pass_index: usize,
        output: SceneGraphTarget,
    ) -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
            graph_command_index,
            graph_pass_index,
            object,
            program_index: 0,
            pass_index: graph_command_index,
            effect_file: "effects/test/effect.json".to_owned(),
            effect: WeEffectKind::Unknown,
            shader: Some("effects/iris".to_owned()),
            source: None,
            input_bindings: Vec::new(),
            output: match output {
                SceneGraphTarget::ObjectFinal(object) => {
                    SceneEffectPassGraphOutput::ObjectFinal(object)
                }
                target => SceneEffectPassGraphOutput::GraphTarget(target),
            },
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
            texture_resources: Vec::new(),
            combos: BTreeMap::new(),
            constants: BTreeMap::new(),
        }
    }

    fn block(
        block_index: usize,
        kind: NativeVulkanSceneLayerCompositorRecordingBlockKind,
    ) -> NativeVulkanSceneLayerCompositorRecordingBlock {
        NativeVulkanSceneLayerCompositorRecordingBlock {
            block_index,
            step_index_start: block_index,
            step_index_end: block_index.saturating_add(1),
            command_count: 1,
            kind,
            graph_pass_index: Some(0),
            graph_draw_index_start: Some(block_index),
            graph_draw_index_end: Some(block_index.saturating_add(1)),
            token_recording_step_index: None,
            command_order: Vec::new(),
        }
    }

    fn execution_pass(draw_index_start: usize, draw_index_end: usize) -> SceneGraphExecutionPass {
        let draw_count = draw_index_end.saturating_sub(draw_index_start);
        SceneGraphExecutionPass {
            pass_index: 0,
            name: "mesh-main".to_owned(),
            input: None,
            output: SceneGraphTarget::Swapchain,
            draw_index_start,
            draw_index_end,
            draw_count,
            indexed_graphics_draw_count: draw_count,
            non_indexed_draw_count: 0,
            indexed_mesh_graphics_draw_count: draw_count,
            quad_draw_count: 0,
            particle_emitter_draw_count: 0,
            target_reads: Vec::new(),
            target_writes: vec![SceneGraphTarget::Swapchain],
        }
    }
}

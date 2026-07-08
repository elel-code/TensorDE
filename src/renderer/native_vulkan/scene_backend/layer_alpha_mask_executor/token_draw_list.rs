//! Token-ordered draw-list recording for WE layer alpha-mask work.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{SceneGraphTarget, SceneObjectId};
use crate::renderer::native_vulkan::NativeVulkanClearColor;
use crate::renderer::native_vulkan::scene_backend::frame_resources::NativeVulkanSceneFrameResources;
use crate::renderer::native_vulkan::scene_backend::render_target::{
    NativeVulkanSceneOffscreenRenderTarget, NativeVulkanSceneRenderTarget,
    NativeVulkanSceneRenderTargetScopePlan, NativeVulkanSceneSwapchainRenderTarget,
    native_vulkan_record_scene_render_target_begin, native_vulkan_record_scene_render_target_end,
};
use crate::renderer::native_vulkan::scene_backend::target_access::{
    NativeVulkanSceneTargetTransitionPlan, native_vulkan_record_scene_target_transition,
};

use super::consumer_command::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan;
use super::consumer_pipeline::{
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
};
use super::copy_back_recording::{
    NativeVulkanSceneLayerAlphaMaskCopyBackGraphRecordPlan,
    native_vulkan_record_scene_layer_alpha_mask_copy_back_graph_node_for_command,
};
use super::producer_pipeline::{
    NativeVulkanSceneLayerAlphaMaskProducerPipelineBindingPlan,
    NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
};
use super::producer_target_graph::{
    NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    NativeVulkanSceneLayerAlphaMaskProducerTargetScopePlan,
};
use super::resource_binds::NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan;
use super::rt_method8_command::{
    NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand,
    NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
    NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind,
    NativeVulkanSceneLayerAlphaMaskRtMethod8RecordedDrawCommandPlan,
    native_vulkan_record_scene_layer_alpha_mask_rt_method8_indexed_draw_command,
};
use super::token_recording::{
    NativeVulkanSceneLayerAlphaMaskTokenRecordingKind,
    NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
    NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
};

#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordContext<
    'a,
> {
    pub device: &'a Device,
    pub command_buffer: vk::CommandBuffer,
    pub swapchain_target: NativeVulkanSceneSwapchainRenderTarget,
    pub generated_swapchain_final_layout: vk::ImageLayout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan
{
    pub scheduled_step_count: usize,
    pub no_draw_step_count: usize,
    pub producer_step_count: usize,
    pub copy_back_step_count: usize,
    pub generated_consumer_step_count: usize,
    pub target_scope_count: usize,
    pub source_shader_read_transition_count: usize,
    pub rt_method8_recorded_command_count: usize,
    pub copy_back_graph_node_count: usize,
    pub rt_method8_indexed_draw_count: usize,
    pub copy_back_direct_draw_count: usize,
    pub steps: Vec<NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep>,
    pub command_order: [&'static str; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep
{
    pub command_index: usize,
    pub object: SceneObjectId,
    pub recording_kind: NativeVulkanSceneLayerAlphaMaskTokenRecordingKind,
    pub target: Option<SceneGraphTarget>,
    pub target_scope: Option<NativeVulkanSceneRenderTargetScopePlan>,
    pub source_transition: Option<NativeVulkanSceneTargetTransitionPlan>,
    pub rt_method8_draw: Option<NativeVulkanSceneLayerAlphaMaskRtMethod8RecordedDrawCommandPlan>,
    pub copy_back_graph_node: Option<NativeVulkanSceneLayerAlphaMaskCopyBackGraphRecordPlan>,
    pub command_order: Vec<&'static str>,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_layer_alpha_mask_token_draw_list(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordContext<'_>,
    token_recording: &NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
    producer_targets: &NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    producer_pipelines: &NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
    generated_commands: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    generated_pipelines: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    rt_method8_commands: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan, String> {
    if token_recording.scheduled_step_count == 0 {
        return Ok(NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan::empty());
    }
    if !token_recording.all_draw_steps_recordable {
        return Err(
            "scene layer alpha-mask token draw-list requires every draw step to be recordable"
                .to_owned(),
        );
    }

    let mut steps = Vec::with_capacity(token_recording.steps.len());
    for step_index in 0..token_recording.steps.len() {
        steps.push(
            native_vulkan_record_scene_layer_alpha_mask_token_draw_list_step(
                frame_resources,
                context,
                token_recording,
                step_index,
                producer_targets,
                producer_pipelines,
                generated_commands,
                generated_pipelines,
                rt_method8_commands,
                resource_binds,
            )?,
        );
    }

    Ok(NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan::from_steps(steps))
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_layer_alpha_mask_token_draw_list_step(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordContext<'_>,
    token_recording: &NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
    token_recording_step_index: usize,
    producer_targets: &NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    producer_pipelines: &NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
    generated_commands: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    generated_pipelines: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    rt_method8_commands: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep, String> {
    let step = token_recording
        .steps
        .get(token_recording_step_index)
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token draw-list step index {token_recording_step_index} is outside recording plan"
            )
        })?;
    record_token_recording_step(
        frame_resources,
        &context,
        step,
        producer_targets,
        producer_pipelines,
        generated_commands,
        generated_pipelines,
        rt_method8_commands,
        resource_binds,
    )
}

impl NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan {
    pub(in crate::renderer::native_vulkan) fn empty() -> Self {
        Self {
            scheduled_step_count: 0,
            no_draw_step_count: 0,
            producer_step_count: 0,
            copy_back_step_count: 0,
            generated_consumer_step_count: 0,
            target_scope_count: 0,
            source_shader_read_transition_count: 0,
            rt_method8_recorded_command_count: 0,
            copy_back_graph_node_count: 0,
            rt_method8_indexed_draw_count: 0,
            copy_back_direct_draw_count: 0,
            steps: Vec::new(),
            command_order: token_draw_list_command_order(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn from_steps(
        steps: Vec<NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep>,
    ) -> Self {
        let no_draw_step_count = steps
            .iter()
            .filter(|step| {
                step.recording_kind
                    == NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::TokenProgramNoDraw
            })
            .count();
        let producer_step_count = steps
            .iter()
            .filter(|step| {
                step.recording_kind
                    == NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::ClippingMaskImage4ProducerRtMethod8
            })
            .count();
        let copy_back_step_count = steps
            .iter()
            .filter(|step| {
                step.recording_kind
                    == NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::FlatTextureCopyBackGraphNode
            })
            .count();
        let generated_consumer_step_count = steps
            .iter()
            .filter(|step| {
                step.recording_kind
                    == NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::GeneratedClippingTargetRtMethod8
            })
            .count();
        let target_scope_count = steps
            .iter()
            .filter(|step| step.target_scope.is_some())
            .count()
            + steps
                .iter()
                .filter_map(|step| step.copy_back_graph_node.as_ref())
                .map(|copy_back| copy_back.target_scope_count)
                .sum::<usize>();
        let source_shader_read_transition_count = steps
            .iter()
            .filter(|step| step.source_transition.is_some())
            .count()
            + steps
                .iter()
                .filter_map(|step| step.copy_back_graph_node.as_ref())
                .map(|copy_back| copy_back.source_shader_read_transition_count)
                .sum::<usize>();
        let rt_method8_recorded_command_count = steps
            .iter()
            .filter(|step| step.rt_method8_draw.is_some())
            .count();
        let copy_back_graph_node_count = steps
            .iter()
            .filter(|step| step.copy_back_graph_node.is_some())
            .count();
        let rt_method8_indexed_draw_count = steps
            .iter()
            .filter_map(|step| step.rt_method8_draw.as_ref())
            .map(|draw| draw.indexed_draw_count)
            .sum::<usize>();
        let copy_back_direct_draw_count = steps
            .iter()
            .filter_map(|step| step.copy_back_graph_node.as_ref())
            .map(|copy_back| copy_back.command_count)
            .sum::<usize>();
        Self {
            scheduled_step_count: steps.len(),
            no_draw_step_count,
            producer_step_count,
            copy_back_step_count,
            generated_consumer_step_count,
            target_scope_count,
            source_shader_read_transition_count,
            rt_method8_recorded_command_count,
            copy_back_graph_node_count,
            rt_method8_indexed_draw_count,
            copy_back_direct_draw_count,
            steps,
            command_order: token_draw_list_command_order(),
        }
    }
}

fn record_token_recording_step(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordContext<'_>,
    step: &NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
    producer_targets: &NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    producer_pipelines: &NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
    generated_commands: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    generated_pipelines: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    rt_method8_commands: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep, String> {
    match step.recording_kind {
        NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::TokenProgramNoDraw => {
            Ok(token_program_step(step))
        }
        NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::ClippingMaskImage4ProducerRtMethod8 => {
            record_producer_step(
                frame_resources,
                context,
                step,
                producer_targets,
                producer_pipelines,
                rt_method8_commands,
            )
        }
        NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::FlatTextureCopyBackGraphNode => {
            record_copy_back_step(frame_resources, context, step, resource_binds)
        }
        NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::GeneratedClippingTargetRtMethod8 => {
            record_generated_consumer_step(
                frame_resources,
                context,
                step,
                generated_commands,
                generated_pipelines,
                rt_method8_commands,
            )
        }
    }
}

fn token_program_step(
    step: &NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
) -> NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep {
    NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep {
        command_index: step.command_index,
        object: step.object,
        recording_kind: step.recording_kind,
        target: None,
        target_scope: None,
        source_transition: None,
        rt_method8_draw: None,
        copy_back_graph_node: None,
        command_order: vec![
            "preserve_token_program_position",
            "skip_vulkan_draw_for_token_program_marker",
        ],
    }
}

fn record_producer_step(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordContext<'_>,
    step: &NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
    producer_targets: &NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    producer_pipelines: &NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
    rt_method8_commands: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep, String> {
    let target_scope = producer_target_scope(step, producer_targets)?;
    let command = rt_method8_command(
        step,
        rt_method8_commands,
        NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::ClippingMaskImage4Producer,
    )?;
    let pipeline = producer_pipeline_binding(step, producer_pipelines, command.heap_bind_index)?;
    let render_target = offscreen_render_target(
        frame_resources,
        target_scope.target,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    )?;
    let clear_color = target_scope
        .clear_first
        .then_some(transparent_clear_color());
    let target_scope_plan = native_vulkan_record_scene_render_target_begin(
        context.device,
        context.command_buffer,
        render_target,
        clear_color,
    )?;
    frame_resources.mark_offscreen_target_layout(
        target_scope.target,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    )?;
    let recorded =
        record_rt_method8_draw(frame_resources, context, command, &pipeline.cache_key())?;
    native_vulkan_record_scene_render_target_end(
        context.device,
        context.command_buffer,
        render_target,
        clear_color,
    )?;
    frame_resources.mark_offscreen_target_layout(
        target_scope.target,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    )?;

    Ok(NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep {
        command_index: step.command_index,
        object: step.object,
        recording_kind: step.recording_kind,
        target: Some(target_scope.target),
        target_scope: Some(target_scope_plan),
        source_transition: None,
        rt_method8_draw: Some(recorded),
        copy_back_graph_node: None,
        command_order: vec![
            "resolve_clippingmaskimage4_producer_target_scope",
            "cmd_begin_r8_alpha_mask_target_scope",
            "record_rt_method8_producer_indexed_draw",
            "cmd_end_r8_alpha_mask_target_scope",
        ],
    })
}

fn record_copy_back_step(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordContext<'_>,
    step: &NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep, String> {
    let graph_node = native_vulkan_record_scene_layer_alpha_mask_copy_back_graph_node_for_command(
        frame_resources,
        context.device,
        context.command_buffer,
        resource_binds,
        step.command_index,
    )?;
    Ok(NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep {
        command_index: step.command_index,
        object: step.object,
        recording_kind: step.recording_kind,
        target: Some(graph_node.target_graph.target),
        target_scope: None,
        source_transition: None,
        rt_method8_draw: None,
        copy_back_graph_node: Some(graph_node),
        command_order: vec![
            "require_intermediate_alpha_mask_ready",
            "record_flattexture_copy_back_graph_node_for_token_command",
        ],
    })
}

fn record_generated_consumer_step(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordContext<'_>,
    step: &NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
    generated_commands: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    generated_pipelines: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    rt_method8_commands: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep, String> {
    let generated = generated_commands
        .commands
        .iter()
        .find(|command| command.command_index == step.command_index)
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token draw-list command {} has no generated CLIPPINGTARGET command",
                step.command_index
            )
        })?;
    let command = rt_method8_command(
        step,
        rt_method8_commands,
        NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::GeneratedClippingTargetConsumer,
    )?;
    let pipeline =
        generated_pipeline_binding(step, generated_pipelines, generated.heap_bind_index)?;
    let source_transition = native_vulkan_record_scene_target_transition(
        frame_resources,
        context.device,
        context.command_buffer,
        generated.source_mask,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        "alpha-mask-generated-clippingtarget-source-sampled-read",
    )?;
    let generated_final_layout = if generated.color_target == SceneGraphTarget::Swapchain {
        context.generated_swapchain_final_layout
    } else {
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
    };
    let render_target = render_target_for_scene_target(
        frame_resources,
        context,
        generated.color_target,
        generated_final_layout,
    )?;
    let target_scope_plan = native_vulkan_record_scene_render_target_begin(
        context.device,
        context.command_buffer,
        render_target,
        None,
    )?;
    mark_render_target_layout(
        frame_resources,
        generated.color_target,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    )?;
    let recorded =
        record_rt_method8_draw(frame_resources, context, command, &pipeline.cache_key())?;
    native_vulkan_record_scene_render_target_end(
        context.device,
        context.command_buffer,
        render_target,
        None,
    )?;
    mark_render_target_layout(
        frame_resources,
        generated.color_target,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    )?;

    Ok(NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep {
        command_index: step.command_index,
        object: step.object,
        recording_kind: step.recording_kind,
        target: Some(generated.color_target),
        target_scope: Some(target_scope_plan),
        source_transition,
        rt_method8_draw: Some(recorded),
        copy_back_graph_node: None,
        command_order: vec![
            "transition_full_alpha_mask_to_shader_read",
            "cmd_begin_generated_clippingtarget_color_scope",
            "record_rt_method8_generated_consumer_indexed_draw",
            "cmd_end_generated_clippingtarget_color_scope",
        ],
    })
}

fn record_rt_method8_draw(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordContext<'_>,
    command: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand,
    cache_key: &crate::renderer::native_vulkan::scene_backend::pipeline::NativeVulkanScenePipelineCacheKey,
) -> Result<NativeVulkanSceneLayerAlphaMaskRtMethod8RecordedDrawCommandPlan, String> {
    let vk_pipeline = frame_resources
        .cached_mesh_pipeline(cache_key)
        .map_err(|err| {
            format!(
                "{err}; scene layer alpha-mask token draw-list command {} requires warmed pipeline before recording",
                command.command_index
            )
        })?
        .pipeline;
    let bind_info =
        frame_resources.layer_alpha_mask_resource_heap_bind_info(command.heap_bind_index)?;
    let geometry = frame_resources
        .layer_alpha_mask_rt_method8_mdlv_geometry_buffers(command.geometry)
        .map_err(|err| {
            format!(
                "{err}; scene layer alpha-mask token draw-list command {} requires retained [layer+0x490] geometry buffers",
                command.command_index
            )
        })?;
    let slices = command
        .slices
        .iter()
        .map(|slice| {
            frame_resources
                .layer_alpha_mask_rt_method8_mdlv_index_slice_buffers(slice.slice)
                .map_err(|err| {
                    format!(
                        "{err}; scene layer alpha-mask token draw-list command {} requires retained R16 index slice {:?}",
                        command.command_index, slice.slice
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    native_vulkan_record_scene_layer_alpha_mask_rt_method8_indexed_draw_command(
        context.device,
        context.command_buffer,
        command,
        vk_pipeline,
        &bind_info,
        geometry,
        &slices,
    )
}

fn producer_target_scope<'a>(
    step: &NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
    producer_targets: &'a NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
) -> Result<&'a NativeVulkanSceneLayerAlphaMaskProducerTargetScopePlan, String> {
    let target_scope_index = step.producer_target_scope_index.ok_or_else(|| {
        format!(
            "scene layer alpha-mask token draw-list command {} has no producer target scope index",
            step.command_index
        )
    })?;
    producer_targets
        .scopes
        .iter()
        .find(|scope| {
            scope.target_scope_index == target_scope_index
                && scope.command_index == step.command_index
                && scope.object == step.object
        })
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token draw-list command {} cannot resolve producer target scope {}",
                step.command_index, target_scope_index
            )
        })
}

fn rt_method8_command<'a>(
    step: &NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
    commands: &'a NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
    kind: NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind,
) -> Result<&'a NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand, String> {
    commands
        .commands
        .iter()
        .find(|command| {
            command.command_index == step.command_index
                && command.object == step.object
                && command.kind == kind
        })
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token draw-list command {} cannot resolve {:?} RT method [8] command",
                step.command_index, kind
            )
        })
}

fn producer_pipeline_binding<'a>(
    step: &NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
    pipelines: &'a NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
    heap_bind_index: usize,
) -> Result<&'a NativeVulkanSceneLayerAlphaMaskProducerPipelineBindingPlan, String> {
    pipelines
        .bindings
        .iter()
        .find(|pipeline| {
            pipeline.command_index == step.command_index
                && pipeline.object == step.object
                && pipeline.heap_bind_index == heap_bind_index
        })
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token draw-list command {} cannot resolve clippingmaskimage4 pipeline binding for heap bind {}",
                step.command_index, heap_bind_index
            )
        })
}

fn generated_pipeline_binding<'a>(
    step: &NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
    pipelines: &'a NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    heap_bind_index: usize,
) -> Result<&'a NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan, String> {
    pipelines
        .bindings
        .iter()
        .find(|pipeline| {
            pipeline.command_index == step.command_index
                && pipeline.object == step.object
                && pipeline.heap_bind_index == heap_bind_index
        })
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token draw-list command {} cannot resolve generated CLIPPINGTARGET pipeline binding for heap bind {}",
                step.command_index, heap_bind_index
            )
        })
}

fn offscreen_render_target(
    frame_resources: &NativeVulkanSceneFrameResources,
    target: SceneGraphTarget,
    final_layout: vk::ImageLayout,
) -> Result<NativeVulkanSceneRenderTarget, String> {
    if target == SceneGraphTarget::Swapchain {
        return Err("scene layer alpha-mask producer target cannot be swapchain".to_owned());
    }
    let binding = frame_resources.offscreen_target_binding(target)?;
    Ok(NativeVulkanSceneRenderTarget::Offscreen(
        NativeVulkanSceneOffscreenRenderTarget {
            target,
            image: binding.image,
            image_view: binding.view,
            extent: vk::Extent2D {
                width: binding.width,
                height: binding.height,
            },
            initial_layout: binding.current_layout,
            final_layout,
        },
    ))
}

fn render_target_for_scene_target(
    frame_resources: &NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordContext<'_>,
    target: SceneGraphTarget,
    final_layout: vk::ImageLayout,
) -> Result<NativeVulkanSceneRenderTarget, String> {
    if target == SceneGraphTarget::Swapchain {
        let mut swapchain = context.swapchain_target;
        swapchain.final_layout = final_layout;
        return Ok(NativeVulkanSceneRenderTarget::Swapchain(swapchain));
    }
    offscreen_render_target(frame_resources, target, final_layout)
}

fn mark_render_target_layout(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    target: SceneGraphTarget,
    layout: vk::ImageLayout,
) -> Result<(), String> {
    if target == SceneGraphTarget::Swapchain {
        return Ok(());
    }
    frame_resources.mark_offscreen_target_layout(target, layout)
}

fn transparent_clear_color() -> NativeVulkanClearColor {
    NativeVulkanClearColor {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    }
}

fn token_draw_list_command_order() -> [&'static str; 8] {
    [
        "read_token_recording_contract",
        "record_producer_steps_in_r8_alpha_mask_target_scopes",
        "record_flattexture_copy_back_steps_at_token_position",
        "transition_full_alpha_mask_before_generated_consumers",
        "record_generated_consumers_in_current_color_target_scopes",
        "bind_descriptor_heaps_only_no_legacy_sets",
        "preserve_we_token_order_for_draw_list",
        "leave_present_frame_integration_to_composelayer_scheduler",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_draw_list_record_plan_counts_recorded_step_shapes() {
        let plan = NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordPlan::from_steps(vec![
            step(NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::TokenProgramNoDraw),
            step(
                NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::ClippingMaskImage4ProducerRtMethod8,
            ),
            step(NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::FlatTextureCopyBackGraphNode),
            step(
                NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::GeneratedClippingTargetRtMethod8,
            ),
        ]);

        assert_eq!(plan.scheduled_step_count, 4);
        assert_eq!(plan.no_draw_step_count, 1);
        assert_eq!(plan.producer_step_count, 1);
        assert_eq!(plan.copy_back_step_count, 1);
        assert_eq!(plan.generated_consumer_step_count, 1);
        assert_eq!(
            plan.command_order[6],
            "preserve_we_token_order_for_draw_list"
        );
    }

    fn step(
        recording_kind: NativeVulkanSceneLayerAlphaMaskTokenRecordingKind,
    ) -> NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep {
        NativeVulkanSceneLayerAlphaMaskTokenDrawListRecordStep {
            command_index: 0,
            object: SceneObjectId(1),
            recording_kind,
            target: None,
            target_scope: None,
            source_transition: None,
            rt_method8_draw: None,
            copy_back_graph_node: None,
            command_order: Vec::new(),
        }
    }
}

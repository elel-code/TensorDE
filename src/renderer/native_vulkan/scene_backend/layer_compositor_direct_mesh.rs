//! Direct mesh block recorder for WE layer compositor schedules.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use vulkanalia::vk;

use crate::engine::scene_engine::{SceneFramePlan, SceneGraphExecutionPlan, SceneGraphTarget};

use super::frame_resources::NativeVulkanSceneFrameResources;
use super::graph_executor::{
    NativeVulkanSceneGraphFrameCommandPlan, NativeVulkanSceneGraphPassCommandPlan,
    NativeVulkanSceneGraphRuntimeFrameContext,
};
use super::layer_compositor_scheduler::{
    NativeVulkanSceneLayerCompositorRecordingBlockKind,
    NativeVulkanSceneLayerCompositorSchedulePlan,
};
use super::pass_command::native_vulkan_record_scene_mesh_pass_draw_commands;
use super::pipeline::NativeVulkanScenePipelineCacheKey;
use super::render_target::{
    NativeVulkanSceneRenderTarget, native_vulkan_record_scene_render_target_begin,
    native_vulkan_record_scene_render_target_end,
};

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_layer_compositor_direct_mesh_blocks<
    'a,
>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneGraphRuntimeFrameContext<'_>,
    frame: &'a SceneFramePlan,
    graph_execution: &SceneGraphExecutionPlan,
    schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
) -> Result<Option<NativeVulkanSceneGraphFrameCommandPlan<'a>>, String> {
    if graph_execution.pass_count == 0 {
        if schedule.recording_block_count != 0 {
            return Err(
                "scene layer compositor direct mesh recorder has blocks for an empty graph"
                    .to_owned(),
            );
        }
        return Ok(Some(empty_direct_mesh_frame_plan(
            context.target_formats.target_format_count(),
        )));
    }
    if !schedule_is_direct_mesh_only(schedule) || graph_execution.pass_count != 1 {
        return Ok(None);
    }

    let execution_pass = &graph_execution.passes[0];
    if execution_pass.input.is_some() || execution_pass.output != SceneGraphTarget::Swapchain {
        return Ok(None);
    }
    let block = schedule.recording_blocks.first().ok_or_else(|| {
        "scene layer compositor direct mesh recorder requires one direct mesh block".to_owned()
    })?;
    if block.graph_pass_index != Some(0)
        || block.graph_draw_index_start != Some(execution_pass.draw_index_start)
        || block.graph_draw_index_end != Some(execution_pass.draw_index_end)
    {
        return Ok(None);
    }

    let graph_pass = frame
        .graph
        .passes
        .get(execution_pass.pass_index)
        .ok_or_else(|| {
            format!(
                "scene layer compositor direct mesh recorder pass {} is outside graph",
                execution_pass.pass_index
            )
        })?;
    let mut swapchain = context.swapchain_target;
    swapchain.final_layout = vk::ImageLayout::PRESENT_SRC_KHR;
    let render_target = NativeVulkanSceneRenderTarget::Swapchain(swapchain);
    let target_scope = native_vulkan_record_scene_render_target_begin(
        context.device,
        context.command_buffer,
        render_target,
        context.clear_color,
    )?;
    let pass_target_format = context.target_formats.format(SceneGraphTarget::Swapchain)?;
    let pass_plan = {
        let resources = &*frame_resources;
        native_vulkan_record_scene_mesh_pass_draw_commands(
            context.device,
            context.command_buffer,
            graph_pass,
            execution_pass.draw_index_start,
            |key| {
                let cache_key =
                    NativeVulkanScenePipelineCacheKey::from_bind_key(key, pass_target_format)?;
                Ok(resources.cached_mesh_pipeline(&cache_key)?.pipeline)
            },
            |draw_index| resources.resource_heap_draw_bind_info_for_draw(draw_index),
            |geometry| resources.mesh_draw_buffers(geometry),
        )?
    };
    native_vulkan_record_scene_render_target_end(
        context.device,
        context.command_buffer,
        render_target,
        context.clear_color,
    )?;

    Ok(Some(NativeVulkanSceneGraphFrameCommandPlan {
        pass_count: 1,
        target_barrier_count: 0,
        target_format_count: context.target_formats.target_format_count(),
        passes: vec![NativeVulkanSceneGraphPassCommandPlan {
            target: SceneGraphTarget::Swapchain,
            target_scope,
            pass: pass_plan,
        }],
        target_barriers: Vec::new(),
        command_order: [
            "resolve_scene_graph_target_formats",
            "record_layer_compositor_direct_mesh_block_target",
            "record_mesh_pass_draw_lists_from_compositor_blocks",
            "record_scene_graph_target_barriers",
        ],
    }))
}

fn schedule_is_direct_mesh_only(schedule: &NativeVulkanSceneLayerCompositorSchedulePlan) -> bool {
    schedule.recording_block_count == schedule.mesh_graph_draw_span_block_count
        && schedule.recording_blocks.iter().all(|block| {
            block.kind == NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan
        })
}

fn empty_direct_mesh_frame_plan(
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
            "record_layer_compositor_direct_mesh_block_target",
            "record_mesh_pass_draw_lists_from_compositor_blocks",
            "record_scene_graph_target_barriers",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::super::layer_compositor_scheduler::{
        NativeVulkanSceneLayerCompositorRecordingBlock,
        NativeVulkanSceneLayerCompositorSchedulePlan,
    };
    use super::*;

    #[test]
    fn direct_mesh_recorder_accepts_only_mesh_span_blocks() {
        let schedule = schedule(vec![block(
            0,
            NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan,
        )]);

        assert!(schedule_is_direct_mesh_only(&schedule));
    }

    #[test]
    fn direct_mesh_recorder_rejects_alpha_mask_token_blocks() {
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

        assert!(!schedule_is_direct_mesh_only(&schedule));
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
            object_final_producer_command_count: 0,
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
            no_draw_marker_block_count: 0,
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
}

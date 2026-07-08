//! Mesh-block recorder for WE layer compositor schedules.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneFramePlan, SceneGraphExecutionPass, SceneGraphExecutionPlan, SceneGraphTarget,
};

use super::frame_resources::NativeVulkanSceneFrameResources;
use super::graph_executor::{
    NativeVulkanSceneGraphFrameCommandPlan, NativeVulkanSceneGraphPassCommandPlan,
    NativeVulkanSceneGraphRuntimeFrameContext,
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_layer_compositor_mesh_blocks<
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
                "scene layer compositor mesh-block recorder has blocks for an empty graph"
                    .to_owned(),
            );
        }
        return Ok(Some(empty_mesh_block_frame_plan(
            context.target_formats.target_format_count(),
        )));
    }
    if !schedule_is_mesh_block_recordable(schedule) || graph_execution.pass_count != 1 {
        return Ok(None);
    }

    let execution_pass = &graph_execution.passes[0];
    if execution_pass.input.is_some() || execution_pass.output != SceneGraphTarget::Swapchain {
        return Ok(None);
    }
    if !mesh_blocks_cover_execution_pass(schedule, execution_pass)? {
        return Ok(None);
    }

    let graph_pass = frame
        .graph
        .passes
        .get(execution_pass.pass_index)
        .ok_or_else(|| {
            format!(
                "scene layer compositor mesh-block recorder pass {} is outside graph",
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
    let mut commands = Vec::with_capacity(
        graph_pass
            .draws
            .len()
            .saturating_mul(2)
            .saturating_add(schedule.recording_blocks.len())
            .saturating_add(2),
    );
    commands.push(NativeVulkanSceneMeshPassCommand::BeginPass {
        name: graph_pass.name.as_str(),
        input: graph_pass.input,
        output: graph_pass.output,
        draw_index_start: execution_pass.draw_index_start,
    });
    let mut draw_span_state = NativeVulkanSceneMeshPassDrawSpanState::default();
    let mut counts = NativeVulkanSceneMeshPassDrawSpanCounts::default();
    {
        let resources = &*frame_resources;
        for block in &schedule.recording_blocks {
            match block.kind {
                NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan => {
                    let draw_index_start = block.graph_draw_index_start.ok_or_else(|| {
                        format!(
                            "scene layer compositor mesh-block recorder block {} has no draw start",
                            block.block_index
                        )
                    })?;
                    let draw_index_end = block.graph_draw_index_end.ok_or_else(|| {
                        format!(
                            "scene layer compositor mesh-block recorder block {} has no draw end",
                            block.block_index
                        )
                    })?;
                    let span_counts = native_vulkan_record_scene_mesh_pass_draw_span_commands(
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
                    )?;
                    counts.pipeline_bind_count = counts
                        .pipeline_bind_count
                        .saturating_add(span_counts.pipeline_bind_count);
                    counts.resource_heap_bind_count = counts
                        .resource_heap_bind_count
                        .saturating_add(span_counts.resource_heap_bind_count);
                    counts.indexed_draw_count = counts
                        .indexed_draw_count
                        .saturating_add(span_counts.indexed_draw_count);
                }
                NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker => {
                    commands.push(NativeVulkanSceneMeshPassCommand::LayerCompositorNoDrawMarker {
                        block_index: block.block_index,
                        step_index_start: block.step_index_start,
                        step_index_end: block.step_index_end,
                    });
                }
                NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime
                | NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep
                | NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired => {
                    return Ok(None);
                }
            }
        }
    }
    commands.push(NativeVulkanSceneMeshPassCommand::EndPass);
    let pass_plan = NativeVulkanSceneMeshPassCommandPlan {
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
            "record_layer_compositor_mesh_block_target",
            "record_mesh_draw_spans_from_compositor_blocks",
            "record_scene_graph_target_barriers",
        ],
    }))
}

fn schedule_is_mesh_block_recordable(
    schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
) -> bool {
    schedule.recording_block_count
        == schedule
            .mesh_graph_draw_span_block_count
            .saturating_add(schedule.no_draw_marker_block_count)
        && schedule.recording_blocks.iter().all(|block| {
            matches!(
                block.kind,
                NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan
                    | NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker
            )
        })
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
                        "scene layer compositor mesh-block recorder block {} has no draw start",
                        block.block_index
                    )
                })?;
                let draw_end = block.graph_draw_index_end.ok_or_else(|| {
                    format!(
                        "scene layer compositor mesh-block recorder block {} has no draw end",
                        block.block_index
                    )
                })?;
                if draw_start != next_draw_index || draw_end <= draw_start {
                    return Ok(false);
                }
                next_draw_index = draw_end;
            }
            NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker => {}
            NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime
            | NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep
            | NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired => {
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
    use super::super::layer_compositor_scheduler::{
        NativeVulkanSceneLayerCompositorRecordingBlock,
        NativeVulkanSceneLayerCompositorSchedulePlan,
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

        assert!(schedule_is_mesh_block_recordable(&schedule));
    }

    #[test]
    fn mesh_block_recorder_rejects_alpha_mask_token_blocks() {
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

        assert!(!schedule_is_mesh_block_recordable(&schedule));
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

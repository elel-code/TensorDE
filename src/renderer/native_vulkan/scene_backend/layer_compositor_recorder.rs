//! WE layer compositor recording-block consumption for native Vulkan runtime.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;

use crate::engine::scene_engine::{SceneGraphTarget, SceneObjectId};

use super::effect_executor::{
    NativeVulkanSceneEffectRuntimeCommandPlan, NativeVulkanSceneEffectRuntimeFramePlan,
};
use super::graph_executor::NativeVulkanSceneGraphFrameCommandPlan;
use super::layer_alpha_mask_executor::NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan;
use super::layer_compositor_scheduler::{
    NativeVulkanSceneLayerCompositorRecordingBlock,
    NativeVulkanSceneLayerCompositorRecordingBlockKind,
    NativeVulkanSceneLayerCompositorSchedulePlan,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerCompositorBlockRecordingPlan {
    pub block_count: usize,
    pub mesh_graph_draw_span_count: usize,
    pub object_final_effect_runtime_count: usize,
    pub alpha_mask_single_step_recorder_count: usize,
    pub no_draw_marker_count: usize,
    pub clear_prep_pending_recorder_count: usize,
    pub schedule_consumed_block_count: usize,
    pub all_non_clear_blocks_have_recording_source: bool,
    pub actual_present_recording_order_replaced: bool,
    pub blocks: Vec<NativeVulkanSceneLayerCompositorBlockRecording>,
    pub command_order: [&'static str; 7],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerCompositorBlockRecording {
    pub block_index: usize,
    pub step_index_start: usize,
    pub step_index_end: usize,
    pub command_count: usize,
    pub object: SceneObjectId,
    pub source: NativeVulkanSceneLayerCompositorBlockRecordingSource,
    pub command_order: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerCompositorBlockRecordingSource {
    MeshGraphDrawSpan {
        graph_pass_index: usize,
        graph_draw_index_start: usize,
        graph_draw_index_end: usize,
    },
    ObjectFinalEffectRuntime {
        target: SceneGraphTarget,
    },
    AlphaMaskTokenDrawListStep {
        token_recording_step_index: usize,
    },
    NoDrawMarker,
    LayerTargetClearPrepPendingRecorder,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_compositor_block_recording(
    schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
    effects: &NativeVulkanSceneEffectRuntimeFramePlan<'_>,
    mesh_frame: &NativeVulkanSceneGraphFrameCommandPlan<'_>,
    token_recording: &NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
    actual_present_recording_order_replaced: bool,
) -> Result<NativeVulkanSceneLayerCompositorBlockRecordingPlan, String> {
    let mut blocks = Vec::with_capacity(schedule.recording_blocks.len());
    for block in &schedule.recording_blocks {
        blocks.push(recording_for_block(
            schedule,
            effects,
            mesh_frame,
            token_recording,
            block,
        )?);
    }
    Ok(
        NativeVulkanSceneLayerCompositorBlockRecordingPlan::from_blocks(
            blocks,
            actual_present_recording_order_replaced,
        ),
    )
}

impl NativeVulkanSceneLayerCompositorBlockRecordingPlan {
    fn from_blocks(
        blocks: Vec<NativeVulkanSceneLayerCompositorBlockRecording>,
        actual_present_recording_order_replaced: bool,
    ) -> Self {
        let mesh_graph_draw_span_count = blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.source,
                    NativeVulkanSceneLayerCompositorBlockRecordingSource::MeshGraphDrawSpan { .. }
                )
            })
            .count();
        let object_final_effect_runtime_count = blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.source,
                    NativeVulkanSceneLayerCompositorBlockRecordingSource::ObjectFinalEffectRuntime {
                        ..
                    }
                )
            })
            .count();
        let alpha_mask_single_step_recorder_count = blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.source,
                    NativeVulkanSceneLayerCompositorBlockRecordingSource::AlphaMaskTokenDrawListStep {
                        ..
                    }
                )
            })
            .count();
        let no_draw_marker_count = blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.source,
                    NativeVulkanSceneLayerCompositorBlockRecordingSource::NoDrawMarker
                )
            })
            .count();
        let clear_prep_pending_recorder_count = blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.source,
                    NativeVulkanSceneLayerCompositorBlockRecordingSource::LayerTargetClearPrepPendingRecorder
                )
            })
            .count();
        let schedule_consumed_block_count = blocks.len();
        Self {
            block_count: blocks.len(),
            mesh_graph_draw_span_count,
            object_final_effect_runtime_count,
            alpha_mask_single_step_recorder_count,
            no_draw_marker_count,
            clear_prep_pending_recorder_count,
            schedule_consumed_block_count,
            all_non_clear_blocks_have_recording_source: true,
            actual_present_recording_order_replaced,
            blocks,
            command_order: [
                "read_layer_compositor_recording_blocks",
                "bind_mesh_blocks_to_recorded_graph_draw_spans",
                "bind_object_final_blocks_to_effect_runtime_outputs",
                "bind_alpha_mask_blocks_to_single_step_token_recorder",
                "preserve_no_draw_token_markers",
                "surface_clear_prep_pending_recorders",
                "emit_compositor_block_recording_plan",
            ],
        }
    }
}

fn recording_for_block(
    schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
    effects: &NativeVulkanSceneEffectRuntimeFramePlan<'_>,
    mesh_frame: &NativeVulkanSceneGraphFrameCommandPlan<'_>,
    token_recording: &NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
    block: &NativeVulkanSceneLayerCompositorRecordingBlock,
) -> Result<NativeVulkanSceneLayerCompositorBlockRecording, String> {
    let first_step = schedule.steps.get(block.step_index_start).ok_or_else(|| {
        format!(
            "scene layer compositor block recorder block {} has step start {} outside schedule",
            block.block_index, block.step_index_start
        )
    })?;
    if block.step_index_end > schedule.steps.len() || block.step_index_start >= block.step_index_end
    {
        return Err(format!(
            "scene layer compositor block recorder block {} has invalid step range {}..{} for {} steps",
            block.block_index,
            block.step_index_start,
            block.step_index_end,
            schedule.steps.len()
        ));
    }
    let source = match block.kind {
        NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan => {
            mesh_graph_draw_span_source(mesh_frame, block)?
        }
        NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime => {
            object_final_effect_runtime_source(effects, first_step.object)?
        }
        NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep => {
            alpha_mask_token_draw_list_step_source(token_recording, block)?
        }
        NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker => {
            NativeVulkanSceneLayerCompositorBlockRecordingSource::NoDrawMarker
        }
        NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired => {
            NativeVulkanSceneLayerCompositorBlockRecordingSource::LayerTargetClearPrepPendingRecorder
        }
    };
    Ok(NativeVulkanSceneLayerCompositorBlockRecording {
        block_index: block.block_index,
        step_index_start: block.step_index_start,
        step_index_end: block.step_index_end,
        command_count: block.command_count,
        object: first_step.object,
        command_order: block_recording_command_order(&source),
        source,
    })
}

fn mesh_graph_draw_span_source(
    mesh_frame: &NativeVulkanSceneGraphFrameCommandPlan<'_>,
    block: &NativeVulkanSceneLayerCompositorRecordingBlock,
) -> Result<NativeVulkanSceneLayerCompositorBlockRecordingSource, String> {
    let graph_pass_index = block.graph_pass_index.ok_or_else(|| {
        format!(
            "scene layer compositor mesh block {} has no graph pass index",
            block.block_index
        )
    })?;
    let draw_start = block.graph_draw_index_start.ok_or_else(|| {
        format!(
            "scene layer compositor mesh block {} has no graph draw start",
            block.block_index
        )
    })?;
    let draw_end = block.graph_draw_index_end.ok_or_else(|| {
        format!(
            "scene layer compositor mesh block {} has no graph draw end",
            block.block_index
        )
    })?;
    if draw_start >= draw_end {
        return Err(format!(
            "scene layer compositor mesh block {} has empty graph draw span {}..{}",
            block.block_index, draw_start, draw_end
        ));
    }
    let pass = mesh_frame.passes.get(graph_pass_index).ok_or_else(|| {
        format!(
            "scene layer compositor mesh block {} references missing graph pass {}",
            block.block_index, graph_pass_index
        )
    })?;
    if draw_start < pass.pass.draw_index_start || draw_end > pass.pass.draw_index_end {
        return Err(format!(
            "scene layer compositor mesh block {} draw span {}..{} is outside recorded pass {} range {}..{}",
            block.block_index,
            draw_start,
            draw_end,
            graph_pass_index,
            pass.pass.draw_index_start,
            pass.pass.draw_index_end
        ));
    }
    Ok(
        NativeVulkanSceneLayerCompositorBlockRecordingSource::MeshGraphDrawSpan {
            graph_pass_index,
            graph_draw_index_start: draw_start,
            graph_draw_index_end: draw_end,
        },
    )
}

fn object_final_effect_runtime_source(
    effects: &NativeVulkanSceneEffectRuntimeFramePlan<'_>,
    object: SceneObjectId,
) -> Result<NativeVulkanSceneLayerCompositorBlockRecordingSource, String> {
    let target = SceneGraphTarget::ObjectFinal(object);
    let has_output = effects.commands.iter().any(|command| {
        matches!(
            command,
            NativeVulkanSceneEffectRuntimeCommandPlan::MaterialPass(pass) if pass.output == target
        )
    });
    if !has_output {
        return Err(format!(
            "scene layer compositor object-final block for object {object:?} has no effect runtime ObjectFinal output"
        ));
    }
    Ok(NativeVulkanSceneLayerCompositorBlockRecordingSource::ObjectFinalEffectRuntime { target })
}

fn alpha_mask_token_draw_list_step_source(
    token_recording: &NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
    block: &NativeVulkanSceneLayerCompositorRecordingBlock,
) -> Result<NativeVulkanSceneLayerCompositorBlockRecordingSource, String> {
    let token_recording_step_index = block.token_recording_step_index.ok_or_else(|| {
        format!(
            "scene layer compositor alpha-mask block {} has no token recording step index",
            block.block_index
        )
    })?;
    if token_recording_step_index >= token_recording.steps.len() {
        return Err(format!(
            "scene layer compositor alpha-mask block {} token recording step {} is outside {} steps",
            block.block_index,
            token_recording_step_index,
            token_recording.steps.len()
        ));
    }
    Ok(
        NativeVulkanSceneLayerCompositorBlockRecordingSource::AlphaMaskTokenDrawListStep {
            token_recording_step_index,
        },
    )
}

fn block_recording_command_order(
    source: &NativeVulkanSceneLayerCompositorBlockRecordingSource,
) -> Vec<&'static str> {
    match source {
        NativeVulkanSceneLayerCompositorBlockRecordingSource::MeshGraphDrawSpan { .. } => vec![
            "consume_mesh_graph_draw_span_block",
            "reuse_command_block_mesh_pass_draw_commands_in_we_layer_order",
        ],
        NativeVulkanSceneLayerCompositorBlockRecordingSource::ObjectFinalEffectRuntime { .. } => {
            vec!["consume_object_final_effect_runtime_output"]
        }
        NativeVulkanSceneLayerCompositorBlockRecordingSource::AlphaMaskTokenDrawListStep {
            ..
        } => vec![
            "consume_alpha_mask_single_step_recorder_block",
            "ready_for_schedule_driven_token_draw_list_recording",
        ],
        NativeVulkanSceneLayerCompositorBlockRecordingSource::NoDrawMarker => {
            vec!["consume_no_draw_layer_marker_block"]
        }
        NativeVulkanSceneLayerCompositorBlockRecordingSource::LayerTargetClearPrepPendingRecorder => {
            vec!["report_layer_target_clear_prep_recorder_missing"]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::SceneEffectPassGraphPlan;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraph, SceneGraphDraw, SceneGraphExecutionPlan,
        SceneGraphPass, SceneGraphPipelineClass, SceneGraphResourceBinding, SceneGraphResourceRole,
        SceneLayerCompositorBlendKey, SceneLayerCompositorCommand, SceneLayerCompositorCondition,
        SceneLayerCompositorEntry, SceneLayerCompositorLayer, SceneLayerCompositorOperation,
        SceneLayerCompositorPlan, SceneLayerCompositorRoute, SceneLayerCompositorTarget,
        SceneMaterialKey, SceneMaterialRenderState, SceneResourceId,
    };
    use crate::renderer::native_vulkan::scene_backend::effect_executor::NativeVulkanSceneEffectRuntimeCommandSequencePlan;
    use crate::renderer::native_vulkan::scene_backend::effect_pipeline_warmup::NativeVulkanSceneEffectPipelineWarmupPlan;
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::{
        NativeVulkanSceneLayerAlphaMaskTokenRecordingKind,
        NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_compositor_scheduler::native_vulkan_plan_scene_layer_compositor_schedule;
    use crate::renderer::native_vulkan::scene_backend::pass_command::NativeVulkanSceneMeshPassCommandPlan;
    use crate::renderer::native_vulkan::scene_backend::render_target::{
        NativeVulkanSceneRenderTargetLoadOp, NativeVulkanSceneRenderTargetScopePlan,
    };
    use vulkanalia::vk;

    #[test]
    fn block_recorder_consumes_mesh_span_and_alpha_mask_single_step_blocks() {
        let object = SceneObjectId(12);
        let graph = SceneGraph {
            passes: vec![graph_pass(
                "scene-main",
                None,
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(object)],
            )],
        };
        let schedule = native_vulkan_plan_scene_layer_compositor_schedule(
            &layer_compositor(vec![tokenized_direct_layer(object)]),
            &graph,
            &SceneGraphExecutionPlan::from_graph(&graph),
            &token_recording_for_tokenized_layer(object),
        )
        .expect("schedule");
        let recording = native_vulkan_plan_scene_layer_compositor_block_recording(
            &schedule,
            &empty_effects(),
            &mesh_frame(0, 0, 1),
            &token_recording_for_tokenized_layer(object),
            false,
        )
        .expect("recording blocks");

        assert_eq!(recording.block_count, 6);
        assert_eq!(recording.mesh_graph_draw_span_count, 1);
        assert_eq!(recording.alpha_mask_single_step_recorder_count, 4);
        assert_eq!(recording.no_draw_marker_count, 1);
        assert_eq!(recording.clear_prep_pending_recorder_count, 0);
        assert!(recording.all_non_clear_blocks_have_recording_source);
        assert!(!recording.actual_present_recording_order_replaced);
        let replaced_recording = native_vulkan_plan_scene_layer_compositor_block_recording(
            &schedule,
            &empty_effects(),
            &mesh_frame(0, 0, 1),
            &token_recording_for_tokenized_layer(object),
            true,
        )
        .expect("recording blocks with replaced present order");
        assert!(replaced_recording.actual_present_recording_order_replaced);
        assert!(matches!(
            recording.blocks[0].source,
            NativeVulkanSceneLayerCompositorBlockRecordingSource::MeshGraphDrawSpan {
                graph_pass_index: 0,
                graph_draw_index_start: 0,
                graph_draw_index_end: 1,
            }
        ));
        assert!(matches!(
            recording.blocks[2].source,
            NativeVulkanSceneLayerCompositorBlockRecordingSource::AlphaMaskTokenDrawListStep {
                token_recording_step_index: 1,
            }
        ));
    }

    #[test]
    fn block_recorder_rejects_mesh_span_outside_recorded_graph_pass() {
        let object = SceneObjectId(1);
        let graph = SceneGraph {
            passes: vec![graph_pass(
                "scene-main",
                None,
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(object)],
            )],
        };
        let schedule = native_vulkan_plan_scene_layer_compositor_schedule(
            &layer_compositor(vec![direct_layer(object)]),
            &graph,
            &SceneGraphExecutionPlan::from_graph(&graph),
            &empty_token_recording(),
        )
        .expect("schedule");

        let err = native_vulkan_plan_scene_layer_compositor_block_recording(
            &schedule,
            &empty_effects(),
            &mesh_frame(0, 5, 6),
            &empty_token_recording(),
            false,
        )
        .expect_err("mesh span must be inside recorded pass");

        assert!(err.contains("outside recorded pass"));
    }

    fn layer_compositor(layers: Vec<SceneLayerCompositorLayer>) -> SceneLayerCompositorPlan {
        let command_count = layers.iter().map(|layer| layer.commands.len()).sum();
        let tokenized_layer_count = layers
            .iter()
            .filter(|layer| layer.uses_tokenized_subdraw)
            .count();
        SceneLayerCompositorPlan {
            layer_count: layers.len(),
            command_count,
            object_final_layer_count: 0,
            tokenized_layer_count,
            layers,
            command_order: SceneLayerCompositorPlan::empty().command_order,
        }
    }

    fn direct_layer(object: SceneObjectId) -> SceneLayerCompositorLayer {
        SceneLayerCompositorLayer {
            object,
            route: SceneLayerCompositorRoute::DirectSwapchain,
            uses_tokenized_subdraw: false,
            has_active_aux_clear_target: false,
            commands: vec![normal_command()],
        }
    }

    fn tokenized_direct_layer(object: SceneObjectId) -> SceneLayerCompositorLayer {
        let mut layer = direct_layer(object);
        layer.uses_tokenized_subdraw = true;
        layer.commands.extend(token_commands());
        layer
    }

    fn normal_command() -> SceneLayerCompositorCommand {
        SceneLayerCompositorCommand {
            entry: SceneLayerCompositorEntry::NormalRenderEntry32,
            operation: SceneLayerCompositorOperation::NormalRender,
            condition: SceneLayerCompositorCondition::Always,
            source: None,
            target: SceneLayerCompositorTarget::Swapchain,
            blend_key: SceneLayerCompositorBlendKey::WrapperPushBlendEnumAndAlphaWriteBits0x2000x8,
        }
    }

    fn token_commands() -> Vec<SceneLayerCompositorCommand> {
        vec![
            token_command(
                SceneLayerCompositorEntry::TokenizedCompositeEntry52,
                SceneLayerCompositorOperation::TokenProgramDispatch,
                SceneLayerCompositorCondition::Always,
                None,
                SceneLayerCompositorTarget::LayerTarget490,
                SceneLayerCompositorBlendKey::Inherit,
            ),
            token_command(
                SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
                SceneLayerCompositorOperation::DrawClippingMask,
                SceneLayerCompositorCondition::Token1OrToken2FirstPair,
                None,
                SceneLayerCompositorTarget::FullAlphaMask,
                SceneLayerCompositorBlendKey::Inherit,
            ),
            token_command(
                SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
                SceneLayerCompositorOperation::DrawClippingMask,
                SceneLayerCompositorCondition::Token2IntermediatePairOrFinalMask,
                None,
                SceneLayerCompositorTarget::FullAlphaMaskIntermediate,
                SceneLayerCompositorBlendKey::Inherit,
            ),
            token_command(
                SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed,
                SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
                SceneLayerCompositorCondition::Token2AfterIntermediateMask,
                Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
                SceneLayerCompositorTarget::FullAlphaMask,
                SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
            ),
            token_command(
                SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53,
                SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
                SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
                Some(SceneLayerCompositorTarget::FullAlphaMask),
                SceneLayerCompositorTarget::LayerTarget490,
                SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0,
            ),
        ]
    }

    fn token_command(
        entry: SceneLayerCompositorEntry,
        operation: SceneLayerCompositorOperation,
        condition: SceneLayerCompositorCondition,
        source: Option<SceneLayerCompositorTarget>,
        target: SceneLayerCompositorTarget,
        blend_key: SceneLayerCompositorBlendKey,
    ) -> SceneLayerCompositorCommand {
        SceneLayerCompositorCommand {
            entry,
            operation,
            condition,
            source,
            target,
            blend_key,
        }
    }

    fn empty_effects() -> NativeVulkanSceneEffectRuntimeFramePlan<'static> {
        let graph = SceneEffectPassGraphPlan::empty();
        NativeVulkanSceneEffectRuntimeFramePlan {
            command_sequence:
                NativeVulkanSceneEffectRuntimeCommandSequencePlan::from_effect_pass_graph(&graph)
                    .expect("empty effect command sequence"),
            pipeline_warmup:
                NativeVulkanSceneEffectPipelineWarmupPlan::from_effect_pass_graph_with_target_formats(
                    &graph,
                    |_| Ok(vk::Format::R8G8B8A8_UNORM),
                )
                .expect("empty effect pipeline warmup"),
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

    fn mesh_frame(
        _pass_index: usize,
        draw_index_start: usize,
        draw_index_end: usize,
    ) -> NativeVulkanSceneGraphFrameCommandPlan<'static> {
        NativeVulkanSceneGraphFrameCommandPlan {
            pass_count: 1,
            target_barrier_count: 0,
            target_format_count: 1,
            passes: vec![
                super::super::graph_executor::NativeVulkanSceneGraphPassCommandPlan {
                    target: SceneGraphTarget::Swapchain,
                    target_scope: NativeVulkanSceneRenderTargetScopePlan {
                        width: 3840,
                        height: 2160,
                        load_op: NativeVulkanSceneRenderTargetLoadOp::Clear,
                        begin_command_order: [
                            "cmd_pipeline_barrier2_color_attachment",
                            "cmd_begin_rendering",
                        ],
                        end_command_order: ["cmd_end_rendering", "cmd_pipeline_barrier2_present"],
                    },
                    pass: NativeVulkanSceneMeshPassCommandPlan {
                        name: "scene-main",
                        input: None,
                        output: SceneGraphTarget::Swapchain,
                        draw_index_start,
                        draw_index_end,
                        draw_count: draw_index_end.saturating_sub(draw_index_start),
                        pipeline_bind_count: 1,
                        resource_heap_bind_count: 1,
                        indexed_draw_count: draw_index_end.saturating_sub(draw_index_start),
                        commands: Vec::new(),
                    },
                },
            ],
            target_barriers: Vec::new(),
            command_order: [
                "resolve_scene_graph_target_formats",
                "record_graph_pass_render_targets",
                "record_mesh_pass_draw_lists",
                "record_scene_graph_target_barriers",
            ],
        }
    }

    fn token_recording_for_tokenized_layer(
        object: SceneObjectId,
    ) -> NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan {
        let steps = vec![
            token_recording_step(
                1,
                object,
                SceneLayerCompositorOperation::TokenProgramDispatch,
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch,
                NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::TokenProgramNoDraw,
            ),
            token_recording_step(
                2,
                object,
                SceneLayerCompositorOperation::DrawClippingMask,
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer,
                NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::ClippingMaskImage4ProducerRtMethod8,
            ),
            token_recording_step(
                3,
                object,
                SceneLayerCompositorOperation::DrawClippingMask,
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer,
                NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::ClippingMaskImage4ProducerRtMethod8,
            ),
            token_recording_step(
                4,
                object,
                SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask,
                NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::FlatTextureCopyBackGraphNode,
            ),
            token_recording_step(
                5,
                object,
                SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer,
                NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::GeneratedClippingTargetRtMethod8,
            ),
        ];
        NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan {
            scheduled_step_count: steps.len(),
            no_draw_step_count: 1,
            producer_recordable_step_count: 2,
            copy_back_recordable_step_count: 1,
            generated_consumer_recordable_step_count: 1,
            draw_recordable_step_count: 4,
            pending_step_count: 0,
            all_draw_steps_recordable: true,
            steps,
            command_order: token_recording_command_order(),
        }
    }

    fn empty_token_recording() -> NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan {
        NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan {
            scheduled_step_count: 0,
            no_draw_step_count: 0,
            producer_recordable_step_count: 0,
            copy_back_recordable_step_count: 0,
            generated_consumer_recordable_step_count: 0,
            draw_recordable_step_count: 0,
            pending_step_count: 0,
            all_draw_steps_recordable: true,
            steps: Vec::new(),
            command_order: token_recording_command_order(),
        }
    }

    fn token_recording_step(
        command_index: usize,
        object: SceneObjectId,
        operation: SceneLayerCompositorOperation,
        schedule_kind: NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
        recording_kind: NativeVulkanSceneLayerAlphaMaskTokenRecordingKind,
    ) -> NativeVulkanSceneLayerAlphaMaskTokenRecordingStep {
        NativeVulkanSceneLayerAlphaMaskTokenRecordingStep {
            command_index,
            object,
            operation,
            schedule_kind,
            recording_kind,
            producer_draw_index: None,
            producer_target_scope_index: None,
            copy_back_command_index: None,
            generated_consumer_draw_index: None,
            generated_consumer_command_index: None,
            rt_method8_command_index: None,
            command_order: Vec::new(),
        }
    }

    fn token_recording_command_order() -> [&'static str; 7] {
        [
            "read_token_schedule",
            "join_producer_target_scopes",
            "join_copy_back_commands",
            "join_generated_consumer_commands",
            "join_rt_method8_indexed_draw_commands",
            "count_recordable_alpha_mask_steps",
            "emit_token_recording_contract",
        ]
    }

    fn graph_pass(
        name: &'static str,
        input: Option<SceneGraphTarget>,
        output: SceneGraphTarget,
        draws: Vec<SceneGraphDraw>,
    ) -> SceneGraphPass {
        SceneGraphPass {
            name: name.to_owned(),
            input,
            output,
            draws,
        }
    }

    fn mesh_draw(object: SceneObjectId) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: SceneMaterialRenderState::translucent_2d(),
            },
            geometry: Some(SceneGeometryId(object.0)),
            puppet: None,
            resources: vec![SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(object.0),
            }],
            index_count: 6,
        }
    }
}

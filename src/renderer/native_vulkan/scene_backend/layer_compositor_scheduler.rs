//! WE layer compositor schedule contract for native Vulkan scene recording.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneGraph, SceneGraphExecutionPlan, SceneGraphTarget, SceneLayerCompositorEntry,
    SceneLayerCompositorOperation, SceneLayerCompositorPlan, SceneLayerCompositorRoute,
    SceneLayerCompositorTarget, SceneObjectId,
};

use super::layer_alpha_mask_executor::{
    NativeVulkanSceneLayerAlphaMaskTokenRecordingKind,
    NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
    NativeVulkanSceneLayerAlphaMaskTokenRecordingStep,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerCompositorSchedulePlan {
    pub layer_count: usize,
    pub command_count: usize,
    pub direct_mesh_graph_command_count: usize,
    pub object_final_producer_command_count: usize,
    pub object_final_composite_command_count: usize,
    pub alpha_mask_token_draw_list_command_count: usize,
    pub token_program_no_draw_count: usize,
    pub clear_prep_early_out_no_draw_count: usize,
    pub clear_prep_recorder_required_count: usize,
    pub recording_block_count: usize,
    pub mesh_graph_draw_span_block_count: usize,
    pub alpha_mask_token_recording_block_count: usize,
    pub no_draw_marker_block_count: usize,
    pub all_alpha_mask_commands_recordable: bool,
    pub steps: Vec<NativeVulkanSceneLayerCompositorScheduleStep>,
    pub recording_blocks: Vec<NativeVulkanSceneLayerCompositorRecordingBlock>,
    pub command_order: [&'static str; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerCompositorScheduleStep {
    pub global_command_index: usize,
    pub layer_index: usize,
    pub layer_command_index: usize,
    pub object: SceneObjectId,
    pub route: SceneLayerCompositorRoute,
    pub entry: SceneLayerCompositorEntry,
    pub operation: SceneLayerCompositorOperation,
    pub scheduled_kind: NativeVulkanSceneLayerCompositorScheduledKind,
    pub graph_pass_index: Option<usize>,
    pub graph_draw_index: Option<usize>,
    pub token_recording_step_index: Option<usize>,
    pub command_order: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerCompositorScheduledKind {
    DirectMeshGraphDraw,
    ObjectFinalProducerEffectRuntime,
    ObjectFinalCompositeGraphDraw,
    AlphaMaskTokenProgramNoDraw,
    AlphaMaskTokenDrawListCommand,
    LayerTargetClearPrepEarlyOutNoDraw,
    LayerTargetClearPrepRecorderRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerCompositorRecordingBlock {
    pub block_index: usize,
    pub step_index_start: usize,
    pub step_index_end: usize,
    pub command_count: usize,
    pub kind: NativeVulkanSceneLayerCompositorRecordingBlockKind,
    pub graph_pass_index: Option<usize>,
    pub graph_draw_index_start: Option<usize>,
    pub graph_draw_index_end: Option<usize>,
    pub token_recording_step_index: Option<usize>,
    pub command_order: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerCompositorRecordingBlockKind {
    MeshGraphDrawSpan,
    ObjectFinalProducerEffectRuntime,
    AlphaMaskTokenDrawListStep,
    NoDrawLayerMarker,
    LayerTargetClearPrepRecorderRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanSceneLayerCompositorGraphDrawPosition {
    pass_index: usize,
    draw_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanSceneLayerCompositorTokenMatch {
    step_index: usize,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_compositor_schedule(
    layer_compositor: &SceneLayerCompositorPlan,
    graph: &SceneGraph,
    graph_execution: &SceneGraphExecutionPlan,
    token_recording: &NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
) -> Result<NativeVulkanSceneLayerCompositorSchedulePlan, String> {
    validate_graph_execution_identity(graph, graph_execution)?;
    let token_steps = token_steps_by_command_index(token_recording)?;
    let mut consumed_token_steps = BTreeSet::new();
    let mut steps = Vec::with_capacity(layer_compositor.command_count);

    for (layer_index, layer) in layer_compositor.layers.iter().enumerate() {
        for (layer_command_index, command) in layer.commands.iter().enumerate() {
            let global_command_index = steps.len();
            let mut command_order = vec![
                "read_we_layer_command_order",
                "classify_layer_command_recorder",
            ];
            let mut graph_pass_index = None;
            let mut graph_draw_index = None;
            let mut token_recording_step_index = None;

            let scheduled_kind = match command.operation {
                SceneLayerCompositorOperation::NormalRender => match layer.route {
                    SceneLayerCompositorRoute::DirectSwapchain => {
                        let position = find_graph_draw_position(
                            graph,
                            graph_execution,
                            layer.object,
                            None,
                            SceneGraphTarget::Swapchain,
                        )?;
                        graph_pass_index = Some(position.pass_index);
                        graph_draw_index = Some(position.draw_index);
                        command_order.extend([
                            "join_direct_layer_to_swapchain_graph_pass",
                            "reuse_retained_mesh_graph_draw",
                        ]);
                        NativeVulkanSceneLayerCompositorScheduledKind::DirectMeshGraphDraw
                    }
                    SceneLayerCompositorRoute::ObjectFinalMeshComposite => {
                        if let Some(prefill_target) =
                            image_layer_prefill_graph_target(command.target)
                        {
                            let position = find_graph_draw_position(
                                graph,
                                graph_execution,
                                layer.object,
                                None,
                                prefill_target,
                            )?;
                            graph_pass_index = Some(position.pass_index);
                            graph_draw_index = Some(position.draw_index);
                            command_order.extend([
                                "join_image_layer_prefill_to_mesh_graph_pass",
                                "record_prefill_before_layer_final_effect_runtime",
                            ]);
                        } else {
                            command_order.extend([
                                "join_object_final_layer_to_effect_runtime_output",
                                "preserve_object_final_before_swapchain_composite",
                            ]);
                        }
                        NativeVulkanSceneLayerCompositorScheduledKind::ObjectFinalProducerEffectRuntime
                    }
                },
                SceneLayerCompositorOperation::ClearPrep => {
                    if layer.has_active_aux_clear_target {
                        command_order.extend([
                            "require_layer_target_clear_recorder",
                            "keep_active_aux_clear_step_in_we_layer_order",
                        ]);
                        NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepRecorderRequired
                    } else {
                        command_order.extend([
                            "apply_0x140207740_aux_3e8_absent_early_out",
                            "preserve_clear_prep_position_as_no_draw_marker",
                        ]);
                        NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepEarlyOutNoDraw
                    }
                }
                SceneLayerCompositorOperation::FullLayerComposite => {
                    let input = layer_compositor_graph_target(
                        command
                            .source
                            .unwrap_or(SceneLayerCompositorTarget::ObjectFinal(layer.object)),
                    )?;
                    let position = find_graph_draw_position(
                        graph,
                        graph_execution,
                        layer.object,
                        Some(input),
                        SceneGraphTarget::Swapchain,
                    )?;
                    graph_pass_index = Some(position.pass_index);
                    graph_draw_index = Some(position.draw_index);
                    command_order.extend([
                        "join_layer_final_input_to_swapchain_graph_pass",
                        "reuse_object_geometry_final_composite_draw",
                    ]);
                    NativeVulkanSceneLayerCompositorScheduledKind::ObjectFinalCompositeGraphDraw
                }
                SceneLayerCompositorOperation::TokenProgramDispatch => {
                    let token_match = require_token_step(
                        global_command_index,
                        layer.object,
                        command.operation,
                        NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::TokenProgramNoDraw,
                        &token_steps,
                    )?;
                    consumed_token_steps.insert(token_match.step_index);
                    token_recording_step_index = Some(token_match.step_index);
                    command_order.extend([
                        "join_token_program_marker_to_alpha_mask_schedule",
                        "preserve_no_draw_token_position",
                    ]);
                    NativeVulkanSceneLayerCompositorScheduledKind::AlphaMaskTokenProgramNoDraw
                }
                SceneLayerCompositorOperation::DrawClippingMask
                | SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
                | SceneLayerCompositorOperation::DrawGeneratedClippingTarget => {
                    let token_match = require_token_step(
                        global_command_index,
                        layer.object,
                        command.operation,
                        expected_alpha_mask_recording_kind(command.operation)?,
                        &token_steps,
                    )?;
                    consumed_token_steps.insert(token_match.step_index);
                    token_recording_step_index = Some(token_match.step_index);
                    command_order.extend([
                        "join_tokenized_command_to_alpha_mask_draw_list",
                        "require_recordable_alpha_mask_step",
                    ]);
                    NativeVulkanSceneLayerCompositorScheduledKind::AlphaMaskTokenDrawListCommand
                }
            };

            steps.push(NativeVulkanSceneLayerCompositorScheduleStep {
                global_command_index,
                layer_index,
                layer_command_index,
                object: layer.object,
                route: layer.route,
                entry: command.entry,
                operation: command.operation,
                scheduled_kind,
                graph_pass_index,
                graph_draw_index,
                token_recording_step_index,
                command_order,
            });
        }
    }

    if consumed_token_steps.len() != token_recording.steps.len() {
        let missing = token_recording
            .steps
            .iter()
            .enumerate()
            .filter(|(step_index, _)| !consumed_token_steps.contains(step_index))
            .map(|(_, step)| step.command_index)
            .collect::<Vec<_>>();
        return Err(format!(
            "scene layer compositor scheduler found alpha-mask token recording steps without WE layer command positions: {missing:?}"
        ));
    }

    Ok(NativeVulkanSceneLayerCompositorSchedulePlan::from_steps(
        layer_compositor.layer_count,
        steps,
    ))
}

impl NativeVulkanSceneLayerCompositorSchedulePlan {
    fn from_steps(
        layer_count: usize,
        steps: Vec<NativeVulkanSceneLayerCompositorScheduleStep>,
    ) -> Self {
        let direct_mesh_graph_command_count = count_kind(
            &steps,
            NativeVulkanSceneLayerCompositorScheduledKind::DirectMeshGraphDraw,
        );
        let object_final_producer_command_count = count_kind(
            &steps,
            NativeVulkanSceneLayerCompositorScheduledKind::ObjectFinalProducerEffectRuntime,
        );
        let object_final_composite_command_count = count_kind(
            &steps,
            NativeVulkanSceneLayerCompositorScheduledKind::ObjectFinalCompositeGraphDraw,
        );
        let alpha_mask_token_draw_list_command_count = count_kind(
            &steps,
            NativeVulkanSceneLayerCompositorScheduledKind::AlphaMaskTokenDrawListCommand,
        );
        let token_program_no_draw_count = count_kind(
            &steps,
            NativeVulkanSceneLayerCompositorScheduledKind::AlphaMaskTokenProgramNoDraw,
        );
        let clear_prep_early_out_no_draw_count = count_kind(
            &steps,
            NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepEarlyOutNoDraw,
        );
        let clear_prep_recorder_required_count = count_kind(
            &steps,
            NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepRecorderRequired,
        );
        let all_alpha_mask_commands_recordable = steps
            .iter()
            .filter(|step| {
                matches!(
                    step.scheduled_kind,
                    NativeVulkanSceneLayerCompositorScheduledKind::AlphaMaskTokenDrawListCommand
                )
            })
            .all(|step| step.token_recording_step_index.is_some());
        let recording_blocks = recording_blocks_from_steps(&steps);
        let mesh_graph_draw_span_block_count = recording_blocks
            .iter()
            .filter(|block| {
                block.kind == NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan
            })
            .count();
        let alpha_mask_token_recording_block_count = recording_blocks
            .iter()
            .filter(|block| {
                block.kind
                    == NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep
            })
            .count();
        let no_draw_marker_block_count = recording_blocks
            .iter()
            .filter(|block| {
                block.kind == NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker
            })
            .count();
        Self {
            layer_count,
            command_count: steps.len(),
            direct_mesh_graph_command_count,
            object_final_producer_command_count,
            object_final_composite_command_count,
            alpha_mask_token_draw_list_command_count,
            token_program_no_draw_count,
            clear_prep_early_out_no_draw_count,
            clear_prep_recorder_required_count,
            recording_block_count: recording_blocks.len(),
            mesh_graph_draw_span_block_count,
            alpha_mask_token_recording_block_count,
            no_draw_marker_block_count,
            all_alpha_mask_commands_recordable,
            steps,
            recording_blocks,
            command_order: layer_compositor_schedule_command_order(),
        }
    }
}

fn layer_compositor_graph_target(
    target: SceneLayerCompositorTarget,
) -> Result<SceneGraphTarget, String> {
    match target {
        SceneLayerCompositorTarget::Swapchain => Ok(SceneGraphTarget::Swapchain),
        SceneLayerCompositorTarget::ObjectFinal(object) => {
            Ok(SceneGraphTarget::ObjectFinal(object))
        }
        SceneLayerCompositorTarget::ImageLayerCompositeA(object) => {
            Ok(SceneGraphTarget::ImageLayerCompositeA(object))
        }
        SceneLayerCompositorTarget::ImageLayerSource(object) => {
            Ok(SceneGraphTarget::ImageLayerSource(object))
        }
        SceneLayerCompositorTarget::FullAlphaMask => Ok(SceneGraphTarget::FullAlphaMask),
        SceneLayerCompositorTarget::FullAlphaMaskIntermediate => {
            Ok(SceneGraphTarget::FullAlphaMaskIntermediate)
        }
        SceneLayerCompositorTarget::LayerTarget490
        | SceneLayerCompositorTarget::EffectTarget3f8
        | SceneLayerCompositorTarget::FallbackImage400
        | SceneLayerCompositorTarget::DirectTarget2d8 => Err(format!(
            "scene layer compositor target {target:?} is not a graph target input"
        )),
    }
}

fn image_layer_prefill_graph_target(
    target: SceneLayerCompositorTarget,
) -> Option<SceneGraphTarget> {
    match target {
        SceneLayerCompositorTarget::ImageLayerCompositeA(object) => {
            Some(SceneGraphTarget::ImageLayerCompositeA(object))
        }
        SceneLayerCompositorTarget::ImageLayerSource(object) => {
            Some(SceneGraphTarget::ImageLayerSource(object))
        }
        SceneLayerCompositorTarget::Swapchain
        | SceneLayerCompositorTarget::ObjectFinal(_)
        | SceneLayerCompositorTarget::LayerTarget490
        | SceneLayerCompositorTarget::EffectTarget3f8
        | SceneLayerCompositorTarget::FallbackImage400
        | SceneLayerCompositorTarget::DirectTarget2d8
        | SceneLayerCompositorTarget::FullAlphaMask
        | SceneLayerCompositorTarget::FullAlphaMaskIntermediate => None,
    }
}

fn recording_blocks_from_steps(
    steps: &[NativeVulkanSceneLayerCompositorScheduleStep],
) -> Vec<NativeVulkanSceneLayerCompositorRecordingBlock> {
    let mut blocks = Vec::new();
    for (step_index, step) in steps.iter().enumerate() {
        if let Some(block) = blocks.last_mut()
            && extend_recording_block(block, step_index, step)
        {
            continue;
        }
        blocks.push(recording_block_from_step(blocks.len(), step_index, step));
    }
    blocks
}

fn extend_recording_block(
    block: &mut NativeVulkanSceneLayerCompositorRecordingBlock,
    step_index: usize,
    step: &NativeVulkanSceneLayerCompositorScheduleStep,
) -> bool {
    if block.kind != NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan
        || !matches!(
            step.scheduled_kind,
            NativeVulkanSceneLayerCompositorScheduledKind::DirectMeshGraphDraw
                | NativeVulkanSceneLayerCompositorScheduledKind::ObjectFinalCompositeGraphDraw
        )
        || block.graph_pass_index != step.graph_pass_index
        || block.graph_draw_index_end != step.graph_draw_index
    {
        return false;
    }
    let Some(draw_index) = step.graph_draw_index else {
        return false;
    };
    block.step_index_end = step_index.saturating_add(1);
    block.command_count = block.command_count.saturating_add(1);
    block.graph_draw_index_end = Some(draw_index.saturating_add(1));
    block
        .command_order
        .push("extend_contiguous_mesh_graph_draw_span");
    true
}

fn recording_block_from_step(
    block_index: usize,
    step_index: usize,
    step: &NativeVulkanSceneLayerCompositorScheduleStep,
) -> NativeVulkanSceneLayerCompositorRecordingBlock {
    let kind = recording_block_kind(step.scheduled_kind);
    let graph_draw_index_end = recording_block_records_mesh_draw_span(kind)
        .then(|| step.graph_draw_index.map(|draw| draw.saturating_add(1)))
        .flatten();
    NativeVulkanSceneLayerCompositorRecordingBlock {
        block_index,
        step_index_start: step_index,
        step_index_end: step_index.saturating_add(1),
        command_count: 1,
        kind,
        graph_pass_index: step.graph_pass_index,
        graph_draw_index_start: step.graph_draw_index,
        graph_draw_index_end,
        token_recording_step_index: step.token_recording_step_index,
        command_order: recording_block_command_order(kind),
    }
}

fn recording_block_records_mesh_draw_span(
    kind: NativeVulkanSceneLayerCompositorRecordingBlockKind,
) -> bool {
    matches!(
        kind,
        NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan
            | NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime
    )
}

fn recording_block_kind(
    kind: NativeVulkanSceneLayerCompositorScheduledKind,
) -> NativeVulkanSceneLayerCompositorRecordingBlockKind {
    match kind {
        NativeVulkanSceneLayerCompositorScheduledKind::DirectMeshGraphDraw
        | NativeVulkanSceneLayerCompositorScheduledKind::ObjectFinalCompositeGraphDraw => {
            NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan
        }
        NativeVulkanSceneLayerCompositorScheduledKind::ObjectFinalProducerEffectRuntime => {
            NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime
        }
        NativeVulkanSceneLayerCompositorScheduledKind::AlphaMaskTokenProgramNoDraw => {
            NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker
        }
        NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepEarlyOutNoDraw => {
            NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker
        }
        NativeVulkanSceneLayerCompositorScheduledKind::AlphaMaskTokenDrawListCommand => {
            NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep
        }
        NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepRecorderRequired => {
            NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired
        }
    }
}

fn recording_block_command_order(
    kind: NativeVulkanSceneLayerCompositorRecordingBlockKind,
) -> Vec<&'static str> {
    match kind {
        NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan => vec![
            "begin_or_reuse_graph_target_scope",
            "record_contiguous_mesh_graph_draw_span",
        ],
        NativeVulkanSceneLayerCompositorRecordingBlockKind::ObjectFinalProducerEffectRuntime => {
            vec![
                "record_optional_image_layer_prefill_mesh_draw",
                "preserve_layer_final_effect_runtime_before_composite",
            ]
        }
        NativeVulkanSceneLayerCompositorRecordingBlockKind::AlphaMaskTokenDrawListStep => vec![
            "record_single_alpha_mask_token_step_at_layer_position",
            "do_not_record_entire_token_stream_out_of_order",
        ],
        NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker => {
            vec!["preserve_no_draw_layer_marker_position"]
        }
        NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired => {
            vec!["require_layer_target_clear_prep_recorder"]
        }
    }
}

fn count_kind(
    steps: &[NativeVulkanSceneLayerCompositorScheduleStep],
    kind: NativeVulkanSceneLayerCompositorScheduledKind,
) -> usize {
    steps
        .iter()
        .filter(|step| step.scheduled_kind == kind)
        .count()
}

fn validate_graph_execution_identity(
    graph: &SceneGraph,
    graph_execution: &SceneGraphExecutionPlan,
) -> Result<(), String> {
    if graph_execution.pass_count != graph.passes.len() {
        return Err(format!(
            "scene layer compositor scheduler graph pass count drift: graph has {}, execution has {}",
            graph.passes.len(),
            graph_execution.pass_count
        ));
    }
    if graph_execution.passes.len() != graph.passes.len() {
        return Err(format!(
            "scene layer compositor scheduler execution pass list drift: graph has {}, execution has {}",
            graph.passes.len(),
            graph_execution.passes.len()
        ));
    }
    for execution_pass in &graph_execution.passes {
        let graph_pass = graph.passes.get(execution_pass.pass_index).ok_or_else(|| {
            format!(
                "scene layer compositor scheduler execution pass index {} outside graph",
                execution_pass.pass_index
            )
        })?;
        if graph_pass.input != execution_pass.input || graph_pass.output != execution_pass.output {
            return Err(format!(
                "scene layer compositor scheduler pass {} target drift: graph {:?}->{:?}, execution {:?}->{:?}",
                execution_pass.pass_index,
                graph_pass.input,
                graph_pass.output,
                execution_pass.input,
                execution_pass.output
            ));
        }
        if graph_pass.draws.len() != execution_pass.draw_count {
            return Err(format!(
                "scene layer compositor scheduler pass {} draw count drift: graph has {}, execution has {}",
                execution_pass.pass_index,
                graph_pass.draws.len(),
                execution_pass.draw_count
            ));
        }
    }
    Ok(())
}

fn find_graph_draw_position(
    graph: &SceneGraph,
    graph_execution: &SceneGraphExecutionPlan,
    object: SceneObjectId,
    input: Option<SceneGraphTarget>,
    output: SceneGraphTarget,
) -> Result<NativeVulkanSceneLayerCompositorGraphDrawPosition, String> {
    let mut found = None;
    for execution_pass in &graph_execution.passes {
        if execution_pass.input != input || execution_pass.output != output {
            continue;
        }
        let graph_pass = &graph.passes[execution_pass.pass_index];
        for (local_draw_index, draw) in graph_pass.draws.iter().enumerate() {
            if draw.object != object {
                continue;
            }
            let draw_index = execution_pass
                .draw_index_start
                .checked_add(local_draw_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer compositor scheduler graph draw index overflow for object {object:?}"
                    )
                })?;
            let position = NativeVulkanSceneLayerCompositorGraphDrawPosition {
                pass_index: execution_pass.pass_index,
                draw_index,
            };
            if found.replace(position).is_some() {
                return Err(format!(
                    "scene layer compositor scheduler found duplicate graph draws for object {object:?} in {:?}->{:?}",
                    input, output
                ));
            }
        }
    }
    found.ok_or_else(|| {
        format!(
            "scene layer compositor scheduler cannot join object {object:?} to graph draw {:?}->{:?}",
            input, output
        )
    })
}

fn token_steps_by_command_index(
    token_recording: &NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
) -> Result<BTreeMap<usize, (usize, &NativeVulkanSceneLayerAlphaMaskTokenRecordingStep)>, String> {
    let mut steps = BTreeMap::new();
    for (step_index, step) in token_recording.steps.iter().enumerate() {
        if let Some((previous_step_index, _)) = steps.insert(step.command_index, (step_index, step))
        {
            return Err(format!(
                "scene layer compositor scheduler duplicate alpha-mask token command index {} at steps {} and {}",
                step.command_index, previous_step_index, step_index
            ));
        }
    }
    Ok(steps)
}

fn require_token_step(
    command_index: usize,
    object: SceneObjectId,
    operation: SceneLayerCompositorOperation,
    expected: NativeVulkanSceneLayerAlphaMaskTokenRecordingKind,
    token_steps: &BTreeMap<usize, (usize, &NativeVulkanSceneLayerAlphaMaskTokenRecordingStep)>,
) -> Result<NativeVulkanSceneLayerCompositorTokenMatch, String> {
    let (step_index, step) = token_steps.get(&command_index).ok_or_else(|| {
        format!(
            "scene layer compositor scheduler command {command_index} {:?} has no alpha-mask token recording step",
            operation
        )
    })?;
    if step.object != object || step.operation != operation {
        return Err(format!(
            "scene layer compositor scheduler command {command_index} token identity drift: layer {:?}/{:?}, token {:?}/{:?}",
            object, operation, step.object, step.operation
        ));
    }
    if step.recording_kind != expected {
        return Err(format!(
            "scene layer compositor scheduler command {command_index} {:?} expected alpha-mask recording {:?}, got {:?}",
            operation, expected, step.recording_kind
        ));
    }
    Ok(NativeVulkanSceneLayerCompositorTokenMatch {
        step_index: *step_index,
    })
}

fn expected_alpha_mask_recording_kind(
    operation: SceneLayerCompositorOperation,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenRecordingKind, String> {
    match operation {
        SceneLayerCompositorOperation::DrawClippingMask => Ok(
            NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::ClippingMaskImage4ProducerRtMethod8,
        ),
        SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask => {
            Ok(NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::FlatTextureCopyBackGraphNode)
        }
        SceneLayerCompositorOperation::DrawGeneratedClippingTarget => {
            Ok(NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::GeneratedClippingTargetRtMethod8)
        }
        operation => Err(format!(
            "scene layer compositor scheduler operation {operation:?} is not an alpha-mask draw command"
        )),
    }
}

fn layer_compositor_schedule_command_order() -> [&'static str; 8] {
    [
        "read_scene_layer_compositor_order",
        "join_direct_layers_to_mesh_graph_draws",
        "join_object_final_producers_to_effect_runtime",
        "join_object_final_composites_to_graph_passes",
        "join_tokenized_commands_to_alpha_mask_token_recording",
        "coalesce_consecutive_mesh_graph_draws_into_recording_blocks",
        "reject_missing_alpha_mask_token_draw_list_steps",
        "emit_schedule_for_present_frame_recorder",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphDraw, SceneGraphPass,
        SceneGraphPipelineClass, SceneLayerCompositorBlendKey, SceneLayerCompositorCommand,
        SceneLayerCompositorCondition, SceneLayerCompositorLayer, SceneLayerCompositorTarget,
        SceneMaterialKey, SceneMaterialRenderState, SceneResourceId,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind;

    #[test]
    fn scheduler_joins_direct_layer_to_mesh_graph_draw() {
        let object = SceneObjectId(7);
        let graph = SceneGraph {
            passes: vec![graph_pass(
                "scene-main",
                None,
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(object)],
            )],
        };
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);
        let schedule = native_vulkan_plan_scene_layer_compositor_schedule(
            &layer_compositor(vec![direct_layer(object)]),
            &graph,
            &graph_execution,
            &empty_token_recording(),
        )
        .expect("layer compositor schedule");

        assert_eq!(schedule.command_count, 1);
        assert_eq!(schedule.direct_mesh_graph_command_count, 1);
        assert_eq!(
            schedule.steps[0].scheduled_kind,
            NativeVulkanSceneLayerCompositorScheduledKind::DirectMeshGraphDraw
        );
        assert_eq!(schedule.steps[0].graph_pass_index, Some(0));
        assert_eq!(schedule.steps[0].graph_draw_index, Some(0));
        assert_eq!(schedule.recording_block_count, 1);
        assert_eq!(
            schedule.recording_blocks[0].kind,
            NativeVulkanSceneLayerCompositorRecordingBlockKind::MeshGraphDrawSpan
        );
        assert_eq!(schedule.recording_blocks[0].graph_draw_index_start, Some(0));
        assert_eq!(schedule.recording_blocks[0].graph_draw_index_end, Some(1));
    }

    #[test]
    fn scheduler_coalesces_consecutive_direct_mesh_draws_into_one_recording_block() {
        let first = SceneObjectId(7);
        let second = SceneObjectId(8);
        let graph = SceneGraph {
            passes: vec![graph_pass(
                "scene-main",
                None,
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(first), mesh_draw(second)],
            )],
        };
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);
        let schedule = native_vulkan_plan_scene_layer_compositor_schedule(
            &layer_compositor(vec![direct_layer(first), direct_layer(second)]),
            &graph,
            &graph_execution,
            &empty_token_recording(),
        )
        .expect("layer compositor schedule");

        assert_eq!(schedule.command_count, 2);
        assert_eq!(schedule.recording_block_count, 1);
        assert_eq!(schedule.mesh_graph_draw_span_block_count, 1);
        assert_eq!(schedule.recording_blocks[0].command_count, 2);
        assert_eq!(schedule.recording_blocks[0].graph_pass_index, Some(0));
        assert_eq!(schedule.recording_blocks[0].graph_draw_index_start, Some(0));
        assert_eq!(schedule.recording_blocks[0].graph_draw_index_end, Some(2));
        assert!(
            schedule.recording_blocks[0]
                .command_order
                .contains(&"extend_contiguous_mesh_graph_draw_span")
        );
    }

    #[test]
    fn scheduler_treats_object_final_clear_prep_without_aux_target_as_early_out_marker() {
        let object = SceneObjectId(9);
        let graph = SceneGraph {
            passes: vec![graph_pass(
                "object-final-composite",
                Some(SceneGraphTarget::ObjectFinal(object)),
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(object)],
            )],
        };
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);
        let schedule = native_vulkan_plan_scene_layer_compositor_schedule(
            &layer_compositor(vec![object_final_layer(object, false)]),
            &graph,
            &graph_execution,
            &empty_token_recording(),
        )
        .expect("object-final clear prep early-out schedule");

        assert_eq!(schedule.command_count, 3);
        assert_eq!(schedule.object_final_producer_command_count, 1);
        assert_eq!(schedule.object_final_composite_command_count, 1);
        assert_eq!(schedule.clear_prep_early_out_no_draw_count, 1);
        assert_eq!(schedule.clear_prep_recorder_required_count, 0);
        assert_eq!(
            schedule.steps[1].scheduled_kind,
            NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepEarlyOutNoDraw
        );
        assert_eq!(
            schedule.recording_blocks[1].kind,
            NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker
        );
    }

    #[test]
    fn scheduler_joins_image_layer_composite_source_to_final_mesh_pass() {
        let object = SceneObjectId(1530);
        let graph = SceneGraph {
            passes: vec![
                graph_pass(
                    "image-layer-prefill",
                    None,
                    SceneGraphTarget::ImageLayerSource(object),
                    vec![mesh_draw(object)],
                ),
                graph_pass(
                    "image-layer-final-composite",
                    Some(SceneGraphTarget::ImageLayerCompositeA(object)),
                    SceneGraphTarget::Swapchain,
                    vec![mesh_draw(object)],
                ),
            ],
        };
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);

        let schedule = native_vulkan_plan_scene_layer_compositor_schedule(
            &layer_compositor(vec![image_layer_composite_layer(object)]),
            &graph,
            &graph_execution,
            &empty_token_recording(),
        )
        .expect("image-layer composite schedule");

        assert_eq!(schedule.object_final_composite_command_count, 1);
        assert_eq!(schedule.steps[0].graph_pass_index, Some(0));
        assert_eq!(schedule.steps[0].graph_draw_index, Some(0));
        assert!(
            schedule.steps[0]
                .command_order
                .contains(&"join_image_layer_prefill_to_mesh_graph_pass")
        );
        assert_eq!(schedule.steps[2].graph_pass_index, Some(1));
        assert_eq!(schedule.steps[2].graph_draw_index, Some(1));
        assert!(
            schedule.steps[2]
                .command_order
                .contains(&"join_layer_final_input_to_swapchain_graph_pass")
        );
    }

    #[test]
    fn scheduler_treats_tokenized_clear_prep_without_aux_target_as_early_out_marker() {
        let object = SceneObjectId(10);
        let graph = SceneGraph {
            passes: vec![graph_pass(
                "object-final-composite",
                Some(SceneGraphTarget::ObjectFinal(object)),
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(object)],
            )],
        };
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);

        let schedule = native_vulkan_plan_scene_layer_compositor_schedule(
            &layer_compositor(vec![object_final_layer(object, true)]),
            &graph,
            &graph_execution,
            &token_recording_for_tokenized_layer_with_start(object, 2),
        )
        .expect("tokenized object-final clear prep schedule");

        assert_eq!(schedule.clear_prep_early_out_no_draw_count, 1);
        assert_eq!(schedule.clear_prep_recorder_required_count, 0);
        assert_eq!(
            schedule.steps[1].scheduled_kind,
            NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepEarlyOutNoDraw
        );
        assert_eq!(
            schedule.recording_blocks[1].kind,
            NativeVulkanSceneLayerCompositorRecordingBlockKind::NoDrawLayerMarker
        );
    }

    #[test]
    fn scheduler_keeps_aux_clear_prep_as_active_recorder_required() {
        let object = SceneObjectId(10);
        let graph = SceneGraph {
            passes: vec![graph_pass(
                "object-final-composite",
                Some(SceneGraphTarget::ObjectFinal(object)),
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(object)],
            )],
        };
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);
        let mut layer = object_final_layer(object, true);
        layer.has_active_aux_clear_target = true;

        let schedule = native_vulkan_plan_scene_layer_compositor_schedule(
            &layer_compositor(vec![layer]),
            &graph,
            &graph_execution,
            &token_recording_for_tokenized_layer_with_start(object, 2),
        )
        .expect("active aux clear prep schedule");

        assert_eq!(schedule.clear_prep_early_out_no_draw_count, 0);
        assert_eq!(schedule.clear_prep_recorder_required_count, 1);
        assert_eq!(
            schedule.steps[1].scheduled_kind,
            NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepRecorderRequired
        );
        assert_eq!(
            schedule.recording_blocks[1].kind,
            NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired
        );
    }

    #[test]
    fn scheduler_joins_token_commands_to_alpha_mask_recording_steps() {
        let object = SceneObjectId(12);
        let graph = SceneGraph {
            passes: vec![graph_pass(
                "scene-main",
                None,
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(object)],
            )],
        };
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);
        let schedule = native_vulkan_plan_scene_layer_compositor_schedule(
            &layer_compositor(vec![tokenized_direct_layer(object)]),
            &graph,
            &graph_execution,
            &token_recording_for_tokenized_layer(object),
        )
        .expect("layer compositor schedule");

        assert_eq!(schedule.command_count, 6);
        assert_eq!(schedule.direct_mesh_graph_command_count, 1);
        assert_eq!(schedule.token_program_no_draw_count, 1);
        assert_eq!(schedule.alpha_mask_token_draw_list_command_count, 4);
        assert_eq!(schedule.recording_block_count, 6);
        assert_eq!(schedule.mesh_graph_draw_span_block_count, 1);
        assert_eq!(schedule.alpha_mask_token_recording_block_count, 4);
        assert_eq!(schedule.no_draw_marker_block_count, 1);
        assert!(schedule.all_alpha_mask_commands_recordable);
        assert_eq!(
            schedule.steps[1].scheduled_kind,
            NativeVulkanSceneLayerCompositorScheduledKind::AlphaMaskTokenProgramNoDraw
        );
        assert_eq!(schedule.steps[1].token_recording_step_index, Some(0));
        assert_eq!(schedule.steps[5].token_recording_step_index, Some(4));
    }

    #[test]
    fn scheduler_rejects_token_step_without_layer_command_position() {
        let object = SceneObjectId(3);
        let graph = SceneGraph {
            passes: vec![graph_pass(
                "scene-main",
                None,
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(object)],
            )],
        };
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);
        let mut token_recording = empty_token_recording();
        token_recording.steps.push(token_recording_step(
            10,
            object,
            SceneLayerCompositorOperation::TokenProgramDispatch,
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch,
            NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::TokenProgramNoDraw,
        ));
        token_recording.scheduled_step_count = 1;
        token_recording.no_draw_step_count = 1;

        let err = native_vulkan_plan_scene_layer_compositor_schedule(
            &layer_compositor(vec![direct_layer(object)]),
            &graph,
            &graph_execution,
            &token_recording,
        )
        .expect_err("orphan token step must fail");

        assert!(err.contains("without WE layer command positions"));
    }

    fn layer_compositor(layers: Vec<SceneLayerCompositorLayer>) -> SceneLayerCompositorPlan {
        let command_count = layers.iter().map(|layer| layer.commands.len()).sum();
        let object_final_layer_count = layers
            .iter()
            .filter(|layer| layer.route == SceneLayerCompositorRoute::ObjectFinalMeshComposite)
            .count();
        let tokenized_layer_count = layers
            .iter()
            .filter(|layer| layer.uses_tokenized_subdraw)
            .count();
        SceneLayerCompositorPlan {
            layer_count: layers.len(),
            command_count,
            object_final_layer_count,
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
            commands: vec![normal_command(SceneLayerCompositorTarget::Swapchain)],
        }
    }

    fn tokenized_direct_layer(object: SceneObjectId) -> SceneLayerCompositorLayer {
        let mut layer = direct_layer(object);
        layer.uses_tokenized_subdraw = true;
        layer.commands.extend(token_commands());
        layer
    }

    fn object_final_layer(
        object: SceneObjectId,
        uses_tokenized_subdraw: bool,
    ) -> SceneLayerCompositorLayer {
        let mut commands = vec![
            normal_command(SceneLayerCompositorTarget::ObjectFinal(object)),
            SceneLayerCompositorCommand {
                entry: SceneLayerCompositorEntry::ClearPrepEntry50,
                operation: SceneLayerCompositorOperation::ClearPrep,
                condition: SceneLayerCompositorCondition::Always,
                source: None,
                target: SceneLayerCompositorTarget::LayerTarget490,
                blend_key: SceneLayerCompositorBlendKey::Inherit,
            },
        ];
        if uses_tokenized_subdraw {
            commands.extend(token_commands());
        }
        commands.push(SceneLayerCompositorCommand {
            entry: SceneLayerCompositorEntry::FullLayerCompositeEntry51,
            operation: SceneLayerCompositorOperation::FullLayerComposite,
            condition: SceneLayerCompositorCondition::Always,
            source: Some(SceneLayerCompositorTarget::ObjectFinal(object)),
            target: SceneLayerCompositorTarget::Swapchain,
            blend_key: SceneLayerCompositorBlendKey::LowBlendNormalViaWrapper128,
        });
        SceneLayerCompositorLayer {
            object,
            route: SceneLayerCompositorRoute::ObjectFinalMeshComposite,
            uses_tokenized_subdraw,
            has_active_aux_clear_target: false,
            commands,
        }
    }

    fn image_layer_composite_layer(object: SceneObjectId) -> SceneLayerCompositorLayer {
        SceneLayerCompositorLayer {
            object,
            route: SceneLayerCompositorRoute::ObjectFinalMeshComposite,
            uses_tokenized_subdraw: false,
            has_active_aux_clear_target: false,
            commands: vec![
                normal_command(SceneLayerCompositorTarget::ImageLayerSource(object)),
                SceneLayerCompositorCommand {
                    entry: SceneLayerCompositorEntry::ClearPrepEntry50,
                    operation: SceneLayerCompositorOperation::ClearPrep,
                    condition: SceneLayerCompositorCondition::Always,
                    source: None,
                    target: SceneLayerCompositorTarget::LayerTarget490,
                    blend_key: SceneLayerCompositorBlendKey::Inherit,
                },
                SceneLayerCompositorCommand {
                    entry: SceneLayerCompositorEntry::FullLayerCompositeEntry51,
                    operation: SceneLayerCompositorOperation::FullLayerComposite,
                    condition: SceneLayerCompositorCondition::Always,
                    source: Some(SceneLayerCompositorTarget::ImageLayerCompositeA(object)),
                    target: SceneLayerCompositorTarget::Swapchain,
                    blend_key: SceneLayerCompositorBlendKey::LowBlendNormalViaWrapper128,
                },
            ],
        }
    }

    fn normal_command(target: SceneLayerCompositorTarget) -> SceneLayerCompositorCommand {
        SceneLayerCompositorCommand {
            entry: SceneLayerCompositorEntry::NormalRenderEntry32,
            operation: SceneLayerCompositorOperation::NormalRender,
            condition: SceneLayerCompositorCondition::Always,
            source: None,
            target,
            blend_key: SceneLayerCompositorBlendKey::WrapperPushBlendEnumAndAlphaWriteBits0x2000x8,
        }
    }

    fn token_commands() -> Vec<SceneLayerCompositorCommand> {
        vec![
            SceneLayerCompositorCommand {
                entry: SceneLayerCompositorEntry::TokenizedCompositeEntry52,
                operation: SceneLayerCompositorOperation::TokenProgramDispatch,
                condition: SceneLayerCompositorCondition::Always,
                source: None,
                target: SceneLayerCompositorTarget::LayerTarget490,
                blend_key: SceneLayerCompositorBlendKey::Inherit,
            },
            SceneLayerCompositorCommand {
                entry: SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
                operation: SceneLayerCompositorOperation::DrawClippingMask,
                condition: SceneLayerCompositorCondition::Token1OrToken2FirstPair,
                source: None,
                target: SceneLayerCompositorTarget::FullAlphaMask,
                blend_key: SceneLayerCompositorBlendKey::Inherit,
            },
            SceneLayerCompositorCommand {
                entry: SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
                operation: SceneLayerCompositorOperation::DrawClippingMask,
                condition: SceneLayerCompositorCondition::Token2IntermediatePairOrFinalMask,
                source: None,
                target: SceneLayerCompositorTarget::FullAlphaMaskIntermediate,
                blend_key: SceneLayerCompositorBlendKey::Inherit,
            },
            SceneLayerCompositorCommand {
                entry: SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed,
                operation: SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
                condition: SceneLayerCompositorCondition::Token2AfterIntermediateMask,
                source: Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
                target: SceneLayerCompositorTarget::FullAlphaMask,
                blend_key: SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
            },
            SceneLayerCompositorCommand {
                entry: SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53,
                operation: SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
                condition: SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
                source: Some(SceneLayerCompositorTarget::FullAlphaMask),
                target: SceneLayerCompositorTarget::LayerTarget490,
                blend_key: SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0,
            },
        ]
    }

    fn token_recording_for_tokenized_layer(
        object: SceneObjectId,
    ) -> NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan {
        token_recording_for_tokenized_layer_with_start(object, 1)
    }

    fn token_recording_for_tokenized_layer_with_start(
        object: SceneObjectId,
        first_command_index: usize,
    ) -> NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan {
        let steps = vec![
            token_recording_step(
                first_command_index,
                object,
                SceneLayerCompositorOperation::TokenProgramDispatch,
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch,
                NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::TokenProgramNoDraw,
            ),
            token_recording_step(
                first_command_index + 1,
                object,
                SceneLayerCompositorOperation::DrawClippingMask,
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer,
                NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::ClippingMaskImage4ProducerRtMethod8,
            ),
            token_recording_step(
                first_command_index + 2,
                object,
                SceneLayerCompositorOperation::DrawClippingMask,
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer,
                NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::ClippingMaskImage4ProducerRtMethod8,
            ),
            token_recording_step(
                first_command_index + 3,
                object,
                SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask,
                NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::FlatTextureCopyBackGraphNode,
            ),
            token_recording_step(
                first_command_index + 4,
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
            resources: vec![crate::engine::scene_engine::SceneGraphResourceBinding {
                slot: 0,
                role: crate::engine::scene_engine::SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(object.0),
            }],
            index_count: 6,
        }
    }
}

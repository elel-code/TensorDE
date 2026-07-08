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
    SceneObjectId,
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
    pub clear_prep_recorder_required_count: usize,
    pub all_alpha_mask_commands_recordable: bool,
    pub steps: Vec<NativeVulkanSceneLayerCompositorScheduleStep>,
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
                        command_order.extend([
                            "join_object_final_layer_to_effect_runtime_output",
                            "preserve_object_final_before_swapchain_composite",
                        ]);
                        NativeVulkanSceneLayerCompositorScheduledKind::ObjectFinalProducerEffectRuntime
                    }
                },
                SceneLayerCompositorOperation::ClearPrep => {
                    command_order.extend([
                        "require_layer_target_clear_recorder",
                        "keep_clear_step_in_we_layer_order",
                    ]);
                    NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepRecorderRequired
                }
                SceneLayerCompositorOperation::FullLayerComposite => {
                    let position = find_graph_draw_position(
                        graph,
                        graph_execution,
                        layer.object,
                        Some(SceneGraphTarget::ObjectFinal(layer.object)),
                        SceneGraphTarget::Swapchain,
                    )?;
                    graph_pass_index = Some(position.pass_index);
                    graph_draw_index = Some(position.draw_index);
                    command_order.extend([
                        "join_object_final_input_to_swapchain_graph_pass",
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
        Self {
            layer_count,
            command_count: steps.len(),
            direct_mesh_graph_command_count,
            object_final_producer_command_count,
            object_final_composite_command_count,
            alpha_mask_token_draw_list_command_count,
            token_program_no_draw_count,
            clear_prep_recorder_required_count,
            all_alpha_mask_commands_recordable,
            steps,
            command_order: layer_compositor_schedule_command_order(),
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
        "preserve_we_layer_order_before_present_frame_recording",
        "reject_missing_alpha_mask_token_draw_list_steps",
        "report_required_clear_recorders",
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
            commands: vec![normal_command(SceneLayerCompositorTarget::Swapchain)],
        }
    }

    fn tokenized_direct_layer(object: SceneObjectId) -> SceneLayerCompositorLayer {
        let mut layer = direct_layer(object);
        layer.uses_tokenized_subdraw = true;
        layer.commands.extend(token_commands());
        layer
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
            resources: vec![crate::engine::scene_engine::SceneGraphResourceBinding {
                slot: 0,
                role: crate::engine::scene_engine::SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(object.0),
            }],
            index_count: 6,
        }
    }
}

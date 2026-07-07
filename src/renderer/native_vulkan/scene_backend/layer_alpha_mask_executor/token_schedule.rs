//! Token-stream scheduler for WE layer alpha-mask producer/copy-back/consumer work.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use std::collections::BTreeMap;

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneGraphTarget, SceneLayerCompositorOperation, SceneLayerCompositorTarget, SceneObjectId,
};

use super::NativeVulkanSceneLayerAlphaMaskRuntimePlan;
use super::resource_binds::{
    NativeVulkanSceneLayerAlphaMaskBindRequirement,
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskTokenCommandResourceBindPlan,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan {
    pub command_count: usize,
    pub scheduled_step_count: usize,
    pub token_program_dispatch_count: usize,
    pub full_mask_producer_count: usize,
    pub intermediate_mask_producer_count: usize,
    pub copy_back_after_intermediate_count: usize,
    pub generated_target_consumer_count: usize,
    pub recorder_ready_step_count: usize,
    pub missing_recorder_step_count: usize,
    pub clippingmaskimage4_pending_recorder_count: usize,
    pub generated_clippingtarget_pending_recorder_count: usize,
    pub steps: Vec<NativeVulkanSceneLayerAlphaMaskTokenScheduleStep>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTokenScheduleStep {
    pub command_index: usize,
    pub object: SceneObjectId,
    pub operation: SceneLayerCompositorOperation,
    pub source: Option<SceneLayerCompositorTarget>,
    pub target: SceneLayerCompositorTarget,
    pub kind: NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
    pub matched_heap_bind_count: usize,
    pub matched_heap_bind_indices: Vec<usize>,
    pub recording_status: NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus,
    pub full_mask_ready_after: bool,
    pub intermediate_mask_ready_after: bool,
    pub command_order: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind {
    TokenProgramDispatch,
    FullMaskProducer,
    IntermediateMaskProducer,
    IntermediateCopyBackToFullMask,
    GeneratedClippingTargetConsumer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus {
    TokenProgramNoDraw,
    PendingClippingMaskImage4ProducerRecorder,
    ReadyFlatTextureCopyBackGraphNode,
    PendingGeneratedClippingTargetRecorder,
}

#[derive(Debug, Clone, Copy, Default)]
struct NativeVulkanSceneLayerAlphaMaskObjectReadiness {
    full_mask_ready: bool,
    intermediate_mask_ready: bool,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_token_schedule(
    runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan, String> {
    if runtime.tokenized_layer_count == 0 {
        return Ok(NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan::empty());
    }
    if resource_binds.token_command_count != runtime.commands.len() {
        return Err(format!(
            "scene layer alpha-mask scheduler requires one resource bind fact per token command, got {} binds for {} commands",
            resource_binds.token_command_count,
            runtime.commands.len()
        ));
    }

    let mut states =
        BTreeMap::<SceneObjectId, NativeVulkanSceneLayerAlphaMaskObjectReadiness>::new();
    let mut steps = Vec::with_capacity(runtime.commands.len());
    for (command_index, command) in runtime.commands.iter().enumerate() {
        let bind = resource_binds
            .token_commands
            .iter()
            .find(|bind| bind.command_index == command_index)
            .ok_or_else(|| {
                format!(
                    "scene layer alpha-mask scheduler command {command_index} has no token bind fact"
                )
            })?;
        validate_bind_fact(command_index, command.object, command.operation, bind)?;
        let state = states.entry(command.object).or_default();
        let kind = schedule_step_kind(command_index, command, state)?;
        let matched_heap_bind_count = bind.matched_bind_count;
        validate_bind_readiness(command_index, kind, matched_heap_bind_count)?;
        let recording_status = recording_status_for_kind(kind);
        steps.push(NativeVulkanSceneLayerAlphaMaskTokenScheduleStep {
            command_index,
            object: command.object,
            operation: command.operation,
            source: command.source,
            target: command.target,
            kind,
            matched_heap_bind_count,
            matched_heap_bind_indices: bind.matched_heap_bind_indices.clone(),
            recording_status,
            full_mask_ready_after: state.full_mask_ready,
            intermediate_mask_ready_after: state.intermediate_mask_ready,
            command_order: schedule_step_command_order(kind, recording_status),
        });
    }

    Ok(NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan::from_steps(steps))
}

impl NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan {
    fn empty() -> Self {
        Self {
            command_count: 0,
            scheduled_step_count: 0,
            token_program_dispatch_count: 0,
            full_mask_producer_count: 0,
            intermediate_mask_producer_count: 0,
            copy_back_after_intermediate_count: 0,
            generated_target_consumer_count: 0,
            recorder_ready_step_count: 0,
            missing_recorder_step_count: 0,
            clippingmaskimage4_pending_recorder_count: 0,
            generated_clippingtarget_pending_recorder_count: 0,
            steps: Vec::new(),
            command_order: [
                "read_alpha_mask_token_stream",
                "match_token_commands_to_heap_bind_facts",
                "track_full_and_intermediate_mask_readiness",
                "place_clippingmaskimage4_producer_steps",
                "place_flattexture_copy_back_after_intermediate",
                "place_generated_clippingtarget_after_full_mask",
            ],
        }
    }

    fn from_steps(steps: Vec<NativeVulkanSceneLayerAlphaMaskTokenScheduleStep>) -> Self {
        Self {
            command_count: steps.len(),
            scheduled_step_count: steps.len(),
            token_program_dispatch_count: steps
                .iter()
                .filter(|step| {
                    step.kind
                        == NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch
                })
                .count(),
            full_mask_producer_count: steps
                .iter()
                .filter(|step| {
                    step.kind
                        == NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer
                })
                .count(),
            intermediate_mask_producer_count: steps
                .iter()
                .filter(|step| {
                    step.kind
                        == NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer
                })
                .count(),
            copy_back_after_intermediate_count: steps
                .iter()
                .filter(|step| {
                    step.kind
                        == NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask
                })
                .count(),
            generated_target_consumer_count: steps
                .iter()
                .filter(|step| {
                    step.kind
                        == NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer
                })
                .count(),
            recorder_ready_step_count: steps
                .iter()
                .filter(|step| {
                    matches!(
                        step.recording_status,
                        NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::TokenProgramNoDraw
                            | NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::ReadyFlatTextureCopyBackGraphNode
                    )
                })
                .count(),
            missing_recorder_step_count: steps
                .iter()
                .filter(|step| {
                    matches!(
                        step.recording_status,
                        NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingClippingMaskImage4ProducerRecorder
                            | NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingGeneratedClippingTargetRecorder
                    )
                })
                .count(),
            clippingmaskimage4_pending_recorder_count: steps
                .iter()
                .filter(|step| {
                    step.recording_status
                        == NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingClippingMaskImage4ProducerRecorder
                })
                .count(),
            generated_clippingtarget_pending_recorder_count: steps
                .iter()
                .filter(|step| {
                    step.recording_status
                        == NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingGeneratedClippingTargetRecorder
                })
                .count(),
            steps,
            command_order: [
                "read_alpha_mask_token_stream",
                "match_token_commands_to_heap_bind_facts",
                "track_full_and_intermediate_mask_readiness",
                "place_clippingmaskimage4_producer_steps",
                "place_flattexture_copy_back_after_intermediate",
                "place_generated_clippingtarget_after_full_mask",
            ],
        }
    }
}

fn validate_bind_fact(
    command_index: usize,
    object: SceneObjectId,
    operation: SceneLayerCompositorOperation,
    bind: &NativeVulkanSceneLayerAlphaMaskTokenCommandResourceBindPlan,
) -> Result<(), String> {
    if bind.object != object || bind.operation != operation {
        return Err(format!(
            "scene layer alpha-mask scheduler token bind mismatch at command {command_index}: command {:?}/{:?}, bind {:?}/{:?}",
            object, operation, bind.object, bind.operation
        ));
    }
    let expected = bind_requirement_for_scheduler(operation)?;
    if bind.requirement != expected {
        return Err(format!(
            "scene layer alpha-mask scheduler command {command_index} expected {:?} bind fact, got {:?}",
            expected, bind.requirement
        ));
    }
    Ok(())
}

fn bind_requirement_for_scheduler(
    operation: SceneLayerCompositorOperation,
) -> Result<NativeVulkanSceneLayerAlphaMaskBindRequirement, String> {
    match operation {
        SceneLayerCompositorOperation::TokenProgramDispatch => {
            Ok(NativeVulkanSceneLayerAlphaMaskBindRequirement::TokenProgramNoResourceBind)
        }
        SceneLayerCompositorOperation::DrawClippingMask => {
            Ok(NativeVulkanSceneLayerAlphaMaskBindRequirement::ClippingMaskImage4)
        }
        SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask => {
            Ok(NativeVulkanSceneLayerAlphaMaskBindRequirement::FlatTextureCopyBackSeparateDrawResourceBind)
        }
        SceneLayerCompositorOperation::DrawGeneratedClippingTarget => {
            Ok(NativeVulkanSceneLayerAlphaMaskBindRequirement::GeneratedClippingTarget)
        }
        operation => Err(format!(
            "scene layer alpha-mask scheduler cannot schedule unsupported operation {operation:?}"
        )),
    }
}

fn schedule_step_kind(
    command_index: usize,
    command: &super::NativeVulkanSceneLayerAlphaMaskCommandPlan,
    state: &mut NativeVulkanSceneLayerAlphaMaskObjectReadiness,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind, String> {
    match command.operation {
        SceneLayerCompositorOperation::TokenProgramDispatch => {
            *state = NativeVulkanSceneLayerAlphaMaskObjectReadiness::default();
            Ok(NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch)
        }
        SceneLayerCompositorOperation::DrawClippingMask => {
            let target = command.target_graph_target.ok_or_else(|| {
                format!(
                    "scene layer alpha-mask scheduler command {command_index} has no alpha-mask graph target"
                )
            })?;
            match target {
                SceneGraphTarget::FullAlphaMask => {
                    state.full_mask_ready = true;
                    Ok(NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer)
                }
                SceneGraphTarget::FullAlphaMaskIntermediate => {
                    state.intermediate_mask_ready = true;
                    Ok(NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer)
                }
                _ => Err(format!(
                    "scene layer alpha-mask scheduler command {command_index} writes unexpected target {target:?}"
                )),
            }
        }
        SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask => {
            if !state.intermediate_mask_ready {
                return Err(format!(
                    "scene layer alpha-mask scheduler command {command_index} copy-back requires an earlier intermediate mask producer"
                ));
            }
            state.full_mask_ready = true;
            Ok(NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask)
        }
        SceneLayerCompositorOperation::DrawGeneratedClippingTarget => {
            if !state.full_mask_ready {
                return Err(format!(
                    "scene layer alpha-mask scheduler command {command_index} generated target draw requires an earlier full alpha-mask producer or copy-back"
                ));
            }
            Ok(NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer)
        }
        operation => Err(format!(
            "scene layer alpha-mask scheduler cannot schedule unsupported operation {operation:?}"
        )),
    }
}

fn validate_bind_readiness(
    command_index: usize,
    kind: NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
    matched_heap_bind_count: usize,
) -> Result<(), String> {
    match kind {
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch => {
            if matched_heap_bind_count != 0 {
                return Err(format!(
                    "scene layer alpha-mask scheduler command {command_index} token program must not bind shader resources"
                ));
            }
        }
        _ => {
            if matched_heap_bind_count == 0 {
                return Err(format!(
                    "scene layer alpha-mask scheduler command {command_index} requires a matched heap bind"
                ));
            }
        }
    }
    Ok(())
}

fn schedule_step_command_order(
    kind: NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
    recording_status: NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus,
) -> Vec<&'static str> {
    match kind {
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch => vec![
            "reset_object_alpha_mask_readiness",
            "dispatch_we_token_program",
        ],
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer => vec![
            "bind_clippingmaskimage4_heap",
            pending_or_recording_label(recording_status),
            "mark_full_alpha_mask_ready",
        ],
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer => vec![
            "bind_clippingmaskimage4_heap",
            pending_or_recording_label(recording_status),
            "mark_intermediate_alpha_mask_ready",
        ],
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask => {
            vec![
                "require_intermediate_alpha_mask_ready",
                "record_flattexture_copy_back_graph_node",
                "mark_full_alpha_mask_ready",
            ]
        }
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer => {
            vec![
                "require_full_alpha_mask_ready",
                "bind_generated_clippingtarget_heap",
                pending_or_recording_label(recording_status),
            ]
        }
    }
}

fn recording_status_for_kind(
    kind: NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
) -> NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus {
    match kind {
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch => {
            NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::TokenProgramNoDraw
        }
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer
        | NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer => {
            NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingClippingMaskImage4ProducerRecorder
        }
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask => {
            NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::ReadyFlatTextureCopyBackGraphNode
        }
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer => {
            NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingGeneratedClippingTargetRecorder
        }
    }
}

fn pending_or_recording_label(
    recording_status: NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus,
) -> &'static str {
    match recording_status {
        NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingClippingMaskImage4ProducerRecorder => {
            "pending_clippingmaskimage4_producer_recorder"
        }
        NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingGeneratedClippingTargetRecorder => {
            "pending_generated_clippingtarget_recorder"
        }
        NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::ReadyFlatTextureCopyBackGraphNode => {
            "record_flattexture_copy_back_graph_node"
        }
        NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::TokenProgramNoDraw => {
            "dispatch_we_token_program"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::copy_back_pipeline::NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan;
    use super::super::{
        NativeVulkanSceneLayerAlphaMaskAccess, NativeVulkanSceneLayerAlphaMaskCommandPlan,
        NativeVulkanSceneLayerAlphaMaskCopyMethod,
        NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan,
        NativeVulkanSceneLayerAlphaMaskTargetPlan,
    };
    use super::*;
    use crate::engine::scene_engine::{
        SceneLayerCompositorBlendKey, SceneLayerCompositorCondition, SceneLayerCompositorEntry,
    };

    #[test]
    fn token_schedule_places_copy_back_after_intermediate_and_before_generated_consumer() {
        let runtime = runtime(vec![
            token_program(),
            draw_mask(SceneLayerCompositorTarget::FullAlphaMask),
            draw_mask(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
            copy_back(),
            generated_target(),
        ]);
        let schedule = native_vulkan_plan_scene_layer_alpha_mask_token_schedule(
            &runtime,
            &resource_binds_for_runtime(&runtime),
        )
        .expect("token schedule");

        assert_eq!(schedule.command_count, 5);
        assert_eq!(schedule.token_program_dispatch_count, 1);
        assert_eq!(schedule.full_mask_producer_count, 1);
        assert_eq!(schedule.intermediate_mask_producer_count, 1);
        assert_eq!(schedule.copy_back_after_intermediate_count, 1);
        assert_eq!(schedule.generated_target_consumer_count, 1);
        assert_eq!(schedule.recorder_ready_step_count, 2);
        assert_eq!(schedule.missing_recorder_step_count, 3);
        assert_eq!(schedule.clippingmaskimage4_pending_recorder_count, 2);
        assert_eq!(schedule.generated_clippingtarget_pending_recorder_count, 1);
        assert_eq!(
            schedule.steps[3].kind,
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask
        );
        assert_eq!(
            schedule.steps[3].recording_status,
            NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::ReadyFlatTextureCopyBackGraphNode
        );
        assert_eq!(schedule.steps[3].matched_heap_bind_indices, vec![3]);
        assert!(schedule.steps[3].full_mask_ready_after);
        assert_eq!(
            schedule.steps[4].kind,
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer
        );
    }

    #[test]
    fn token_schedule_rejects_copy_back_before_intermediate_producer() {
        let runtime = runtime(vec![token_program(), copy_back()]);
        let err = native_vulkan_plan_scene_layer_alpha_mask_token_schedule(
            &runtime,
            &resource_binds_for_runtime(&runtime),
        )
        .expect_err("copy-back without intermediate producer must fail");

        assert!(err.contains("requires an earlier intermediate mask producer"));
    }

    #[test]
    fn token_schedule_rejects_generated_target_before_full_mask_ready() {
        let runtime = runtime(vec![token_program(), generated_target()]);
        let err = native_vulkan_plan_scene_layer_alpha_mask_token_schedule(
            &runtime,
            &resource_binds_for_runtime(&runtime),
        )
        .expect_err("generated target without full mask must fail");

        assert!(err.contains("requires an earlier full alpha-mask producer or copy-back"));
    }

    fn runtime(
        commands: Vec<NativeVulkanSceneLayerAlphaMaskCommandPlan>,
    ) -> NativeVulkanSceneLayerAlphaMaskRuntimePlan {
        NativeVulkanSceneLayerAlphaMaskRuntimePlan {
            tokenized_layer_count: 1,
            command_count: commands.len(),
            required_target_count: 2,
            pipeline_warmup: NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan {
                cache_key_count: 0,
                keys: Vec::new(),
                command_order: [
                    "select_clippingmaskimage4_shader",
                    "select_puppet_skinning_mesh_vertex_layout",
                    "select_r8_unorm_alpha_mask_target_format",
                    "include_required_we_slots_0_1_for_mask_generator",
                ],
                cache_keys: Vec::new(),
            },
            target_scope_count: 0,
            alpha_mask_attachment_write_count: 0,
            alpha_mask_shader_sample_count: 0,
            token_program_dispatch_count: 0,
            draw_clipping_mask_count: 0,
            draw_style_copy_back_count: 0,
            generated_clipping_target_draw_count: 0,
            transfer_copy_count: 0,
            targets: vec![
                NativeVulkanSceneLayerAlphaMaskTargetPlan {
                    target: SceneGraphTarget::FullAlphaMask,
                    format: "R8_UNORM",
                    width: 1920,
                    height: 1080,
                    scale: 2,
                },
                NativeVulkanSceneLayerAlphaMaskTargetPlan {
                    target: SceneGraphTarget::FullAlphaMaskIntermediate,
                    format: "R8_UNORM",
                    width: 1920,
                    height: 1080,
                    scale: 2,
                },
            ],
            commands,
            command_order: [
                "read_we_vtable_52_53_token_program",
                "validate_full_alpha_mask_targets_r8_half_extent",
                "derive_clippingmaskimage4_pipeline_warmup_key",
                "lower_clippingmaskimage4_to_alpha_mask_attachment_writes",
                "lower_flattexture_copy_back_to_draw_blend_key_0x100",
                "preserve_generated_clippingtarget_full_mask_sample",
                "track_alpha_mask_usage_like_godot_rendering_device_graph",
            ],
        }
    }

    fn resource_binds_for_runtime(
        runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    ) -> NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan {
        let token_commands = runtime
            .commands
            .iter()
            .enumerate()
            .map(|(command_index, command)| {
                let requirement = bind_requirement_for_scheduler(command.operation)
                    .expect("scheduler requirement");
                let matched_heap_bind_indices = if requirement
                    == NativeVulkanSceneLayerAlphaMaskBindRequirement::TokenProgramNoResourceBind
                {
                    Vec::new()
                } else {
                    vec![command_index]
                };
                NativeVulkanSceneLayerAlphaMaskTokenCommandResourceBindPlan {
                    command_index,
                    object: command.object,
                    operation: command.operation,
                    target: command.target,
                    source: command.source,
                    requirement,
                    matched_bind_count: matched_heap_bind_indices.len(),
                    matched_heap_bind_indices,
                    command_order: Vec::new(),
                }
            })
            .collect::<Vec<_>>();
        NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan {
            heap_bind_count: 0,
            resource_heap_bind_count: 0,
            clippingmaskimage4_bind_count: 0,
            generated_clippingtarget_bind_count: 0,
            flattexture_copy_back_bind_count: 0,
            token_command_count: token_commands.len(),
            token_command_resource_bind_count: 0,
            draw_clipping_mask_command_bind_count: 0,
            generated_clippingtarget_command_bind_count: 0,
            copy_back_command_count: 0,
            copy_back_draw_resource_count: 0,
            copy_back_draw_bind_count: 0,
            binds: Vec::new(),
            token_commands,
            copy_back_draws: Vec::new(),
            copy_back_draw_binds: Vec::new(),
            copy_back_pipelines: NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan {
                pipeline_count: 0,
                cache_key_count: 0,
                texture_slot_mask: 0,
                keys: Vec::new(),
                command_order: [
                    "read_copy_back_draw_resources",
                    "read_copy_back_heap_bind_pairings",
                    "derive_minimalalpha_copy_back_pipeline_keys",
                    "map_copy_back_texture_slots_to_descriptor_heap_offsets",
                    "preserve_render_state_flattexture_copy_back_draw_shape",
                ],
                cache_keys: Vec::new(),
            },
            command_order: [
                "read_current_alpha_mask_resource_heap_plan",
                "resolve_texture_bind_bind_info",
                "classify_alpha_mask_descriptor_heap_bind",
                "match_resource_binds_to_token_commands",
                "require_heap_bind_for_tokenized_mask_draws",
                "lower_flattexture_copy_back_to_minimalalpha_draw_resource",
                "pair_flattexture_copy_back_draws_with_heap_binds",
                "derive_flattexture_copy_back_pipeline_mapping",
                "preserve_flattexture_copy_back_as_blend_key_0x100_draw",
            ],
        }
    }

    fn token_program() -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
        command(
            SceneLayerCompositorOperation::TokenProgramDispatch,
            SceneLayerCompositorCondition::Always,
            None,
            SceneLayerCompositorTarget::LayerTarget490,
            NativeVulkanSceneLayerAlphaMaskAccess::TokenProgram,
            NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
            SceneLayerCompositorBlendKey::Inherit,
        )
    }

    fn draw_mask(target: SceneLayerCompositorTarget) -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
        command(
            SceneLayerCompositorOperation::DrawClippingMask,
            SceneLayerCompositorCondition::Token1OrToken2FirstPair,
            None,
            target,
            NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskAttachmentWrite,
            NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
            SceneLayerCompositorBlendKey::Inherit,
        )
    }

    fn copy_back() -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
        command(
            SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
            SceneLayerCompositorCondition::Token2AfterIntermediateMask,
            Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
            SceneLayerCompositorTarget::FullAlphaMask,
            NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskSampleAndAttachmentWrite,
            NativeVulkanSceneLayerAlphaMaskCopyMethod::FlatTextureDrawDestColorBlendKey0x100,
            SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
        )
    }

    fn generated_target() -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
        command(
            SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
            SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
            Some(SceneLayerCompositorTarget::FullAlphaMask),
            SceneLayerCompositorTarget::LayerTarget490,
            NativeVulkanSceneLayerAlphaMaskAccess::FullMaskSampleForGeneratedTarget,
            NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
            SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0,
        )
    }

    fn command(
        operation: SceneLayerCompositorOperation,
        condition: SceneLayerCompositorCondition,
        source: Option<SceneLayerCompositorTarget>,
        target: SceneLayerCompositorTarget,
        access: NativeVulkanSceneLayerAlphaMaskAccess,
        copy_method: NativeVulkanSceneLayerAlphaMaskCopyMethod,
        blend_key: SceneLayerCompositorBlendKey,
    ) -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
        NativeVulkanSceneLayerAlphaMaskCommandPlan {
            object: SceneObjectId(7),
            entry: match operation {
                SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask => {
                    SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed
                }
                SceneLayerCompositorOperation::TokenProgramDispatch => {
                    SceneLayerCompositorEntry::TokenizedCompositeEntry52
                }
                _ => SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
            },
            operation,
            condition,
            source,
            target,
            source_graph_target: source.and_then(graph_target),
            target_graph_target: graph_target(target),
            access,
            copy_method,
            blend_key,
        }
    }

    fn graph_target(target: SceneLayerCompositorTarget) -> Option<SceneGraphTarget> {
        match target {
            SceneLayerCompositorTarget::FullAlphaMask => Some(SceneGraphTarget::FullAlphaMask),
            SceneLayerCompositorTarget::FullAlphaMaskIntermediate => {
                Some(SceneGraphTarget::FullAlphaMaskIntermediate)
            }
            _ => None,
        }
    }
}

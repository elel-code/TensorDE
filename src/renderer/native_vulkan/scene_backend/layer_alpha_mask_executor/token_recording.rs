//! Token-ordered recording contract for WE layer alpha-mask work.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;

use crate::engine::scene_engine::{SceneLayerCompositorOperation, SceneObjectId};

use super::consumer_command::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan;
use super::copy_back_runtime::NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan;
use super::producer_draws::NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan;
use super::producer_target_graph::NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan;
use super::rt_method8_command::{
    NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
    NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind,
};
use super::token_schedule::{
    NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
    NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan {
    pub scheduled_step_count: usize,
    pub no_draw_step_count: usize,
    pub producer_recordable_step_count: usize,
    pub copy_back_recordable_step_count: usize,
    pub generated_consumer_recordable_step_count: usize,
    pub draw_recordable_step_count: usize,
    pub pending_step_count: usize,
    pub all_draw_steps_recordable: bool,
    pub steps: Vec<NativeVulkanSceneLayerAlphaMaskTokenRecordingStep>,
    pub command_order: [&'static str; 7],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTokenRecordingStep {
    pub command_index: usize,
    pub object: SceneObjectId,
    pub operation: SceneLayerCompositorOperation,
    pub schedule_kind: NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
    pub recording_kind: NativeVulkanSceneLayerAlphaMaskTokenRecordingKind,
    pub producer_draw_index: Option<usize>,
    pub producer_target_scope_index: Option<usize>,
    pub copy_back_command_index: Option<usize>,
    pub generated_consumer_draw_index: Option<usize>,
    pub generated_consumer_command_index: Option<usize>,
    pub rt_method8_command_index: Option<usize>,
    pub command_order: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskTokenRecordingKind {
    TokenProgramNoDraw,
    ClippingMaskImage4ProducerRtMethod8,
    FlatTextureCopyBackGraphNode,
    GeneratedClippingTargetRtMethod8,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_token_recording(
    schedule: &NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
    producer_draws: &NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
    producer_targets: &NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    generated_consumers: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    rt_method8_commands: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
    copy_back_commands: &NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan, String> {
    if schedule.scheduled_step_count == 0 {
        return Ok(NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan::empty());
    }

    let mut steps = Vec::with_capacity(schedule.steps.len());
    for step in &schedule.steps {
        steps.push(match step.kind {
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch => {
                token_program_recording_step(step)
            }
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer
            | NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer => {
                producer_recording_step(step, producer_draws, producer_targets, rt_method8_commands)?
            }
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask => {
                copy_back_recording_step(step, copy_back_commands)?
            }
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer => {
                generated_consumer_recording_step(
                    step,
                    generated_consumers,
                    rt_method8_commands,
                )?
            }
        });
    }

    Ok(NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan::from_steps(steps))
}

impl NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan {
    fn empty() -> Self {
        Self {
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

    fn from_steps(steps: Vec<NativeVulkanSceneLayerAlphaMaskTokenRecordingStep>) -> Self {
        let no_draw_step_count = steps
            .iter()
            .filter(|step| {
                step.recording_kind
                    == NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::TokenProgramNoDraw
            })
            .count();
        let producer_recordable_step_count = steps
            .iter()
            .filter(|step| {
                step.recording_kind
                    == NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::ClippingMaskImage4ProducerRtMethod8
            })
            .count();
        let copy_back_recordable_step_count = steps
            .iter()
            .filter(|step| {
                step.recording_kind
                    == NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::FlatTextureCopyBackGraphNode
            })
            .count();
        let generated_consumer_recordable_step_count = steps
            .iter()
            .filter(|step| {
                step.recording_kind
                    == NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::GeneratedClippingTargetRtMethod8
            })
            .count();
        let draw_recordable_step_count = producer_recordable_step_count
            + copy_back_recordable_step_count
            + generated_consumer_recordable_step_count;
        let pending_step_count = steps
            .len()
            .saturating_sub(no_draw_step_count + draw_recordable_step_count);
        Self {
            scheduled_step_count: steps.len(),
            no_draw_step_count,
            producer_recordable_step_count,
            copy_back_recordable_step_count,
            generated_consumer_recordable_step_count,
            draw_recordable_step_count,
            pending_step_count,
            all_draw_steps_recordable: pending_step_count == 0,
            steps,
            command_order: token_recording_command_order(),
        }
    }
}

fn token_program_recording_step(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
) -> NativeVulkanSceneLayerAlphaMaskTokenRecordingStep {
    NativeVulkanSceneLayerAlphaMaskTokenRecordingStep {
        command_index: step.command_index,
        object: step.object,
        operation: step.operation,
        schedule_kind: step.kind,
        recording_kind: NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::TokenProgramNoDraw,
        producer_draw_index: None,
        producer_target_scope_index: None,
        copy_back_command_index: None,
        generated_consumer_draw_index: None,
        generated_consumer_command_index: None,
        rt_method8_command_index: None,
        command_order: vec![
            "preserve_token_program_dispatch_position",
            "no_vulkan_draw_for_token_program_marker",
        ],
    }
}

fn producer_recording_step(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    producer_draws: &NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
    producer_targets: &NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    rt_method8_commands: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenRecordingStep, String> {
    let draw = producer_draws
        .draws
        .iter()
        .find(|draw| draw.command_index == step.command_index)
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token recording command {} has no clippingmaskimage4 producer draw contract",
                step.command_index
            )
        })?;
    let target_scope = producer_targets
        .scopes
        .iter()
        .find(|scope| scope.producer_draw_index == draw.producer_draw_index)
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token recording command {} has no producer target scope",
                step.command_index
            )
        })?;
    let rt_method8 = rt_method8_commands
        .commands
        .iter()
        .find(|command| {
            command.command_index == step.command_index
                && command.kind
                    == NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::ClippingMaskImage4Producer
        })
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token recording command {} has no producer RT method [8] indexed draw command",
                step.command_index
            )
        })?;
    if target_scope.command_index != step.command_index || draw.object != step.object {
        return Err(format!(
            "scene layer alpha-mask token recording command {} producer draw/target scope identity drift",
            step.command_index
        ));
    }
    Ok(NativeVulkanSceneLayerAlphaMaskTokenRecordingStep {
        command_index: step.command_index,
        object: step.object,
        operation: step.operation,
        schedule_kind: step.kind,
        recording_kind:
            NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::ClippingMaskImage4ProducerRtMethod8,
        producer_draw_index: Some(draw.producer_draw_index),
        producer_target_scope_index: Some(target_scope.target_scope_index),
        copy_back_command_index: None,
        generated_consumer_draw_index: None,
        generated_consumer_command_index: None,
        rt_method8_command_index: Some(rt_method8.command_index),
        command_order: vec![
            "begin_r8_alpha_mask_producer_target_scope",
            "bind_clippingmaskimage4_pipeline_and_heap",
            "record_layer_0x490_rt_method8_indexed_draw",
            "end_r8_alpha_mask_producer_target_scope",
        ],
    })
}

fn copy_back_recording_step(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    copy_back_commands: &NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenRecordingStep, String> {
    let command = copy_back_commands
        .commands
        .iter()
        .find(|command| command.command_index == step.command_index)
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token recording command {} has no flattexture copy-back command",
                step.command_index
            )
        })?;
    Ok(NativeVulkanSceneLayerAlphaMaskTokenRecordingStep {
        command_index: step.command_index,
        object: step.object,
        operation: step.operation,
        schedule_kind: step.kind,
        recording_kind:
            NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::FlatTextureCopyBackGraphNode,
        producer_draw_index: None,
        producer_target_scope_index: None,
        copy_back_command_index: Some(command.command_index),
        generated_consumer_draw_index: None,
        generated_consumer_command_index: None,
        rt_method8_command_index: None,
        command_order: vec![
            "require_intermediate_mask_ready",
            "record_full_alpha_mask_copy_back_graph_node",
        ],
    })
}

fn generated_consumer_recording_step(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    generated_consumers: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    rt_method8_commands: &NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskTokenRecordingStep, String> {
    let command = generated_consumers
        .commands
        .iter()
        .find(|command| command.command_index == step.command_index)
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token recording command {} has no generated CLIPPINGTARGET command",
                step.command_index
            )
        })?;
    let rt_method8 = rt_method8_commands
        .commands
        .iter()
        .find(|command| {
            command.command_index == step.command_index
                && command.kind
                    == NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::GeneratedClippingTargetConsumer
        })
        .ok_or_else(|| {
            format!(
                "scene layer alpha-mask token recording command {} has no generated consumer RT method [8] indexed draw command",
                step.command_index
            )
        })?;
    Ok(NativeVulkanSceneLayerAlphaMaskTokenRecordingStep {
        command_index: step.command_index,
        object: step.object,
        operation: step.operation,
        schedule_kind: step.kind,
        recording_kind:
            NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::GeneratedClippingTargetRtMethod8,
        producer_draw_index: None,
        producer_target_scope_index: None,
        copy_back_command_index: None,
        generated_consumer_draw_index: Some(command.consumer_draw_index),
        generated_consumer_command_index: Some(command.command_index),
        rt_method8_command_index: Some(rt_method8.command_index),
        command_order: vec![
            "begin_generated_clippingtarget_color_scope",
            "bind_generated_clippingtarget_pipeline_heap_and_uniform",
            "record_layer_0x490_rt_method8_indexed_draw",
            "end_generated_clippingtarget_color_scope",
        ],
    })
}

fn token_recording_command_order() -> [&'static str; 7] {
    [
        "read_token_schedule_order",
        "join_producer_steps_to_target_scopes_and_rt_method8_draws",
        "join_copy_back_steps_to_flattexture_graph_node_commands",
        "join_generated_consumer_steps_to_color_scopes_and_rt_method8_draws",
        "reject_orphan_copy_back_without_surrounding_token_contracts",
        "preserve_we_token_order_for_future_vulkan_recording",
        "forbid_partial_alpha_mask_draw_execution",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorCondition,
        SceneLayerCompositorOperation, SceneLayerCompositorTarget, SceneLayerCompositorBlendKey,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::consumer_command::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan;
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::copy_back_command::NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan;
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::copy_back_geometry::{
        FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY, FLATTEXTURE_COPY_BACK_VERTEX_BYTES,
        FLATTEXTURE_COPY_BACK_VERTEX_COUNT, FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES,
        NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers,
        NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan,
        native_vulkan_scene_layer_alpha_mask_copy_back_fullscreen_triangle_payload,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::producer_draws::NativeVulkanSceneLayerAlphaMaskProducerDrawPlan;
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::producer_target_graph::NativeVulkanSceneLayerAlphaMaskProducerTargetScopePlan;
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::rt_method8_command::{
        NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand,
        NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawSliceCommand,
    };
    use crate::renderer::native_vulkan::scene_backend::pipeline::NativeVulkanScenePipelineVertexLayout;
    use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;
    use crate::renderer::native_vulkan::scene_backend::resource_buffers::NativeVulkanSceneGpuBufferRecordBinding;
    use crate::renderer::native_vulkan::scene_backend::resource_storage::{
        NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRole,
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice,
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind,
    };
    use crate::renderer::native_vulkan::scene_backend::resource_buffers::NativeVulkanSceneGpuBufferKey;
    use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorVkFormat;
    use vulkanalia::vk::{self, Handle};

    #[test]
    fn token_recording_requires_every_draw_step_contract() {
        let schedule = schedule();
        let plan = native_vulkan_plan_scene_layer_alpha_mask_token_recording(
            &schedule,
            &producer_draws(),
            &producer_targets(),
            &generated_consumers(),
            &rt_method8_commands(),
            &copy_back_commands(),
        )
        .expect("token recording plan");

        assert_eq!(plan.scheduled_step_count, 4);
        assert_eq!(plan.no_draw_step_count, 1);
        assert_eq!(plan.producer_recordable_step_count, 1);
        assert_eq!(plan.copy_back_recordable_step_count, 1);
        assert_eq!(plan.generated_consumer_recordable_step_count, 1);
        assert_eq!(plan.draw_recordable_step_count, 3);
        assert_eq!(plan.pending_step_count, 0);
        assert!(plan.all_draw_steps_recordable);
        assert_eq!(
            plan.steps[1].recording_kind,
            NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::ClippingMaskImage4ProducerRtMethod8
        );
        assert_eq!(
            plan.steps[2].recording_kind,
            NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::FlatTextureCopyBackGraphNode
        );
        assert_eq!(
            plan.steps[3].recording_kind,
            NativeVulkanSceneLayerAlphaMaskTokenRecordingKind::GeneratedClippingTargetRtMethod8
        );
    }

    #[test]
    fn token_recording_rejects_orphan_copy_back() {
        let mut copy_back = copy_back_commands();
        copy_back.commands.clear();
        copy_back.command_count = 0;

        let err = native_vulkan_plan_scene_layer_alpha_mask_token_recording(
            &schedule(),
            &producer_draws(),
            &producer_targets(),
            &generated_consumers(),
            &rt_method8_commands(),
            &copy_back,
        )
        .expect_err("copy-back without command must fail");

        assert!(err.contains("flattexture copy-back command"));
    }

    fn schedule() -> NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan {
        NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan {
            command_count: 4,
            scheduled_step_count: 4,
            token_program_dispatch_count: 1,
            full_mask_producer_count: 0,
            intermediate_mask_producer_count: 1,
            copy_back_after_intermediate_count: 1,
            generated_target_consumer_count: 1,
            recorder_ready_step_count: 2,
            missing_recorder_step_count: 2,
            clippingmaskimage4_pending_recorder_count: 1,
            generated_clippingtarget_pending_recorder_count: 1,
            steps: vec![
                step(
                    0,
                    NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch,
                    SceneLayerCompositorOperation::TokenProgramDispatch,
                ),
                step(
                    1,
                    NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer,
                    SceneLayerCompositorOperation::DrawClippingMask,
                ),
                step(
                    2,
                    NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask,
                    SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
                ),
                step(
                    3,
                    NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer,
                    SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
                ),
            ],
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

    fn step(
        command_index: usize,
        kind: NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
        operation: SceneLayerCompositorOperation,
    ) -> NativeVulkanSceneLayerAlphaMaskTokenScheduleStep {
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStep {
            command_index,
            object: SceneObjectId(7),
            operation,
            source: None,
            target: SceneLayerCompositorTarget::LayerTarget490,
            kind,
            matched_heap_bind_count: usize::from(command_index != 0),
            matched_heap_bind_indices: (command_index != 0).then_some(command_index).into_iter().collect(),
            recording_status: match kind {
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch => {
                    super::super::token_schedule::NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::TokenProgramNoDraw
                }
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask => {
                    super::super::token_schedule::NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::ReadyFlatTextureCopyBackGraphNode
                }
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer
                | NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer => {
                    super::super::token_schedule::NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingClippingMaskImage4ProducerRecorder
                }
                NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer => {
                    super::super::token_schedule::NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingGeneratedClippingTargetRecorder
                }
            },
            full_mask_ready_after: command_index >= 2,
            intermediate_mask_ready_after: command_index >= 1,
            command_order: Vec::new(),
        }
    }

    fn producer_draws() -> NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan {
        NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan {
            command_count: 1,
            producer_draw_count: 1,
            full_mask_producer_count: 0,
            intermediate_mask_producer_count: 1,
            clear_target_scope_count: 0,
            load_target_scope_count: 1,
            texture_slot_mask: 0x3,
            draws: vec![NativeVulkanSceneLayerAlphaMaskProducerDrawPlan {
                producer_draw_index: 0,
                command_index: 1,
                object: SceneObjectId(7),
                condition: SceneLayerCompositorCondition::Token2IntermediatePairOrFinalMask,
                target: SceneGraphTarget::FullAlphaMaskIntermediate,
                target_byte: 1,
                clear_first: false,
                target_scope_load_op: NativeVulkanSceneRenderTargetLoadOp::Load,
                material: "materials/util/clippingmaskimage4.json",
                shader: "we/clippingmaskimage4",
                pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
                target_format: "R8_UNORM",
                texture_slot_mask: 0x3,
                optional_morph_texture_slot: 5,
                heap_bind_count: 1,
                heap_bind_indices: vec![1],
                subdraw_mask_texture_field_offset: "0x38",
                subdraw_invert_flag: "0x44 bit 0x2",
                draw_receiver: "[layer+0x490]",
                draw_receiver_vtable_offset: "0x40",
                reference_points: ["", "", "", ""],
                command_order: ["", "", "", "", "", ""],
            }],
            command_order: ["", "", "", "", "", ""],
        }
    }

    fn producer_targets() -> NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan {
        NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan {
            producer_draw_count: 1,
            target_scope_count: 1,
            clear_target_scope_count: 0,
            load_target_scope_count: 1,
            load_requires_initialized_target_count: 1,
            clear_allows_undefined_target_count: 0,
            scopes: vec![NativeVulkanSceneLayerAlphaMaskProducerTargetScopePlan {
                target_scope_index: 0,
                producer_draw_index: 0,
                command_index: 1,
                object: SceneObjectId(7),
                target: SceneGraphTarget::FullAlphaMaskIntermediate,
                target_byte: 1,
                width: 960,
                height: 540,
                format: "R8_UNORM",
                required_layout: "color-attachment-optimal",
                load_op: NativeVulkanSceneRenderTargetLoadOp::Load,
                clear_first: false,
                allows_undefined_initial_layout: false,
                requires_initialized_initial_layout: true,
                target_color_attachment_write_count: 1,
                current_layout_source: "retained_offscreen_target_store_at_record_time",
                command_order: ["", "", "", "", ""],
            }],
            command_order: ["", "", "", "", "", ""],
        }
    }

    fn generated_consumers() -> NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan {
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan {
            command_count: 1,
            warmed_pipeline_count: 1,
            descriptor_heap_bind_count: 1,
            target_scope_count: 1,
            pipeline_bind_count: 1,
            resource_heap_bind_count: 1,
            rt_method_8_bridge_count: 1,
            rt_method_8_indexed_draw_count: 1,
            commands: vec![
                NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan {
                    consumer_draw_index: 0,
                    command_index: 3,
                    object: SceneObjectId(7),
                    shader: "we/genericimage4",
                    shader_combo_values: vec![
                        "CLIPPINGTARGET=1".to_owned(),
                        "CLIPPINGUVS=1".to_owned(),
                    ],
                    source_mask: SceneGraphTarget::FullAlphaMask,
                    draw_receiver: SceneLayerCompositorTarget::LayerTarget490,
                    color_target: SceneGraphTarget::ObjectFinal(SceneObjectId(7)),
                    target_format: NativeVulkanSceneTextureDescriptorVkFormat::R8G8B8A8Unorm,
                    target_format_label: "R8G8B8A8_UNORM",
                    width: 1920,
                    height: 1080,
                    pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
                    vertex_layout: NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv,
                    heap_bind_index: 3,
                    heap_slice_index: 3,
                    base_resource_descriptor_index: 3,
                    base_sampler_descriptor_index: 3,
                    resource_descriptor_count: 3,
                    texture_count: 2,
                    material_uniform_buffer_handle: 1,
                    material_uniform_device_address: 2,
                    material_uniform_bytes: 48,
                    material_uniform_payload_hash: 3,
                    shader_mappings: Vec::new(),
                    material_source: "+0x428",
                    blend_byte_source: "+0x1f0",
                    geometry_source: "[layer+0x490]",
                    rt_method8_bridge_index: 1,
                    rt_method8_call_site: "0x14020908c",
                    rt_method8_method_vma: "0x1400eacd0",
                    effective_alpha_formula: "texture_alpha * g_Color4.a",
                    pipeline_bind_count: 1,
                    resource_heap_bind_count: 1,
                    target_bind_count: 1,
                    rt_method_8_indexed_draw_count: 1,
                    draw_call: "vkCmdDrawIndexed",
                    command_order: ["", "", "", "", "", "", "", ""],
                },
            ],
            command_order: ["", "", "", "", "", ""],
        }
    }

    fn copy_back_commands() -> NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan {
        NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan {
            command_count: 1,
            warmed_pipeline_count: 1,
            descriptor_heap_bind_count: 1,
            render_state_geometry_bind_count: 1,
            pipeline_bind_count: 1,
            resource_heap_bind_count: 1,
            direct_draw_count: 1,
            commands: vec![NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan {
                command_index: 2,
                object: SceneObjectId(7),
                shader: "util/minimalalpha",
                source: SceneGraphTarget::FullAlphaMaskIntermediate,
                target: SceneGraphTarget::FullAlphaMask,
                target_format: NativeVulkanSceneTextureDescriptorVkFormat::R8Unorm,
                blend_key: SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
                heap_bind_index: 2,
                heap_slice_index: 2,
                base_resource_descriptor_index: 2,
                base_sampler_descriptor_index: 2,
                geometry: copy_back_geometry_plan(),
                pipeline_bind_count: 1,
                resource_heap_bind_count: 1,
                direct_draw_count: 1,
                draw_call: "vkCmdDraw",
                command_order: ["", "", "", "", ""],
            }],
            command_order: ["", "", "", "", "", ""],
        }
    }

    fn copy_back_geometry_plan() -> NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan {
        NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryPlan::from_raster_geometry_and_buffers(
            FLATTEXTURE_COPY_BACK_RASTER_GEOMETRY,
            NativeVulkanSceneLayerAlphaMaskCopyBackRenderStateGeometryBuffers {
                vertex: vk::Buffer::from_raw(0x44),
                vertex_bytes: FLATTEXTURE_COPY_BACK_VERTEX_BYTES,
                vertex_count: FLATTEXTURE_COPY_BACK_VERTEX_COUNT,
                vertex_stride_bytes: FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES,
                vertex_payload_hash:
                    native_vulkan_scene_layer_alpha_mask_copy_back_fullscreen_triangle_payload(
                        false,
                    )
                    .payload_hash,
            },
        )
        .expect("copy-back render-state geometry plan")
    }

    fn rt_method8_commands() -> NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan {
        NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan {
            requirement_count: 2,
            command_count: 2,
            producer_command_count: 1,
            generated_consumer_command_count: 1,
            geometry_bind_count: 2,
            slice_bind_count: 2,
            indexed_draw_count: 2,
            r16_index_draw_count: 2,
            commands: vec![
                rt_method8_command(1, NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::ClippingMaskImage4Producer),
                rt_method8_command(3, NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::GeneratedClippingTargetConsumer),
            ],
            command_order: ["", "", "", "", "", "", ""],
        }
    }

    fn rt_method8_command(
        command_index: usize,
        kind: NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind,
    ) -> NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand {
        let geometry = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
            object: SceneObjectId(7),
            entry_owner_index: 0,
        };
        let owner = NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(geometry);
        NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommand {
            command_index,
            object: SceneObjectId(7),
            kind,
            shader: match kind {
                NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::ClippingMaskImage4Producer => "we/clippingmaskimage4",
                NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::GeneratedClippingTargetConsumer => "we/genericimage4",
            },
            pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
            rt_method8_call_site: "0x14020d83e",
            rt_method8_method_vma: "0x1400eacd0",
            heap_bind_index: command_index,
            uniform_binding_index: command_index,
            uniform_contract: match kind {
                NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::ClippingMaskImage4Producer => {
                    "clippingmaskimage4:g_RenderVar0+clear-scalar+slot0-slot1-slot5"
                }
                NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawKind::GeneratedClippingTargetConsumer => {
                    "genericimage4:CLIPPINGTARGET+CLIPPINGUVS+active-clipping+material-0x428"
                }
            },
            geometry,
            vertex: buffer_record(owner, NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex),
            geometry_index: buffer_record(owner, NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex),
            slices: vec![NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawSliceCommand {
                requirement_index: 0,
                slice: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice {
                    object: SceneObjectId(7),
                    entry_owner_index: 0,
                    subdraw_index: 0,
                    kind: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind::FirstListAppendToken0,
                },
                helper_vma: "0x14020c850",
                index: buffer_record(
                    NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvIndexSlice(
                        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice {
                            object: SceneObjectId(7),
                            entry_owner_index: 0,
                            subdraw_index: 0,
                            kind: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind::FirstListAppendToken0,
                        },
                    ),
                    NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvSliceIndex,
                ),
                index_count: 3,
                index_type: "VK_INDEX_TYPE_UINT16",
                draw_call: "vkCmdDrawIndexed",
                command_order: ["", "", "", "", ""],
            }],
            geometry_bind_count: 1,
            slice_bind_count: 1,
            indexed_draw_count: 1,
            index_type: "VK_INDEX_TYPE_UINT16",
            draw_call: "vkCmdDrawIndexed",
            receiver: "[layer+0x490].vtable+0x40",
            reference_points: ["", "", "", "", ""],
            command_order: ["", "", "", "", "", "", "", ""],
        }
    }

    fn buffer_record(
        owner: NativeVulkanSceneGpuBufferOwner,
        role: NativeVulkanSceneGpuBufferRole,
    ) -> NativeVulkanSceneGpuBufferRecordBinding {
        NativeVulkanSceneGpuBufferRecordBinding {
            key: NativeVulkanSceneGpuBufferKey { owner, role },
            bytes: 6,
            payload_hash: 1,
        }
    }
}

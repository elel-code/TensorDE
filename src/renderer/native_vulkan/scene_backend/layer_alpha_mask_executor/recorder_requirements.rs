//! Recorder input contracts for WE alpha-mask token steps.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorCondition,
    SceneLayerCompositorEntry, SceneLayerCompositorOperation, SceneLayerCompositorTarget,
    SceneObjectId,
};
use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;

use super::consumer_command::{
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
};
use super::consumer_draws::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan;
use super::consumer_pipeline::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan;
use super::consumer_target::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan;
use super::consumer_uniform::{
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformBindingPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformPlan,
};
use super::producer_draws::NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan;
use super::producer_target_graph::NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan;
use super::resource_binds::{
    NativeVulkanSceneLayerAlphaMaskBindRequirement,
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskTokenCommandResourceBindPlan,
};
use super::token_schedule::{
    NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus,
    NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
    NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
};
use super::{
    CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK, CLIPPINGTARGET_TEXTURE_SLOT_MASK,
    FLATTEXTURE_COPY_BACK_TEXTURE_SLOT_MASK, NativeVulkanSceneLayerAlphaMaskCopyMethod,
    NativeVulkanSceneLayerAlphaMaskRuntimePlan,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan
{
    pub step_count: usize,
    pub requirement_count: usize,
    pub token_program_requirement_count: usize,
    pub clippingmaskimage4_producer_requirement_count: usize,
    pub flattexture_copy_back_ready_requirement_count: usize,
    pub generated_clippingtarget_consumer_requirement_count: usize,
    pub pending_recorder_requirement_count: usize,
    pub ready_graph_node_requirement_count: usize,
    pub no_draw_requirement_count: usize,
    pub missing_we_fact_count: usize,
    pub requirements: Vec<NativeVulkanSceneLayerAlphaMaskRecorderRequirement>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRecorderRequirement {
    pub command_index: usize,
    pub object: SceneObjectId,
    pub entry: SceneLayerCompositorEntry,
    pub operation: SceneLayerCompositorOperation,
    pub condition: SceneLayerCompositorCondition,
    pub source: Option<SceneLayerCompositorTarget>,
    pub target: SceneLayerCompositorTarget,
    pub source_graph_target: Option<SceneGraphTarget>,
    pub target_graph_target: Option<SceneGraphTarget>,
    pub kind: NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind,
    pub recording_status: NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus,
    pub shader: Option<&'static str>,
    pub pipeline_class: Option<SceneGraphPipelineClass>,
    pub target_format: Option<&'static str>,
    pub texture_slot_mask: u32,
    pub heap_bind_count: usize,
    pub heap_bind_indices: Vec<usize>,
    pub producer_draw_index: Option<usize>,
    pub producer_target_scope_index: Option<usize>,
    pub generated_consumer_draw_index: Option<usize>,
    pub generated_consumer_uniform_index: Option<usize>,
    pub target_scope_load_op: Option<NativeVulkanSceneRenderTargetLoadOp>,
    pub requires_initialized_initial_layout: Option<bool>,
    pub source_mask: Option<SceneGraphTarget>,
    pub target_mask: Option<SceneGraphTarget>,
    pub missing_we_facts: Vec<&'static str>,
    pub reference_points: Vec<&'static str>,
    pub command_order: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind {
    TokenProgramDispatch,
    ClippingMaskImage4Producer,
    FlatTextureCopyBackGraphNode,
    GeneratedClippingTargetConsumer,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_recorder_requirements(
    runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    schedule: &NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
    producer_draws: &NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
    producer_target_graph: &NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    generated_consumer_draws: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    generated_consumer_targets: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
    generated_consumer_pipelines: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    generated_consumer_commands: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    generated_consumer_uniforms: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan, String> {
    if runtime.tokenized_layer_count == 0 {
        return Ok(NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan::empty());
    }
    if runtime.commands.len() != schedule.command_count {
        return Err(format!(
            "scene layer alpha-mask recorder requirements expected {} scheduled commands for {} runtime commands",
            schedule.command_count,
            runtime.commands.len()
        ));
    }
    if resource_binds.token_command_count != schedule.command_count {
        return Err(format!(
            "scene layer alpha-mask recorder requirements expected {} token heap-bind facts, got {}",
            schedule.command_count, resource_binds.token_command_count
        ));
    }

    let mut requirements = Vec::with_capacity(schedule.steps.len());
    for step in &schedule.steps {
        let command = runtime.commands.get(step.command_index).ok_or_else(|| {
            format!(
                "scene layer alpha-mask recorder requirement references missing command {}",
                step.command_index
            )
        })?;
        validate_step_command(step, command)?;
        let token_bind = resource_binds
            .token_commands
            .iter()
            .find(|bind| bind.command_index == step.command_index)
            .ok_or_else(|| {
                format!(
                    "scene layer alpha-mask recorder requirement command {} has no token heap-bind fact",
                    step.command_index
                )
            })?;
        validate_step_token_bind(step, token_bind)?;
        requirements.push(requirement_from_step(
            step,
            command,
            resource_binds,
            producer_draws,
            producer_target_graph,
            generated_consumer_draws,
            generated_consumer_targets,
            generated_consumer_pipelines,
            generated_consumer_commands,
            generated_consumer_uniforms,
        )?);
    }

    Ok(NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan::from_requirements(requirements))
}

impl NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan {
    fn empty() -> Self {
        Self {
            step_count: 0,
            requirement_count: 0,
            token_program_requirement_count: 0,
            clippingmaskimage4_producer_requirement_count: 0,
            flattexture_copy_back_ready_requirement_count: 0,
            generated_clippingtarget_consumer_requirement_count: 0,
            pending_recorder_requirement_count: 0,
            ready_graph_node_requirement_count: 0,
            no_draw_requirement_count: 0,
            missing_we_fact_count: 0,
            requirements: Vec::new(),
            command_order: recorder_requirement_command_order(),
        }
    }

    fn from_requirements(
        requirements: Vec<NativeVulkanSceneLayerAlphaMaskRecorderRequirement>,
    ) -> Self {
        Self {
            step_count: requirements.len(),
            requirement_count: requirements.len(),
            token_program_requirement_count: requirements
                .iter()
                .filter(|requirement| {
                    requirement.kind
                        == NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::TokenProgramDispatch
                })
                .count(),
            clippingmaskimage4_producer_requirement_count: requirements
                .iter()
                .filter(|requirement| {
                    requirement.kind
                        == NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::ClippingMaskImage4Producer
                })
                .count(),
            flattexture_copy_back_ready_requirement_count: requirements
                .iter()
                .filter(|requirement| {
                    requirement.kind
                        == NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::FlatTextureCopyBackGraphNode
                })
                .count(),
            generated_clippingtarget_consumer_requirement_count: requirements
                .iter()
                .filter(|requirement| {
                    requirement.kind
                        == NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::GeneratedClippingTargetConsumer
                })
                .count(),
            pending_recorder_requirement_count: requirements
                .iter()
                .filter(|requirement| {
                    matches!(
                        requirement.recording_status,
                        NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingClippingMaskImage4ProducerRecorder
                            | NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingGeneratedClippingTargetRecorder
                    )
                })
                .count(),
            ready_graph_node_requirement_count: requirements
                .iter()
                .filter(|requirement| {
                    requirement.recording_status
                        == NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::ReadyFlatTextureCopyBackGraphNode
                })
                .count(),
            no_draw_requirement_count: requirements
                .iter()
                .filter(|requirement| {
                    requirement.recording_status
                        == NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::TokenProgramNoDraw
                })
                .count(),
            missing_we_fact_count: requirements
                .iter()
                .map(|requirement| requirement.missing_we_facts.len())
                .sum(),
            requirements,
            command_order: recorder_requirement_command_order(),
        }
    }
}

fn requirement_from_step(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    command: &super::NativeVulkanSceneLayerAlphaMaskCommandPlan,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    producer_draws: &NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
    producer_target_graph: &NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    generated_consumer_draws: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    generated_consumer_targets: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
    generated_consumer_pipelines: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    generated_consumer_commands: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    generated_consumer_uniforms: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskRecorderRequirement, String> {
    match step.kind {
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch => {
            Ok(base_requirement(
                step,
                command,
                NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::TokenProgramDispatch,
                None,
                None,
                None,
                0,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Vec::new(),
                vec![
                    "reverse-engineered/docs/exe/clipping-pipeline.md: token stream 0x14020883e..0x140208bd5",
                    "reverse-engineered/docs/exe/blend-and-render.md: tokenized [52]/[53] loops",
                ],
                vec![
                    "read_we_token_stream",
                    "reset_alpha_mask_readiness",
                    "no_gpu_draw_for_dispatch_token",
                ],
            ))
        }
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer
        | NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer => {
            let target = command.target_graph_target.ok_or_else(|| {
                format!(
                    "scene layer alpha-mask producer command {} has no alpha-mask target",
                    step.command_index
                )
            })?;
            validate_producer_target(step.command_index, step.kind, target)?;
            let producer = producer_draws
                .draws
                .iter()
                .find(|draw| draw.command_index == step.command_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask producer command {} has no producer draw contract",
                        step.command_index
                    )
                })?;
            let target_scope = producer_target_graph
                .scopes
                .iter()
                .find(|scope| scope.producer_draw_index == producer.producer_draw_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask producer command {} has no producer target scope contract",
                        step.command_index
                    )
                })?;
            Ok(base_requirement(
                step,
                command,
                NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::ClippingMaskImage4Producer,
                Some("we/clippingmaskimage4"),
                Some(SceneGraphPipelineClass::PuppetSkinning),
                Some("R8_UNORM"),
                CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
                Some(producer.producer_draw_index),
                Some(target_scope.target_scope_index),
                None,
                None,
                Some(target_scope.load_op),
                Some(target_scope.requires_initialized_initial_layout),
                None,
                Some(target),
                clippingmaskimage4_missing_we_facts(),
                vec![
                    "reverse-engineered/docs/exe/clipping-pipeline.md: 0x14020d6a0 target byte and slots +0xd0/+0xd8/+0xf8",
                    "reverse-engineered/docs/exe/composelayer-and-effecttarget.md: 0x14020d6a0 clear/prep and [layer+0x490].vtable+0x40",
                    "reverse-engineered/docs/exe/blend-and-render.md: [layer+0x490] RT method [8] classification",
                ],
                vec![
                    "bind_clippingmaskimage4_resource_heap",
                    "transition_alpha_mask_target_to_color_attachment",
                    "apply_0x14020d6a0_clear_policy",
                    "record_layer_0x490_rt_method_8_mask_draw",
                    "retain_alpha_mask_target_layout",
                ],
            ))
        }
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask => {
            validate_copy_back_command(step, command, resource_binds)?;
            Ok(base_requirement(
                step,
                command,
                NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::FlatTextureCopyBackGraphNode,
                Some("util/minimalalpha"),
                Some(SceneGraphPipelineClass::LayerUtilityIndexed),
                Some("R8_UNORM"),
                FLATTEXTURE_COPY_BACK_TEXTURE_SLOT_MASK,
                None,
                None,
                None,
                None,
                Some(NativeVulkanSceneRenderTargetLoadOp::Load),
                Some(true),
                Some(SceneGraphTarget::FullAlphaMaskIntermediate),
                Some(SceneGraphTarget::FullAlphaMask),
                Vec::new(),
                vec![
                    "reverse-engineered/docs/exe/composelayer-and-effecttarget.md: 0x14020d9ed flattexture copy-back",
                    "reverse-engineered/docs/exe/blend-and-render.md: 0x1401ede30 indexed utility target",
                    "reverse-engineered/docs/exe/d3d11-context-calls.md: wrapper +0x108 blend-key bit 0x100",
                ],
                vec![
                    "require_intermediate_alpha_mask_ready",
                    "transition_intermediate_mask_to_shader_read",
                    "transition_full_mask_to_color_attachment",
                    "bind_util_minimalalpha_copy_back_heap",
                    "record_flattexture_copy_back_graph_node",
                ],
            ))
        }
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer => {
            validate_generated_consumer_command(step, command)?;
            let consumer = generated_consumer_draws
                .bindings
                .iter()
                .find(|consumer| consumer.command_index == step.command_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask generated consumer command {} has no generated draw contract",
                        step.command_index
                    )
                })?;
            validate_generated_consumer_draw_contract(step, consumer)?;
            let target = generated_consumer_targets
                .target_for_consumer_draw(consumer.consumer_draw_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask generated consumer command {} has no LayerTarget490 target contract",
                        step.command_index
                    )
                })?;
            let pipeline = generated_consumer_pipelines
                .bindings
                .iter()
                .find(|pipeline| pipeline.consumer_draw_index == consumer.consumer_draw_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask generated consumer command {} has no generated pipeline contract",
                        step.command_index
                    )
                })?;
            let generated_command = generated_consumer_commands
                .command_for_consumer_draw(consumer.consumer_draw_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask generated consumer command {} has no generated command-list contract",
                        step.command_index
                    )
                })?;
            let generated_uniform = generated_consumer_uniforms
                .uniform_for_consumer_draw(consumer.consumer_draw_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask generated consumer command {} has no generated uniform contract",
                        step.command_index
                    )
                })?;
            if generated_command.command_index != pipeline.command_index
                || generated_command.command_index != target.command_index
                || generated_command.color_target != target.color_target
                || generated_command.pipeline_class != pipeline.pipeline_class
            {
                return Err(format!(
                    "scene layer alpha-mask generated consumer command {} recorder command-list contract drifted from target/pipeline plans",
                    step.command_index
                ));
            }
            validate_generated_consumer_uniform_contract(
                step.command_index,
                generated_command,
                generated_uniform,
            )?;
            Ok(base_requirement(
                step,
                command,
                NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind::GeneratedClippingTargetConsumer,
                Some("we/genericimage4"),
                Some(generated_command.pipeline_class),
                Some(generated_command.target_format_label),
                CLIPPINGTARGET_TEXTURE_SLOT_MASK,
                None,
                None,
                Some(consumer.consumer_draw_index),
                Some(generated_uniform.uniform_binding_index),
                None,
                None,
                Some(SceneGraphTarget::FullAlphaMask),
                Some(target.color_target),
                generated_clippingtarget_missing_we_facts(),
                vec![
                    "reverse-engineered/docs/exe/clipping-pipeline.md: CLIPPINGTARGET consumes g_Texture8",
                    "reverse-engineered/docs/exe/clipping-pipeline.md: CLIPPINGUVS projected screen UV formula",
                    "reverse-engineered/docs/exe/clipping-pipeline.md: active clipping uniform upload at 0x14020cff0",
                    "reverse-engineered/docs/exe/clipping-pipeline.md: 0x140208b8f copies subdraw +0x40 to generated material +0x1f0",
                    "reverse-engineered/docs/exe/blend-and-render.md: token generated draw at 0x140208bbb/0x14020908c",
                    "reverse-engineered/docs/exe/blend-and-render.md: [layer+0x490] is RT method [8] draw receiver, separate from current color target",
                ],
                vec![
                    "require_full_alpha_mask_ready",
                    "resolve_layer_0x490_current_color_target",
                    "bind_generated_clippingtarget_resource_heap",
                    "bind_generated_clippingtarget_pipeline_variant",
                    "use_generated_clippingtarget_command_plan",
                    "apply_generated_clippingtarget_uniform_contract",
                    "record_layer_0x490_rt_method_8_generated_draw",
                ],
            ))
        }
    }
}

fn base_requirement(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    command: &super::NativeVulkanSceneLayerAlphaMaskCommandPlan,
    kind: NativeVulkanSceneLayerAlphaMaskRecorderRequirementKind,
    shader: Option<&'static str>,
    pipeline_class: Option<SceneGraphPipelineClass>,
    target_format: Option<&'static str>,
    texture_slot_mask: u32,
    producer_draw_index: Option<usize>,
    producer_target_scope_index: Option<usize>,
    generated_consumer_draw_index: Option<usize>,
    generated_consumer_uniform_index: Option<usize>,
    target_scope_load_op: Option<NativeVulkanSceneRenderTargetLoadOp>,
    requires_initialized_initial_layout: Option<bool>,
    source_mask: Option<SceneGraphTarget>,
    target_mask: Option<SceneGraphTarget>,
    missing_we_facts: Vec<&'static str>,
    reference_points: Vec<&'static str>,
    command_order: Vec<&'static str>,
) -> NativeVulkanSceneLayerAlphaMaskRecorderRequirement {
    NativeVulkanSceneLayerAlphaMaskRecorderRequirement {
        command_index: step.command_index,
        object: step.object,
        entry: command.entry,
        operation: command.operation,
        condition: command.condition,
        source: command.source,
        target: command.target,
        source_graph_target: command.source_graph_target,
        target_graph_target: command.target_graph_target,
        kind,
        recording_status: step.recording_status,
        shader,
        pipeline_class,
        target_format,
        texture_slot_mask,
        heap_bind_count: step.matched_heap_bind_count,
        heap_bind_indices: step.matched_heap_bind_indices.clone(),
        producer_draw_index,
        producer_target_scope_index,
        generated_consumer_draw_index,
        generated_consumer_uniform_index,
        target_scope_load_op,
        requires_initialized_initial_layout,
        source_mask,
        target_mask,
        missing_we_facts,
        reference_points,
        command_order,
    }
}

fn validate_step_command(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    command: &super::NativeVulkanSceneLayerAlphaMaskCommandPlan,
) -> Result<(), String> {
    if step.object != command.object
        || step.operation != command.operation
        || step.source != command.source
        || step.target != command.target
    {
        return Err(format!(
            "scene layer alpha-mask recorder requirement step {} does not match runtime command",
            step.command_index
        ));
    }
    Ok(())
}

fn validate_step_token_bind(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    token_bind: &NativeVulkanSceneLayerAlphaMaskTokenCommandResourceBindPlan,
) -> Result<(), String> {
    let expected_requirement = requirement_for_step_kind(step.kind);
    if token_bind.requirement != expected_requirement {
        return Err(format!(
            "scene layer alpha-mask recorder requirement command {} expected {:?} heap-bind fact, got {:?}",
            step.command_index, expected_requirement, token_bind.requirement
        ));
    }
    if step.matched_heap_bind_indices != token_bind.matched_heap_bind_indices {
        return Err(format!(
            "scene layer alpha-mask recorder requirement command {} heap-bind indices drifted between scheduler and bind plan",
            step.command_index
        ));
    }
    Ok(())
}

fn validate_producer_target(
    command_index: usize,
    kind: NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
    target: SceneGraphTarget,
) -> Result<(), String> {
    match (kind, target) {
        (
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer,
            SceneGraphTarget::FullAlphaMask,
        )
        | (
            NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer,
            SceneGraphTarget::FullAlphaMaskIntermediate,
        ) => Ok(()),
        _ => Err(format!(
            "scene layer alpha-mask producer command {command_index} has incompatible target {target:?} for {kind:?}"
        )),
    }
}

fn validate_copy_back_command(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    command: &super::NativeVulkanSceneLayerAlphaMaskCommandPlan,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
) -> Result<(), String> {
    if command.copy_method
        != NativeVulkanSceneLayerAlphaMaskCopyMethod::FlatTextureDrawDestColorBlendKey0x100
        || command.source_graph_target != Some(SceneGraphTarget::FullAlphaMaskIntermediate)
        || command.target_graph_target != Some(SceneGraphTarget::FullAlphaMask)
    {
        return Err(format!(
            "scene layer alpha-mask copy-back command {} must be intermediate -> full with blend-key bit 0x100",
            step.command_index
        ));
    }
    let matching = resource_binds
        .copy_back_draw_binds
        .iter()
        .filter(|bind| bind.command_index == step.command_index)
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(format!(
            "scene layer alpha-mask copy-back command {} requires exactly one retained copy-back draw heap bind, got {}",
            step.command_index,
            matching.len()
        ));
    }
    if step.matched_heap_bind_indices != [matching[0].heap_bind_index] {
        return Err(format!(
            "scene layer alpha-mask copy-back command {} heap-bind index mismatch: schedule {:?}, draw {}",
            step.command_index, step.matched_heap_bind_indices, matching[0].heap_bind_index
        ));
    }
    Ok(())
}

fn validate_generated_consumer_command(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    command: &super::NativeVulkanSceneLayerAlphaMaskCommandPlan,
) -> Result<(), String> {
    if command.source_graph_target != Some(SceneGraphTarget::FullAlphaMask) {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} must sample FullAlphaMask",
            step.command_index
        ));
    }
    Ok(())
}

fn validate_generated_consumer_draw_contract(
    step: &NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    consumer: &super::consumer_draws::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan,
) -> Result<(), String> {
    if consumer.object != step.object {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} object mismatch: step {:?}, draw {:?}",
            step.command_index, step.object, consumer.object
        ));
    }
    if step.matched_heap_bind_indices != [consumer.heap_bind_index] {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} heap-bind index mismatch: schedule {:?}, draw {}",
            step.command_index, step.matched_heap_bind_indices, consumer.heap_bind_index
        ));
    }
    if consumer.source_mask != SceneGraphTarget::FullAlphaMask {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} must sample FullAlphaMask, got {:?}",
            step.command_index, consumer.source_mask
        ));
    }
    if consumer.target != SceneLayerCompositorTarget::LayerTarget490 {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} must draw through layer+0x490, got {:?}",
            step.command_index, consumer.target
        ));
    }
    Ok(())
}

fn validate_generated_consumer_uniform_contract(
    command_index: usize,
    generated_command: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan,
    generated_uniform: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformBindingPlan,
) -> Result<(), String> {
    if generated_uniform.command_index != generated_command.command_index
        || generated_uniform.consumer_draw_index != generated_command.consumer_draw_index
        || generated_uniform.object != generated_command.object
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} uniform contract identity drifted"
        ));
    }
    if generated_uniform.source_mask != SceneGraphTarget::FullAlphaMask
        || generated_uniform.color_target != generated_command.color_target
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} uniform contract target drifted"
        ));
    }
    if generated_uniform.shader != "we/genericimage4"
        || !generated_uniform
            .shader_combo_values
            .iter()
            .any(|combo| combo == "CLIPPINGTARGET=1")
        || !generated_uniform
            .shader_combo_values
            .iter()
            .any(|combo| combo == "CLIPPINGUVS=1")
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} uniform contract lost CLIPPINGTARGET/CLIPPINGUVS shader semantics"
        ));
    }
    if generated_uniform.screen_uv_formula != "(v_ScreenPos.xy / v_ScreenPos.z) * 0.5 + 0.5"
        || generated_uniform.alpha_apply_formula
            != "gl_FragColor.a *= texSample2D(g_Texture8, screenUV).r"
        || generated_uniform.active_clipping_max_count != 0x0b
        || generated_uniform.material_uniform_buffer_handle == 0
        || generated_uniform.material_uniform_device_address == 0
        || generated_uniform.material_uniform_bytes == 0
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} uniform contract drifted from WE screen-UV/active-clipping/material facts"
        ));
    }
    Ok(())
}

fn requirement_for_step_kind(
    kind: NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
) -> NativeVulkanSceneLayerAlphaMaskBindRequirement {
    match kind {
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::TokenProgramDispatch => {
            NativeVulkanSceneLayerAlphaMaskBindRequirement::TokenProgramNoResourceBind
        }
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::FullMaskProducer
        | NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateMaskProducer => {
            NativeVulkanSceneLayerAlphaMaskBindRequirement::ClippingMaskImage4
        }
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::IntermediateCopyBackToFullMask => {
            NativeVulkanSceneLayerAlphaMaskBindRequirement::FlatTextureCopyBackSeparateDrawResourceBind
        }
        NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer => {
            NativeVulkanSceneLayerAlphaMaskBindRequirement::GeneratedClippingTarget
        }
    }
}

fn clippingmaskimage4_missing_we_facts() -> Vec<&'static str> {
    vec![
        "0x14020d6a0 subdraw entry -> layer+0x490 RT method [8] geometry binding",
        "0x14020d6a0 g_RenderVar0.x / clear scalar uniform location",
        "clippingmaskimage4 morph texture slot 5 enable condition",
    ]
}

fn generated_clippingtarget_missing_we_facts() -> Vec<&'static str> {
    vec!["generated draw geometry payload/buffer binding for [layer+0x490].vtable+0x40"]
}

fn recorder_requirement_command_order() -> [&'static str; 6] {
    [
        "read_alpha_mask_token_schedule",
        "validate_schedule_heap_bind_contract",
        "lower_0x14020d6a0_producers_to_recorder_requirements",
        "lower_flattexture_copy_back_to_ready_graph_node_requirement",
        "lower_generated_clippingtarget_to_recorder_requirements",
        "count_missing_we_recorder_facts",
    ]
}

#[cfg(test)]
#[path = "recorder_requirements_tests.rs"]
mod tests;

//! Scene runtime frame wiring for the native Vulkan backend.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneFramePlan, SceneGraphExecutionPlan, SceneGraphPipelineClass, SceneGraphTarget,
    SceneLayerCompositorLayer, SceneLayerCompositorOperation, SceneLayerCompositorPlan,
    SceneLayerCompositorRoute, SceneLayerCompositorTarget, SceneObjectId,
};
use crate::renderer::native_vulkan::NativeVulkanClearColor;

use super::draw_family::{
    NativeVulkanSceneDrawFamilyExecutorPlan, native_vulkan_require_scene_mesh_executor_families,
};
use super::effect_executor::{
    NativeVulkanSceneEffectRuntimeFrameContext, NativeVulkanSceneEffectRuntimeFramePlan,
    native_vulkan_plan_scene_effect_object_command_streams,
    native_vulkan_plan_scene_effect_runtime_preflight,
    native_vulkan_scene_effect_runtime_frame_from_recorded_commands,
};
use super::frame_resources::NativeVulkanSceneFrameResources;
use super::graph_executor::{
    NativeVulkanSceneGraphFrameCommandPlan, NativeVulkanSceneGraphRuntimeFrameContext,
};
use super::layer_alpha_mask_executor::{
    NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformPlan,
    NativeVulkanSceneLayerAlphaMaskLayerTargetBinding,
    NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
    NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    NativeVulkanSceneLayerAlphaMaskProducerUniformPlan,
    NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan,
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan,
    NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirementPlan,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirementPlan,
    NativeVulkanSceneLayerAlphaMaskRuntimePlan, NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
    NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
    native_vulkan_plan_scene_layer_alpha_mask_copy_back_runtime_commands,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_runtime_commands,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_targets,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_uniforms,
    native_vulkan_plan_scene_layer_alpha_mask_producer_draws,
    native_vulkan_plan_scene_layer_alpha_mask_producer_pipelines,
    native_vulkan_plan_scene_layer_alpha_mask_producer_target_graph,
    native_vulkan_plan_scene_layer_alpha_mask_producer_uniforms,
    native_vulkan_plan_scene_layer_alpha_mask_recorder_requirements,
    native_vulkan_plan_scene_layer_alpha_mask_resource_binds,
    native_vulkan_plan_scene_layer_alpha_mask_rt_method8_bridges,
    native_vulkan_plan_scene_layer_alpha_mask_rt_method8_indexed_draw_commands,
    native_vulkan_plan_scene_layer_alpha_mask_rt_method8_mdlv_geometry_buffers,
    native_vulkan_plan_scene_layer_alpha_mask_rt_method8_mdlv_index_slices,
    native_vulkan_plan_scene_layer_alpha_mask_runtime_frame,
    native_vulkan_plan_scene_layer_alpha_mask_token_recording,
    native_vulkan_plan_scene_layer_alpha_mask_token_schedule,
};
use super::layer_aux_clear_prep::{
    NativeVulkanSceneLayerAuxClearPrepFramePlan, native_vulkan_plan_scene_layer_aux_clear_prep,
};
use super::layer_aux_clear_scope::{
    NativeVulkanSceneLayerAuxClearScopeFramePlan, native_vulkan_plan_scene_layer_aux_clear_scopes,
};
use super::layer_aux_material_draws::{
    NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
    native_vulkan_plan_scene_layer_aux_material_draws,
};
use super::layer_compositor_command_blocks::{
    NativeVulkanSceneLayerCompositorAlphaTokenBlockInputs,
    NativeVulkanSceneLayerCompositorCommandBlockRecordPlan,
    NativeVulkanSceneLayerCompositorEffectBlockInputs,
    native_vulkan_record_scene_layer_compositor_command_blocks,
    native_vulkan_scene_layer_compositor_effect_blocks_are_recordable,
};
use super::layer_compositor_recorder::{
    NativeVulkanSceneLayerCompositorBlockRecordingPlan,
    native_vulkan_plan_scene_layer_compositor_block_recording,
};
use super::layer_compositor_scheduler::{
    NativeVulkanSceneLayerCompositorSchedulePlan,
    native_vulkan_plan_scene_layer_compositor_schedule,
};
use super::pipeline_warmup::NativeVulkanSceneMeshPipelineWarmupPlan;
use super::render_target::NativeVulkanSceneSwapchainRenderTarget;
use super::target_formats::NativeVulkanSceneGraphTargetFormatPlan;

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshRuntimeFrameContext<'a> {
    pub device: &'a Device,
    pub command_buffer: vk::CommandBuffer,
    pub target: NativeVulkanSceneSwapchainRenderTarget,
    pub target_formats: &'a NativeVulkanSceneGraphTargetFormatPlan,
    pub clear_color: Option<NativeVulkanClearColor>,
}

pub(in crate::renderer::native_vulkan) type NativeVulkanSceneRuntimeFrameContext<'a> =
    NativeVulkanSceneMeshRuntimeFrameContext<'a>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneRuntimeFramePlan<'a> {
    pub effects: NativeVulkanSceneEffectRuntimeFramePlan<'a>,
    pub layer_alpha_masks: NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    pub layer_alpha_mask_resource_binds: NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    pub layer_alpha_mask_token_schedule: NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
    pub layer_alpha_mask_producer_draws: NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
    pub layer_alpha_mask_producer_pipelines: NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
    pub layer_alpha_mask_producer_target_graph:
        NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    pub layer_alpha_mask_producer_uniforms: NativeVulkanSceneLayerAlphaMaskProducerUniformPlan,
    pub layer_alpha_mask_generated_consumer_draws:
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    pub layer_alpha_mask_rt_method8_bridges: NativeVulkanSceneLayerAlphaMaskRtMethod8BridgePlan,
    pub layer_alpha_mask_rt_method8_mdlv_geometry_buffers:
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvGeometryBufferRequirementPlan,
    pub layer_alpha_mask_rt_method8_mdlv_index_slices:
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceRequirementPlan,
    pub layer_alpha_mask_generated_consumer_targets:
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
    pub layer_alpha_mask_generated_consumer_pipelines:
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    pub layer_alpha_mask_generated_consumer_commands:
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    pub layer_alpha_mask_generated_consumer_uniforms:
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformPlan,
    pub layer_alpha_mask_recorder_requirements:
        NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan,
    pub layer_alpha_mask_rt_method8_indexed_draw_commands:
        NativeVulkanSceneLayerAlphaMaskRtMethod8IndexedDrawCommandPlan,
    pub layer_alpha_mask_copy_back_commands:
        NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan,
    pub layer_alpha_mask_token_recording: NativeVulkanSceneLayerAlphaMaskTokenRecordingPlan,
    pub layer_aux_clear_prep: NativeVulkanSceneLayerAuxClearPrepFramePlan,
    pub layer_aux_material_draws: NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
    pub layer_aux_clear_scopes: NativeVulkanSceneLayerAuxClearScopeFramePlan,
    pub layer_compositor_schedule: NativeVulkanSceneLayerCompositorSchedulePlan,
    pub layer_compositor_block_recording: NativeVulkanSceneLayerCompositorBlockRecordingPlan,
    pub mesh: NativeVulkanSceneMeshRuntimeFramePlan<'a>,
    pub command_order: [&'static str; 27],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshRuntimeFramePlan<'a> {
    pub graph_execution: SceneGraphExecutionPlan,
    pub draw_family_executor: NativeVulkanSceneDrawFamilyExecutorPlan,
    pub pipeline_warmup: NativeVulkanSceneMeshPipelineWarmupPlan,
    pub layer_compositor_command_blocks:
        Option<NativeVulkanSceneLayerCompositorCommandBlockRecordPlan>,
    pub frame: NativeVulkanSceneGraphFrameCommandPlan<'a>,
    pub command_order: [&'static str; 4],
}

impl<'a> NativeVulkanSceneMeshRuntimeFramePlan<'a> {
    fn from_parts(
        graph_execution: SceneGraphExecutionPlan,
        draw_family_executor: NativeVulkanSceneDrawFamilyExecutorPlan,
        pipeline_warmup: NativeVulkanSceneMeshPipelineWarmupPlan,
        layer_compositor_command_blocks: Option<
            NativeVulkanSceneLayerCompositorCommandBlockRecordPlan,
        >,
        frame: NativeVulkanSceneGraphFrameCommandPlan<'a>,
    ) -> Self {
        Self {
            graph_execution,
            draw_family_executor,
            pipeline_warmup,
            layer_compositor_command_blocks,
            frame,
            command_order: [
                "build_scene_graph_execution_plan",
                "select_scene_draw_family_executors",
                "require_warmed_mesh_pipelines",
                "record_layer_compositor_command_blocks",
            ],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_runtime_frame<'a>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneRuntimeFrameContext<'_>,
    frame: &'a SceneFramePlan,
) -> Result<NativeVulkanSceneRuntimeFramePlan<'a>, String> {
    let effect_context = NativeVulkanSceneEffectRuntimeFrameContext {
        device: context.device,
        command_buffer: context.command_buffer,
        target_formats: context.target_formats,
    };
    let layer_alpha_masks = native_vulkan_plan_scene_layer_alpha_mask_runtime_frame(
        frame_resources,
        &frame.layer_compositor,
        context.target.extent,
    )?;
    for key in layer_alpha_masks.pipeline_warmup.cache_keys() {
        frame_resources.cached_mesh_pipeline(key).map_err(|err| {
            format!(
                "{err}; scene layer alpha-mask runtime requires clippingmaskimage4 pipeline warmup before present-frame recording"
            )
        })?;
    }
    let layer_alpha_mask_resource_binds = native_vulkan_plan_scene_layer_alpha_mask_resource_binds(
        frame_resources,
        &layer_alpha_masks,
    )?;
    let layer_alpha_mask_token_schedule = native_vulkan_plan_scene_layer_alpha_mask_token_schedule(
        &layer_alpha_masks,
        &layer_alpha_mask_resource_binds,
    )?;
    let layer_alpha_mask_producer_draws = native_vulkan_plan_scene_layer_alpha_mask_producer_draws(
        &layer_alpha_masks,
        &layer_alpha_mask_resource_binds,
        &layer_alpha_mask_token_schedule,
    )?;
    let layer_alpha_mask_producer_pipelines =
        native_vulkan_plan_scene_layer_alpha_mask_producer_pipelines(
            &layer_alpha_mask_producer_draws,
            &layer_alpha_mask_resource_binds,
        )?;
    for key in layer_alpha_mask_producer_pipelines.cache_keys() {
        frame_resources.cached_mesh_pipeline(key).map_err(|err| {
            format!(
                "{err}; scene layer alpha-mask runtime requires clippingmaskimage4 producer pipeline warmup before command-list assembly"
            )
        })?;
    }
    let layer_alpha_mask_producer_target_graph =
        native_vulkan_plan_scene_layer_alpha_mask_producer_target_graph(
            &layer_alpha_masks,
            &layer_alpha_mask_producer_draws,
        )?;
    let layer_alpha_mask_producer_uniforms =
        native_vulkan_plan_scene_layer_alpha_mask_producer_uniforms(
            &layer_alpha_mask_producer_draws,
            &layer_alpha_mask_producer_pipelines,
        )?;
    let layer_alpha_mask_generated_consumer_draws =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws(
            &layer_alpha_masks,
            &layer_alpha_mask_resource_binds,
            &layer_alpha_mask_token_schedule,
        )?;
    let layer_alpha_mask_rt_method8_bridges =
        native_vulkan_plan_scene_layer_alpha_mask_rt_method8_bridges(
            &layer_alpha_masks,
            &layer_alpha_mask_producer_draws,
            &layer_alpha_mask_generated_consumer_draws,
        )?;
    let layer_alpha_mask_rt_method8_mdlv_geometry_buffers =
        native_vulkan_plan_scene_layer_alpha_mask_rt_method8_mdlv_geometry_buffers(
            &layer_alpha_mask_rt_method8_bridges,
        )?;
    let layer_alpha_mask_rt_method8_mdlv_index_slices =
        native_vulkan_plan_scene_layer_alpha_mask_rt_method8_mdlv_index_slices(
            &layer_alpha_mask_rt_method8_mdlv_geometry_buffers,
            |geometry| {
                frame_resources
                    .layer_alpha_mask_rt_method8_mdlv_index_slice_buffer_records_for_geometry(
                        geometry,
                    )
            },
        )?;
    let layer_alpha_mask_generated_consumer_targets =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_targets(
            &layer_alpha_mask_generated_consumer_draws,
            |object, target| {
                native_vulkan_resolve_scene_layer_490_color_target(
                    frame_resources,
                    context.target_formats,
                    context.target.extent,
                    &frame.layer_compositor,
                    object,
                    target,
                )
            },
        )?;
    let layer_alpha_mask_generated_consumer_pipelines =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets(
            &layer_alpha_mask_generated_consumer_draws,
            &layer_alpha_mask_generated_consumer_targets,
        )?;
    for key in layer_alpha_mask_generated_consumer_pipelines.cache_keys() {
        frame_resources.cached_mesh_pipeline(key).map_err(|err| {
            format!(
                "{err}; scene layer alpha-mask runtime requires generated CLIPPINGTARGET consumer pipeline warmup before command-list assembly"
            )
        })?;
    }
    let layer_alpha_mask_generated_consumer_commands =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_runtime_commands(
            frame_resources,
            &layer_alpha_mask_generated_consumer_draws,
            &layer_alpha_mask_generated_consumer_targets,
            &layer_alpha_mask_generated_consumer_pipelines,
            &layer_alpha_mask_rt_method8_bridges,
        )?;
    let layer_alpha_mask_generated_consumer_uniforms =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_uniforms(
            &layer_alpha_mask_generated_consumer_commands,
        )?;
    let layer_alpha_mask_recorder_requirements =
        native_vulkan_plan_scene_layer_alpha_mask_recorder_requirements(
            &layer_alpha_masks,
            &layer_alpha_mask_resource_binds,
            &layer_alpha_mask_token_schedule,
            &layer_alpha_mask_producer_draws,
            &layer_alpha_mask_producer_target_graph,
            &layer_alpha_mask_producer_uniforms,
            &layer_alpha_mask_generated_consumer_draws,
            &layer_alpha_mask_rt_method8_bridges,
            &layer_alpha_mask_rt_method8_mdlv_geometry_buffers,
            &layer_alpha_mask_rt_method8_mdlv_index_slices,
            &layer_alpha_mask_generated_consumer_targets,
            &layer_alpha_mask_generated_consumer_pipelines,
            &layer_alpha_mask_generated_consumer_commands,
            &layer_alpha_mask_generated_consumer_uniforms,
        )?;
    let layer_alpha_mask_rt_method8_indexed_draw_commands =
        native_vulkan_plan_scene_layer_alpha_mask_rt_method8_indexed_draw_commands(
            &layer_alpha_mask_recorder_requirements,
            |geometry| {
                frame_resources.layer_alpha_mask_rt_method8_mdlv_geometry_buffer_records(geometry)
            },
        )?;
    let layer_alpha_mask_copy_back_commands =
        native_vulkan_plan_scene_layer_alpha_mask_copy_back_runtime_commands(
            frame_resources,
            &layer_alpha_mask_resource_binds,
        )?;
    let layer_alpha_mask_token_recording =
        native_vulkan_plan_scene_layer_alpha_mask_token_recording(
            &layer_alpha_mask_token_schedule,
            &layer_alpha_mask_producer_draws,
            &layer_alpha_mask_producer_target_graph,
            &layer_alpha_mask_generated_consumer_commands,
            &layer_alpha_mask_rt_method8_indexed_draw_commands,
            &layer_alpha_mask_copy_back_commands,
        )?;
    let mesh_graph_execution = SceneGraphExecutionPlan::from_graph(&frame.graph);
    let layer_compositor_schedule = native_vulkan_plan_scene_layer_compositor_schedule(
        &frame.layer_compositor,
        &frame.graph,
        &mesh_graph_execution,
        &layer_alpha_mask_token_recording,
    )?;
    let effect_command_streams =
        native_vulkan_plan_scene_effect_object_command_streams(&frame.effect_pass_graph)?;
    let defer_object_final_effects =
        native_vulkan_scene_layer_compositor_effect_blocks_are_recordable(
            &layer_compositor_schedule,
            &frame.effect_pass_graph,
            &effect_command_streams,
        );
    let effect_graph_command_count = scene_effect_graph_command_count(&frame.effect_pass_graph);
    if effect_graph_command_count != 0 && !defer_object_final_effects {
        return Err(format!(
            "scene runtime refuses eager effect + mesh graph fallback: {} effect commands must be represented as WE layer-compositor ObjectFinal command blocks",
            effect_graph_command_count
        ));
    }
    let effect_preflight = native_vulkan_plan_scene_effect_runtime_preflight(
        frame_resources,
        &effect_context,
        &frame.effect_pass_graph,
    )?;
    let (
        mesh,
        layer_compositor_effect_commands,
        layer_aux_clear_prep,
        layer_aux_material_draws,
        layer_aux_clear_scopes,
    ) = native_vulkan_record_scene_mesh_runtime_frame(
        frame_resources,
        NativeVulkanSceneMeshRuntimeFrameContext {
            device: context.device,
            command_buffer: context.command_buffer,
            target: context.target,
            target_formats: context.target_formats,
            clear_color: context.clear_color,
        },
        frame,
        mesh_graph_execution,
        &layer_compositor_schedule,
        NativeVulkanSceneLayerCompositorAlphaTokenBlockInputs {
            token_recording: &layer_alpha_mask_token_recording,
            producer_targets: &layer_alpha_mask_producer_target_graph,
            producer_pipelines: &layer_alpha_mask_producer_pipelines,
            generated_commands: &layer_alpha_mask_generated_consumer_commands,
            generated_pipelines: &layer_alpha_mask_generated_consumer_pipelines,
            rt_method8_commands: &layer_alpha_mask_rt_method8_indexed_draw_commands,
            resource_binds: &layer_alpha_mask_resource_binds,
        },
        NativeVulkanSceneLayerCompositorEffectBlockInputs {
            context: NativeVulkanSceneEffectRuntimeFrameContext {
                device: context.device,
                command_buffer: context.command_buffer,
                target_formats: context.target_formats,
            },
            graph: &frame.effect_pass_graph,
            command_streams: &effect_command_streams,
        },
    )?;
    let effects = native_vulkan_scene_effect_runtime_frame_from_recorded_commands(
        &frame.effect_pass_graph,
        effect_preflight,
        layer_compositor_effect_commands,
    );
    if effects.command_count != effect_graph_command_count {
        return Err(format!(
            "scene runtime layer-compositor effect command drift: expected {}, recorded {}",
            effect_graph_command_count, effects.command_count
        ));
    }
    let layer_compositor_block_recording =
        native_vulkan_plan_scene_layer_compositor_block_recording(
            &layer_compositor_schedule,
            &effects,
            &mesh.frame,
            &layer_alpha_mask_token_recording,
            mesh.layer_compositor_command_blocks.is_some(),
        )?;
    Ok(NativeVulkanSceneRuntimeFramePlan {
        effects,
        layer_alpha_masks,
        layer_alpha_mask_resource_binds,
        layer_alpha_mask_token_schedule,
        layer_alpha_mask_producer_draws,
        layer_alpha_mask_producer_pipelines,
        layer_alpha_mask_producer_target_graph,
        layer_alpha_mask_producer_uniforms,
        layer_alpha_mask_generated_consumer_draws,
        layer_alpha_mask_rt_method8_bridges,
        layer_alpha_mask_rt_method8_mdlv_geometry_buffers,
        layer_alpha_mask_rt_method8_mdlv_index_slices,
        layer_alpha_mask_generated_consumer_targets,
        layer_alpha_mask_generated_consumer_pipelines,
        layer_alpha_mask_generated_consumer_commands,
        layer_alpha_mask_generated_consumer_uniforms,
        layer_alpha_mask_recorder_requirements,
        layer_alpha_mask_rt_method8_indexed_draw_commands,
        layer_alpha_mask_copy_back_commands,
        layer_alpha_mask_token_recording,
        layer_aux_clear_prep,
        layer_aux_material_draws,
        layer_aux_clear_scopes,
        layer_compositor_schedule,
        layer_compositor_block_recording,
        mesh,
        command_order: [
            "record_scene_effect_graph_runtime",
            "plan_scene_layer_alpha_mask_token_runtime",
            "require_warmed_layer_alpha_mask_pipelines",
            "plan_layer_alpha_mask_resource_heap_binds",
            "plan_layer_alpha_mask_token_schedule",
            "plan_layer_alpha_mask_producer_draws",
            "plan_layer_alpha_mask_producer_pipelines",
            "plan_layer_alpha_mask_producer_target_graph",
            "plan_layer_alpha_mask_producer_uniforms",
            "plan_layer_alpha_mask_generated_consumer_draws",
            "plan_layer_alpha_mask_rt_method8_bridges",
            "plan_layer_alpha_mask_rt_method8_mdlv_geometry_buffers",
            "plan_layer_alpha_mask_rt_method8_mdlv_index_slices",
            "plan_layer_alpha_mask_generated_consumer_targets",
            "plan_layer_alpha_mask_generated_consumer_pipelines",
            "plan_layer_alpha_mask_generated_consumer_commands",
            "plan_layer_alpha_mask_generated_consumer_uniforms",
            "plan_layer_alpha_mask_recorder_requirements",
            "plan_layer_alpha_mask_rt_method8_indexed_draw_commands",
            "plan_layer_alpha_mask_copy_back_command_list",
            "plan_layer_alpha_mask_token_recording_contract",
            "plan_scene_layer_compositor_schedule",
            "plan_scene_layer_aux_clear_prep",
            "plan_scene_layer_aux_material_draw_receivers",
            "plan_scene_layer_aux_clear_target_scopes",
            "record_scene_mesh_graph_runtime",
            "plan_scene_layer_compositor_block_recording",
        ],
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_resolve_scene_layer_490_color_target(
    frame_resources: &NativeVulkanSceneFrameResources,
    target_formats: &NativeVulkanSceneGraphTargetFormatPlan,
    swapchain_extent: vk::Extent2D,
    layer_compositor: &SceneLayerCompositorPlan,
    object: SceneObjectId,
    target: SceneLayerCompositorTarget,
) -> Result<NativeVulkanSceneLayerAlphaMaskLayerTargetBinding, String> {
    if target != SceneLayerCompositorTarget::LayerTarget490 {
        return Err(format!(
            "scene layer alpha-mask generated consumer target resolver requires LayerTarget490, got {target:?}"
        ));
    }
    let layer = layer_compositor.layer_for_object(object).ok_or_else(|| {
        format!("scene layer alpha-mask generated consumer target resolver missing layer for object {object:?}")
    })?;
    let color_target = native_vulkan_scene_layer_490_color_graph_target(layer)?;
    let format = target_formats.format(color_target)?;
    let (width, height) = if color_target == SceneGraphTarget::Swapchain {
        (swapchain_extent.width, swapchain_extent.height)
    } else {
        let binding = frame_resources.offscreen_target_binding(color_target)?;
        if binding.format != format {
            return Err(format!(
                "scene layer alpha-mask generated consumer color target {:?} format mismatch: target plan {:?}, retained {:?}",
                color_target, format, binding.format
            ));
        }
        (binding.width, binding.height)
    };
    Ok(NativeVulkanSceneLayerAlphaMaskLayerTargetBinding {
        object,
        layer_target: target,
        color_target,
        format,
        width,
        height,
        pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
    })
}

fn native_vulkan_scene_layer_490_color_graph_target(
    layer: &SceneLayerCompositorLayer,
) -> Result<SceneGraphTarget, String> {
    match layer.route {
        SceneLayerCompositorRoute::DirectSwapchain => Ok(SceneGraphTarget::Swapchain),
        SceneLayerCompositorRoute::ObjectFinalMeshComposite => {
            let source = layer
                .commands
                .iter()
                .find(|command| {
                    command.operation == SceneLayerCompositorOperation::FullLayerComposite
                        && command.target == SceneLayerCompositorTarget::Swapchain
                })
                .and_then(|command| command.source)
                .unwrap_or(SceneLayerCompositorTarget::ObjectFinal(layer.object));
            scene_layer_color_target_from_compositor_target(layer.object, source)
        }
    }
}

fn scene_layer_color_target_from_compositor_target(
    object: SceneObjectId,
    target: SceneLayerCompositorTarget,
) -> Result<SceneGraphTarget, String> {
    match target {
        SceneLayerCompositorTarget::Swapchain => Ok(SceneGraphTarget::Swapchain),
        SceneLayerCompositorTarget::ObjectFinal(target_object) if target_object == object => {
            Ok(SceneGraphTarget::ObjectFinal(target_object))
        }
        SceneLayerCompositorTarget::ImageLayerCompositeA(target_object)
            if target_object == object =>
        {
            Ok(SceneGraphTarget::ImageLayerCompositeA(target_object))
        }
        SceneLayerCompositorTarget::ImageLayerSource(target_object) if target_object == object => {
            Ok(SceneGraphTarget::ImageLayerSource(target_object))
        }
        _ => Err(format!(
            "scene layer alpha-mask LayerTarget490 color target for object {object:?} cannot resolve layer source {target:?}"
        )),
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_mesh_runtime_frame<'a>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneMeshRuntimeFrameContext<'_>,
    frame: &'a SceneFramePlan,
    graph_execution: SceneGraphExecutionPlan,
    layer_compositor_schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
    alpha_inputs: NativeVulkanSceneLayerCompositorAlphaTokenBlockInputs<'_>,
    effect_inputs: NativeVulkanSceneLayerCompositorEffectBlockInputs<'a, '_, '_>,
) -> Result<
    (
        NativeVulkanSceneMeshRuntimeFramePlan<'a>,
        Vec<super::effect_executor::NativeVulkanSceneEffectRuntimeCommandPlan<'a>>,
        NativeVulkanSceneLayerAuxClearPrepFramePlan,
        NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
        NativeVulkanSceneLayerAuxClearScopeFramePlan,
    ),
    String,
> {
    let draw_family_executor =
        native_vulkan_require_scene_mesh_executor_families(&graph_execution.draw_family_plan)?;
    let pipeline_warmup = NativeVulkanSceneMeshPipelineWarmupPlan::from_graph_with_target_formats(
        &frame.graph,
        |target| context.target_formats.format(target),
    )?;

    for key in pipeline_warmup.cache_keys() {
        frame_resources.cached_mesh_pipeline(key).map_err(|err| {
            format!(
                "{err}; scene mesh runtime requires pipeline warmup before present-frame recording"
            )
        })?;
    }

    let graph_context = NativeVulkanSceneGraphRuntimeFrameContext {
        device: context.device,
        command_buffer: context.command_buffer,
        swapchain_target: context.target,
        target_formats: context.target_formats,
        clear_color: context.clear_color,
    };
    let aux_clear_prep =
        native_vulkan_plan_scene_layer_aux_clear_prep(layer_compositor_schedule, &frame.residency)?;
    let aux_material_draws =
        native_vulkan_plan_scene_layer_aux_material_draws(&aux_clear_prep, &frame.residency)?;
    let aux_clear_scopes = native_vulkan_plan_scene_layer_aux_clear_scopes(
        frame_resources,
        &aux_clear_prep,
        &aux_material_draws,
    )?;
    let (layer_compositor_command_blocks, layer_compositor_effect_commands, frame_plan) = {
        let command_block_output = native_vulkan_record_scene_layer_compositor_command_blocks(
            frame_resources,
            graph_context,
            frame,
            &graph_execution,
            layer_compositor_schedule,
            &aux_clear_prep,
            &aux_material_draws,
            &aux_clear_scopes,
            alpha_inputs,
            effect_inputs,
        )?;
        (
            Some(command_block_output.command_blocks),
            command_block_output.effect_commands,
            command_block_output.mesh_frame,
        )
    };

    let mesh = NativeVulkanSceneMeshRuntimeFramePlan::from_parts(
        graph_execution,
        draw_family_executor,
        pipeline_warmup,
        layer_compositor_command_blocks,
        frame_plan,
    );
    Ok((
        mesh,
        layer_compositor_effect_commands,
        aux_clear_prep,
        aux_material_draws,
        aux_clear_scopes,
    ))
}

fn scene_effect_graph_command_count(
    graph: &crate::engine::scene_engine::SceneEffectPassGraphPlan,
) -> usize {
    graph
        .material_pass_count
        .saturating_add(graph.copy_command_count)
        .saturating_add(graph.swap_command_count)
}

#[cfg(test)]
mod tests {
    use super::super::graph_executor::NativeVulkanSceneGraphPassCommandPlan;
    use super::super::pass_command::NativeVulkanSceneMeshPassCommandPlan;
    use super::super::render_target::{
        NativeVulkanSceneRenderTargetLoadOp, NativeVulkanSceneRenderTargetScopePlan,
    };
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraph, SceneGraphDraw, SceneGraphDrawFamilyPlan,
        SceneGraphPass, SceneGraphTarget, SceneLayerCompositorBlendKey,
        SceneLayerCompositorCommand, SceneLayerCompositorCondition, SceneLayerCompositorEntry,
        SceneMaterialKey, SceneObjectId,
    };

    #[test]
    fn runtime_frame_plan_preserves_command_block_execution_order() {
        let graph = mesh_graph(vec![mesh_draw(SceneObjectId(1))]);
        let warmup = NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(
            &graph,
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("warmup plan");
        let pass = NativeVulkanSceneMeshPassCommandPlan {
            name: "scene-main",
            input: None,
            output: SceneGraphTarget::Swapchain,
            draw_index_start: 0,
            draw_index_end: 1,
            draw_count: 1,
            pipeline_bind_count: 1,
            resource_heap_bind_count: 1,
            indexed_draw_count: 1,
            commands: Vec::new(),
        };
        let frame = NativeVulkanSceneGraphFrameCommandPlan {
            pass_count: 1,
            target_barrier_count: 0,
            target_format_count: 1,
            passes: vec![NativeVulkanSceneGraphPassCommandPlan {
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
                pass,
            }],
            target_barriers: Vec::new(),
            command_order: [
                "resolve_scene_graph_target_formats",
                "record_graph_pass_render_targets",
                "record_mesh_pass_draw_lists",
                "record_scene_graph_target_barriers",
            ],
        };

        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);
        let draw_family_executor = native_vulkan_require_scene_mesh_executor_families(
            &SceneGraphDrawFamilyPlan::from_graph(&graph),
        )
        .expect("mesh family executor");
        let plan = NativeVulkanSceneMeshRuntimeFramePlan::from_parts(
            graph_execution,
            draw_family_executor,
            warmup,
            None,
            frame,
        );

        assert_eq!(
            plan.command_order,
            [
                "build_scene_graph_execution_plan",
                "select_scene_draw_family_executors",
                "require_warmed_mesh_pipelines",
                "record_layer_compositor_command_blocks"
            ]
        );
        assert_eq!(plan.draw_family_executor.missing_executor_draw_count, 0);
        assert!(plan.layer_compositor_command_blocks.is_none());
        assert_eq!(plan.pipeline_warmup.cache_keys().len(), 1);
        assert_eq!(plan.frame.pass_count, 1);
        assert_eq!(plan.frame.passes[0].pass.draw_count, 1);
    }

    #[test]
    fn layer_490_color_target_follows_image_layer_final_source() {
        let object = SceneObjectId(1530);
        let layer = compositor_layer_with_final_source(
            object,
            Some(SceneLayerCompositorTarget::ImageLayerCompositeA(object)),
        );

        let target = native_vulkan_scene_layer_490_color_graph_target(&layer)
            .expect("image-layer LayerTarget490 color target");

        assert_eq!(target, SceneGraphTarget::ImageLayerCompositeA(object));
    }

    #[test]
    fn layer_490_color_target_keeps_object_final_and_direct_routes() {
        let object = SceneObjectId(7);
        let object_final_layer = compositor_layer_with_final_source(
            object,
            Some(SceneLayerCompositorTarget::ObjectFinal(object)),
        );
        let direct_layer = SceneLayerCompositorLayer {
            object,
            route: SceneLayerCompositorRoute::DirectSwapchain,
            uses_tokenized_subdraw: true,
            has_active_aux_clear_target: false,
            commands: Vec::new(),
        };

        assert_eq!(
            native_vulkan_scene_layer_490_color_graph_target(&object_final_layer)
                .expect("object-final LayerTarget490 color target"),
            SceneGraphTarget::ObjectFinal(object)
        );
        assert_eq!(
            native_vulkan_scene_layer_490_color_graph_target(&direct_layer)
                .expect("direct LayerTarget490 color target"),
            SceneGraphTarget::Swapchain
        );
    }

    fn mesh_graph(draws: Vec<SceneGraphDraw>) -> SceneGraph {
        SceneGraph {
            passes: vec![SceneGraphPass {
                name: "scene-main".to_owned(),
                input: None,
                output: SceneGraphTarget::Swapchain,
                draws,
            }],
        }
    }

    fn compositor_layer_with_final_source(
        object: SceneObjectId,
        source: Option<SceneLayerCompositorTarget>,
    ) -> SceneLayerCompositorLayer {
        SceneLayerCompositorLayer {
            object,
            route: SceneLayerCompositorRoute::ObjectFinalMeshComposite,
            uses_tokenized_subdraw: true,
            has_active_aux_clear_target: false,
            commands: vec![SceneLayerCompositorCommand {
                entry: SceneLayerCompositorEntry::FullLayerCompositeEntry51,
                operation: SceneLayerCompositorOperation::FullLayerComposite,
                condition: SceneLayerCompositorCondition::Always,
                source,
                target: SceneLayerCompositorTarget::Swapchain,
                blend_key: SceneLayerCompositorBlendKey::LowBlendNormalViaWrapper128,
            }],
        }
    }

    fn mesh_draw(object: SceneObjectId) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline: crate::engine::scene_engine::SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            geometry: Some(SceneGeometryId(object.0)),
            puppet: None,
            resources: vec![crate::engine::scene_engine::SceneGraphResourceBinding {
                slot: 0,
                role: crate::engine::scene_engine::SceneGraphResourceRole::shader_texture(0),
                resource: crate::engine::scene_engine::SceneResourceId(object.0),
            }],
            index_count: 6,
        }
    }
}

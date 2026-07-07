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
    SceneLayerCompositorPlan, SceneLayerCompositorRoute, SceneLayerCompositorTarget, SceneObjectId,
};
use crate::renderer::native_vulkan::NativeVulkanClearColor;

use super::draw_family::{
    NativeVulkanSceneDrawFamilyExecutorPlan, native_vulkan_require_scene_mesh_executor_families,
};
use super::effect_executor::{
    NativeVulkanSceneEffectRuntimeFrameContext, NativeVulkanSceneEffectRuntimeFramePlan,
    native_vulkan_record_scene_effect_runtime_frame,
};
use super::frame_resources::NativeVulkanSceneFrameResources;
use super::graph_executor::{
    NativeVulkanSceneGraphFrameCommandPlan, NativeVulkanSceneGraphRuntimeFrameContext,
    native_vulkan_record_scene_graph_frame_commands,
};
use super::layer_alpha_mask_executor::{
    NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
    NativeVulkanSceneLayerAlphaMaskLayerTargetBinding,
    NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskProducerPipelinePlan,
    NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan,
    NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan,
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskRuntimePlan, NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
    native_vulkan_plan_scene_layer_alpha_mask_copy_back_runtime_commands,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_runtime_commands,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_targets,
    native_vulkan_plan_scene_layer_alpha_mask_producer_draws,
    native_vulkan_plan_scene_layer_alpha_mask_producer_pipelines,
    native_vulkan_plan_scene_layer_alpha_mask_producer_target_graph,
    native_vulkan_plan_scene_layer_alpha_mask_recorder_requirements,
    native_vulkan_plan_scene_layer_alpha_mask_resource_binds,
    native_vulkan_plan_scene_layer_alpha_mask_runtime_frame,
    native_vulkan_plan_scene_layer_alpha_mask_token_schedule,
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
    pub layer_alpha_mask_generated_consumer_draws:
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    pub layer_alpha_mask_generated_consumer_targets:
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
    pub layer_alpha_mask_generated_consumer_pipelines:
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    pub layer_alpha_mask_generated_consumer_commands:
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    pub layer_alpha_mask_recorder_requirements:
        NativeVulkanSceneLayerAlphaMaskRecorderRequirementPlan,
    pub layer_alpha_mask_copy_back_commands:
        NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan,
    pub mesh: NativeVulkanSceneMeshRuntimeFramePlan<'a>,
    pub command_order: [&'static str; 15],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshRuntimeFramePlan<'a> {
    pub graph_execution: SceneGraphExecutionPlan,
    pub draw_family_executor: NativeVulkanSceneDrawFamilyExecutorPlan,
    pub pipeline_warmup: NativeVulkanSceneMeshPipelineWarmupPlan,
    pub frame: NativeVulkanSceneGraphFrameCommandPlan<'a>,
    pub command_order: [&'static str; 4],
}

impl<'a> NativeVulkanSceneMeshRuntimeFramePlan<'a> {
    fn from_parts(
        graph_execution: SceneGraphExecutionPlan,
        draw_family_executor: NativeVulkanSceneDrawFamilyExecutorPlan,
        pipeline_warmup: NativeVulkanSceneMeshPipelineWarmupPlan,
        frame: NativeVulkanSceneGraphFrameCommandPlan<'a>,
    ) -> Self {
        Self {
            graph_execution,
            draw_family_executor,
            pipeline_warmup,
            frame,
            command_order: [
                "build_scene_graph_execution_plan",
                "select_scene_draw_family_executors",
                "require_warmed_mesh_pipelines",
                "record_scene_graph_frame_commands",
            ],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_runtime_frame<'a>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneRuntimeFrameContext<'_>,
    frame: &'a SceneFramePlan,
) -> Result<NativeVulkanSceneRuntimeFramePlan<'a>, String> {
    let effects = native_vulkan_record_scene_effect_runtime_frame(
        frame_resources,
        NativeVulkanSceneEffectRuntimeFrameContext {
            device: context.device,
            command_buffer: context.command_buffer,
            target_formats: context.target_formats,
        },
        &frame.effect_pass_graph,
    )?;
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
    let layer_alpha_mask_generated_consumer_draws =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws(
            &layer_alpha_masks,
            &layer_alpha_mask_resource_binds,
            &layer_alpha_mask_token_schedule,
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
        )?;
    let layer_alpha_mask_recorder_requirements =
        native_vulkan_plan_scene_layer_alpha_mask_recorder_requirements(
            &layer_alpha_masks,
            &layer_alpha_mask_resource_binds,
            &layer_alpha_mask_token_schedule,
            &layer_alpha_mask_producer_draws,
            &layer_alpha_mask_producer_target_graph,
            &layer_alpha_mask_generated_consumer_draws,
            &layer_alpha_mask_generated_consumer_targets,
            &layer_alpha_mask_generated_consumer_pipelines,
            &layer_alpha_mask_generated_consumer_commands,
        )?;
    let layer_alpha_mask_copy_back_commands =
        native_vulkan_plan_scene_layer_alpha_mask_copy_back_runtime_commands(
            frame_resources,
            &layer_alpha_mask_resource_binds,
        )?;
    let mesh = native_vulkan_record_scene_mesh_runtime_frame(
        frame_resources,
        NativeVulkanSceneMeshRuntimeFrameContext {
            device: context.device,
            command_buffer: context.command_buffer,
            target: context.target,
            target_formats: context.target_formats,
            clear_color: context.clear_color,
        },
        frame,
    )?;
    Ok(NativeVulkanSceneRuntimeFramePlan {
        effects,
        layer_alpha_masks,
        layer_alpha_mask_resource_binds,
        layer_alpha_mask_token_schedule,
        layer_alpha_mask_producer_draws,
        layer_alpha_mask_producer_pipelines,
        layer_alpha_mask_producer_target_graph,
        layer_alpha_mask_generated_consumer_draws,
        layer_alpha_mask_generated_consumer_targets,
        layer_alpha_mask_generated_consumer_pipelines,
        layer_alpha_mask_generated_consumer_commands,
        layer_alpha_mask_recorder_requirements,
        layer_alpha_mask_copy_back_commands,
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
            "plan_layer_alpha_mask_generated_consumer_draws",
            "plan_layer_alpha_mask_generated_consumer_targets",
            "plan_layer_alpha_mask_generated_consumer_pipelines",
            "plan_layer_alpha_mask_generated_consumer_commands",
            "plan_layer_alpha_mask_recorder_requirements",
            "plan_layer_alpha_mask_copy_back_command_list",
            "record_scene_mesh_graph_runtime",
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
    let color_target = match layer.route {
        SceneLayerCompositorRoute::DirectSwapchain => SceneGraphTarget::Swapchain,
        SceneLayerCompositorRoute::ObjectFinalMeshComposite => {
            SceneGraphTarget::ObjectFinal(object)
        }
    };
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_mesh_runtime_frame<'a>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneMeshRuntimeFrameContext<'_>,
    frame: &'a SceneFramePlan,
) -> Result<NativeVulkanSceneMeshRuntimeFramePlan<'a>, String> {
    let graph_execution = SceneGraphExecutionPlan::from_graph(&frame.graph);
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

    let frame_plan = native_vulkan_record_scene_graph_frame_commands(
        frame_resources,
        NativeVulkanSceneGraphRuntimeFrameContext {
            device: context.device,
            command_buffer: context.command_buffer,
            swapchain_target: context.target,
            target_formats: context.target_formats,
            clear_color: context.clear_color,
        },
        frame,
        &graph_execution,
    )?;

    Ok(NativeVulkanSceneMeshRuntimeFramePlan::from_parts(
        graph_execution,
        draw_family_executor,
        pipeline_warmup,
        frame_plan,
    ))
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
        SceneGraphPass, SceneGraphTarget, SceneMaterialKey, SceneObjectId,
    };

    #[test]
    fn runtime_frame_plan_preserves_hot_path_execution_order() {
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
            frame,
        );

        assert_eq!(
            plan.command_order,
            [
                "build_scene_graph_execution_plan",
                "select_scene_draw_family_executors",
                "require_warmed_mesh_pipelines",
                "record_scene_graph_frame_commands"
            ]
        );
        assert_eq!(plan.draw_family_executor.missing_executor_draw_count, 0);
        assert_eq!(plan.pipeline_warmup.cache_keys().len(), 1);
        assert_eq!(plan.frame.pass_count, 1);
        assert_eq!(plan.frame.passes[0].pass.draw_count, 1);
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

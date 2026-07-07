//! Scene mesh runtime frame wiring for the native Vulkan backend.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{SceneFramePlan, SceneGraphExecutionPlan};
use crate::renderer::native_vulkan::NativeVulkanClearColor;

use super::frame_resources::NativeVulkanSceneFrameResources;
use super::graph_executor::{
    NativeVulkanSceneGraphFrameCommandPlan, NativeVulkanSceneGraphRuntimeFrameContext,
    native_vulkan_record_scene_graph_frame_commands,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshRuntimeFramePlan<'a> {
    pub graph_execution: SceneGraphExecutionPlan,
    pub pipeline_warmup: NativeVulkanSceneMeshPipelineWarmupPlan,
    pub frame: NativeVulkanSceneGraphFrameCommandPlan<'a>,
    pub command_order: [&'static str; 3],
}

impl<'a> NativeVulkanSceneMeshRuntimeFramePlan<'a> {
    fn from_parts(
        graph_execution: SceneGraphExecutionPlan,
        pipeline_warmup: NativeVulkanSceneMeshPipelineWarmupPlan,
        frame: NativeVulkanSceneGraphFrameCommandPlan<'a>,
    ) -> Self {
        Self {
            graph_execution,
            pipeline_warmup,
            frame,
            command_order: [
                "build_scene_graph_execution_plan",
                "require_warmed_mesh_pipelines",
                "record_scene_graph_frame_commands",
            ],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_mesh_runtime_frame<'a>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneMeshRuntimeFrameContext<'_>,
    frame: &'a SceneFramePlan,
) -> Result<NativeVulkanSceneMeshRuntimeFramePlan<'a>, String> {
    let graph_execution = SceneGraphExecutionPlan::from_graph(&frame.graph);
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
        SceneBlendContract, SceneGeometryId, SceneGraph, SceneGraphDraw, SceneGraphPass,
        SceneGraphTarget, SceneMaterialKey, SceneObjectId,
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
            pipeline_warmup: warmup.clone(),
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
                "require_warmed_mesh_pipelines",
                "record_graph_pass_render_targets",
                "record_mesh_pass_draw_lists",
                "record_scene_graph_target_barriers",
            ],
        };

        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);
        let plan =
            NativeVulkanSceneMeshRuntimeFramePlan::from_parts(graph_execution, warmup, frame);

        assert_eq!(
            plan.command_order,
            [
                "build_scene_graph_execution_plan",
                "require_warmed_mesh_pipelines",
                "record_scene_graph_frame_commands"
            ]
        );
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

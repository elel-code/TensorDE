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

use super::frame_command::{
    NativeVulkanSceneMeshFrameCommandPlan, native_vulkan_record_scene_mesh_frame_commands,
};
use super::frame_resources::NativeVulkanSceneFrameResources;
use super::pipeline::NativeVulkanScenePipelineCacheKey;
use super::pipeline_warmup::NativeVulkanSceneMeshPipelineWarmupPlan;
use super::render_target::NativeVulkanSceneSwapchainRenderTarget;

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshRuntimeFrameContext<'a> {
    pub device: &'a Device,
    pub command_buffer: vk::CommandBuffer,
    pub target: NativeVulkanSceneSwapchainRenderTarget,
    pub target_format: vk::Format,
    pub clear_color: Option<NativeVulkanClearColor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshRuntimeFramePlan<'a> {
    pub graph_execution: SceneGraphExecutionPlan,
    pub pipeline_warmup: NativeVulkanSceneMeshPipelineWarmupPlan,
    pub frame: NativeVulkanSceneMeshFrameCommandPlan<'a>,
    pub command_order: [&'static str; 3],
}

impl<'a> NativeVulkanSceneMeshRuntimeFramePlan<'a> {
    fn from_parts(
        graph_execution: SceneGraphExecutionPlan,
        pipeline_warmup: NativeVulkanSceneMeshPipelineWarmupPlan,
        frame: NativeVulkanSceneMeshFrameCommandPlan<'a>,
    ) -> Self {
        Self {
            graph_execution,
            pipeline_warmup,
            frame,
            command_order: [
                "select_scene_graph_executor",
                "require_warmed_mesh_pipelines",
                "record_mesh_frame_commands",
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
    let (pass, draw_index_start) = graph_execution.single_swapchain_indexed_pass(&frame.graph)?;
    let pipeline_warmup = NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(
        &frame.graph,
        context.target_format,
    )?;

    for key in pipeline_warmup.cache_keys() {
        frame_resources.cached_mesh_pipeline(key).map_err(|err| {
            format!(
                "{err}; scene mesh runtime requires pipeline warmup before present-frame recording"
            )
        })?;
    }

    let frame_plan = native_vulkan_record_scene_mesh_frame_commands(
        context.device,
        context.command_buffer,
        context.target,
        context.clear_color,
        pass,
        draw_index_start,
        |key| {
            let cache_key =
                NativeVulkanScenePipelineCacheKey::from_bind_key(key, context.target_format)?;
            Ok(frame_resources.cached_mesh_pipeline(&cache_key)?.pipeline)
        },
        |draw_index| frame_resources.resource_heap_draw_bind_info_for_draw(draw_index),
        |geometry| frame_resources.mesh_draw_buffers(geometry),
    )?;

    Ok(NativeVulkanSceneMeshRuntimeFramePlan::from_parts(
        graph_execution,
        pipeline_warmup,
        frame_plan,
    ))
}

#[cfg(test)]
mod tests {
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
        let frame = NativeVulkanSceneMeshFrameCommandPlan::from_target_and_pass(
            NativeVulkanSceneRenderTargetScopePlan {
                width: 3840,
                height: 2160,
                load_op: NativeVulkanSceneRenderTargetLoadOp::Clear,
                begin_command_order: [
                    "cmd_pipeline_barrier2_color_attachment",
                    "cmd_begin_rendering",
                ],
                end_command_order: ["cmd_end_rendering", "cmd_pipeline_barrier2_present"],
            },
            NativeVulkanSceneMeshPassCommandPlan {
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
            },
        );

        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);
        let plan =
            NativeVulkanSceneMeshRuntimeFramePlan::from_parts(graph_execution, warmup, frame);

        assert_eq!(
            plan.command_order,
            [
                "select_scene_graph_executor",
                "require_warmed_mesh_pipelines",
                "record_mesh_frame_commands"
            ]
        );
        assert!(
            plan.graph_execution
                .supports_single_swapchain_indexed_runtime
        );
        assert_eq!(plan.pipeline_warmup.cache_keys().len(), 1);
        assert_eq!(plan.frame.pass.draw_count, 1);
    }

    #[test]
    fn runtime_pass_requires_exactly_one_swapchain_pass() {
        let graph = mesh_graph(vec![mesh_draw(SceneObjectId(1))]);
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);

        let pass = graph_execution
            .single_swapchain_indexed_pass(&graph)
            .expect("scene-main pass");

        assert_eq!(pass.1, 0);
        assert_eq!(graph_execution.passes[0].draw_index_end, 1);
        assert_eq!(pass.0.name, "scene-main");
        assert_eq!(
            pass.0.output,
            crate::engine::scene_engine::SceneGraphTarget::Swapchain
        );
    }

    #[test]
    fn runtime_pass_rejects_empty_graph() {
        let graph = crate::engine::scene_engine::SceneGraph::default();
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);
        let err = graph_execution
            .single_swapchain_indexed_pass(&graph)
            .expect_err("empty graph must fail");

        assert!(err.contains("graph executor"));
    }

    #[test]
    fn runtime_pass_rejects_multiple_passes() {
        let graph = SceneGraph {
            passes: vec![
                mesh_pass("scene-main", SceneGraphTarget::Swapchain),
                mesh_pass("scene-second", SceneGraphTarget::Swapchain),
            ],
        };
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);

        let err = graph_execution
            .single_swapchain_indexed_pass(&graph)
            .expect_err("multiple passes must fail until graph executor exists");

        assert!(err.contains("passes=2"));
    }

    #[test]
    fn runtime_pass_rejects_non_swapchain_target_without_target_executor() {
        let graph = SceneGraph {
            passes: vec![mesh_pass(
                "scene-offscreen",
                SceneGraphTarget::ImageLocalMain(0),
            )],
        };
        let graph_execution = SceneGraphExecutionPlan::from_graph(&graph);

        let err = graph_execution
            .single_swapchain_indexed_pass(&graph)
            .expect_err("offscreen target must fail until target executor exists");

        assert!(err.contains("swapchain_outputs=0"));
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

    fn mesh_pass(name: &str, output: SceneGraphTarget) -> SceneGraphPass {
        SceneGraphPass {
            name: name.to_owned(),
            input: None,
            output,
            draws: vec![mesh_draw(SceneObjectId(1))],
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

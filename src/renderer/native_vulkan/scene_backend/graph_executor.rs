//! Scene graph pass executor for retained Vulkan scene resources.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneFramePlan, SceneGraphExecutionPass, SceneGraphExecutionPlan, SceneGraphTarget,
};
use crate::renderer::native_vulkan::NativeVulkanClearColor;

use super::frame_resources::NativeVulkanSceneFrameResources;
use super::pass_command::{
    NativeVulkanSceneMeshPassCommandPlan, native_vulkan_record_scene_mesh_pass_draw_commands,
};
use super::pipeline::NativeVulkanScenePipelineCacheKey;
use super::render_target::{
    NativeVulkanSceneOffscreenRenderTarget, NativeVulkanSceneRenderTarget,
    NativeVulkanSceneRenderTargetScopePlan, NativeVulkanSceneSwapchainRenderTarget,
    native_vulkan_record_scene_render_target_begin, native_vulkan_record_scene_render_target_end,
};
use super::target_barriers::{
    NativeVulkanSceneTargetBarrierImage, NativeVulkanSceneTargetBarrierPlan,
    native_vulkan_record_scene_target_barrier, native_vulkan_scene_target_usage_layout,
};
use super::target_formats::NativeVulkanSceneGraphTargetFormatPlan;

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneGraphRuntimeFrameContext<'a> {
    pub device: &'a Device,
    pub command_buffer: vk::CommandBuffer,
    pub swapchain_target: NativeVulkanSceneSwapchainRenderTarget,
    pub target_formats: &'a NativeVulkanSceneGraphTargetFormatPlan,
    pub clear_color: Option<NativeVulkanClearColor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneGraphFrameCommandPlan<'a> {
    pub pass_count: usize,
    pub target_barrier_count: usize,
    pub target_format_count: usize,
    pub passes: Vec<NativeVulkanSceneGraphPassCommandPlan<'a>>,
    pub target_barriers: Vec<NativeVulkanSceneTargetBarrierPlan>,
    pub command_order: [&'static str; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneGraphPassCommandPlan<'a> {
    pub target: SceneGraphTarget,
    pub target_scope: NativeVulkanSceneRenderTargetScopePlan,
    pub pass: NativeVulkanSceneMeshPassCommandPlan<'a>,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_graph_frame_commands<'a>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneGraphRuntimeFrameContext<'_>,
    frame: &'a SceneFramePlan,
    graph_execution: &SceneGraphExecutionPlan,
) -> Result<NativeVulkanSceneGraphFrameCommandPlan<'a>, String> {
    let mut passes = Vec::with_capacity(graph_execution.passes.len());
    let mut target_barriers = Vec::with_capacity(graph_execution.target_barriers.len());
    for execution_pass in &graph_execution.passes {
        native_vulkan_record_scene_graph_target_barriers_before_pass(
            frame_resources,
            &context,
            graph_execution,
            execution_pass.pass_index,
            &mut target_barriers,
        )?;

        let graph_pass = frame
            .graph
            .passes
            .get(execution_pass.pass_index)
            .ok_or_else(|| {
                format!(
                    "scene graph executor pass index {} is outside graph pass list",
                    execution_pass.pass_index
                )
            })?;
        let render_target =
            resolve_pass_render_target(frame_resources, &context, graph_execution, execution_pass)?;
        native_vulkan_record_scene_graph_pass_input_access(
            frame_resources,
            &context,
            execution_pass,
        )?;
        let clear_color = pass_clear_color(
            graph_execution,
            execution_pass,
            render_target,
            context.clear_color,
        );
        let target_scope = native_vulkan_record_scene_render_target_begin(
            context.device,
            context.command_buffer,
            render_target,
            clear_color,
        )?;
        mark_output_target_layout(
            frame_resources,
            execution_pass.output,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        )?;

        let pass_target_format = context.target_formats.format(execution_pass.output)?;
        let pass_plan = {
            let resources = &*frame_resources;
            native_vulkan_record_scene_mesh_pass_draw_commands(
                context.device,
                context.command_buffer,
                graph_pass,
                execution_pass.draw_index_start,
                |key| {
                    let cache_key =
                        NativeVulkanScenePipelineCacheKey::from_bind_key(key, pass_target_format)?;
                    Ok(resources.cached_mesh_pipeline(&cache_key)?.pipeline)
                },
                |draw_index| resources.resource_heap_draw_bind_info_for_draw(draw_index),
                |geometry| resources.mesh_draw_buffers(geometry),
            )?
        };

        native_vulkan_record_scene_render_target_end(
            context.device,
            context.command_buffer,
            render_target,
            clear_color,
        )?;
        mark_output_target_layout(
            frame_resources,
            execution_pass.output,
            render_target.final_layout(),
        )?;

        passes.push(NativeVulkanSceneGraphPassCommandPlan {
            target: execution_pass.output,
            target_scope,
            pass: pass_plan,
        });
    }

    Ok(NativeVulkanSceneGraphFrameCommandPlan {
        pass_count: passes.len(),
        target_barrier_count: target_barriers.len(),
        target_format_count: context.target_formats.target_format_count(),
        passes,
        target_barriers,
        command_order: [
            "resolve_scene_graph_target_formats",
            "record_graph_pass_render_targets",
            "record_mesh_pass_draw_lists",
            "record_scene_graph_target_barriers",
        ],
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_graph_target_barriers_before_pass(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneGraphRuntimeFrameContext<'_>,
    graph_execution: &SceneGraphExecutionPlan,
    pass_index: usize,
    target_barriers: &mut Vec<NativeVulkanSceneTargetBarrierPlan>,
) -> Result<(), String> {
    for barrier in graph_execution
        .target_barriers
        .iter()
        .filter(|barrier| barrier.after_pass == pass_index)
    {
        let image = target_barrier_image(frame_resources, context, barrier.target)?;
        let plan = native_vulkan_record_scene_target_barrier(
            context.device,
            context.command_buffer,
            barrier,
            image,
        )?;
        mark_output_target_layout(
            frame_resources,
            barrier.target,
            native_vulkan_scene_target_usage_layout(barrier.next_usage),
        )?;
        target_barriers.push(plan);
    }
    Ok(())
}

fn target_barrier_image(
    frame_resources: &NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneGraphRuntimeFrameContext<'_>,
    target: SceneGraphTarget,
) -> Result<NativeVulkanSceneTargetBarrierImage, String> {
    let image = match target {
        SceneGraphTarget::Swapchain => context.swapchain_target.image,
        target => frame_resources.offscreen_target_binding(target)?.image,
    };
    Ok(NativeVulkanSceneTargetBarrierImage { target, image })
}

fn resolve_pass_render_target(
    frame_resources: &NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneGraphRuntimeFrameContext<'_>,
    graph_execution: &SceneGraphExecutionPlan,
    execution_pass: &SceneGraphExecutionPass,
) -> Result<NativeVulkanSceneRenderTarget, String> {
    let final_layout = pass_output_final_layout(
        graph_execution,
        execution_pass.output,
        execution_pass.pass_index,
    )?;
    match execution_pass.output {
        SceneGraphTarget::Swapchain => {
            let mut target = context.swapchain_target;
            target.final_layout = final_layout;
            Ok(NativeVulkanSceneRenderTarget::Swapchain(target))
        }
        target => {
            let binding = frame_resources.offscreen_target_binding(target)?;
            Ok(NativeVulkanSceneRenderTarget::Offscreen(
                NativeVulkanSceneOffscreenRenderTarget {
                    target,
                    image: binding.image,
                    image_view: binding.view,
                    extent: vk::Extent2D {
                        width: binding.width,
                        height: binding.height,
                    },
                    initial_layout: binding.current_layout,
                    final_layout,
                },
            ))
        }
    }
}

fn pass_output_final_layout(
    graph_execution: &SceneGraphExecutionPlan,
    target: SceneGraphTarget,
    pass_index: usize,
) -> Result<vk::ImageLayout, String> {
    if target != SceneGraphTarget::Swapchain {
        return Ok(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    }
    let lifetime = graph_execution
        .target_lifetimes
        .iter()
        .find(|lifetime| lifetime.target == target)
        .ok_or_else(|| "scene graph execution has no swapchain target lifetime".to_owned())?;
    if lifetime.last_use_pass == pass_index {
        Ok(vk::ImageLayout::PRESENT_SRC_KHR)
    } else {
        Ok(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_graph_pass_input_access(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneGraphRuntimeFrameContext<'_>,
    execution_pass: &SceneGraphExecutionPass,
) -> Result<(), String> {
    let Some(input) = execution_pass.input else {
        return Ok(());
    };
    if input == SceneGraphTarget::Swapchain {
        return Err("scene mesh graph cannot sample the swapchain as a pass input".to_owned());
    }
    if input == execution_pass.output {
        return Err(format!(
            "scene mesh graph pass '{}' reads and writes {:?}; explicit ping-pong targets are required",
            execution_pass.name, input
        ));
    }
    super::effect_executor::target_access::record_effect_target_shader_read_access(
        frame_resources,
        context.device,
        context.command_buffer,
        input,
    )?;
    Ok(())
}

fn pass_clear_color(
    graph_execution: &SceneGraphExecutionPlan,
    execution_pass: &SceneGraphExecutionPass,
    render_target: NativeVulkanSceneRenderTarget,
    swapchain_clear_color: Option<NativeVulkanClearColor>,
) -> Option<NativeVulkanClearColor> {
    if execution_pass.output == SceneGraphTarget::Swapchain {
        return is_first_write_pass(graph_execution, execution_pass)
            .then_some(swapchain_clear_color)
            .flatten();
    }
    if render_target.initial_layout() == vk::ImageLayout::UNDEFINED
        || is_first_write_pass(graph_execution, execution_pass)
    {
        Some(transparent_clear_color())
    } else {
        None
    }
}

fn is_first_write_pass(
    graph_execution: &SceneGraphExecutionPlan,
    execution_pass: &SceneGraphExecutionPass,
) -> bool {
    graph_execution
        .target_lifetimes
        .iter()
        .find(|lifetime| lifetime.target == execution_pass.output)
        .and_then(|lifetime| lifetime.first_write_pass)
        == Some(execution_pass.pass_index)
}

fn transparent_clear_color() -> NativeVulkanClearColor {
    NativeVulkanClearColor {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    }
}

fn mark_output_target_layout(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    target: SceneGraphTarget,
    layout: vk::ImageLayout,
) -> Result<(), String> {
    if target == SceneGraphTarget::Swapchain {
        return Ok(());
    }
    frame_resources.mark_offscreen_target_layout(target, layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraph, SceneGraphDraw, SceneGraphPass,
        SceneGraphPipelineClass, SceneMaterialKey, SceneObjectId,
    };
    use vulkanalia::vk::Handle;

    #[test]
    fn graph_executor_keeps_swapchain_present_only_on_last_swapchain_pass() {
        let graph = SceneGraph {
            passes: vec![
                pass(
                    "scene-a",
                    None,
                    SceneGraphTarget::Swapchain,
                    SceneObjectId(1),
                ),
                pass(
                    "scene-b",
                    None,
                    SceneGraphTarget::Swapchain,
                    SceneObjectId(2),
                ),
            ],
        };
        let execution = SceneGraphExecutionPlan::from_graph(&graph);

        assert_eq!(
            pass_output_final_layout(&execution, SceneGraphTarget::Swapchain, 0).unwrap(),
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        );
        assert_eq!(
            pass_output_final_layout(&execution, SceneGraphTarget::Swapchain, 1).unwrap(),
            vk::ImageLayout::PRESENT_SRC_KHR
        );
    }

    #[test]
    fn graph_executor_clears_internal_target_on_first_frame_write() {
        let graph = SceneGraph {
            passes: vec![pass(
                "effect",
                None,
                SceneGraphTarget::EffectTarget(0),
                SceneObjectId(1),
            )],
        };
        let execution = SceneGraphExecutionPlan::from_graph(&graph);
        let render_target =
            NativeVulkanSceneRenderTarget::Offscreen(NativeVulkanSceneOffscreenRenderTarget {
                target: SceneGraphTarget::EffectTarget(0),
                image: vk::Image::from_raw(1),
                image_view: vk::ImageView::from_raw(2),
                extent: vk::Extent2D {
                    width: 1920,
                    height: 1080,
                },
                initial_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            });

        let clear = pass_clear_color(&execution, &execution.passes[0], render_target, None)
            .expect("first write clears target");

        assert_eq!(clear.a, 0.0);
    }

    fn pass(
        name: &str,
        input: Option<SceneGraphTarget>,
        output: SceneGraphTarget,
        object: SceneObjectId,
    ) -> SceneGraphPass {
        SceneGraphPass {
            name: name.to_owned(),
            input,
            output,
            draws: vec![SceneGraphDraw {
                object,
                pipeline: SceneGraphPipelineClass::Mesh,
                material: SceneMaterialKey {
                    shader: "we/genericimage4".to_owned(),
                    blend: SceneBlendContract::TranslucentAlpha,
                    render_state:
                        crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(),
                },
                geometry: Some(SceneGeometryId(object.0)),
                puppet: None,
                resources: Vec::new(),
                index_count: 6,
            }],
        }
    }
}

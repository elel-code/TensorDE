//! Scene frame command recording boundary.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{SceneGeometryId, SceneGraphPass, SceneResourceId};
use crate::renderer::native_vulkan::NativeVulkanClearColor;

use super::pass_command::{
    NativeVulkanSceneMeshPassCommandPlan, native_vulkan_record_scene_mesh_pass_draw_commands,
};
use super::pipeline::NativeVulkanScenePipelineKey;
use super::render_target::{
    NativeVulkanSceneRenderTargetScopePlan, NativeVulkanSceneSwapchainRenderTarget,
    native_vulkan_record_scene_swapchain_render_target_begin,
    native_vulkan_record_scene_swapchain_render_target_end,
};
use super::resource_buffers::NativeVulkanSceneMeshDrawBuffers;
use super::texture_heap::NativeVulkanSceneTextureHeapDrawBindInfo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneMeshFrameCommandPlan<'a> {
    pub target: NativeVulkanSceneRenderTargetScopePlan,
    pub pass: NativeVulkanSceneMeshPassCommandPlan<'a>,
    pub command_order: [&'static str; 5],
}

impl<'a> NativeVulkanSceneMeshFrameCommandPlan<'a> {
    pub fn from_target_and_pass(
        target: NativeVulkanSceneRenderTargetScopePlan,
        pass: NativeVulkanSceneMeshPassCommandPlan<'a>,
    ) -> Self {
        Self {
            target,
            pass,
            command_order: [
                "cmd_pipeline_barrier2_color_attachment",
                "cmd_begin_rendering",
                "scene_mesh_pass_draw_list",
                "cmd_end_rendering",
                "cmd_pipeline_barrier2_present",
            ],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_mesh_frame_commands<
    'a,
    PipelineForKey,
    TextureHeapBindForResource,
    MeshBuffersForGeometry,
>(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    target: NativeVulkanSceneSwapchainRenderTarget,
    clear_color: Option<NativeVulkanClearColor>,
    pass: &'a SceneGraphPass,
    pipeline_for_key: PipelineForKey,
    texture_heap_bind_for_resource: TextureHeapBindForResource,
    mesh_buffers: MeshBuffersForGeometry,
) -> Result<NativeVulkanSceneMeshFrameCommandPlan<'a>, String>
where
    PipelineForKey: FnMut(NativeVulkanScenePipelineKey<'a>) -> Result<vk::Pipeline, String>,
    TextureHeapBindForResource:
        FnMut(SceneResourceId) -> Result<NativeVulkanSceneTextureHeapDrawBindInfo, String>,
    MeshBuffersForGeometry:
        FnMut(SceneGeometryId) -> Result<NativeVulkanSceneMeshDrawBuffers, String>,
{
    let target_plan = native_vulkan_record_scene_swapchain_render_target_begin(
        device,
        command_buffer,
        target,
        clear_color,
    )?;
    let pass_plan = native_vulkan_record_scene_mesh_pass_draw_commands(
        device,
        command_buffer,
        pass,
        pipeline_for_key,
        texture_heap_bind_for_resource,
        mesh_buffers,
    )?;
    native_vulkan_record_scene_swapchain_render_target_end(
        device,
        command_buffer,
        target,
        clear_color,
    )?;

    Ok(NativeVulkanSceneMeshFrameCommandPlan::from_target_and_pass(
        target_plan,
        pass_plan,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::pass_command::NativeVulkanSceneMeshPassCommandPlan;
    use super::super::render_target::NativeVulkanSceneRenderTargetLoadOp;
    use super::*;
    use crate::engine::scene_engine::SceneGraphTarget;

    #[test]
    fn mesh_frame_plan_wraps_target_scope_around_draw_list() {
        let plan = NativeVulkanSceneMeshFrameCommandPlan::from_target_and_pass(
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
                draw_count: 2,
                pipeline_bind_count: 1,
                texture_heap_bind_count: 0,
                indexed_draw_count: 2,
                commands: Vec::new(),
            },
        );

        assert_eq!(
            plan.command_order,
            [
                "cmd_pipeline_barrier2_color_attachment",
                "cmd_begin_rendering",
                "scene_mesh_pass_draw_list",
                "cmd_end_rendering",
                "cmd_pipeline_barrier2_present"
            ]
        );
        assert_eq!(plan.target.width, 3840);
        assert_eq!(plan.pass.draw_count, 2);
        assert_eq!(plan.pass.pipeline_bind_count, 1);
    }

    #[test]
    fn mesh_frame_plan_keeps_empty_pass_commands_empty() {
        let plan = NativeVulkanSceneMeshFrameCommandPlan::from_target_and_pass(
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
                draw_count: 0,
                pipeline_bind_count: 0,
                texture_heap_bind_count: 0,
                indexed_draw_count: 0,
                commands: Vec::new(),
            },
        );

        assert_eq!(plan.pass.draw_count, 0);
        assert_eq!(plan.pass.commands.len(), 0);
    }
}

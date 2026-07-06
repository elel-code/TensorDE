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

use crate::engine::scene_engine::{SceneFramePlan, SceneGraph, SceneGraphPass, SceneGraphTarget};
use crate::renderer::native_vulkan::NativeVulkanClearColor;
use crate::renderer::native_vulkan::vulkan::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot;

use super::frame_command::{
    NativeVulkanSceneMeshFrameCommandPlan, native_vulkan_record_scene_mesh_frame_commands,
};
use super::frame_resources::NativeVulkanSceneFrameResources;
use super::pipeline::NativeVulkanScenePipelineCacheKey;
use super::pipeline_factory::{
    NativeVulkanSceneMeshPipelineLayoutSpec, NativeVulkanSceneMeshPipelineShaders,
};
use super::pipeline_warmup::NativeVulkanSceneMeshPipelineWarmupPlan;
use super::render_target::NativeVulkanSceneSwapchainRenderTarget;
use super::texture_descriptors::NativeVulkanSceneTextureDescriptorFramePlan;

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshRuntimeFrameContext<'a> {
    pub device: &'a Device,
    pub memory_properties: &'a vk::PhysicalDeviceMemoryProperties,
    pub command_pool: vk::CommandPool,
    pub queue: vk::Queue,
    pub descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    pub command_buffer: vk::CommandBuffer,
    pub target: NativeVulkanSceneSwapchainRenderTarget,
    pub target_format: vk::Format,
    pub clear_color: Option<NativeVulkanClearColor>,
    pub shaders: NativeVulkanSceneMeshPipelineShaders<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshRuntimeFramePlan<'a> {
    pub residency_command_count: usize,
    pub texture_descriptors: NativeVulkanSceneTextureDescriptorFramePlan,
    pub texture_image_action_count: usize,
    pub texture_heap_action_count: usize,
    pub gpu_buffer_action_count: usize,
    pub pipeline_warmup: NativeVulkanSceneMeshPipelineWarmupPlan,
    pub frame: NativeVulkanSceneMeshFrameCommandPlan<'a>,
    pub command_order: [&'static str; 7],
}

impl<'a> NativeVulkanSceneMeshRuntimeFramePlan<'a> {
    fn from_parts(
        residency_command_count: usize,
        texture_descriptors: NativeVulkanSceneTextureDescriptorFramePlan,
        texture_image_action_count: usize,
        texture_heap_action_count: usize,
        gpu_buffer_action_count: usize,
        pipeline_warmup: NativeVulkanSceneMeshPipelineWarmupPlan,
        frame: NativeVulkanSceneMeshFrameCommandPlan<'a>,
    ) -> Self {
        Self {
            residency_command_count,
            texture_descriptors,
            texture_image_action_count,
            texture_heap_action_count,
            gpu_buffer_action_count,
            pipeline_warmup,
            frame,
            command_order: [
                "sync_residency",
                "prepare_texture_descriptors",
                "sync_texture_images",
                "sync_texture_descriptor_heap",
                "sync_gpu_uploads",
                "warm_mesh_pipelines",
                "record_mesh_frame_commands",
            ],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_mesh_runtime_frame<'a>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanSceneMeshRuntimeFrameContext<'_>,
    resources: &[crate::engine::scene_engine::SceneResource],
    frame: &'a SceneFramePlan,
) -> Result<NativeVulkanSceneMeshRuntimeFramePlan<'a>, String> {
    let pass = native_vulkan_scene_mesh_runtime_pass(&frame.graph)?;
    let pipeline_warmup = NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(
        &frame.graph,
        context.target_format,
    )?;

    let residency_command_count = frame_resources.sync_residency_plan(&frame.residency).len();
    let texture_descriptors = frame_resources.texture_descriptor_frame_plan(&frame.graph)?;
    let texture_image_action_count = frame_resources
        .sync_texture_images(
            context.device,
            context.memory_properties,
            context.command_pool,
            context.queue,
            resources,
        )?
        .len();
    let texture_heap_action_count = frame_resources
        .sync_texture_descriptor_heap(
            context.device,
            context.memory_properties,
            context.descriptor_heap_properties,
            &texture_descriptors,
        )?
        .len();
    let texture_descriptor_heap_plan = frame_resources
        .current_texture_heap_frame_plan()
        .ok_or_else(|| "scene mesh runtime missing texture descriptor heap frame plan".to_owned())?
        .descriptor_heap_plan
        .clone();
    let pipeline_layout = NativeVulkanSceneMeshPipelineLayoutSpec {
        texture_descriptor_heap_plan: &texture_descriptor_heap_plan,
    };
    let gpu_buffer_action_count = frame_resources
        .sync_gpu_uploads(
            context.device,
            context.memory_properties,
            context.command_pool,
            context.queue,
            resources,
        )?
        .len();

    for key in pipeline_warmup.cache_keys().iter().cloned() {
        frame_resources.resolve_mesh_pipeline(
            context.device,
            key,
            context.shaders,
            pipeline_layout,
        )?;
    }

    let frame_plan = native_vulkan_record_scene_mesh_frame_commands(
        context.device,
        context.command_buffer,
        context.target,
        context.clear_color,
        pass,
        |key| {
            let cache_key =
                NativeVulkanScenePipelineCacheKey::from_bind_key(key, context.target_format)?;
            Ok(frame_resources.cached_mesh_pipeline(&cache_key)?.pipeline)
        },
        |texture_set| frame_resources.texture_heap_draw_bind_info_for_set(texture_set),
        |geometry| frame_resources.mesh_draw_buffers(geometry),
    )?;

    Ok(NativeVulkanSceneMeshRuntimeFramePlan::from_parts(
        residency_command_count,
        texture_descriptors,
        texture_image_action_count,
        texture_heap_action_count,
        gpu_buffer_action_count,
        pipeline_warmup,
        frame_plan,
    ))
}

fn native_vulkan_scene_mesh_runtime_pass(graph: &SceneGraph) -> Result<&SceneGraphPass, String> {
    match graph.passes.as_slice() {
        [pass] if pass.output == SceneGraphTarget::Swapchain => Ok(pass),
        [pass] => Err(format!(
            "scene mesh runtime requires one swapchain pass, got {:?}",
            pass.output
        )),
        [] => Err("scene mesh runtime requires one scene graph pass".to_owned()),
        passes => Err(format!(
            "scene mesh runtime requires one scene graph pass, got {}",
            passes.len()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::super::pass_command::NativeVulkanSceneMeshPassCommandPlan;
    use super::super::render_target::{
        NativeVulkanSceneRenderTargetLoadOp, NativeVulkanSceneRenderTargetScopePlan,
    };
    use super::super::texture_descriptors::{
        NativeVulkanSceneTextureDescriptorBinding, NativeVulkanSceneTextureDescriptorFramePlan,
    };
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphDraw, SceneMaterialKey, SceneObjectId,
    };

    #[test]
    fn runtime_frame_plan_preserves_godot_style_execution_order() {
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
                draw_count: 1,
                pipeline_bind_count: 1,
                texture_heap_bind_count: 1,
                indexed_draw_count: 1,
                commands: Vec::new(),
            },
        );

        let descriptors = NativeVulkanSceneTextureDescriptorFramePlan {
            draw_count: 1,
            binding_count: 1,
            bindings: vec![NativeVulkanSceneTextureDescriptorBinding {
                draw_index: 0,
                object: SceneObjectId(1),
                slot: 0,
                role: crate::engine::scene_engine::SceneGraphResourceRole::shader_texture(0),
                resource: crate::engine::scene_engine::SceneResourceId(9),
                width: Some(1024),
                height: Some(1024),
                format: Some(crate::engine::scene_engine::SceneTextureFormat::R8G8B8A8Unorm),
                mip_count: Some(1),
                payload_bytes: Some(4_194_304),
                shader_mapping: "set0.binding0.g_Texture0".to_owned(),
            }],
            descriptor_model: "VK_EXT_descriptor_heap",
            command_order: [
                "resolve_resident_texture_descriptors",
                "bind_descriptor_heap_texture_mapping",
            ],
        };

        let plan = NativeVulkanSceneMeshRuntimeFramePlan::from_parts(
            2,
            descriptors,
            4,
            1,
            3,
            warmup,
            frame,
        );

        assert_eq!(
            plan.command_order,
            [
                "sync_residency",
                "prepare_texture_descriptors",
                "sync_texture_images",
                "sync_texture_descriptor_heap",
                "sync_gpu_uploads",
                "warm_mesh_pipelines",
                "record_mesh_frame_commands"
            ]
        );
        assert_eq!(plan.residency_command_count, 2);
        assert_eq!(plan.texture_descriptors.binding_count, 1);
        assert_eq!(plan.texture_image_action_count, 4);
        assert_eq!(plan.texture_heap_action_count, 1);
        assert_eq!(plan.gpu_buffer_action_count, 3);
        assert_eq!(plan.pipeline_warmup.cache_keys().len(), 1);
        assert_eq!(plan.frame.pass.draw_count, 1);
    }

    #[test]
    fn runtime_pass_requires_exactly_one_swapchain_pass() {
        let graph = mesh_graph(vec![mesh_draw(SceneObjectId(1))]);

        let pass = native_vulkan_scene_mesh_runtime_pass(&graph).expect("scene-main pass");

        assert_eq!(pass.name, "scene-main");
        assert_eq!(pass.output, SceneGraphTarget::Swapchain);
    }

    #[test]
    fn runtime_pass_rejects_empty_graph() {
        let err = native_vulkan_scene_mesh_runtime_pass(&SceneGraph::default())
            .expect_err("empty graph must fail");

        assert!(err.contains("requires one scene graph pass"));
    }

    #[test]
    fn runtime_pass_rejects_multiple_passes() {
        let graph = SceneGraph {
            passes: vec![
                mesh_pass("scene-main", SceneGraphTarget::Swapchain),
                mesh_pass("scene-second", SceneGraphTarget::Swapchain),
            ],
        };

        let err = native_vulkan_scene_mesh_runtime_pass(&graph)
            .expect_err("multiple passes must fail until graph executor exists");

        assert!(err.contains("got 2"));
    }

    #[test]
    fn runtime_pass_rejects_non_swapchain_target_without_target_executor() {
        let graph = SceneGraph {
            passes: vec![mesh_pass(
                "scene-offscreen",
                SceneGraphTarget::ImageLocalMain(0),
            )],
        };

        let err = native_vulkan_scene_mesh_runtime_pass(&graph)
            .expect_err("offscreen target must fail until target executor exists");

        assert!(err.contains("one swapchain pass"));
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
                writes_depth: false,
                tests_depth: false,
            },
            geometry: Some(SceneGeometryId(object.0)),
            puppet: None,
            resources: Vec::new(),
            index_count: 6,
        }
    }
}

//! Graph-node recording for WE alpha-mask flattexture copy-back draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use super::copy_back_command::{
    NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan,
    native_vulkan_record_scene_layer_alpha_mask_copy_back_command,
};
use super::copy_back_pipeline::NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan;
use super::copy_back_runtime::render_state_copy_back_geometry_buffers;
use super::copy_back_target_graph::{
    NativeVulkanSceneLayerAlphaMaskCopyBackTargetGraphPlan,
    native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph,
};
use super::resource_binds::NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan;
use crate::renderer::native_vulkan::scene_backend::frame_resources::NativeVulkanSceneFrameResources;
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo;
use crate::renderer::native_vulkan::scene_backend::render_target::{
    NativeVulkanSceneOffscreenRenderTarget, NativeVulkanSceneRenderTarget,
    NativeVulkanSceneRenderTargetScopePlan, native_vulkan_record_scene_render_target_begin,
    native_vulkan_record_scene_render_target_end,
};
use crate::renderer::native_vulkan::scene_backend::target_access::{
    NativeVulkanSceneTargetTransitionPlan, native_vulkan_record_scene_target_transition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackGraphRecordPlan
{
    pub command_count: usize,
    pub source_shader_read_transition_count: usize,
    pub target_color_write_transition_count: usize,
    pub target_scope_count: usize,
    pub target_graph: NativeVulkanSceneLayerAlphaMaskCopyBackTargetGraphPlan,
    pub source_transition: Option<NativeVulkanSceneTargetTransitionPlan>,
    pub target_transition: Option<NativeVulkanSceneTargetTransitionPlan>,
    pub target_scope: NativeVulkanSceneRenderTargetScopePlan,
    pub commands: Vec<NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan>,
    pub command_order: [&'static str; 7],
}

struct NativeVulkanSceneLayerAlphaMaskCopyBackResolvedCommand {
    pipeline: NativeVulkanSceneLayerAlphaMaskCopyBackPipelineKeyPlan,
    vk_pipeline: vk::Pipeline,
    bind_info: NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_layer_alpha_mask_copy_back_graph_node(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    device: &Device,
    command_buffer: vk::CommandBuffer,
    resource_binds: &NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
) -> Result<Option<NativeVulkanSceneLayerAlphaMaskCopyBackGraphRecordPlan>, String> {
    let pipelines = &resource_binds.copy_back_pipelines;
    if pipelines.keys.is_empty() {
        return Ok(None);
    }

    let geometry = render_state_copy_back_geometry_buffers(frame_resources)?;
    let mut resolved = Vec::with_capacity(pipelines.keys.len());
    for pipeline in &pipelines.keys {
        let cache_key = pipeline.cache_key();
        let vk_pipeline = frame_resources
            .cached_mesh_pipeline(&cache_key)
            .map_err(|err| {
                format!(
                    "{err}; scene layer alpha-mask copy-back command {} requires warmed util/minimalalpha pipeline before graph recording",
                    pipeline.command_index
                )
            })?
            .pipeline;
        let bind_info =
            frame_resources.layer_alpha_mask_resource_heap_bind_info(pipeline.heap_bind_index)?;
        resolved.push(NativeVulkanSceneLayerAlphaMaskCopyBackResolvedCommand {
            pipeline: pipeline.clone(),
            vk_pipeline,
            bind_info,
        });
    }

    let target_graph =
        native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph(frame_resources)?;
    let source_transition = native_vulkan_record_scene_target_transition(
        frame_resources,
        device,
        command_buffer,
        target_graph.source,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        "alpha-mask-copy-back-source-sampled-read",
    )?;
    let target_transition = native_vulkan_record_scene_target_transition(
        frame_resources,
        device,
        command_buffer,
        target_graph.target,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        "alpha-mask-copy-back-target-color-write",
    )?;
    let target_binding = frame_resources.offscreen_target_binding(target_graph.target)?;
    let render_target =
        NativeVulkanSceneRenderTarget::Offscreen(NativeVulkanSceneOffscreenRenderTarget {
            target: target_binding.target,
            image: target_binding.image,
            image_view: target_binding.view,
            extent: vk::Extent2D {
                width: target_binding.width,
                height: target_binding.height,
            },
            initial_layout: target_binding.current_layout,
            final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        });
    let target_scope = native_vulkan_record_scene_render_target_begin(
        device,
        command_buffer,
        render_target,
        None,
    )?;
    frame_resources.mark_offscreen_target_layout(
        target_graph.target,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    )?;

    let mut commands = Vec::with_capacity(resolved.len());
    for command in resolved {
        commands.push(
            native_vulkan_record_scene_layer_alpha_mask_copy_back_command(
                device,
                command_buffer,
                &command.pipeline,
                command.vk_pipeline,
                &command.bind_info,
                geometry,
            )?,
        );
    }

    native_vulkan_record_scene_render_target_end(device, command_buffer, render_target, None)?;
    frame_resources.mark_offscreen_target_layout(
        target_graph.target,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    )?;

    Ok(Some(
        NativeVulkanSceneLayerAlphaMaskCopyBackGraphRecordPlan::from_recorded_parts(
            target_graph,
            source_transition,
            target_transition,
            target_scope,
            commands,
        ),
    ))
}

impl NativeVulkanSceneLayerAlphaMaskCopyBackGraphRecordPlan {
    fn from_recorded_parts(
        target_graph: NativeVulkanSceneLayerAlphaMaskCopyBackTargetGraphPlan,
        source_transition: Option<NativeVulkanSceneTargetTransitionPlan>,
        target_transition: Option<NativeVulkanSceneTargetTransitionPlan>,
        target_scope: NativeVulkanSceneRenderTargetScopePlan,
        commands: Vec<NativeVulkanSceneLayerAlphaMaskCopyBackCommandPlan>,
    ) -> Self {
        Self {
            command_count: commands.len(),
            source_shader_read_transition_count: source_transition.iter().count(),
            target_color_write_transition_count: target_transition.iter().count(),
            target_scope_count: 1,
            target_graph,
            source_transition,
            target_transition,
            target_scope,
            commands,
            command_order: [
                "resolve_copy_back_pipeline_heap_and_geometry",
                "require_intermediate_mask_producer_completed",
                "transition_intermediate_mask_to_shader_read",
                "transition_full_mask_to_color_attachment",
                "cmd_begin_full_alpha_mask_load_scope",
                "record_util_minimalalpha_copy_back_draws",
                "cmd_end_full_alpha_mask_load_scope",
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::copy_back_target_graph::native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph_from_bindings;
    use super::*;
    use crate::engine::scene_engine::SceneGraphTarget;
    use crate::renderer::native_vulkan::scene_backend::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
    use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;
    use vulkanalia::vk::Handle;

    #[test]
    fn copy_back_graph_record_plan_counts_access_scope_and_draws() {
        let target_graph =
            native_vulkan_plan_scene_layer_alpha_mask_copy_back_target_graph_from_bindings(
                binding(
                    SceneGraphTarget::FullAlphaMaskIntermediate,
                    1,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
                binding(
                    SceneGraphTarget::FullAlphaMask,
                    11,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ),
            )
            .expect("copy-back target graph");
        let target_scope = NativeVulkanSceneRenderTargetScopePlan {
            width: 1920,
            height: 1080,
            load_op: NativeVulkanSceneRenderTargetLoadOp::Load,
            begin_command_order: ["retain_color_attachment_layout", "cmd_begin_rendering"],
            end_command_order: ["cmd_end_rendering", "retain_color_attachment_layout"],
        };
        let plan = NativeVulkanSceneLayerAlphaMaskCopyBackGraphRecordPlan::from_recorded_parts(
            target_graph,
            Some(transition(
                SceneGraphTarget::FullAlphaMaskIntermediate,
                "color-attachment-optimal",
                "shader-read-only-optimal",
            )),
            Some(transition(
                SceneGraphTarget::FullAlphaMask,
                "shader-read-only-optimal",
                "color-attachment-optimal",
            )),
            target_scope,
            Vec::new(),
        );

        assert_eq!(plan.command_count, 0);
        assert_eq!(plan.source_shader_read_transition_count, 1);
        assert_eq!(plan.target_color_write_transition_count, 1);
        assert_eq!(plan.target_scope_count, 1);
        assert_eq!(
            plan.target_scope.load_op,
            NativeVulkanSceneRenderTargetLoadOp::Load
        );
        assert_eq!(
            plan.command_order,
            [
                "resolve_copy_back_pipeline_heap_and_geometry",
                "require_intermediate_mask_producer_completed",
                "transition_intermediate_mask_to_shader_read",
                "transition_full_mask_to_color_attachment",
                "cmd_begin_full_alpha_mask_load_scope",
                "record_util_minimalalpha_copy_back_draws",
                "cmd_end_full_alpha_mask_load_scope"
            ]
        );
    }

    fn binding(
        target: SceneGraphTarget,
        raw: u64,
        current_layout: vk::ImageLayout,
    ) -> NativeVulkanSceneOffscreenTargetBinding {
        NativeVulkanSceneOffscreenTargetBinding {
            target,
            image: vk::Image::from_raw(raw),
            view: vk::ImageView::from_raw(raw + 1),
            sampler: vk::Sampler::from_raw(raw + 2),
            format: vk::Format::R8_UNORM,
            width: 1920,
            height: 1080,
            current_layout,
        }
    }

    fn transition(
        target: SceneGraphTarget,
        old_layout: &'static str,
        new_layout: &'static str,
    ) -> NativeVulkanSceneTargetTransitionPlan {
        NativeVulkanSceneTargetTransitionPlan {
            target,
            old_layout,
            new_layout,
            src_stage: "fragment-shader",
            dst_stage: "color-attachment-output",
            src_access: "shader-sampled-read",
            dst_access: "color-attachment-write",
            reason: "test",
            command_order: [
                "map_scene_target_layout_to_vk_sync2",
                "cmd_pipeline_barrier2_scene_target",
            ],
        }
    }
}

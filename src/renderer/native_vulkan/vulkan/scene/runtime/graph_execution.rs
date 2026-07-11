//! Interleaved effect-target and scene-color graph execution.
//!
//! Each object graph composites to scene color before its aliased image-local
//! targets are reused by the next graph.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::engine::scene::SceneRenderingDeviceGraphPlan;

use super::draw_recording::{
    SceneGpuDrawRange, record_scene_draw_extent, record_scene_mesh_draw_ranges,
};
use super::{
    NativeVulkanClearColor, SceneGpuResources, color_subresource_range, effect_target,
};

pub(super) fn scene_graph_execution_order(
    graph: &SceneRenderingDeviceGraphPlan,
    capture_scene_graph: Option<u32>,
) -> Result<Vec<u32>, String> {
    let mut order = Vec::new();
    for pass in &graph.pass_nodes {
        if capture_scene_graph.is_some_and(|selected| selected != pass.graph_index) {
            continue;
        }
        if order.last().copied() != Some(pass.graph_index) {
            order.push(pass.graph_index);
        }
    }
    if let Some(selected) = capture_scene_graph
        && order.is_empty()
    {
        return Err(format!(
            "captured scene graph {selected} does not exist or has no render passes"
        ));
    }
    Ok(order)
}

pub(super) fn record_scene_graphs_to_swapchain(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    swapchain_view: vk::ImageView,
    old_layout: vk::ImageLayout,
    extent: vk::Extent2D,
    clear_color: NativeVulkanClearColor,
    scene: &SceneGpuResources,
    reference_phase: usize,
) -> Result<(), String> {
    transition_swapchain_to_attachment(device, command_buffer, swapchain_image, old_layout);
    let reference_slots = &scene
        .sampled_binding_cycle
        .get(reference_phase)
        .ok_or_else(|| format!("scene sampled binding phase {reference_phase} is missing"))?
        .initial_reference_physical_slots;
    let mut scene_color_initialized = false;

    for graph_index in &scene.graph_execution_order {
        if !scene_color_initialized
            && effect_target::graph_copies_scene_color(
                &scene.effect_target_commands,
                *graph_index,
            )
        {
            begin_scene_color_rendering(
                device,
                command_buffer,
                swapchain_view,
                extent,
                clear_color,
                false,
            );
            unsafe {
                device.cmd_end_rendering(command_buffer);
            }
            scene_color_initialized = true;
        }
        let mut record_effect_draws = |draw_start, draw_count| {
            record_scene_mesh_draw_ranges(
                device,
                command_buffer,
                scene,
                &[SceneGpuDrawRange {
                    start: draw_start,
                    count: draw_count,
                }],
                extent,
            )
        };
        effect_target::record_scene_effect_target_graph_passes(
            device,
            command_buffer,
            swapchain_image,
            extent,
            *graph_index,
            &scene.effect_target_commands,
            &scene.effect_target_allocations,
            reference_slots,
            &scene.effect_targets,
            &mut record_effect_draws,
        )?;

        let graph_ranges = scene
            .scene_color_draw_ranges
            .iter()
            .filter(|range| range.graph_index == *graph_index)
            .collect::<Vec<_>>();
        if graph_ranges.is_empty() {
            continue;
        }
        begin_scene_color_rendering(
            device,
            command_buffer,
            swapchain_view,
            extent,
            clear_color,
            scene_color_initialized,
        );
        record_scene_draw_extent(device, command_buffer, extent);
        for graph_range in graph_ranges {
            record_scene_mesh_draw_ranges(
                device,
                command_buffer,
                scene,
                &[graph_range.range],
                extent,
            )?;
        }
        unsafe {
            device.cmd_end_rendering(command_buffer);
        }
        scene_color_initialized = true;
    }

    if !scene_color_initialized {
        begin_scene_color_rendering(
            device,
            command_buffer,
            swapchain_view,
            extent,
            clear_color,
            false,
        );
        unsafe {
            device.cmd_end_rendering(command_buffer);
        }
    }
    Ok(())
}

fn begin_scene_color_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_view: vk::ImageView,
    extent: vk::Extent2D,
    clear_color: NativeVulkanClearColor,
    initialized: bool,
) {
    let clear_value = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [clear_color.r, clear_color.g, clear_color.b, clear_color.a],
        },
    };
    let color_attachment = vk::RenderingAttachmentInfo::builder()
        .image_view(swapchain_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(if initialized {
            vk::AttachmentLoadOp::LOAD
        } else {
            vk::AttachmentLoadOp::CLEAR
        })
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear_value)
        .build();
    let color_attachments = [color_attachment];
    let rendering_info = vk::RenderingInfo::builder()
        .render_area(
            vk::Rect2D::builder()
                .offset(vk::Offset2D { x: 0, y: 0 })
                .extent(extent)
                .build(),
        )
        .layer_count(1)
        .color_attachments(&color_attachments)
        .build();
    unsafe {
        device.cmd_begin_rendering(command_buffer, &rendering_info);
    }
}

fn transition_swapchain_to_attachment(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
) {
    let barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(match old_layout {
            vk::ImageLayout::UNDEFINED => vk::PipelineStageFlags2::TOP_OF_PIPE,
            _ => vk::PipelineStageFlags2::ALL_COMMANDS,
        })
        .src_access_mask(vk::AccessFlags2::empty())
        .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .old_layout(old_layout)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(color_subresource_range())
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(
            command_buffer,
            &vk::DependencyInfo::builder()
                .image_memory_barriers(&[barrier])
                .build(),
        );
    }
}

pub(super) fn transition_swapchain_to_present(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
) {
    let barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
        .dst_access_mask(vk::AccessFlags2::empty())
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(color_subresource_range())
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(
            command_buffer,
            &vk::DependencyInfo::builder()
                .image_memory_barriers(&[barrier])
                .build(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{SceneRenderingDeviceGraphPlan, SceneRenderingDevicePassNode};

    #[test]
    fn execution_order_keeps_graphs_contiguous() {
        let graph = SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![pass(0), pass(0), pass(2)],
            ..empty_graph()
        };
        assert_eq!(
            scene_graph_execution_order(&graph, None).expect("execution order"),
            vec![0, 2]
        );
        assert_eq!(
            scene_graph_execution_order(&graph, Some(2)).expect("isolated graph"),
            vec![2]
        );
        assert!(scene_graph_execution_order(&graph, Some(1)).is_err());
    }

    fn pass(graph_index: u32) -> SceneRenderingDevicePassNode {
        SceneRenderingDevicePassNode {
            graph_index,
            pass_record_index: 0,
            pass_id: 0,
            role: crate::engine::scene::SceneRenderPassKind::BaseMaterial,
            target: crate::engine::scene::SceneRenderTargetKind::SceneColor,
            target_name: crate::engine::scene::SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            mesh_draw_start: 0,
            mesh_draw_count: 0,
        }
    }

    fn empty_graph() -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes: Vec::new(),
            target_allocations: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            mesh_draws: Vec::new(),
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: Vec::new(),
            resolved_object_count: 0,
            resolved_visible_object_count: 0,
            resolved_attachment_link_count: 0,
            resolved_visible_effect_instance_count: 0,
            resolved_visible_effect_pass_count: 0,
            resolved_visible_effect_fbo_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 0,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 0,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        }
    }
}

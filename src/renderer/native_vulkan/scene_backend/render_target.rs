//! Scene render target scope recording for Vulkan dynamic rendering.
//!
//! References:
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::renderer::native_vulkan::NativeVulkanClearColor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NativeVulkanSceneRenderTargetLoadOp {
    Clear,
    Load,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneRenderTargetScopePlan {
    pub width: u32,
    pub height: u32,
    pub load_op: NativeVulkanSceneRenderTargetLoadOp,
    pub begin_command_order: [&'static str; 2],
    pub end_command_order: [&'static str; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanSceneSwapchainRenderTarget {
    pub image: vk::Image,
    pub image_view: vk::ImageView,
    pub extent: vk::Extent2D,
    pub initial_layout: vk::ImageLayout,
    pub final_layout: vk::ImageLayout,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_swapchain_render_target_scope_plan(
    target: NativeVulkanSceneSwapchainRenderTarget,
    clear_color: Option<NativeVulkanClearColor>,
) -> Result<NativeVulkanSceneRenderTargetScopePlan, String> {
    validate_scene_swapchain_render_target(target, clear_color)?;
    Ok(NativeVulkanSceneRenderTargetScopePlan {
        width: target.extent.width,
        height: target.extent.height,
        load_op: match clear_color {
            Some(_) => NativeVulkanSceneRenderTargetLoadOp::Clear,
            None => NativeVulkanSceneRenderTargetLoadOp::Load,
        },
        begin_command_order: [
            "cmd_pipeline_barrier2_color_attachment",
            "cmd_begin_rendering",
        ],
        end_command_order: ["cmd_end_rendering", "cmd_pipeline_barrier2_present"],
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_swapchain_render_target_begin(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    target: NativeVulkanSceneSwapchainRenderTarget,
    clear_color: Option<NativeVulkanClearColor>,
) -> Result<NativeVulkanSceneRenderTargetScopePlan, String> {
    let plan = native_vulkan_scene_swapchain_render_target_scope_plan(target, clear_color)?;
    unsafe {
        let target_to_color = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(target.initial_layout)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(target.image)
            .subresource_range(scene_color_subresource_range())
            .build();
        let image_barriers = [target_to_color];
        let dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&image_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &dependency);

        let clear_value = clear_color.map(|color| vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [color.r, color.g, color.b, color.a],
            },
        });
        let color_attachment = scene_color_attachment(target.image_view, clear_value);
        let color_attachments = [color_attachment];
        let render_area = vk::Rect2D::builder()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(target.extent)
            .build();
        let rendering_info = vk::RenderingInfo::builder()
            .render_area(render_area)
            .layer_count(1)
            .color_attachments(&color_attachments)
            .build();
        device.cmd_begin_rendering(command_buffer, &rendering_info);
    }
    Ok(plan)
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_record_scene_swapchain_render_target_end(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    target: NativeVulkanSceneSwapchainRenderTarget,
    clear_color: Option<NativeVulkanClearColor>,
) -> Result<NativeVulkanSceneRenderTargetScopePlan, String> {
    let plan = native_vulkan_scene_swapchain_render_target_scope_plan(target, clear_color)?;
    unsafe {
        device.cmd_end_rendering(command_buffer);

        let color_to_final = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags2::empty())
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(target.final_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(target.image)
            .subresource_range(scene_color_subresource_range())
            .build();
        let image_barriers = [color_to_final];
        let dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&image_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }
    Ok(plan)
}

fn validate_scene_swapchain_render_target(
    target: NativeVulkanSceneSwapchainRenderTarget,
    clear_color: Option<NativeVulkanClearColor>,
) -> Result<(), String> {
    if target.image == vk::Image::null() {
        return Err("scene swapchain render target requires a valid vk::Image".to_owned());
    }
    if target.image_view == vk::ImageView::null() {
        return Err("scene swapchain render target requires a valid vk::ImageView".to_owned());
    }
    if target.extent.width == 0 || target.extent.height == 0 {
        return Err("scene swapchain render target requires non-zero extent".to_owned());
    }
    if clear_color.is_none() && target.initial_layout == vk::ImageLayout::UNDEFINED {
        return Err(
            "scene swapchain render target cannot load from VK_IMAGE_LAYOUT_UNDEFINED".to_owned(),
        );
    }
    if target.final_layout == vk::ImageLayout::UNDEFINED {
        return Err("scene swapchain render target final layout cannot be UNDEFINED".to_owned());
    }
    Ok(())
}

fn scene_color_attachment(
    image_view: vk::ImageView,
    clear_value: Option<vk::ClearValue>,
) -> vk::RenderingAttachmentInfo {
    let mut attachment = vk::RenderingAttachmentInfo::builder()
        .image_view(image_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(if clear_value.is_some() {
            vk::AttachmentLoadOp::CLEAR
        } else {
            vk::AttachmentLoadOp::LOAD
        })
        .store_op(vk::AttachmentStoreOp::STORE);
    if let Some(clear_value) = clear_value {
        attachment = attachment.clear_value(clear_value);
    }
    attachment.build()
}

fn scene_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_scope_plan_clears_undefined_swapchain_image() {
        let plan = native_vulkan_scene_swapchain_render_target_scope_plan(
            target(vk::ImageLayout::UNDEFINED, vk::ImageLayout::PRESENT_SRC_KHR),
            Some(NativeVulkanClearColor::default()),
        )
        .expect("clear target scope plan");

        assert_eq!(plan.width, 3840);
        assert_eq!(plan.height, 2160);
        assert_eq!(plan.load_op, NativeVulkanSceneRenderTargetLoadOp::Clear);
        assert_eq!(
            plan.begin_command_order,
            [
                "cmd_pipeline_barrier2_color_attachment",
                "cmd_begin_rendering"
            ]
        );
        assert_eq!(
            plan.end_command_order,
            ["cmd_end_rendering", "cmd_pipeline_barrier2_present"]
        );
    }

    #[test]
    fn target_scope_plan_rejects_load_from_undefined_layout() {
        let err = native_vulkan_scene_swapchain_render_target_scope_plan(
            target(vk::ImageLayout::UNDEFINED, vk::ImageLayout::PRESENT_SRC_KHR),
            None,
        )
        .expect_err("load from undefined must fail");

        assert!(err.contains("cannot load from VK_IMAGE_LAYOUT_UNDEFINED"));
    }

    #[test]
    fn target_scope_plan_rejects_zero_extent() {
        let err = native_vulkan_scene_swapchain_render_target_scope_plan(
            NativeVulkanSceneSwapchainRenderTarget {
                extent: vk::Extent2D {
                    width: 0,
                    height: 2160,
                },
                ..target(
                    vk::ImageLayout::PRESENT_SRC_KHR,
                    vk::ImageLayout::PRESENT_SRC_KHR,
                )
            },
            Some(NativeVulkanClearColor::default()),
        )
        .expect_err("zero extent must fail");

        assert!(err.contains("non-zero extent"));
    }

    #[test]
    fn target_scope_plan_rejects_null_handles() {
        let err = native_vulkan_scene_swapchain_render_target_scope_plan(
            NativeVulkanSceneSwapchainRenderTarget {
                image: vk::Image::null(),
                ..target(
                    vk::ImageLayout::PRESENT_SRC_KHR,
                    vk::ImageLayout::PRESENT_SRC_KHR,
                )
            },
            Some(NativeVulkanClearColor::default()),
        )
        .expect_err("null image must fail");

        assert!(err.contains("valid vk::Image"));
    }

    fn target(
        initial_layout: vk::ImageLayout,
        final_layout: vk::ImageLayout,
    ) -> NativeVulkanSceneSwapchainRenderTarget {
        NativeVulkanSceneSwapchainRenderTarget {
            image: vk::Image::from_raw(1),
            image_view: vk::ImageView::from_raw(2),
            extent: vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            initial_layout,
            final_layout,
        }
    }
}

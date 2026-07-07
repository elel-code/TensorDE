//! Scene frame queue present boundary.
//!
//! References:
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, KhrSwapchainExtensionDeviceCommands};

use super::frame_completion::NativeVulkanSceneFrameSubmission;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFramePresentContext {
    pub frame_submission: NativeVulkanSceneFrameSubmission,
    pub swapchain: vk::SwapchainKHR,
    pub image_index: u32,
    pub render_finished: vk::Semaphore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFramePresentPlan {
    pub frame_submission: NativeVulkanSceneFrameSubmission,
    pub image_index: u32,
    pub present_mode_policy: &'static str,
    pub command_order: [&'static str; 1],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_frame_present_plan(
    context: NativeVulkanSceneFramePresentContext,
) -> Result<NativeVulkanSceneFramePresentPlan, String> {
    validate_scene_frame_present_context(context)?;
    Ok(NativeVulkanSceneFramePresentPlan {
        frame_submission: context.frame_submission,
        image_index: context.image_index,
        present_mode_policy: "fifo-latest-ready-required",
        command_order: ["queue_present_khr_scene_frame"],
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_present_scene_frame(
    device: &Device,
    queue: vk::Queue,
    context: NativeVulkanSceneFramePresentContext,
) -> Result<NativeVulkanSceneFramePresentPlan, String> {
    let plan = native_vulkan_scene_frame_present_plan(context)?;
    let swapchains = [context.swapchain];
    let image_indices = [context.image_index];
    let wait_semaphores = [context.render_finished];
    let present_info = vk::PresentInfoKHR::builder()
        .wait_semaphores(&wait_semaphores)
        .swapchains(&swapchains)
        .image_indices(&image_indices);
    unsafe {
        device
            .queue_present_khr(queue, &present_info)
            .map_err(|err| format!("vkQueuePresentKHR(scene frame): {err:?}"))?;
    }
    Ok(plan)
}

fn validate_scene_frame_present_context(
    context: NativeVulkanSceneFramePresentContext,
) -> Result<(), String> {
    if context.frame_submission.submission_index == 0 {
        return Err("scene frame present requires a non-zero submission index".to_owned());
    }
    if context.swapchain == vk::SwapchainKHR::null() {
        return Err("scene frame present requires a valid swapchain".to_owned());
    }
    if context.render_finished == vk::Semaphore::null() {
        return Err("scene frame present requires a valid render-finished semaphore".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulkanalia::vk::Handle;

    #[test]
    fn scene_frame_present_plan_requires_fifo_latest_ready_policy() {
        let plan = native_vulkan_scene_frame_present_plan(present_context()).expect("present plan");

        assert_eq!(
            plan.frame_submission,
            NativeVulkanSceneFrameSubmission::new(1, 4)
        );
        assert_eq!(plan.image_index, 3);
        assert_eq!(plan.present_mode_policy, "fifo-latest-ready-required");
        assert_eq!(plan.command_order, ["queue_present_khr_scene_frame"]);
    }

    #[test]
    fn scene_frame_present_plan_rejects_null_vulkan_handles() {
        let mut context = present_context();
        context.swapchain = vk::SwapchainKHR::null();
        assert!(
            native_vulkan_scene_frame_present_plan(context)
                .expect_err("null swapchain")
                .contains("valid swapchain")
        );

        let mut context = present_context();
        context.render_finished = vk::Semaphore::null();
        assert!(
            native_vulkan_scene_frame_present_plan(context)
                .expect_err("null render semaphore")
                .contains("render-finished semaphore")
        );
    }

    #[test]
    fn scene_frame_present_plan_rejects_zero_submission_index() {
        let mut context = present_context();
        context.frame_submission = NativeVulkanSceneFrameSubmission::new(1, 0);

        assert!(
            native_vulkan_scene_frame_present_plan(context)
                .expect_err("zero submission")
                .contains("non-zero submission index")
        );
    }

    fn present_context() -> NativeVulkanSceneFramePresentContext {
        NativeVulkanSceneFramePresentContext {
            frame_submission: NativeVulkanSceneFrameSubmission::new(1, 4),
            swapchain: vk::SwapchainKHR::from_raw(11),
            image_index: 3,
            render_finished: vk::Semaphore::from_raw(12),
        }
    }
}

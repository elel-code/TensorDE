//! Scene frame QueueSubmit2 boundary for swapchain presentation.
//!
//! References:
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use super::frame_completion::NativeVulkanSceneFrameSubmission;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameSubmitContext {
    pub frame_submission: NativeVulkanSceneFrameSubmission,
    pub command_buffer: vk::CommandBuffer,
    pub image_available: vk::Semaphore,
    pub render_finished: vk::Semaphore,
    pub in_flight_fence: vk::Fence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanScenePrepareSubmitContext {
    pub frame_submission: NativeVulkanSceneFrameSubmission,
    pub command_buffer: vk::CommandBuffer,
    pub in_flight_fence: vk::Fence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameSubmitPlan {
    pub frame_submission: NativeVulkanSceneFrameSubmission,
    pub wait_stage: &'static str,
    pub signal_stage: &'static str,
    pub command_order: [&'static str; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanScenePrepareSubmitPlan {
    pub frame_submission: NativeVulkanSceneFrameSubmission,
    pub wait_stage: &'static str,
    pub signal_stage: &'static str,
    pub command_order: [&'static str; 2],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_frame_submit_plan(
    context: NativeVulkanSceneFrameSubmitContext,
) -> Result<NativeVulkanSceneFrameSubmitPlan, String> {
    validate_scene_frame_submit_context(context)?;
    Ok(NativeVulkanSceneFrameSubmitPlan {
        frame_submission: context.frame_submission,
        wait_stage: "color_attachment_output",
        signal_stage: "all_commands",
        command_order: ["reset_scene_frame_fence", "queue_submit2_scene_frame"],
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_prepare_submit_plan(
    context: NativeVulkanScenePrepareSubmitContext,
) -> Result<NativeVulkanScenePrepareSubmitPlan, String> {
    validate_scene_prepare_submit_context(context)?;
    Ok(NativeVulkanScenePrepareSubmitPlan {
        frame_submission: context.frame_submission,
        wait_stage: "none",
        signal_stage: "fence_only",
        command_order: ["reset_scene_prepare_fence", "queue_submit2_scene_prepare"],
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_submit_scene_frame_commands2(
    device: &Device,
    queue: vk::Queue,
    context: NativeVulkanSceneFrameSubmitContext,
) -> Result<NativeVulkanSceneFrameSubmitPlan, String> {
    let plan = native_vulkan_scene_frame_submit_plan(context)?;
    let wait = vk::SemaphoreSubmitInfo::builder()
        .semaphore(context.image_available)
        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .build();
    let waits = [wait];
    let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
        .command_buffer(context.command_buffer)
        .build();
    let command_buffer_infos = [command_buffer_info];
    let signal = vk::SemaphoreSubmitInfo::builder()
        .semaphore(context.render_finished)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .build();
    let signals = [signal];
    let submit_info = vk::SubmitInfo2::builder()
        .wait_semaphore_infos(&waits)
        .command_buffer_infos(&command_buffer_infos)
        .signal_semaphore_infos(&signals)
        .build();

    unsafe {
        device
            .reset_fences(&[context.in_flight_fence])
            .map_err(|err| format!("vkResetFences(scene frame submit): {err:?}"))?;
        device
            .queue_submit2(queue, &[submit_info], context.in_flight_fence)
            .map_err(|err| format!("vkQueueSubmit2(scene frame): {err:?}"))?;
    }

    Ok(plan)
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_submit_scene_prepare_commands2(
    device: &Device,
    queue: vk::Queue,
    context: NativeVulkanScenePrepareSubmitContext,
) -> Result<NativeVulkanScenePrepareSubmitPlan, String> {
    let plan = native_vulkan_scene_prepare_submit_plan(context)?;
    let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
        .command_buffer(context.command_buffer)
        .build();
    let command_buffer_infos = [command_buffer_info];
    let submit_info = vk::SubmitInfo2::builder()
        .command_buffer_infos(&command_buffer_infos)
        .build();

    unsafe {
        device
            .reset_fences(&[context.in_flight_fence])
            .map_err(|err| format!("vkResetFences(scene prepare submit): {err:?}"))?;
        device
            .queue_submit2(queue, &[submit_info], context.in_flight_fence)
            .map_err(|err| format!("vkQueueSubmit2(scene prepare): {err:?}"))?;
    }

    Ok(plan)
}

fn validate_scene_frame_submit_context(
    context: NativeVulkanSceneFrameSubmitContext,
) -> Result<(), String> {
    if context.frame_submission.submission_index == 0 {
        return Err("scene frame submit requires a non-zero submission index".to_owned());
    }
    if context.command_buffer == vk::CommandBuffer::null() {
        return Err("scene frame submit requires a valid command buffer".to_owned());
    }
    if context.image_available == vk::Semaphore::null() {
        return Err("scene frame submit requires a valid image-available semaphore".to_owned());
    }
    if context.render_finished == vk::Semaphore::null() {
        return Err("scene frame submit requires a valid render-finished semaphore".to_owned());
    }
    if context.in_flight_fence == vk::Fence::null() {
        return Err("scene frame submit requires a valid in-flight fence".to_owned());
    }
    Ok(())
}

fn validate_scene_prepare_submit_context(
    context: NativeVulkanScenePrepareSubmitContext,
) -> Result<(), String> {
    if context.frame_submission.submission_index == 0 {
        return Err("scene prepare submit requires a non-zero submission index".to_owned());
    }
    if context.command_buffer == vk::CommandBuffer::null() {
        return Err("scene prepare submit requires a valid command buffer".to_owned());
    }
    if context.in_flight_fence == vk::Fence::null() {
        return Err("scene prepare submit requires a valid in-flight fence".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulkanalia::vk::Handle;

    #[test]
    fn scene_frame_submit_plan_uses_submit2_with_swapchain_semaphores() {
        let plan = native_vulkan_scene_frame_submit_plan(submit_context()).expect("submit plan");

        assert_eq!(
            plan.frame_submission,
            NativeVulkanSceneFrameSubmission::new(2, 9)
        );
        assert_eq!(plan.wait_stage, "color_attachment_output");
        assert_eq!(plan.signal_stage, "all_commands");
        assert_eq!(
            plan.command_order,
            ["reset_scene_frame_fence", "queue_submit2_scene_frame"]
        );
    }

    #[test]
    fn scene_frame_submit_plan_rejects_null_vulkan_handles() {
        let mut context = submit_context();
        context.command_buffer = vk::CommandBuffer::null();
        assert!(
            native_vulkan_scene_frame_submit_plan(context)
                .expect_err("null command buffer")
                .contains("command buffer")
        );

        let mut context = submit_context();
        context.image_available = vk::Semaphore::null();
        assert!(
            native_vulkan_scene_frame_submit_plan(context)
                .expect_err("null image semaphore")
                .contains("image-available semaphore")
        );

        let mut context = submit_context();
        context.render_finished = vk::Semaphore::null();
        assert!(
            native_vulkan_scene_frame_submit_plan(context)
                .expect_err("null render semaphore")
                .contains("render-finished semaphore")
        );

        let mut context = submit_context();
        context.in_flight_fence = vk::Fence::null();
        assert!(
            native_vulkan_scene_frame_submit_plan(context)
                .expect_err("null fence")
                .contains("in-flight fence")
        );
    }

    #[test]
    fn scene_frame_submit_plan_rejects_zero_submission_index() {
        let mut context = submit_context();
        context.frame_submission = NativeVulkanSceneFrameSubmission::new(0, 0);

        assert!(
            native_vulkan_scene_frame_submit_plan(context)
                .expect_err("zero submission")
                .contains("non-zero submission index")
        );
    }

    #[test]
    fn scene_prepare_submit_plan_has_no_swapchain_semaphore_waits() {
        let plan = native_vulkan_scene_prepare_submit_plan(prepare_context())
            .expect("prepare submit plan");

        assert_eq!(
            plan.frame_submission,
            NativeVulkanSceneFrameSubmission::new(0, 7)
        );
        assert_eq!(plan.wait_stage, "none");
        assert_eq!(plan.signal_stage, "fence_only");
        assert_eq!(
            plan.command_order,
            ["reset_scene_prepare_fence", "queue_submit2_scene_prepare"]
        );
    }

    #[test]
    fn scene_prepare_submit_plan_rejects_null_vulkan_handles() {
        let mut context = prepare_context();
        context.command_buffer = vk::CommandBuffer::null();
        assert!(
            native_vulkan_scene_prepare_submit_plan(context)
                .expect_err("null command buffer")
                .contains("command buffer")
        );

        let mut context = prepare_context();
        context.in_flight_fence = vk::Fence::null();
        assert!(
            native_vulkan_scene_prepare_submit_plan(context)
                .expect_err("null fence")
                .contains("in-flight fence")
        );
    }

    fn submit_context() -> NativeVulkanSceneFrameSubmitContext {
        NativeVulkanSceneFrameSubmitContext {
            frame_submission: NativeVulkanSceneFrameSubmission::new(2, 9),
            command_buffer: vk::CommandBuffer::from_raw(11),
            image_available: vk::Semaphore::from_raw(12),
            render_finished: vk::Semaphore::from_raw(13),
            in_flight_fence: vk::Fence::from_raw(14),
        }
    }

    fn prepare_context() -> NativeVulkanScenePrepareSubmitContext {
        NativeVulkanScenePrepareSubmitContext {
            frame_submission: NativeVulkanSceneFrameSubmission::new(0, 7),
            command_buffer: vk::CommandBuffer::from_raw(21),
            in_flight_fence: vk::Fence::from_raw(22),
        }
    }
}

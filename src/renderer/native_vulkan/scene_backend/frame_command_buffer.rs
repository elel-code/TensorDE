//! Scene frame command-buffer lifecycle.
//!
//! References:
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameCommandBufferBeginPlan {
    pub usage: &'static str,
    pub command_order: [&'static str; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameCommandBufferEndPlan {
    pub command_order: [&'static str; 1],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_frame_command_buffer_begin_plan(
    command_buffer: vk::CommandBuffer,
) -> Result<NativeVulkanSceneFrameCommandBufferBeginPlan, String> {
    validate_scene_frame_command_buffer(command_buffer)?;
    Ok(NativeVulkanSceneFrameCommandBufferBeginPlan {
        usage: "one_time_submit",
        command_order: [
            "reset_command_buffer_scene_frame",
            "begin_command_buffer_scene_frame",
        ],
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_begin_scene_frame_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
) -> Result<NativeVulkanSceneFrameCommandBufferBeginPlan, String> {
    let plan = native_vulkan_scene_frame_command_buffer_begin_plan(command_buffer)?;
    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| format!("vkResetCommandBuffer(scene frame): {err:?}"))?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| format!("vkBeginCommandBuffer(scene frame): {err:?}"))?;
    }
    Ok(plan)
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_frame_command_buffer_end_plan(
    command_buffer: vk::CommandBuffer,
) -> Result<NativeVulkanSceneFrameCommandBufferEndPlan, String> {
    validate_scene_frame_command_buffer(command_buffer)?;
    Ok(NativeVulkanSceneFrameCommandBufferEndPlan {
        command_order: ["end_command_buffer_scene_frame"],
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_end_scene_frame_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
) -> Result<NativeVulkanSceneFrameCommandBufferEndPlan, String> {
    let plan = native_vulkan_scene_frame_command_buffer_end_plan(command_buffer)?;
    unsafe {
        device
            .end_command_buffer(command_buffer)
            .map_err(|err| format!("vkEndCommandBuffer(scene frame): {err:?}"))?;
    }
    Ok(plan)
}

fn validate_scene_frame_command_buffer(command_buffer: vk::CommandBuffer) -> Result<(), String> {
    if command_buffer == vk::CommandBuffer::null() {
        return Err(
            "scene frame command-buffer lifecycle requires a valid command buffer".to_owned(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulkanalia::vk::Handle;

    #[test]
    fn scene_frame_command_buffer_begin_plan_resets_and_begins_once() {
        let plan = native_vulkan_scene_frame_command_buffer_begin_plan(command_buffer())
            .expect("begin plan");

        assert_eq!(plan.usage, "one_time_submit");
        assert_eq!(
            plan.command_order,
            [
                "reset_command_buffer_scene_frame",
                "begin_command_buffer_scene_frame"
            ]
        );
    }

    #[test]
    fn scene_frame_command_buffer_end_plan_ends_command_buffer() {
        let plan =
            native_vulkan_scene_frame_command_buffer_end_plan(command_buffer()).expect("end plan");

        assert_eq!(plan.command_order, ["end_command_buffer_scene_frame"]);
    }

    #[test]
    fn scene_frame_command_buffer_lifecycle_rejects_null_handle() {
        assert!(
            native_vulkan_scene_frame_command_buffer_begin_plan(vk::CommandBuffer::null())
                .expect_err("null begin")
                .contains("valid command buffer")
        );
        assert!(
            native_vulkan_scene_frame_command_buffer_end_plan(vk::CommandBuffer::null())
                .expect_err("null end")
                .contains("valid command buffer")
        );
    }

    fn command_buffer() -> vk::CommandBuffer {
        vk::CommandBuffer::from_raw(31)
    }
}

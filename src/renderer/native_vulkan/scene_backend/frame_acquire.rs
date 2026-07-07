//! Scene swapchain image acquire boundary.
//!
//! References:
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, KhrSwapchainExtensionDeviceCommands};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameAcquireContext {
    pub swapchain: vk::SwapchainKHR,
    pub image_available: vk::Semaphore,
    pub timeout_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameAcquirePlan {
    pub timeout_ns: u64,
    pub command_order: [&'static str; 1],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameAcquireResult {
    pub image_index: u32,
    pub plan: NativeVulkanSceneFrameAcquirePlan,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_frame_acquire_plan(
    context: NativeVulkanSceneFrameAcquireContext,
) -> Result<NativeVulkanSceneFrameAcquirePlan, String> {
    validate_scene_frame_acquire_context(context)?;
    Ok(NativeVulkanSceneFrameAcquirePlan {
        timeout_ns: context.timeout_ns,
        command_order: ["acquire_next_image_khr_scene_frame"],
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_acquire_scene_frame_image(
    device: &Device,
    context: NativeVulkanSceneFrameAcquireContext,
) -> Result<NativeVulkanSceneFrameAcquireResult, String> {
    let plan = native_vulkan_scene_frame_acquire_plan(context)?;
    let (image_index, _) = unsafe {
        device.acquire_next_image_khr(
            context.swapchain,
            context.timeout_ns,
            context.image_available,
            vk::Fence::null(),
        )
    }
    .map_err(|err| format!("vkAcquireNextImageKHR(scene frame): {err:?}"))?;

    Ok(NativeVulkanSceneFrameAcquireResult { image_index, plan })
}

fn validate_scene_frame_acquire_context(
    context: NativeVulkanSceneFrameAcquireContext,
) -> Result<(), String> {
    if context.swapchain == vk::SwapchainKHR::null() {
        return Err("scene frame acquire requires a valid swapchain".to_owned());
    }
    if context.image_available == vk::Semaphore::null() {
        return Err("scene frame acquire requires a valid image-available semaphore".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulkanalia::vk::Handle;

    #[test]
    fn scene_frame_acquire_plan_uses_swapchain_acquire() {
        let plan = native_vulkan_scene_frame_acquire_plan(acquire_context()).expect("acquire plan");

        assert_eq!(plan.timeout_ns, u64::MAX);
        assert_eq!(plan.command_order, ["acquire_next_image_khr_scene_frame"]);
    }

    #[test]
    fn scene_frame_acquire_plan_rejects_null_swapchain_or_semaphore() {
        let mut context = acquire_context();
        context.swapchain = vk::SwapchainKHR::null();
        assert!(
            native_vulkan_scene_frame_acquire_plan(context)
                .expect_err("null swapchain")
                .contains("valid swapchain")
        );

        let mut context = acquire_context();
        context.image_available = vk::Semaphore::null();
        assert!(
            native_vulkan_scene_frame_acquire_plan(context)
                .expect_err("null semaphore")
                .contains("image-available semaphore")
        );
    }

    fn acquire_context() -> NativeVulkanSceneFrameAcquireContext {
        NativeVulkanSceneFrameAcquireContext {
            swapchain: vk::SwapchainKHR::from_raw(11),
            image_available: vk::Semaphore::from_raw(12),
            timeout_ns: u64::MAX,
        }
    }
}

//! Scene swapchain image acquire boundary.
//!
//! References:
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use std::mem::MaybeUninit;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

pub(in crate::renderer::native_vulkan) const NATIVE_VULKAN_SCENE_FRAME_ACQUIRE_NONBLOCKING_TIMEOUT_NS:
    u64 = 0;

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
        command_order: ["try_acquire_next_image_khr_scene_frame"],
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_try_acquire_scene_frame_image(
    device: &Device,
    context: NativeVulkanSceneFrameAcquireContext,
) -> Result<Option<NativeVulkanSceneFrameAcquireResult>, String> {
    let plan = native_vulkan_scene_frame_acquire_plan(context)?;
    let mut image_index = MaybeUninit::<u32>::uninit();
    let status = unsafe {
        (device.commands().acquire_next_image_khr)(
            device.handle(),
            context.swapchain,
            context.timeout_ns,
            context.image_available,
            vk::Fence::null(),
            image_index.as_mut_ptr(),
        )
    };

    match native_vulkan_scene_frame_acquire_status(status)? {
        NativeVulkanSceneFrameAcquireStatus::Ready => {
            Ok(Some(NativeVulkanSceneFrameAcquireResult {
                image_index: unsafe { image_index.assume_init() },
                plan,
            }))
        }
        NativeVulkanSceneFrameAcquireStatus::NotReady => Ok(None),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeVulkanSceneFrameAcquireStatus {
    Ready,
    NotReady,
}

fn native_vulkan_scene_frame_acquire_status(
    status: vk::Result,
) -> Result<NativeVulkanSceneFrameAcquireStatus, String> {
    if status == vk::Result::SUCCESS || status == vk::Result::SUBOPTIMAL_KHR {
        Ok(NativeVulkanSceneFrameAcquireStatus::Ready)
    } else if status == vk::Result::NOT_READY || status == vk::Result::TIMEOUT {
        Ok(NativeVulkanSceneFrameAcquireStatus::NotReady)
    } else {
        Err(format!("vkAcquireNextImageKHR(scene frame): {status:?}"))
    }
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

        assert_eq!(
            plan.timeout_ns,
            NATIVE_VULKAN_SCENE_FRAME_ACQUIRE_NONBLOCKING_TIMEOUT_NS
        );
        assert_eq!(
            plan.command_order,
            ["try_acquire_next_image_khr_scene_frame"]
        );
    }

    #[test]
    fn scene_frame_acquire_status_does_not_read_image_index_when_not_ready() {
        assert_eq!(
            native_vulkan_scene_frame_acquire_status(vk::Result::SUCCESS).unwrap(),
            NativeVulkanSceneFrameAcquireStatus::Ready
        );
        assert_eq!(
            native_vulkan_scene_frame_acquire_status(vk::Result::SUBOPTIMAL_KHR).unwrap(),
            NativeVulkanSceneFrameAcquireStatus::Ready
        );
        assert_eq!(
            native_vulkan_scene_frame_acquire_status(vk::Result::NOT_READY).unwrap(),
            NativeVulkanSceneFrameAcquireStatus::NotReady
        );
        assert_eq!(
            native_vulkan_scene_frame_acquire_status(vk::Result::TIMEOUT).unwrap(),
            NativeVulkanSceneFrameAcquireStatus::NotReady
        );
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
            timeout_ns: NATIVE_VULKAN_SCENE_FRAME_ACQUIRE_NONBLOCKING_TIMEOUT_NS,
        }
    }
}

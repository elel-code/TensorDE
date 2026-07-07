//! Scene mesh present-frame orchestration.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{SceneFramePlan, SceneResource};
use crate::renderer::native_vulkan::NativeVulkanClearColor;
use crate::renderer::native_vulkan::vulkan::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot;

use super::frame_acquire::{
    NATIVE_VULKAN_SCENE_FRAME_ACQUIRE_NONBLOCKING_TIMEOUT_NS, NativeVulkanSceneFrameAcquirePlan,
    native_vulkan_try_acquire_scene_frame_image,
};
use super::frame_command_buffer::{
    NativeVulkanSceneFrameCommandBufferBeginPlan, NativeVulkanSceneFrameCommandBufferEndPlan,
    native_vulkan_begin_scene_frame_command_buffer, native_vulkan_end_scene_frame_command_buffer,
};
use super::frame_completion::NativeVulkanSceneFrameResourceRelease;
use super::frame_present::{NativeVulkanSceneFramePresentPlan, native_vulkan_present_scene_frame};
use super::frame_resources::NativeVulkanSceneFrameResources;
use super::frame_slots::{
    NativeVulkanSceneFrameSlotPreparePlan, NativeVulkanSceneFrameSlotResources,
};
use super::frame_submit::{
    NativeVulkanSceneFrameSubmitPlan, native_vulkan_submit_scene_frame_commands2,
};
use super::pipeline_factory::NativeVulkanSceneMeshPipelineShaders;
use super::runtime::{
    NativeVulkanSceneMeshRuntimeFrameContext, NativeVulkanSceneMeshRuntimeFramePlan,
    native_vulkan_record_scene_mesh_runtime_frame,
};

pub(in crate::renderer::native_vulkan) struct NativeVulkanScenePresentFrameContext<'a> {
    pub device: &'a Device,
    pub queue: vk::Queue,
    pub memory_properties: &'a vk::PhysicalDeviceMemoryProperties,
    pub descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_images: &'a [vk::Image],
    pub swapchain_extent: vk::Extent2D,
    pub target_format: vk::Format,
    pub clear_color: Option<NativeVulkanClearColor>,
    pub shaders: NativeVulkanSceneMeshPipelineShaders<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanScenePresentFramePlan<'a> {
    pub frame_index: u64,
    pub frame_slot: u32,
    pub image_index: u32,
    pub completed_resource_release: Option<NativeVulkanSceneFrameResourceRelease>,
    pub slot_prepare: NativeVulkanSceneFrameSlotPreparePlan,
    pub acquire: NativeVulkanSceneFrameAcquirePlan,
    pub command_buffer_begin: NativeVulkanSceneFrameCommandBufferBeginPlan,
    pub runtime_frame: NativeVulkanSceneMeshRuntimeFramePlan<'a>,
    pub command_buffer_end: NativeVulkanSceneFrameCommandBufferEndPlan,
    pub submit: NativeVulkanSceneFrameSubmitPlan,
    pub present: NativeVulkanSceneFramePresentPlan,
    pub command_order: [&'static str; 9],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_present_frame_slot(
    frame_index: u64,
    frame_slot_count: usize,
) -> Result<u32, String> {
    if frame_slot_count == 0 {
        return Err(
            "scene present frame slot selection requires at least one frame slot".to_owned(),
        );
    }
    u32::try_from(frame_index % frame_slot_count as u64)
        .map_err(|_| "scene present frame slot index exceeds u32".to_owned())
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_present_scene_mesh_runtime_frame<'a>(
    frame_index: u64,
    frame_slots: &mut NativeVulkanSceneFrameSlotResources,
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: NativeVulkanScenePresentFrameContext<'_>,
    resources: &[SceneResource],
    frame: &'a SceneFramePlan,
) -> Result<NativeVulkanScenePresentFramePlan<'a>, String> {
    validate_scene_present_frame_context(&context)?;
    let frame_slot =
        native_vulkan_scene_present_frame_slot(frame_index, frame_slots.frame_slot_count())?;
    let slot_prepare = frame_slots
        .try_prepare_frame_slot(context.device, frame_slot)?
        .ok_or_else(|| {
            format!(
                "scene frame slot {frame_slot} would block; previous submission is still in flight"
            )
        })?;
    let completed_resource_release = slot_prepare.completed_submission.map(|submission| {
        frame_resources.release_completed_frame_resources(context.device, submission)
    });
    let frame_submission = frame_slots.begin_frame_submission(frame_slot)?;
    let slot_sync = frame_slots.slot_sync(frame_slot)?;
    let mut submitted_to_queue = false;

    let result = (|| -> Result<NativeVulkanScenePresentFramePlan<'a>, String> {
        let acquire = native_vulkan_try_acquire_scene_frame_image(
            context.device,
            super::frame_acquire::NativeVulkanSceneFrameAcquireContext {
                swapchain: context.swapchain,
                image_available: slot_sync.image_available,
                timeout_ns: NATIVE_VULKAN_SCENE_FRAME_ACQUIRE_NONBLOCKING_TIMEOUT_NS,
            },
        )?
        .ok_or_else(|| "scene frame acquire would block; no swapchain image is ready".to_owned())?;
        let target = frame_slots.swapchain_target(
            context.swapchain_images,
            acquire.image_index,
            context.swapchain_extent,
        )?;
        let command_buffer_begin = native_vulkan_begin_scene_frame_command_buffer(
            context.device,
            slot_sync.command_buffer,
        )?;
        let runtime_frame = native_vulkan_record_scene_mesh_runtime_frame(
            frame_resources,
            NativeVulkanSceneMeshRuntimeFrameContext {
                device: context.device,
                memory_properties: context.memory_properties,
                descriptor_heap_properties: context.descriptor_heap_properties,
                command_buffer: slot_sync.command_buffer,
                frame_submission,
                target,
                target_format: context.target_format,
                clear_color: context.clear_color,
                shaders: context.shaders,
            },
            resources,
            frame,
        )?;
        let command_buffer_end =
            native_vulkan_end_scene_frame_command_buffer(context.device, slot_sync.command_buffer)?;
        let submit = native_vulkan_submit_scene_frame_commands2(
            context.device,
            context.queue,
            super::frame_submit::NativeVulkanSceneFrameSubmitContext {
                frame_submission,
                command_buffer: slot_sync.command_buffer,
                image_available: slot_sync.image_available,
                render_finished: slot_sync.render_finished,
                in_flight_fence: slot_sync.in_flight_fence,
            },
        )?;
        submitted_to_queue = true;
        frame_slots.mark_swapchain_image_presented(acquire.image_index)?;
        let present = native_vulkan_present_scene_frame(
            context.device,
            context.queue,
            super::frame_present::NativeVulkanSceneFramePresentContext {
                frame_submission,
                swapchain: context.swapchain,
                image_index: acquire.image_index,
                render_finished: slot_sync.render_finished,
            },
        )?;

        Ok(NativeVulkanScenePresentFramePlan {
            frame_index,
            frame_slot,
            image_index: acquire.image_index,
            completed_resource_release,
            slot_prepare,
            acquire: acquire.plan,
            command_buffer_begin,
            runtime_frame,
            command_buffer_end,
            submit,
            present,
            command_order: [
                "prepare_scene_frame_slot",
                "try_acquire_next_image_khr_scene_frame",
                "begin_command_buffer_scene_frame",
                "record_scene_mesh_runtime_frame",
                "end_command_buffer_scene_frame",
                "reset_scene_frame_fence",
                "queue_submit2_scene_frame",
                "mark_swapchain_image_presented",
                "queue_present_khr_scene_frame",
            ],
        })
    })();

    if result.is_err() && !submitted_to_queue {
        let _ = frame_slots.abort_frame_submission(frame_submission);
        let _ = frame_resources.release_completed_frame_resources(context.device, frame_submission);
    }

    result
}

fn validate_scene_present_frame_context(
    context: &NativeVulkanScenePresentFrameContext<'_>,
) -> Result<(), String> {
    if context.queue == vk::Queue::null() {
        return Err("scene present frame requires a valid queue".to_owned());
    }
    if context.swapchain == vk::SwapchainKHR::null() {
        return Err("scene present frame requires a valid swapchain".to_owned());
    }
    if context.swapchain_images.is_empty() {
        return Err("scene present frame requires swapchain images".to_owned());
    }
    if context.swapchain_extent.width == 0 || context.swapchain_extent.height == 0 {
        return Err("scene present frame requires non-zero swapchain extent".to_owned());
    }
    if context.target_format == vk::Format::UNDEFINED {
        return Err("scene present frame requires a defined target format".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_present_frame_slot_wraps_by_frame_slot_count() {
        assert_eq!(native_vulkan_scene_present_frame_slot(0, 3).unwrap(), 0);
        assert_eq!(native_vulkan_scene_present_frame_slot(1, 3).unwrap(), 1);
        assert_eq!(native_vulkan_scene_present_frame_slot(2, 3).unwrap(), 2);
        assert_eq!(native_vulkan_scene_present_frame_slot(3, 3).unwrap(), 0);
        assert_eq!(native_vulkan_scene_present_frame_slot(7, 3).unwrap(), 1);
    }

    #[test]
    fn scene_present_frame_slot_rejects_empty_frame_slot_set() {
        assert!(
            native_vulkan_scene_present_frame_slot(0, 0)
                .expect_err("empty slot set")
                .contains("at least one frame slot")
        );
    }
}

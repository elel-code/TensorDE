use std::time::Instant;

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder, KhrSwapchainExtensionDeviceCommands};

use super::super::present_timing::VulkanaliaPresentTimingConfig;
use super::scene_duration_micros;

#[derive(Debug, Clone, Copy)]
pub(super) struct VulkanaliaSceneQueuePresentFrame {
    pub(super) command_buffer: vk::CommandBuffer,
    pub(super) image_available: vk::Semaphore,
    pub(super) render_finished: vk::Semaphore,
    pub(super) fence: vk::Fence,
    pub(super) image_index: u32,
    pub(super) present_frame_index: u32,
    pub(super) label: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct VulkanaliaSceneQueuePresentResult {
    pub(super) present_id: Option<u64>,
    pub(super) queue_submit_micros: u64,
    pub(super) queue_present_micros: u64,
    pub(super) present_wait_after_present: bool,
}

pub(super) fn execute_scene_queue_submit_and_present(
    device: &Device,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    present_timing: VulkanaliaPresentTimingConfig,
    frame: VulkanaliaSceneQueuePresentFrame,
) -> Result<VulkanaliaSceneQueuePresentResult, String> {
    let submit_started_at = Instant::now();
    submit_scene_render_command_buffer2(
        device,
        queue,
        frame.command_buffer,
        frame.image_available,
        frame.render_finished,
        frame.fence,
        frame.label,
    )?;
    let queue_submit_micros = scene_duration_micros(submit_started_at.elapsed());

    let swapchains = [swapchain];
    let image_indices = [frame.image_index];
    let wait_semaphores = [frame.render_finished];
    let present_id = present_timing.present_id(frame.present_frame_index);
    let present_id_values = [present_id.unwrap_or(0)];
    let mut present_id2_info = present_id.map(|_| {
        vk::PresentId2KHR::builder()
            .present_ids(&present_id_values)
            .build()
    });
    let mut present_info = vk::PresentInfoKHR::builder()
        .wait_semaphores(&wait_semaphores)
        .swapchains(&swapchains)
        .image_indices(&image_indices);
    if present_timing.present_id2_enabled {
        if let Some(present_id2_info) = present_id2_info.as_mut() {
            present_info = present_info.push_next(present_id2_info);
        }
    }

    let present_started_at = Instant::now();
    unsafe {
        device
            .queue_present_khr(queue, &present_info)
            .map_err(|err| format!("vkQueuePresentKHR(vulkanalia {}): {err:?}", frame.label))?;
    }
    let queue_present_micros = scene_duration_micros(present_started_at.elapsed());
    let present_wait_after_present =
        present_timing.wait_after_queue_present(device, swapchain, present_id, frame.label)?;

    Ok(VulkanaliaSceneQueuePresentResult {
        present_id,
        queue_submit_micros,
        queue_present_micros,
        present_wait_after_present,
    })
}

fn submit_scene_render_command_buffer2(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    fence: vk::Fence,
    label: &'static str,
) -> Result<(), String> {
    let wait = vk::SemaphoreSubmitInfo::builder()
        .semaphore(image_available)
        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .build();
    let waits = [wait];
    let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
        .command_buffer(command_buffer)
        .build();
    let command_buffer_infos = [command_buffer_info];
    let signal = vk::SemaphoreSubmitInfo::builder()
        .semaphore(render_finished)
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
            .queue_submit2(queue, &[submit_info], fence)
            .map_err(|err| format!("vkQueueSubmit2(vulkanalia {label}): {err:?}"))?;
    }

    Ok(())
}

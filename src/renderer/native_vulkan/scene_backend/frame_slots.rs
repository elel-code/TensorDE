//! Scene present frame-slot resource owner.
//!
//! References:
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use super::frame_completion::{
    NativeVulkanSceneFrameCompletion, NativeVulkanSceneFrameCompletionTracker,
    NativeVulkanSceneFrameSubmission,
};
use super::render_target::NativeVulkanSceneSwapchainRenderTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameSlotPlan {
    pub frame_slot_count: usize,
    pub command_buffer_count: usize,
    pub semaphore_pair_count: usize,
    pub fence_count: usize,
    pub swapchain_image_view_count: usize,
    pub command_order: [&'static str; 5],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameSlotSync {
    pub frame_slot: u32,
    pub command_buffer: vk::CommandBuffer,
    pub image_available: vk::Semaphore,
    pub render_finished: vk::Semaphore,
    pub in_flight_fence: vk::Fence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameSlotPreparePlan {
    pub frame_slot: u32,
    pub had_in_flight_submission: bool,
    pub completed_submission: Option<NativeVulkanSceneFrameSubmission>,
    pub fence_poll_status: &'static str,
    pub command_order: [&'static str; 2],
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameSlotResources {
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
    swapchain_image_views: Vec<vk::ImageView>,
    swapchain_image_layouts: Vec<vk::ImageLayout>,
    completion_tracker: NativeVulkanSceneFrameCompletionTracker,
}

impl NativeVulkanSceneFrameSlotResources {
    pub(in crate::renderer::native_vulkan) fn create(
        device: &Device,
        swapchain_images: &[vk::Image],
        swapchain_format: vk::Format,
        queue_family_index: u32,
    ) -> Result<Self, String> {
        if swapchain_images.is_empty() {
            return Err("scene frame slots require at least one swapchain image".to_owned());
        }
        if swapchain_format == vk::Format::UNDEFINED {
            return Err("scene frame slots require a defined swapchain format".to_owned());
        }

        let mut swapchain_image_views = Vec::new();
        let mut command_pool = vk::CommandPool::null();
        let mut image_available = Vec::new();
        let mut render_finished = Vec::new();
        let mut in_flight = Vec::new();

        let result = (|| -> Result<Self, String> {
            swapchain_image_views =
                create_scene_swapchain_image_views(device, swapchain_images, swapchain_format)?;

            let command_pool_info = vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_family_index);
            command_pool = unsafe { device.create_command_pool(&command_pool_info, None) }
                .map_err(|err| format!("vkCreateCommandPool(scene frame slots): {err:?}"))?;

            let command_buffer_info = vk::CommandBufferAllocateInfo::builder()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(swapchain_images.len() as u32);
            let command_buffers = unsafe { device.allocate_command_buffers(&command_buffer_info) }
                .map_err(|err| format!("vkAllocateCommandBuffers(scene frame slots): {err:?}"))?;

            let semaphore_info = vk::SemaphoreCreateInfo::builder();
            let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
            for frame_slot in 0..swapchain_images.len() {
                image_available.push(
                    unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|err| {
                        format!(
                            "vkCreateSemaphore(scene image_available slot {frame_slot}): {err:?}"
                        )
                    })?,
                );
                render_finished.push(
                    unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|err| {
                        format!(
                            "vkCreateSemaphore(scene render_finished slot {frame_slot}): {err:?}"
                        )
                    })?,
                );
                in_flight.push(unsafe { device.create_fence(&fence_info, None) }.map_err(
                    |err| format!("vkCreateFence(scene frame slot {frame_slot}): {err:?}"),
                )?);
            }

            Ok(Self {
                command_pool,
                command_buffers,
                image_available: std::mem::take(&mut image_available),
                render_finished: std::mem::take(&mut render_finished),
                in_flight: std::mem::take(&mut in_flight),
                swapchain_image_views: std::mem::take(&mut swapchain_image_views),
                swapchain_image_layouts: vec![vk::ImageLayout::UNDEFINED; swapchain_images.len()],
                completion_tracker: NativeVulkanSceneFrameCompletionTracker::new(
                    swapchain_images.len() as u32,
                )?,
            })
        })();

        if result.is_err() {
            destroy_partial_scene_frame_slot_resources(
                device,
                swapchain_image_views,
                command_pool,
                image_available,
                render_finished,
                in_flight,
            );
        }
        result
    }

    pub(in crate::renderer::native_vulkan) fn plan(&self) -> NativeVulkanSceneFrameSlotPlan {
        NativeVulkanSceneFrameSlotPlan {
            frame_slot_count: self.command_buffers.len(),
            command_buffer_count: self.command_buffers.len(),
            semaphore_pair_count: self.image_available.len().min(self.render_finished.len()),
            fence_count: self.in_flight.len(),
            swapchain_image_view_count: self.swapchain_image_views.len(),
            command_order: [
                "create_swapchain_image_views",
                "create_command_pool",
                "allocate_command_buffers",
                "create_frame_semaphore_pairs",
                "create_frame_fences",
            ],
        }
    }

    pub(in crate::renderer::native_vulkan) fn frame_slot_count(&self) -> usize {
        self.command_buffers.len()
    }

    pub(in crate::renderer::native_vulkan) fn begin_frame_submission(
        &mut self,
        frame_slot: u32,
    ) -> Result<NativeVulkanSceneFrameSubmission, String> {
        self.completion_tracker.begin_frame(frame_slot)
    }

    pub(in crate::renderer::native_vulkan) fn complete_frame_submission(
        &mut self,
        submission: NativeVulkanSceneFrameSubmission,
    ) -> Result<NativeVulkanSceneFrameCompletion, String> {
        self.completion_tracker.complete_frame(submission)
    }

    pub(in crate::renderer::native_vulkan) fn abort_frame_submission(
        &mut self,
        submission: NativeVulkanSceneFrameSubmission,
    ) -> Result<(), String> {
        self.completion_tracker.abort_frame(submission)
    }

    pub(in crate::renderer::native_vulkan) fn prepare_frame_slot_plan(
        &self,
        frame_slot: u32,
    ) -> Result<NativeVulkanSceneFrameSlotPreparePlan, String> {
        let state = self.completion_tracker.slot_state(frame_slot)?;
        Ok(NativeVulkanSceneFrameSlotPreparePlan {
            frame_slot,
            had_in_flight_submission: state.in_flight,
            completed_submission: state.in_flight.then_some(state.last_submitted).flatten(),
            fence_poll_status: "signaled",
            command_order: [
                "poll_scene_frame_fence",
                "complete_scene_frame_submission_if_in_flight",
            ],
        })
    }

    pub(in crate::renderer::native_vulkan) fn try_prepare_frame_slot(
        &mut self,
        device: &Device,
        frame_slot: u32,
    ) -> Result<Option<NativeVulkanSceneFrameSlotPreparePlan>, String> {
        let plan = self.prepare_frame_slot_plan(frame_slot)?;
        let fence = self.slot_sync(frame_slot)?.in_flight_fence;
        let status = unsafe { device.get_fence_status(fence) }
            .map_err(|err| format!("vkGetFenceStatus(scene frame slot {frame_slot}): {err:?}"))?;
        if status == vk::SuccessCode::NOT_READY {
            return Ok(None);
        }
        if status != vk::SuccessCode::SUCCESS {
            return Err(format!(
                "vkGetFenceStatus(scene frame slot {frame_slot}) returned unexpected status {status:?}"
            ));
        }
        if let Some(submission) = plan.completed_submission {
            self.complete_frame_submission(submission)?;
        }
        Ok(Some(plan))
    }

    pub(in crate::renderer::native_vulkan) fn slot_sync(
        &self,
        frame_slot: u32,
    ) -> Result<NativeVulkanSceneFrameSlotSync, String> {
        let index = frame_slot as usize;
        Ok(NativeVulkanSceneFrameSlotSync {
            frame_slot,
            command_buffer: *self
                .command_buffers
                .get(index)
                .ok_or_else(|| format!("scene frame slot {frame_slot} has no command buffer"))?,
            image_available: *self
                .image_available
                .get(index)
                .ok_or_else(|| format!("scene frame slot {frame_slot} has no image semaphore"))?,
            render_finished: *self
                .render_finished
                .get(index)
                .ok_or_else(|| format!("scene frame slot {frame_slot} has no render semaphore"))?,
            in_flight_fence: *self
                .in_flight
                .get(index)
                .ok_or_else(|| format!("scene frame slot {frame_slot} has no in-flight fence"))?,
        })
    }

    pub(in crate::renderer::native_vulkan) fn swapchain_target(
        &self,
        swapchain_images: &[vk::Image],
        image_index: u32,
        extent: vk::Extent2D,
    ) -> Result<NativeVulkanSceneSwapchainRenderTarget, String> {
        let index = image_index as usize;
        Ok(NativeVulkanSceneSwapchainRenderTarget {
            image: *swapchain_images
                .get(index)
                .ok_or_else(|| format!("scene swapchain image {image_index} is unavailable"))?,
            image_view: *self.swapchain_image_views.get(index).ok_or_else(|| {
                format!("scene swapchain image view {image_index} is unavailable")
            })?,
            extent,
            initial_layout: *self.swapchain_image_layouts.get(index).ok_or_else(|| {
                format!("scene swapchain image layout {image_index} is unavailable")
            })?,
            final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
        })
    }

    pub(in crate::renderer::native_vulkan) fn mark_swapchain_image_presented(
        &mut self,
        image_index: u32,
    ) -> Result<(), String> {
        let layout = self
            .swapchain_image_layouts
            .get_mut(image_index as usize)
            .ok_or_else(|| format!("scene swapchain image layout {image_index} is unavailable"))?;
        *layout = vk::ImageLayout::PRESENT_SRC_KHR;
        Ok(())
    }

    pub(in crate::renderer::native_vulkan) fn destroy_all(self, device: &Device) {
        let _ = unsafe { device.device_wait_idle() };
        destroy_partial_scene_frame_slot_resources(
            device,
            self.swapchain_image_views,
            self.command_pool,
            self.image_available,
            self.render_finished,
            self.in_flight,
        );
    }
}

fn create_scene_swapchain_image_views(
    device: &Device,
    images: &[vk::Image],
    format: vk::Format,
) -> Result<Vec<vk::ImageView>, String> {
    let mut views = Vec::with_capacity(images.len());
    for (image_index, image) in images.iter().copied().enumerate() {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::_2D)
            .format(format)
            .subresource_range(scene_color_subresource_range());
        match unsafe { device.create_image_view(&create_info, None) } {
            Ok(view) => views.push(view),
            Err(err) => {
                for view in views {
                    unsafe {
                        device.destroy_image_view(view, None);
                    }
                }
                return Err(format!(
                    "vkCreateImageView(scene swapchain image {image_index}): {err:?}"
                ));
            }
        }
    }
    Ok(views)
}

fn destroy_partial_scene_frame_slot_resources(
    device: &Device,
    swapchain_image_views: Vec<vk::ImageView>,
    command_pool: vk::CommandPool,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
) {
    unsafe {
        for fence in in_flight {
            if fence != vk::Fence::null() {
                device.destroy_fence(fence, None);
            }
        }
        for semaphore in render_finished {
            if semaphore != vk::Semaphore::null() {
                device.destroy_semaphore(semaphore, None);
            }
        }
        for semaphore in image_available {
            if semaphore != vk::Semaphore::null() {
                device.destroy_semaphore(semaphore, None);
            }
        }
        if command_pool != vk::CommandPool::null() {
            device.destroy_command_pool(command_pool, None);
        }
        for view in swapchain_image_views {
            if view != vk::ImageView::null() {
                device.destroy_image_view(view, None);
            }
        }
    }
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
    use vulkanalia::vk::Handle;

    #[test]
    fn frame_slot_resources_report_godot_style_creation_plan() {
        let resources = test_resources();

        let plan = resources.plan();

        assert_eq!(plan.frame_slot_count, 2);
        assert_eq!(plan.command_buffer_count, 2);
        assert_eq!(plan.semaphore_pair_count, 2);
        assert_eq!(plan.fence_count, 2);
        assert_eq!(plan.swapchain_image_view_count, 2);
        assert_eq!(
            plan.command_order,
            [
                "create_swapchain_image_views",
                "create_command_pool",
                "allocate_command_buffers",
                "create_frame_semaphore_pairs",
                "create_frame_fences"
            ]
        );
    }

    #[test]
    fn frame_slot_resources_return_slot_sync_handles() {
        let resources = test_resources();

        let sync = resources.slot_sync(1).expect("slot sync");

        assert_eq!(sync.frame_slot, 1);
        assert_eq!(sync.command_buffer, vk::CommandBuffer::from_raw(22));
        assert_eq!(sync.image_available, vk::Semaphore::from_raw(32));
        assert_eq!(sync.render_finished, vk::Semaphore::from_raw(42));
        assert_eq!(sync.in_flight_fence, vk::Fence::from_raw(52));
        assert!(
            resources
                .slot_sync(2)
                .expect_err("slot outside resources")
                .contains("command buffer")
        );
    }

    #[test]
    fn frame_slot_resources_build_swapchain_render_target() {
        let resources = test_resources();
        let images = [vk::Image::from_raw(101), vk::Image::from_raw(102)];

        let target = resources
            .swapchain_target(
                &images,
                1,
                vk::Extent2D {
                    width: 3840,
                    height: 2160,
                },
            )
            .expect("swapchain target");

        assert_eq!(target.image, vk::Image::from_raw(102));
        assert_eq!(target.image_view, vk::ImageView::from_raw(62));
        assert_eq!(target.extent.width, 3840);
        assert_eq!(target.initial_layout, vk::ImageLayout::UNDEFINED);
        assert_eq!(target.final_layout, vk::ImageLayout::PRESENT_SRC_KHR);
    }

    #[test]
    fn frame_slot_resources_track_swapchain_present_layout() {
        let mut resources = test_resources();
        let images = [vk::Image::from_raw(101), vk::Image::from_raw(102)];

        resources
            .mark_swapchain_image_presented(1)
            .expect("mark presented");
        let target = resources
            .swapchain_target(
                &images,
                1,
                vk::Extent2D {
                    width: 3840,
                    height: 2160,
                },
            )
            .expect("swapchain target");

        assert_eq!(target.initial_layout, vk::ImageLayout::PRESENT_SRC_KHR);
        assert!(
            resources
                .mark_swapchain_image_presented(2)
                .expect_err("missing image layout")
                .contains("layout")
        );
    }

    #[test]
    fn frame_slot_resources_forward_completion_tracker() {
        let mut resources = test_resources();

        let first = resources.begin_frame_submission(1).expect("begin");
        assert_eq!(first, NativeVulkanSceneFrameSubmission::new(1, 1));
        assert!(
            resources
                .begin_frame_submission(1)
                .expect_err("in-flight slot")
                .contains("still in flight")
        );

        let completion = resources
            .complete_frame_submission(first)
            .expect("complete");
        assert!(completion.newly_completed);
        let second = resources.begin_frame_submission(1).expect("reuse");
        assert_eq!(second, NativeVulkanSceneFrameSubmission::new(1, 2));
    }

    #[test]
    fn frame_slot_resources_abort_unsubmitted_frame() {
        let mut resources = test_resources();

        let first = resources.begin_frame_submission(1).expect("begin");
        resources
            .abort_frame_submission(first)
            .expect("abort frame");
        let second = resources.begin_frame_submission(1).expect("begin again");

        assert_eq!(second, NativeVulkanSceneFrameSubmission::new(1, 2));
    }

    #[test]
    fn frame_slot_prepare_plan_exposes_completion_before_fence_reset() {
        let mut resources = test_resources();

        let initial = resources
            .prepare_frame_slot_plan(1)
            .expect("initial prepare plan");
        assert!(!initial.had_in_flight_submission);
        assert_eq!(initial.completed_submission, None);
        assert_eq!(
            initial.command_order,
            [
                "poll_scene_frame_fence",
                "complete_scene_frame_submission_if_in_flight"
            ]
        );
        assert_eq!(initial.fence_poll_status, "signaled");

        let submitted = resources.begin_frame_submission(1).expect("begin");
        let in_flight = resources
            .prepare_frame_slot_plan(1)
            .expect("in-flight prepare plan");
        assert!(in_flight.had_in_flight_submission);
        assert_eq!(in_flight.completed_submission, Some(submitted));
    }

    fn test_resources() -> NativeVulkanSceneFrameSlotResources {
        NativeVulkanSceneFrameSlotResources {
            command_pool: vk::CommandPool::from_raw(10),
            command_buffers: vec![
                vk::CommandBuffer::from_raw(21),
                vk::CommandBuffer::from_raw(22),
            ],
            image_available: vec![vk::Semaphore::from_raw(31), vk::Semaphore::from_raw(32)],
            render_finished: vec![vk::Semaphore::from_raw(41), vk::Semaphore::from_raw(42)],
            in_flight: vec![vk::Fence::from_raw(51), vk::Fence::from_raw(52)],
            swapchain_image_views: vec![vk::ImageView::from_raw(61), vk::ImageView::from_raw(62)],
            swapchain_image_layouts: vec![vk::ImageLayout::UNDEFINED; 2],
            completion_tracker: NativeVulkanSceneFrameCompletionTracker::new(2).expect("tracker"),
        }
    }
}

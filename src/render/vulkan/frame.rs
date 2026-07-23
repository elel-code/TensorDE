use std::{
    collections::HashMap,
    os::fd::{FromRawFd, OwnedFd},
};

use thiserror::Error;
use vulkanalia::vk::KhrExternalSemaphoreFdExtensionDeviceCommands;
use vulkanalia::vk::{DeviceV1_0, DeviceV1_2, DeviceV1_3, Handle, HasBuilder};
use vulkanalia::{Device, vk};

use crate::render::{DescriptorHeapLayout, FrameSubmission};

use super::{
    heap::{DescriptorHeapError, DescriptorHeapResource, SamplerHeapLayout},
    import::ClientImageInfo,
    pipeline::{ClientImagePipeline, ClientPipelineError},
    target::NativeOutputImageInfo,
};

pub(super) use super::heap::sampler_heap_layout;

mod record;
use record::{SceneRecord, prepare_draws, record_scene};

const COMMAND_BUFFER_COUNT: usize = 3;

pub(super) struct VulkanFrameExecutor {
    command_pool: vk::CommandPool,
    command_buffers: [vk::CommandBuffer; COMMAND_BUFFER_COUNT],
    retire_values: [u64; COMMAND_BUFFER_COUNT],
    timeline: vk::Semaphore,
    render_complete: [vk::Semaphore; COMMAND_BUFFER_COUNT],
    heap: DescriptorHeapResource,
    graphics_queue_family: u32,
    pipelines: HashMap<vk::Format, ClientImagePipeline>,
}

impl VulkanFrameExecutor {
    pub(super) fn new(
        instance: &vulkanalia::Instance,
        device: &Device,
        physical_device: vk::PhysicalDevice,
        graphics_queue_family: u32,
        heap_layout: DescriptorHeapLayout,
        sampler_heap_layout: SamplerHeapLayout,
    ) -> Result<Self, VulkanFrameError> {
        debug_assert_eq!(record::DRAW_PUSH_DATA_SIZE, 64);
        let pool_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(graphics_queue_family);
        let command_pool = unsafe { device.create_command_pool(&pool_info, None) }
            .map_err(VulkanFrameError::Vulkan)?;
        let allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(COMMAND_BUFFER_COUNT as u32);
        let command_buffers = match unsafe { device.allocate_command_buffers(&allocate_info) } {
            Ok(buffers) if buffers.len() == COMMAND_BUFFER_COUNT => {
                [buffers[0], buffers[1], buffers[2]]
            }
            Ok(buffers) => {
                unsafe {
                    device.free_command_buffers(command_pool, &buffers);
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(VulkanFrameError::Vulkan(
                    vk::ErrorCode::INITIALIZATION_FAILED,
                ));
            }
            Err(error) => {
                unsafe { device.destroy_command_pool(command_pool, None) };
                return Err(VulkanFrameError::Vulkan(error));
            }
        };
        let mut timeline_info = vk::SemaphoreTypeCreateInfo::builder()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0)
            .build();
        let semaphore_info = vk::SemaphoreCreateInfo::builder().push_next(&mut timeline_info);
        let timeline = match unsafe { device.create_semaphore(&semaphore_info, None) } {
            Ok(semaphore) => semaphore,
            Err(error) => {
                unsafe {
                    device.free_command_buffers(command_pool, &command_buffers);
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(VulkanFrameError::Vulkan(error));
            }
        };
        let mut render_complete = Vec::with_capacity(COMMAND_BUFFER_COUNT);
        let mut export_info = vk::ExportSemaphoreCreateInfo::builder()
            .handle_types(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
            .build();
        let export_semaphore_info = vk::SemaphoreCreateInfo::builder().push_next(&mut export_info);
        for _ in 0..COMMAND_BUFFER_COUNT {
            match unsafe { device.create_semaphore(&export_semaphore_info, None) } {
                Ok(semaphore) => render_complete.push(semaphore),
                Err(error) => {
                    unsafe {
                        for semaphore in render_complete {
                            device.destroy_semaphore(semaphore, None);
                        }
                        device.destroy_semaphore(timeline, None);
                        device.free_command_buffers(command_pool, &command_buffers);
                        device.destroy_command_pool(command_pool, None);
                    }
                    return Err(VulkanFrameError::Vulkan(error));
                }
            }
        }
        let render_complete = [render_complete[0], render_complete[1], render_complete[2]];
        let heap = match DescriptorHeapResource::new(
            instance,
            device,
            physical_device,
            heap_layout,
            sampler_heap_layout,
        ) {
            Ok(heap) => heap,
            Err(source) => {
                unsafe {
                    device.destroy_semaphore(timeline, None);
                    for semaphore in render_complete {
                        device.destroy_semaphore(semaphore, None);
                    }
                    device.free_command_buffers(command_pool, &command_buffers);
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(VulkanFrameError::DescriptorHeap(source));
            }
        };
        Ok(Self {
            command_pool,
            command_buffers,
            retire_values: [0; COMMAND_BUFFER_COUNT],
            timeline,
            render_complete,
            heap,
            graphics_queue_family,
            pipelines: HashMap::new(),
        })
    }

    pub(super) fn completed(&self, device: &Device) -> Result<u64, vk::ErrorCode> {
        unsafe { device.get_semaphore_counter_value(self.timeline) }
    }

    pub(super) fn submit(
        &mut self,
        device: &Device,
        queue: vk::Queue,
        frame: &FrameSubmission,
        image: &NativeOutputImageInfo,
        client_images: &[ClientImageInfo],
        completed_value: u64,
    ) -> Result<OwnedFd, VulkanFrameError> {
        // Validate the scene-to-Vulkan descriptor contract before touching a
        // command buffer.  Returning after `begin_command_buffer` would leave
        // that buffer in the recording state and make the failure path depend
        // on a later reset.
        if client_images.len()
            != usize::try_from(frame.client_image_descriptors).unwrap_or(usize::MAX)
        {
            return Err(VulkanFrameError::DescriptorImageCountMismatch {
                expected: frame.client_image_descriptors,
                found: client_images.len(),
            });
        }
        let Some((slot, command_buffer)) = self
            .command_buffers
            .iter()
            .enumerate()
            .find(|(slot, _)| self.retire_values[*slot] <= completed_value)
            .map(|(slot, command_buffer)| (slot, *command_buffer))
        else {
            return Err(VulkanFrameError::NoCommandBuffer);
        };
        let mut descriptor_views = Vec::with_capacity(1 + client_images.len());
        descriptor_views.push(image.view_info);
        descriptor_views.extend(client_images.iter().map(|client| client.view_info));
        // Descriptor encoding is a host-side operation.  Do it before
        // beginning the command buffer so a write/flush failure cannot leave
        // the buffer in the recording state.
        self.heap
            .prepare_image_descriptors(device, frame.descriptors, &descriptor_views)
            .map_err(VulkanFrameError::DescriptorHeap)?;
        let draws = prepare_draws(
            frame,
            self.heap.descriptor_stride(),
            self.heap.resource_heap_base(),
        )
        .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let pipeline = if draws.is_empty() {
            None
        } else {
            Some(self.pipeline_for(device, image.view_info.format)?)
        };
        let begin = vk::CommandBufferBeginInfo::builder();
        unsafe {
            device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(VulkanFrameError::Vulkan)?;
            device
                .begin_command_buffer(command_buffer, &begin)
                .map_err(VulkanFrameError::Vulkan)?;
        }
        unsafe {
            self.heap
                .record_copy_and_bind(device, command_buffer, frame.descriptors);
            record_scene(
                device,
                command_buffer,
                SceneRecord {
                    frame,
                    output: *image,
                    clients: client_images,
                    pipeline,
                    graphics_queue_family: self.graphics_queue_family,
                    draws: &draws,
                },
            );
            device
                .end_command_buffer(command_buffer)
                .map_err(VulkanFrameError::Vulkan)?;
        }

        let command_info = vk::CommandBufferSubmitInfo::builder()
            .command_buffer(command_buffer)
            .build();
        let signal_info = vk::SemaphoreSubmitInfo::builder()
            .semaphore(self.timeline)
            .value(frame.timeline_value)
            .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .build();
        let render_complete_info = vk::SemaphoreSubmitInfo::builder()
            .semaphore(self.render_complete[slot])
            .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .build();
        let signal_infos = [signal_info, render_complete_info];
        let submit_info = vk::SubmitInfo2::builder()
            .command_buffer_infos(std::slice::from_ref(&command_info))
            .signal_semaphore_infos(&signal_infos);
        unsafe {
            device
                .queue_submit2(queue, std::slice::from_ref(&submit_info), vk::Fence::null())
                .map_err(VulkanFrameError::Vulkan)?;
        }
        self.retire_values[slot] = frame.timeline_value;
        let fd_info = vk::SemaphoreGetFdInfoKHR::builder()
            .semaphore(self.render_complete[slot])
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
        let raw_fd = unsafe { device.get_semaphore_fd_khr(&fd_info) }
            .map_err(VulkanFrameError::ExportSyncFd)?;
        let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
        Ok(fd)
    }

    pub(super) unsafe fn destroy(&mut self, device: &Device) {
        unsafe {
            for (_, pipeline) in std::mem::take(&mut self.pipelines) {
                pipeline.destroy(device);
            }
            device.destroy_semaphore(self.timeline, None);
            for semaphore in self.render_complete {
                device.destroy_semaphore(semaphore, None);
            }
            device.free_command_buffers(self.command_pool, &self.command_buffers);
            device.destroy_command_pool(self.command_pool, None);
            self.heap.destroy(device);
        }
    }

    fn pipeline_for(
        &mut self,
        device: &Device,
        format: vk::Format,
    ) -> Result<vk::Pipeline, VulkanFrameError> {
        if !self.pipelines.contains_key(&format) {
            let pipeline = ClientImagePipeline::new(
                device,
                format,
                self.heap.descriptor_stride(),
                self.heap.resource_heap_base(),
            )
            .map_err(VulkanFrameError::Pipeline)?;
            self.pipelines.insert(format, pipeline);
        }
        Ok(self
            .pipelines
            .get(&format)
            .expect("pipeline was inserted or already present")
            .handle())
    }
}

#[derive(Debug, Error)]
pub(super) enum VulkanFrameError {
    #[error("no reusable command buffer is available")]
    NoCommandBuffer,
    #[error("Vulkan command failed: {0:?}")]
    Vulkan(vk::ErrorCode),
    #[error("descriptor heap operation failed: {0}")]
    DescriptorHeap(DescriptorHeapError),
    #[error("frame draw preparation failed: {0}")]
    Record(String),
    #[error("client image pipeline creation failed: {0}")]
    Pipeline(ClientPipelineError),
    #[error("frame expected {expected} client image descriptors, got {found}")]
    DescriptorImageCountMismatch { expected: u32, found: usize },
    #[error("failed to export the frame completion SYNC_FD: {0:?}")]
    ExportSyncFd(vk::ErrorCode),
}

impl VulkanFrameError {
    pub(super) const fn was_submitted(&self) -> bool {
        matches!(self, Self::ExportSyncFd(_))
    }
}

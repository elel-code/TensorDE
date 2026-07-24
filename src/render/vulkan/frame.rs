use std::{
    collections::{HashMap, hash_map::Entry},
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
    pipeline::{
        ClientImagePipeline, ClientPipelineError, CursorPipeline, CursorPipelineError,
        SolidPipeline, SolidPipelineError,
    },
    target::NativeOutputImageInfo,
};

pub(super) use super::heap::sampler_heap_layout;

mod record;
use record::{SceneRecord, prepare_cursor_draw, prepare_draws, prepare_solid_draws, record_scene};

const COMMAND_BUFFER_COUNT: usize = 3;

pub(super) struct FrameExecution<'a> {
    pub(super) frame: &'a FrameSubmission,
    pub(super) output: &'a NativeOutputImageInfo,
    pub(super) client_images: &'a [ClientImageInfo],
    pub(super) acquire_semaphores: &'a [vk::Semaphore],
    pub(super) completed_value: u64,
}

pub(super) struct VulkanFrameExecutor {
    command_pool: vk::CommandPool,
    command_buffers: [vk::CommandBuffer; COMMAND_BUFFER_COUNT],
    retire_values: [u64; COMMAND_BUFFER_COUNT],
    timeline: vk::Semaphore,
    render_complete: [vk::Semaphore; COMMAND_BUFFER_COUNT],
    heap: DescriptorHeapResource,
    graphics_queue_family: u32,
    pipelines: HashMap<vk::Format, ClientImagePipeline>,
    cursor_pipelines: HashMap<vk::Format, CursorPipeline>,
    solid_pipelines: HashMap<vk::Format, SolidPipeline>,
}

impl VulkanFrameExecutor {
    pub(super) fn new(
        instance: &vulkanalia::Instance,
        device: &Device,
        physical_device: vk::PhysicalDevice,
        graphics_queue_family: u32,
        heap_layout: DescriptorHeapLayout,
        sampler_heap_layout: SamplerHeapLayout,
        bootstrap_pipeline_format: vk::Format,
    ) -> Result<Self, VulkanFrameError> {
        debug_assert_eq!(record::DRAW_PUSH_DATA_SIZE, 64);
        debug_assert_eq!(record::CURSOR_PUSH_DATA_SIZE, 16);
        debug_assert_eq!(record::SOLID_PUSH_DATA_SIZE, 32);
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
        let mut executor = Self {
            command_pool,
            command_buffers,
            retire_values: [0; COMMAND_BUFFER_COUNT],
            timeline,
            render_complete,
            heap,
            graphics_queue_family,
            pipelines: HashMap::new(),
            cursor_pipelines: HashMap::new(),
            solid_pipelines: HashMap::new(),
        };
        if let Err(error) = executor.pipeline_for(device, bootstrap_pipeline_format) {
            unsafe { executor.destroy(device) };
            return Err(error);
        }
        if let Err(error) = executor.cursor_pipeline_for(device, bootstrap_pipeline_format) {
            unsafe { executor.destroy(device) };
            return Err(error);
        }
        if let Err(error) = executor.solid_pipeline_for(device, bootstrap_pipeline_format) {
            unsafe { executor.destroy(device) };
            return Err(error);
        }
        Ok(executor)
    }

    pub(super) fn completed(&self, device: &Device) -> Result<u64, vk::ErrorCode> {
        unsafe { device.get_semaphore_counter_value(self.timeline) }
    }

    pub(super) fn submit(
        &mut self,
        device: &Device,
        queue: vk::Queue,
        execution: FrameExecution<'_>,
    ) -> Result<OwnedFd, VulkanFrameError> {
        let FrameExecution {
            frame,
            output: image,
            client_images,
            acquire_semaphores,
            completed_value,
        } = execution;
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
        let cursor = prepare_cursor_draw(frame)
            .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let solids = prepare_solid_draws(frame)
            .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let client_pipeline = if draws.is_empty() {
            None
        } else {
            Some(self.pipeline_for(device, image.view_info.format)?)
        };
        let cursor_pipeline = if cursor.is_some() {
            let pipeline = self.cursor_pipeline_for(device, image.view_info.format)?;
            Some((pipeline.handle(), pipeline.layout()))
        } else {
            None
        };
        let solid_pipeline = if solids.is_empty() {
            None
        } else {
            let pipeline = self.solid_pipeline_for(device, image.view_info.format)?;
            Some((pipeline.handle(), pipeline.layout()))
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
                    client_pipeline,
                    solid_pipeline,
                    cursor_pipeline,
                    graphics_queue_family: self.graphics_queue_family,
                    draws: &draws,
                    solids: &solids,
                    cursor,
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
        let wait_infos = acquire_semaphores
            .iter()
            .copied()
            .map(acquire_wait_info)
            .collect::<Vec<_>>();
        let mut submit_info = vk::SubmitInfo2::builder()
            .command_buffer_infos(std::slice::from_ref(&command_info))
            .signal_semaphore_infos(&signal_infos);
        if !wait_infos.is_empty() {
            submit_info = submit_info.wait_semaphore_infos(&wait_infos);
        }
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
            for (_, pipeline) in std::mem::take(&mut self.cursor_pipelines) {
                pipeline.destroy(device);
            }
            for (_, pipeline) in std::mem::take(&mut self.solid_pipelines) {
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

    fn cursor_pipeline_for(
        &mut self,
        device: &Device,
        format: vk::Format,
    ) -> Result<&CursorPipeline, VulkanFrameError> {
        match self.cursor_pipelines.entry(format) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let pipeline = CursorPipeline::new(device, format)
                    .map_err(VulkanFrameError::CursorPipeline)?;
                Ok(entry.insert(pipeline))
            }
        }
    }

    fn solid_pipeline_for(
        &mut self,
        device: &Device,
        format: vk::Format,
    ) -> Result<&SolidPipeline, VulkanFrameError> {
        match self.solid_pipelines.entry(format) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let pipeline =
                    SolidPipeline::new(device, format).map_err(VulkanFrameError::SolidPipeline)?;
                Ok(entry.insert(pipeline))
            }
        }
    }
}

fn acquire_wait_info(semaphore: vk::Semaphore) -> vk::SemaphoreSubmitInfo {
    vk::SemaphoreSubmitInfo::builder()
        .semaphore(semaphore)
        .stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
        .build()
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
    #[error("cursor pipeline creation failed: {0}")]
    CursorPipeline(CursorPipelineError),
    #[error("solid pipeline creation failed: {0}")]
    SolidPipeline(SolidPipelineError),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_acquire_waits_at_the_first_sampling_stage() {
        let semaphore = vk::Semaphore::from_raw(7);
        let wait = acquire_wait_info(semaphore);
        assert_eq!(wait.semaphore, semaphore);
        assert_eq!(wait.value, 0);
        assert_eq!(wait.stage_mask, vk::PipelineStageFlags2::FRAGMENT_SHADER);
    }
}

use std::{
    collections::{HashMap, hash_map::Entry},
    os::fd::OwnedFd,
};

use thiserror::Error;
use vulkan_renderer::vulkanalia::vk;
use vulkan_renderer::{
    BinarySemaphore, BinarySemaphoreDescriptor, CommandEncoderDescriptor, DescriptorAllocation,
    DescriptorHeap, DescriptorHeapUploadBatch, Device as RendererDevice, Error as RendererError,
    FrameToken, HeapDescriptorType, SampledImageDescriptor, SampledImageDescriptorWriteBatch,
    SamplerBinding, SamplerDescriptor, SemaphoreWait, TextureFormat, TextureLayout,
};

use crate::render::FrameSubmission;

use super::{
    import::{ClientImageCache, ClientImageInfo},
    pipeline::{ClientImagePipeline, CursorPipeline, FocusRingPipeline, TensorPipelineError},
    target::NativeOutputImageInfo,
};

mod record;
use record::{
    SceneBarrierScratch, SceneRecord, prepare_cursor_draws, prepare_draws,
    prepare_focus_ring_draws, prepare_scene_draws, record_scene,
};

const FRAME_COMPLETION_SEMAPHORE_COUNT: usize = 3;

pub(super) struct FrameExecution<'a> {
    pub(super) frame: &'a FrameSubmission,
    pub(super) descriptor_allocation: &'a DescriptorAllocation,
    pub(super) submission: FrameToken,
    pub(super) output: &'a NativeOutputImageInfo,
    pub(super) client_image_cache: &'a ClientImageCache,
    pub(super) acquire_waits: &'a [SemaphoreWait],
    pub(super) completed_value: u64,
}

pub(super) struct VulkanFrameExecutor {
    completion_retire_values: [u64; FRAME_COMPLETION_SEMAPHORE_COUNT],
    render_complete: Option<[BinarySemaphore; FRAME_COMPLETION_SEMAPHORE_COUNT]>,
    resource_heap: DescriptorHeap,
    sampler_heap: DescriptorHeap,
    linear_sampler: SamplerBinding,
    descriptor_stride: u64,
    linear_sampler_index: u32,
    // Resolve frame-local buffer IDs into value-only Vulkan image snapshots
    // without allocating a new vector every repaint. The cache owns actual
    // images; this buffer is only retained encoding/recording scratch.
    resolved_client_images: Vec<ClientImageInfo>,
    sampled_descriptors: Vec<SampledImageDescriptor>,
    descriptor_writes: SampledImageDescriptorWriteBatch,
    descriptor_uploads: DescriptorHeapUploadBatch,
    sampler_upload_pending: bool,
    graphics_queue_family: u32,
    scene_barriers: SceneBarrierScratch,
    pipelines: HashMap<TextureFormat, ClientImagePipeline>,
    cursor_pipelines: HashMap<TextureFormat, CursorPipeline>,
    focus_ring_pipelines: HashMap<TextureFormat, FocusRingPipeline>,
}

impl VulkanFrameExecutor {
    pub(super) fn new(
        renderer: &RendererDevice,
        resource_heap: DescriptorHeap,
        sampler_heap: DescriptorHeap,
        bootstrap_pipeline_format: TextureFormat,
    ) -> Result<Self, VulkanFrameError> {
        let graphics_queue_family = renderer.queues().graphics_family;
        debug_assert_eq!(record::DRAW_PUSH_DATA_SIZE, 64);
        debug_assert_eq!(record::CURSOR_PUSH_DATA_SIZE, 16);
        debug_assert_eq!(record::FOCUS_RING_PUSH_DATA_SIZE, 64);
        let descriptor_stride = resource_heap
            .allocation_stride(HeapDescriptorType::SampledImage)
            .map_err(|error| {
                VulkanFrameError::DescriptorHeap(RendererError::Validation(error.to_string()))
            })?;
        let linear_sampler = SamplerBinding::new(&sampler_heap, SamplerDescriptor::linear_clamp())
            .map_err(VulkanFrameError::DescriptorHeap)?;
        let linear_sampler_index = linear_sampler
            .shader_heap_index()
            .map_err(VulkanFrameError::DescriptorHeap)?;
        let mut render_complete = Vec::with_capacity(FRAME_COMPLETION_SEMAPHORE_COUNT);
        for _ in 0..FRAME_COMPLETION_SEMAPHORE_COUNT {
            match renderer.create_exportable_sync_fd_semaphore(&BinarySemaphoreDescriptor {
                label: Some("tensor-frame-complete".into()),
            }) {
                Ok(semaphore) => render_complete.push(semaphore),
                Err(error) => {
                    return Err(VulkanFrameError::CreateCompletionSemaphore(
                        error.to_string(),
                    ));
                }
            }
        }
        let render_complete: [BinarySemaphore; FRAME_COMPLETION_SEMAPHORE_COUNT] = render_complete
            .try_into()
            .expect("fixed completion semaphore count converts into an array");
        let mut executor = Self {
            completion_retire_values: [0; FRAME_COMPLETION_SEMAPHORE_COUNT],
            render_complete: Some(render_complete),
            resource_heap,
            sampler_heap,
            linear_sampler,
            descriptor_stride,
            linear_sampler_index,
            resolved_client_images: Vec::with_capacity(64),
            sampled_descriptors: Vec::with_capacity(64),
            descriptor_writes: SampledImageDescriptorWriteBatch::with_capacity(64),
            descriptor_uploads: DescriptorHeapUploadBatch::with_capacity(2),
            sampler_upload_pending: true,
            graphics_queue_family,
            scene_barriers: SceneBarrierScratch::new(),
            pipelines: HashMap::new(),
            cursor_pipelines: HashMap::new(),
            focus_ring_pipelines: HashMap::new(),
        };
        if let Err(error) = executor.ensure_client_pipeline(renderer, bootstrap_pipeline_format) {
            executor.destroy();
            return Err(error);
        }
        if let Err(error) = executor.cursor_pipeline_for(renderer, bootstrap_pipeline_format) {
            executor.destroy();
            return Err(error);
        }
        if let Err(error) = executor.focus_ring_pipeline_for(renderer, bootstrap_pipeline_format) {
            executor.destroy();
            return Err(error);
        }
        Ok(executor)
    }

    pub(super) fn submit(
        &mut self,
        renderer: &RendererDevice,
        execution: FrameExecution<'_>,
    ) -> Result<OwnedFd, VulkanFrameError> {
        let FrameExecution {
            frame,
            descriptor_allocation,
            submission,
            output: image,
            client_image_cache,
            acquire_waits,
            completed_value,
        } = execution;
        if submission.value() != frame.timeline_value {
            return Err(VulkanFrameError::SubmissionTimelineMismatch {
                frame: frame.timeline_value,
                submission: submission.value(),
            });
        }
        // Validate the scene-to-Vulkan descriptor contract before touching a
        // command buffer.  Returning after `begin_command_buffer` would leave
        // that buffer in the recording state and make the failure path depend
        // on a later reset.
        let client_ids = frame.draw_plan.images();
        if client_ids.len() != usize::try_from(frame.client_image_descriptors).unwrap_or(usize::MAX)
        {
            return Err(VulkanFrameError::DescriptorImageCountMismatch {
                expected: frame.client_image_descriptors,
                found: client_ids.len(),
            });
        }
        self.resolved_client_images.clear();
        self.resolved_client_images.reserve(client_ids.len());
        for id in client_ids {
            let image = client_image_cache
                .image_info(*id)
                .ok_or(VulkanFrameError::MissingClientImage(*id))?;
            self.resolved_client_images.push(image);
        }
        let Some(slot) = self
            .completion_retire_values
            .iter()
            .enumerate()
            .find(|(_, retire_value)| **retire_value <= completed_value)
            .map(|(slot, _)| slot)
        else {
            return Err(VulkanFrameError::NoCompletionSemaphore);
        };
        let draws = prepare_draws(frame, self.descriptor_stride, self.linear_sampler_index)
            .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let cursors =
            prepare_cursor_draws(frame, self.descriptor_stride, self.linear_sampler_index)
                .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let focus_rings = prepare_focus_ring_draws(frame)
            .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let scene_draws = prepare_scene_draws(frame.draw_plan.scene_draws(), &draws, &focus_rings)
            .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let needs_client_pipeline = !draws.is_empty() || cursors.has_textures();
        if needs_client_pipeline {
            self.ensure_client_pipeline(renderer, image.format)?;
        }
        if cursors.has_vectors() {
            self.cursor_pipeline_for(renderer, image.format)?;
        }
        if !focus_rings.is_empty() {
            self.focus_ring_pipeline_for(renderer, image.format)?;
        }
        // Create every pipeline before borrowing the client one. The shared
        // pipeline is retained by the renderer while Tensor keeps only the
        // product-specific scene order and command-recording policy.
        let client_pipeline = if needs_client_pipeline {
            Some(
                self.pipelines
                    .get(&image.format)
                    .expect("client pipeline was created before recording")
                    .pipeline(),
            )
        } else {
            None
        };
        let cursor_pipeline = if cursors.has_vectors() {
            let pipeline = self
                .cursor_pipelines
                .get(&image.format)
                .expect("cursor pipeline was created before recording");
            Some(pipeline.pipeline())
        } else {
            None
        };
        let focus_ring_pipeline = if focus_rings.is_empty() {
            None
        } else {
            let pipeline = self
                .focus_ring_pipelines
                .get(&image.format)
                .expect("focus-ring pipeline was created before recording");
            Some(pipeline.pipeline())
        };
        let client_images = self.resolved_client_images.as_slice();
        self.sampled_descriptors.clear();
        self.sampled_descriptors.reserve(1 + client_images.len());
        self.sampled_descriptors
            .push(image.sampled_descriptor.clone());
        self.sampled_descriptors.extend(
            client_images
                .iter()
                .map(|client| client.sampled_descriptor.clone()),
        );
        // Descriptor encoding is a host-side operation.  Do it before
        // beginning the command buffer so a write/flush failure cannot leave
        // the buffer in the recording state.
        unsafe {
            self.resource_heap.write_sampled_images(
                descriptor_allocation,
                &self.sampled_descriptors,
                TextureLayout::General,
                &mut self.descriptor_writes,
            )
        }
        .map_err(VulkanFrameError::DescriptorHeap)?;
        let mut encoder = renderer
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("tensor-frame".into()),
            })
            .map_err(VulkanFrameError::CommandEncoder)?;
        unsafe {
            let sampler_upload = self
                .sampler_upload_pending
                .then(|| self.linear_sampler.upload_range());
            self.sampler_heap
                .record_upload_and_bind(
                    &mut encoder,
                    sampler_upload.as_slice(),
                    &mut self.descriptor_uploads,
                )
                .map_err(VulkanFrameError::DescriptorHeap)?;
            self.resource_heap
                .record_upload_and_bind(
                    &mut encoder,
                    std::slice::from_ref(&descriptor_allocation.upload_range()),
                    &mut self.descriptor_uploads,
                )
                .map_err(VulkanFrameError::DescriptorHeap)?;
            record_scene(
                &mut encoder,
                SceneRecord {
                    frame,
                    output: image,
                    clients: client_images,
                    client_pipeline,
                    focus_ring_pipeline,
                    cursor_pipeline,
                    graphics_queue_family: self.graphics_queue_family,
                    scene_draws: &scene_draws,
                    cursors: &cursors,
                },
                &mut self.scene_barriers,
            )
            .map_err(VulkanFrameError::SharedRecord)?;
        }
        let command_buffer = encoder.finish().map_err(VulkanFrameError::CommandEncoder)?;

        let render_complete = &self
            .render_complete
            .as_ref()
            .expect("frame executor completion semaphores remain live")[slot];
        let queue = renderer.queue();
        unsafe {
            queue
                .submit_with_binary_signals_at(
                    submission,
                    [command_buffer],
                    acquire_waits,
                    std::slice::from_ref(&render_complete),
                )
                .map_err(VulkanFrameError::SharedSubmit)?;
        }
        self.sampler_upload_pending = false;
        self.completion_retire_values[slot] = frame.timeline_value;
        let fd = unsafe {
            self.render_complete
                .as_ref()
                .expect("frame executor completion semaphores remain live")[slot]
                .export_sync_fd()
        }
        .map_err(|source| VulkanFrameError::ExportSyncFd(source.to_string()))?;
        Ok(fd)
    }

    pub(super) fn destroy(&mut self) {
        self.pipelines.clear();
        self.cursor_pipelines.clear();
        self.focus_ring_pipelines.clear();
        drop(self.render_complete.take());
    }

    fn ensure_client_pipeline(
        &mut self,
        renderer: &RendererDevice,
        format: TextureFormat,
    ) -> Result<(), VulkanFrameError> {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.pipelines.entry(format) {
            let pipeline =
                ClientImagePipeline::new(renderer, format).map_err(VulkanFrameError::Pipeline)?;
            entry.insert(pipeline);
        }
        Ok(())
    }

    fn cursor_pipeline_for(
        &mut self,
        renderer: &RendererDevice,
        format: TextureFormat,
    ) -> Result<&CursorPipeline, VulkanFrameError> {
        match self.cursor_pipelines.entry(format) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let pipeline = CursorPipeline::new(renderer, format)
                    .map_err(VulkanFrameError::CursorPipeline)?;
                Ok(entry.insert(pipeline))
            }
        }
    }

    fn focus_ring_pipeline_for(
        &mut self,
        renderer: &RendererDevice,
        format: TextureFormat,
    ) -> Result<&FocusRingPipeline, VulkanFrameError> {
        match self.focus_ring_pipelines.entry(format) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let pipeline = FocusRingPipeline::new(renderer, format)
                    .map_err(VulkanFrameError::FocusRingPipeline)?;
                Ok(entry.insert(pipeline))
            }
        }
    }
}

#[derive(Debug, Error)]
pub(super) enum VulkanFrameError {
    #[error("no reusable frame-completion semaphore is available")]
    NoCompletionSemaphore,
    #[error("shared descriptor heap operation failed: {0}")]
    DescriptorHeap(#[source] RendererError),
    #[error("frame draw preparation failed: {0}")]
    Record(String),
    #[error("client image pipeline creation failed: {0}")]
    Pipeline(TensorPipelineError),
    #[error("cursor pipeline creation failed: {0}")]
    CursorPipeline(TensorPipelineError),
    #[error("focus-ring pipeline creation failed: {0}")]
    FocusRingPipeline(TensorPipelineError),
    #[error("frame expected {expected} client image descriptors, got {found}")]
    DescriptorImageCountMismatch { expected: u32, found: usize },
    #[error("scene references client image {0:?}, but its Vulkan import is unavailable")]
    MissingClientImage(crate::ecs::SurfaceBufferId),
    #[error("shared renderer rejected Tensor's frame queue submission: {0}")]
    SharedSubmit(#[source] RendererError),
    #[error(
        "frame timeline {frame} does not match the shared renderer submission timeline {submission}"
    )]
    SubmissionTimelineMismatch { frame: u64, submission: u64 },
    #[error("shared renderer failed to export the frame completion SYNC_FD: {0}")]
    ExportSyncFd(String),
    #[error("shared renderer failed to create a frame completion semaphore: {0}")]
    CreateCompletionSemaphore(String),
    #[error("shared renderer failed to record Tensor's frame command buffer: {0}")]
    CommandEncoder(#[source] RendererError),
    #[error("shared renderer rejected Tensor's dynamic-rendering command stream: {0}")]
    SharedRecord(#[source] RendererError),
}

impl VulkanFrameError {
    pub(super) const fn was_submitted(&self) -> bool {
        matches!(self, Self::ExportSyncFd(_))
    }

    pub(super) const fn is_device_lost(&self) -> bool {
        matches!(
            self,
            Self::DescriptorHeap(RendererError::Vulkan {
                source: vk::ErrorCode::DEVICE_LOST,
                ..
            }) | Self::CommandEncoder(RendererError::Vulkan {
                source: vk::ErrorCode::DEVICE_LOST,
                ..
            }) | Self::SharedRecord(RendererError::Vulkan {
                source: vk::ErrorCode::DEVICE_LOST,
                ..
            }) | Self::SharedSubmit(RendererError::Vulkan {
                source: vk::ErrorCode::DEVICE_LOST,
                ..
            })
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_submission_mismatch_is_not_reported_as_submitted() {
        let error = VulkanFrameError::SubmissionTimelineMismatch {
            frame: 7,
            submission: 8,
        };
        assert!(!error.was_submitted());
    }
}

use std::{
    collections::{HashMap, hash_map::Entry},
    os::fd::OwnedFd,
};

use thiserror::Error;
use vulkan_renderer::{
    BinarySemaphore, BinarySemaphoreDescriptor, CommandEncoderDescriptor, DescriptorAllocation,
    DescriptorHeap, DescriptorHeapUploadBatch, Device as RendererDevice, Error as RendererError,
    Extent2D, FrameToken, HeapDescriptorType, MemoryAllocator, RetainedColorTargetPool,
    RetainedColorTargetPoolDescriptor, SampledImageDescriptor, SampledImageDescriptorWriteBatch,
    SamplerBinding, SamplerDescriptor, SemaphoreWait, TextureFormat, TextureLayout,
};

use crate::render::{
    FrameSubmission, OutputCaptureRequest, OutputCaptureResult,
    frame::{BACKDROP_INTERMEDIATE_LANE_COUNT, CompositionPath},
};

use super::{
    capture::{CaptureReadbackManager, PreparedCaptureTap},
    import::{ClientImageCache, ClientImageInfo, ClientImportError},
    pipeline::{
        BackdropFilterPipeline, ClientImagePipeline, CursorPipeline, FocusRingPipeline,
        ManagedClientImagePipeline, ShadowPipeline, TensorPipelineError,
    },
    target::NativeOutputImageInfo,
};

mod record;
use record::{
    BackdropScenePlanError, BackdropSceneRecord, BackdropSceneScratch, CaptureRecord,
    SceneBarrierScratch, SceneRecord, descriptor_index, prepare_cursor_draws, prepare_draws,
    prepare_focus_ring_draws, prepare_scene_draws, prepare_shadow_draws, record_backdrop_scene,
    record_scene,
};

const FRAME_COMPLETION_SEMAPHORE_COUNT: usize = 3;
const MAX_BACKDROP_TARGETS: usize =
    FRAME_COMPLETION_SEMAPHORE_COUNT * BACKDROP_INTERMEDIATE_LANE_COUNT;
const MAX_BACKDROP_RETAINED_BYTES: u64 = 768 * 1024 * 1024;
const MAX_BACKDROP_EXTENT: Extent2D = Extent2D::new(8_192, 8_192);

pub(super) struct FrameExecution<'a> {
    pub(super) frame: &'a FrameSubmission,
    pub(super) descriptor_allocation: &'a DescriptorAllocation,
    pub(super) submission: FrameToken,
    pub(super) output: &'a NativeOutputImageInfo,
    pub(super) client_image_cache: &'a ClientImageCache,
    pub(super) acquire_waits: &'a [SemaphoreWait],
    pub(super) completed_value: u64,
    pub(super) capture: Option<OutputCaptureRequest>,
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
    backdrop_scene: BackdropSceneScratch,
    pipelines: HashMap<TextureFormat, ClientImagePipeline>,
    managed_pipelines: HashMap<TextureFormat, ManagedClientImagePipeline>,
    cursor_pipelines: HashMap<TextureFormat, CursorPipeline>,
    focus_ring_pipelines: HashMap<TextureFormat, FocusRingPipeline>,
    shadow_pipelines: HashMap<TextureFormat, ShadowPipeline>,
    backdrop_filter_pipelines: HashMap<TextureFormat, BackdropFilterPipeline>,
    // The pool is allocation-lazy: ordinary direct frames retain no backdrop
    // images and never enter its acquisition path.
    backdrop_targets: RetainedColorTargetPool,
    capture_readback: CaptureReadbackManager,
}

impl VulkanFrameExecutor {
    pub(super) fn new(
        renderer: &RendererDevice,
        resource_heap: DescriptorHeap,
        sampler_heap: DescriptorHeap,
        intermediate_allocator: MemoryAllocator,
        bootstrap_pipeline_format: TextureFormat,
    ) -> Result<Self, VulkanFrameError> {
        let graphics_queue_family = renderer.queues().graphics_family;
        debug_assert_eq!(record::DRAW_PUSH_DATA_SIZE, 64);
        debug_assert_eq!(record::MANAGED_DRAW_PUSH_DATA_SIZE, 128);
        debug_assert_eq!(record::CURSOR_PUSH_DATA_SIZE, 16);
        debug_assert_eq!(record::FOCUS_RING_PUSH_DATA_SIZE, 64);
        debug_assert_eq!(record::SHADOW_PUSH_DATA_SIZE, 64);
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
            backdrop_scene: BackdropSceneScratch::new(),
            pipelines: HashMap::new(),
            managed_pipelines: HashMap::new(),
            cursor_pipelines: HashMap::new(),
            focus_ring_pipelines: HashMap::new(),
            shadow_pipelines: HashMap::new(),
            backdrop_filter_pipelines: HashMap::new(),
            backdrop_targets: RetainedColorTargetPool::new(
                intermediate_allocator.clone(),
                backdrop_target_pool_descriptor(),
            )
            .map_err(VulkanFrameError::CreateBackdropPool)?,
            capture_readback: CaptureReadbackManager::new(intermediate_allocator)
                .map_err(VulkanFrameError::CreateCapturePool)?,
        };
        if let Err(error) = executor.prepare_output_format(renderer, bootstrap_pipeline_format) {
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
            capture,
        } = execution;
        if submission.value() != frame.timeline_value {
            return Err(VulkanFrameError::SubmissionTimelineMismatch {
                frame: frame.timeline_value,
                submission: submission.value(),
            });
        }
        let backdrops = match frame.pass_plan.path() {
            CompositionPath::DirectSinglePass => None,
            CompositionPath::BackdropDependentMultiPass(backdrops) => {
                let _filter_pipeline = self
                    .backdrop_filter_pipelines
                    .get(&image.format)
                    .ok_or(VulkanFrameError::BackdropPipelineUnavailable(image.format))?
                    .pipeline();
                Some(backdrops.as_slice())
            }
        };
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
        for descriptor in client_ids {
            let image = client_image_cache
                .image_info(descriptor.buffer_id, descriptor.view_encoding)
                .map_err(VulkanFrameError::ClientImageView)?
                .ok_or(VulkanFrameError::MissingClientImage(descriptor.buffer_id))?;
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
        let draws = prepare_draws(
            frame,
            self.descriptor_stride,
            self.linear_sampler_index,
            |descriptor| {
                usize::try_from(descriptor.checked_sub(1)?)
                    .ok()
                    .and_then(|index| self.resolved_client_images.get(index))
                    .map(|image| image.texture_format)
            },
        )
        .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let cursors =
            prepare_cursor_draws(frame, self.descriptor_stride, self.linear_sampler_index)
                .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let focus_rings = prepare_focus_ring_draws(frame)
            .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let shadows = prepare_shadow_draws(frame)
            .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let scene_draws = prepare_scene_draws(
            frame.draw_plan.scene_draws(),
            &draws,
            &shadows,
            &focus_rings,
        )
        .map_err(|error| VulkanFrameError::Record(error.to_string()))?;
        let needs_client_pipeline =
            !draws.is_empty() || cursors.has_textures() || backdrops.is_some();
        let needs_managed_pipeline = draws.iter().any(|draw| draw.is_color_managed());
        if needs_client_pipeline {
            self.ensure_client_pipeline(renderer, image.format)?;
        }
        if needs_managed_pipeline {
            self.ensure_managed_client_pipeline(renderer, image.format)?;
        }
        if cursors.has_vectors() {
            self.cursor_pipeline_for(renderer, image.format)?;
        }
        if !focus_rings.is_empty() {
            self.focus_ring_pipeline_for(renderer, image.format)?;
        }
        if !shadows.is_empty() {
            self.shadow_pipeline_for(renderer, image.format)?;
        }
        if let Some(backdrops) = backdrops {
            self.backdrop_scene
                .prepare(&scene_draws, backdrops)
                .map_err(VulkanFrameError::BackdropScenePlan)?;
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
        let managed_client_pipeline = if needs_managed_pipeline {
            Some(
                self.managed_pipelines
                    .get(&image.format)
                    .expect("managed client pipeline was created before recording")
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
        let shadow_pipeline = if shadows.is_empty() {
            None
        } else {
            let pipeline = self
                .shadow_pipelines
                .get(&image.format)
                .expect("shadow pipeline was created before recording");
            Some(pipeline.pipeline())
        };
        let client_images = self.resolved_client_images.as_slice();
        let mut backdrop_reservations = None;
        let mut capture_tap = match capture {
            Some(request) => Some(
                self.capture_readback
                    .prepare(request, image.format, completed_value)
                    .map_err(VulkanFrameError::CapturePool)?,
            ),
            None => None,
        };
        let command_buffer = (|| {
            self.sampled_descriptors.clear();
            self.sampled_descriptors.reserve(
                1 + client_images.len() + backdrops.map_or(0, |_| BACKDROP_INTERMEDIATE_LANE_COUNT),
            );
            self.sampled_descriptors
                .push(image.sampled_descriptor.clone());
            self.sampled_descriptors.extend(
                client_images
                    .iter()
                    .map(|client| client.sampled_descriptor.clone()),
            );
            let backdrop_recording = if let Some(backdrops) = backdrops {
                let intermediate = frame
                    .pass_plan
                    .intermediate()
                    .expect("multi-pass frames carry retained intermediate requirements");
                let requests = intermediate.retained_target_requests(image.format);
                let first_relative = frame
                    .client_image_descriptors
                    .checked_add(1)
                    .ok_or(VulkanFrameError::BackdropDescriptorIndexOverflow)?;
                let lane_descriptor_indices = [
                    descriptor_index(frame.descriptors, self.descriptor_stride, first_relative)
                        .map_err(|error| VulkanFrameError::Record(error.to_string()))?,
                    descriptor_index(
                        frame.descriptors,
                        self.descriptor_stride,
                        first_relative
                            .checked_add(1)
                            .ok_or(VulkanFrameError::BackdropDescriptorIndexOverflow)?,
                    )
                    .map_err(|error| VulkanFrameError::Record(error.to_string()))?,
                ];
                let acquired = self
                    .backdrop_targets
                    .acquire_batch(requests, completed_value)
                    .map_err(VulkanFrameError::BackdropPool)?;
                let lanes = acquired.targets;
                backdrop_reservations = Some(acquired.reservations());
                self.sampled_descriptors.extend(
                    lanes
                        .iter()
                        .map(|lane| SampledImageDescriptor::from_image_view(lane.view)),
                );
                Some((
                    backdrops,
                    lanes,
                    lane_descriptor_indices,
                    requests[0].extent,
                ))
            } else {
                None
            };
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
                if let Some((backdrops, lanes, lane_descriptor_indices, intermediate_extent)) =
                    backdrop_recording
                {
                    let filter_pipeline = self
                        .backdrop_filter_pipelines
                        .get(&image.format)
                        .expect("backdrop pipeline was prepared before target acquisition")
                        .pipeline();
                    record_backdrop_scene(
                        &mut encoder,
                        BackdropSceneRecord {
                            frame,
                            output: image,
                            clients: client_images,
                            client_pipeline: client_pipeline
                                .expect("multi-pass composition uses the sampled-image pipeline"),
                            managed_client_pipeline,
                            shadow_pipeline,
                            focus_ring_pipeline,
                            cursor_pipeline,
                            filter_pipeline,
                            graphics_queue_family: self.graphics_queue_family,
                            scene_draws: &scene_draws,
                            cursors: &cursors,
                            backdrops,
                            lanes,
                            lane_descriptor_indices,
                            sampler_index: self.linear_sampler_index,
                            intermediate_extent,
                            capture: capture_tap.as_ref().map(capture_record),
                        },
                        &mut self.scene_barriers,
                        &self.backdrop_scene,
                    )
                    .map_err(VulkanFrameError::SharedRecord)?;
                } else {
                    record_scene(
                        &mut encoder,
                        SceneRecord {
                            frame,
                            output: image,
                            clients: client_images,
                            client_pipeline,
                            managed_client_pipeline,
                            shadow_pipeline,
                            focus_ring_pipeline,
                            cursor_pipeline,
                            graphics_queue_family: self.graphics_queue_family,
                            scene_draws: &scene_draws,
                            cursors: &cursors,
                            capture: capture_tap.as_ref().map(capture_record),
                        },
                        &mut self.scene_barriers,
                    )
                    .map_err(VulkanFrameError::SharedRecord)?;
                }
            }
            encoder.finish().map_err(VulkanFrameError::CommandEncoder)
        })();
        let command_buffer = match command_buffer {
            Ok(command_buffer) => command_buffer,
            Err(error) => {
                if let Some(reservations) = backdrop_reservations {
                    self.backdrop_targets
                        .release_batch(reservations)
                        .expect("fresh backdrop reservations remain valid before submission");
                }
                if let Some(capture) = capture_tap.take() {
                    self.capture_readback.release(capture);
                }
                return Err(error);
            }
        };

        let render_complete = &self
            .render_complete
            .as_ref()
            .expect("frame executor completion semaphores remain live")[slot];
        let queue = renderer.queue();
        if let Err(error) = unsafe {
            queue.submit_with_binary_signals_at(
                submission,
                [command_buffer],
                acquire_waits,
                std::slice::from_ref(&render_complete),
            )
        } {
            if let Some(reservations) = backdrop_reservations {
                self.backdrop_targets
                    .release_batch(reservations)
                    .expect("rejected submission leaves backdrop reservations reusable");
            }
            if let Some(capture) = capture_tap.take() {
                self.capture_readback.release(capture);
            }
            return Err(VulkanFrameError::SharedSubmit(error));
        }
        self.sampler_upload_pending = false;
        self.completion_retire_values[slot] = frame.timeline_value;
        if let Some(capture) = capture_tap.take() {
            self.capture_readback.frame_submitted(
                renderer,
                capture,
                image.format,
                frame.timeline_value,
            );
        }
        if let Some(reservations) = backdrop_reservations {
            self.backdrop_targets
                .retire_batch(reservations, frame.timeline_value)
                .map_err(VulkanFrameError::BackdropRetireAfterSubmit)?;
        }
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
        self.managed_pipelines.clear();
        self.cursor_pipelines.clear();
        self.focus_ring_pipelines.clear();
        self.shadow_pipelines.clear();
        self.backdrop_filter_pipelines.clear();
        drop(self.render_complete.take());
    }

    pub(super) fn drain_completed_captures(
        &mut self,
        completed_timeline: u64,
    ) -> Vec<OutputCaptureResult> {
        self.capture_readback.drain_completed(completed_timeline)
    }

    pub(super) fn reject_capture(&mut self, request: OutputCaptureRequest, reason: String) {
        self.capture_readback.reject(request, reason);
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

    fn ensure_managed_client_pipeline(
        &mut self,
        renderer: &RendererDevice,
        format: TextureFormat,
    ) -> Result<(), VulkanFrameError> {
        if let Entry::Vacant(entry) = self.managed_pipelines.entry(format) {
            let pipeline = ManagedClientImagePipeline::new(renderer, format)
                .map_err(VulkanFrameError::Pipeline)?;
            entry.insert(pipeline);
        }
        Ok(())
    }

    pub(super) fn prepare_output_format(
        &mut self,
        renderer: &RendererDevice,
        format: TextureFormat,
    ) -> Result<(), VulkanFrameError> {
        self.ensure_client_pipeline(renderer, format)?;
        self.ensure_managed_client_pipeline(renderer, format)?;
        self.cursor_pipeline_for(renderer, format)?;
        self.focus_ring_pipeline_for(renderer, format)?;
        self.shadow_pipeline_for(renderer, format)?;
        self.backdrop_filter_pipeline_for(renderer, format)?;
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

    fn shadow_pipeline_for(
        &mut self,
        renderer: &RendererDevice,
        format: TextureFormat,
    ) -> Result<&ShadowPipeline, VulkanFrameError> {
        match self.shadow_pipelines.entry(format) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let pipeline = ShadowPipeline::new(renderer, format)
                    .map_err(VulkanFrameError::ShadowPipeline)?;
                Ok(entry.insert(pipeline))
            }
        }
    }

    fn backdrop_filter_pipeline_for(
        &mut self,
        renderer: &RendererDevice,
        format: TextureFormat,
    ) -> Result<&BackdropFilterPipeline, VulkanFrameError> {
        match self.backdrop_filter_pipelines.entry(format) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let pipeline = BackdropFilterPipeline::new(renderer, format)
                    .map_err(VulkanFrameError::BackdropFilterPipeline)?;
                Ok(entry.insert(pipeline))
            }
        }
    }
}

fn capture_record(capture: &PreparedCaptureTap) -> CaptureRecord<'_> {
    CaptureRecord {
        request: capture.request,
        image: &capture.image,
    }
}

fn backdrop_target_pool_descriptor() -> RetainedColorTargetPoolDescriptor {
    RetainedColorTargetPoolDescriptor {
        label: Some("tensor-backdrop-intermediate".into()),
        max_targets: MAX_BACKDROP_TARGETS,
        max_retained_bytes: MAX_BACKDROP_RETAINED_BYTES,
        max_extent: MAX_BACKDROP_EXTENT,
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
    #[error("shadow pipeline creation failed: {0}")]
    ShadowPipeline(TensorPipelineError),
    #[error("backdrop intermediate pool creation failed: {0}")]
    CreateBackdropPool(#[source] RendererError),
    #[error("capture target pool creation failed: {0}")]
    CreateCapturePool(#[source] RendererError),
    #[error("capture target acquisition failed: {0}")]
    CapturePool(#[source] RendererError),
    #[error("backdrop intermediate target acquisition failed: {0}")]
    BackdropPool(#[source] RendererError),
    #[error("backdrop descriptor index overflowed")]
    BackdropDescriptorIndexOverflow,
    #[error("submitted backdrop targets could not be retired: {0}")]
    BackdropRetireAfterSubmit(#[source] RendererError),
    #[error("backdrop filter pipeline creation failed: {0}")]
    BackdropFilterPipeline(TensorPipelineError),
    #[error("backdrop filter pipeline for output format {0:?} was not prepared at registration")]
    BackdropPipelineUnavailable(TextureFormat),
    #[error("backdrop scene-order lowering failed: {0}")]
    BackdropScenePlan(#[source] BackdropScenePlanError),
    #[error("frame expected {expected} client image descriptors, got {found}")]
    DescriptorImageCountMismatch { expected: u32, found: usize },
    #[error("scene references client image {0:?}, but its Vulkan import is unavailable")]
    MissingClientImage(crate::ecs::SurfaceBufferId),
    #[error("client image has no view compatible with its committed color state: {0}")]
    ClientImageView(#[source] ClientImportError),
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
        matches!(
            self,
            Self::ExportSyncFd(_) | Self::BackdropRetireAfterSubmit(_)
        )
    }

    pub(super) const fn is_device_lost(&self) -> bool {
        match self {
            Self::DescriptorHeap(error)
            | Self::BackdropPool(error)
            | Self::CapturePool(error)
            | Self::CommandEncoder(error)
            | Self::SharedRecord(error)
            | Self::SharedSubmit(error) => error.is_device_lost(),
            _ => false,
        }
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

    #[test]
    fn post_submit_backdrop_retirement_is_reported_as_submitted() {
        let error = VulkanFrameError::BackdropRetireAfterSubmit(RendererError::Validation(
            "stale reservation".into(),
        ));
        assert!(error.was_submitted());
    }

    #[test]
    fn backdrop_pool_is_bounded_for_two_lanes_and_three_in_flight_frames() {
        let descriptor = backdrop_target_pool_descriptor();
        assert_eq!(descriptor.max_targets, 6);
        assert_eq!(descriptor.max_retained_bytes, 768 * 1024 * 1024);
        assert_eq!(descriptor.max_extent, Extent2D::new(8_192, 8_192));
    }
}

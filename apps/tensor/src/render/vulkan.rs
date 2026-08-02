#![allow(unsafe_code)]

#[cfg(feature = "tty")]
use std::{
    collections::BTreeMap,
    os::fd::{AsFd, OwnedFd},
    sync::Arc,
};

use tensor_host::Fourcc;
use tracing::{debug, info};
use vulkan_renderer::vulkanalia::{Version, vk};
use vulkan_renderer::{
    BackendProfile, DeviceDescriptor, Features, Instance as RendererInstance, InstanceDescriptor,
    Limits, ROADMAP_2026_API_VERSION, TextureFormat,
};
#[cfg(feature = "tty")]
use vulkan_renderer::{
    DescriptorHeapDescriptor, DescriptorHeapKind, DescriptorHeapMemory, HeapDescriptorType,
    MemoryAllocator, MemoryAllocatorConfig,
};

#[cfg(feature = "tty")]
use super::{
    CursorOverlays, Dmabuf, FrameScheduler, FrameSubmission, NativeCursorTarget,
    NativeOutputTarget, RenderOutputId,
};
use super::{
    DescriptorHeapProperties, DeviceSelectionError, DrmDeviceIdentity, DrmNodeId,
    NativeInteropCapabilities, RendererTarget, VulkanFormatCapability,
};
#[cfg(feature = "tty")]
use crate::ecs::SurfaceBufferId;

mod error;
#[cfg(feature = "tty")]
mod frame;
#[cfg(feature = "tty")]
mod import;
#[cfg(feature = "tty")]
mod pipeline;
mod probe;
#[cfg(feature = "tty")]
mod sync;
#[cfg(feature = "tty")]
mod target;

pub(crate) use error::RendererError;
use probe::probe_devices;

#[cfg(feature = "tty")]
pub(crate) use sync::ClientReleaseFence;
#[cfg(feature = "tty")]
pub(crate) use target::{NativeCursorBuffer, NativeOutputBuffer, NativeOutputBuffers};

#[cfg(feature = "tty")]
const DESCRIPTOR_HEAP_BYTES: u64 = 16 * 1024 * 1024;

#[cfg(feature = "tty")]
const MAX_PENDING_SYNC_FDS_PER_OUTPUT: usize = 3;

const OUTPUT_FORMATS: &[(Fourcc, vk::Format)] = &[
    (Fourcc::XRGB8888, vk::Format::B8G8R8A8_SRGB),
    (Fourcc::ARGB8888, vk::Format::B8G8R8A8_SRGB),
    (Fourcc::XBGR8888, vk::Format::R8G8B8A8_SRGB),
    (Fourcc::ABGR8888, vk::Format::R8G8B8A8_SRGB),
    (Fourcc::XRGB2101010, vk::Format::A2R10G10B10_UNORM_PACK32),
    (Fourcc::ARGB2101010, vk::Format::A2R10G10B10_UNORM_PACK32),
    (Fourcc::XBGR2101010, vk::Format::A2B10G10R10_UNORM_PACK32),
    (Fourcc::ABGR2101010, vk::Format::A2B10G10R10_UNORM_PACK32),
];

pub(crate) struct VulkanRenderer {
    #[cfg(feature = "tty")]
    native_targets: target::NativeTargetManager,
    #[cfg(feature = "tty")]
    client_images: import::ClientImageCache,
    _owner: VulkanOwner,
    target: RendererTarget,
    selected: SelectedDevice,
    #[cfg(feature = "tty")]
    frames: FrameScheduler,
    #[cfg(feature = "tty")]
    pending_sync_fds: BTreeMap<(RenderOutputId, u64), OwnedFd>,
    #[cfg(feature = "tty")]
    client_sync: sync::ClientSyncManager,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SelectedDevice {
    pub(crate) name: String,
    pub(crate) api_version: Version,
    pub(crate) device_type: vk::PhysicalDeviceType,
    pub(crate) graphics_queue_family: u32,
    pub(crate) primary_node: DrmNodeId,
    pub(crate) render_node: DrmNodeId,
    pub(crate) interop: NativeInteropCapabilities,
    pub(crate) formats: Vec<VulkanFormatCapability>,
    pub(crate) descriptor_heap: DescriptorHeapProperties,
}

impl VulkanRenderer {
    pub(crate) fn new(target: RendererTarget) -> Result<Self, RendererError> {
        if target.api_version != ROADMAP_2026_API_VERSION {
            return Err(RendererError::UnsupportedRendererProfile {
                required: ROADMAP_2026_API_VERSION,
                requested: target.api_version,
            });
        }
        let instance = RendererInstance::new(InstanceDescriptor {
            profile: BackendProfile::Roadmap2026,
            extra_instance_extensions: Vec::new(),
        })
        .map_err(|source| RendererError::Probe(source.to_string()))?;
        let probed = probe_devices(&instance)?;
        for device in &probed {
            debug!(
                ordinal = device.candidate.ordinal,
                name = device.candidate.name,
                api = %device.candidate.api_version,
                device_type = ?device.candidate.device_type,
                descriptor_heap = device.candidate.descriptor_heap_supported,
                sampler_heap_alignment = device.candidate.descriptor_heap.sampler_heap_alignment,
                descriptor_heap_alignment = device.candidate.descriptor_heap.resource_heap_alignment,
                sampler_heap_max = device.candidate.descriptor_heap.max_sampler_heap_size,
                descriptor_heap_max = device.candidate.descriptor_heap.max_resource_heap_size,
                sampler_heap_reserved = device.candidate.descriptor_heap.min_sampler_heap_reserved_range_with_embedded,
                descriptor_heap_reserved = device.candidate.descriptor_heap.min_resource_heap_reserved_range,
                sampler_descriptor_size = device.candidate.descriptor_heap.sampler_descriptor_size,
                sampler_descriptor_alignment = device.candidate.descriptor_heap.sampler_descriptor_alignment,
                buffer_descriptor_alignment = device.candidate.descriptor_heap.buffer_descriptor_alignment,
                image_descriptor_size = device.candidate.descriptor_heap.image_descriptor_size,
                image_descriptor_alignment = device.candidate.descriptor_heap.image_descriptor_alignment,
                max_push_data_size = device.candidate.descriptor_heap.max_push_data_size,
                max_embedded_samplers = device.candidate.descriptor_heap.max_descriptor_heap_embedded_samplers,
                buffer_device_address = device.candidate.buffer_device_address_supported,
                timeline_semaphore = device.candidate.timeline_semaphore_supported,
                dynamic_rendering = device.candidate.dynamic_rendering_supported,
                maintenance5 = device.candidate.maintenance5_supported,
                graphics_queue_family = ?device.candidate.graphics_queue_family,
                native_output_formats = device.candidate.native_output_format_count,
                "shared Vulkan renderer adapter probed"
            );
        }
        let selected_ordinal = target
            .device
            .select(probed.iter().map(|device| &device.candidate))?
            .ordinal;
        let selected = probed
            .into_iter()
            .find(|device| device.candidate.ordinal == selected_ordinal)
            .ok_or_else(|| {
                RendererError::Probe("selected Vulkan adapter disappeared during setup".into())
            })?;
        #[cfg(feature = "tty")]
        let bootstrap_output_format = selected
            .formats
            .iter()
            .copied()
            .find(|format| format.supports_output_export())
            .ok_or_else(|| {
                RendererError::Frame(
                    "the selected device has no exportable format for the client pipeline".into(),
                )
            })?;
        #[cfg(feature = "tty")]
        let bootstrap_client_pipeline_format =
            texture_format_for_fourcc(bootstrap_output_format.format.code).ok_or_else(|| {
                RendererError::Frame(
                    "the selected device has no typed exportable format for the client pipeline"
                        .into(),
                )
            })?;
        let graphics_queue_family = selected
            .candidate
            .graphics_queue_family
            .ok_or(DeviceSelectionError::MissingGraphicsQueue)?;
        let (primary_node, render_node) = selected
            .candidate
            .drm
            .and_then(DrmDeviceIdentity::node_pair)
            .ok_or(DeviceSelectionError::MissingDrmNodePair)?;
        #[cfg(feature = "tty")]
        let native_targets = target::NativeTargetManager::new(render_node);
        let (backend, _queue) = selected
            .adapter
            .request_device(DeviceDescriptor {
                label: Some("tensor-compositor".into()),
                required_features: Features::STANDARD_DEFAULTS
                    | Features::EXTERNAL_MEMORY_DMA_BUF
                    | Features::EXTERNAL_SEMAPHORE_SYNC_FD,
                required_limits: Limits::downlevel_defaults(),
                required_extensions: Vec::new(),
                video_decode: None,
            })
            .map_err(|source| RendererError::CreateSharedDevice(source.to_string()))?;
        #[cfg(feature = "tty")]
        let client_image_allocator = backend
            .create_memory_allocator(client_image_allocator_config())
            .map_err(|source| RendererError::CreateFrameResources(source.to_string()))?;
        #[cfg(feature = "tty")]
        let (resource_heap, sampler_heap, frames) = {
            let properties = selected.candidate.descriptor_heap;
            let descriptor_capacity =
                preferred_resource_heap_capacity(properties).ok_or_else(|| {
                    RendererError::Frame(
                    "selected device has no usable device-local resource descriptor heap capacity"
                        .into(),
                )
                })?;
            let resource_heap = backend
                .create_descriptor_heap_with_memory(
                    &DescriptorHeapDescriptor {
                        label: Some("tensor-resource-descriptor-heap".into()),
                        kind: DescriptorHeapKind::Resource,
                        descriptor_capacity,
                        embedded_samplers: false,
                    },
                    DescriptorHeapMemory::DeviceLocal,
                )
                .map_err(|source| RendererError::CreateFrameResources(source.to_string()))?;
            let descriptor_stride = resource_heap
                .allocation_stride(HeapDescriptorType::SampledImage)
                .map_err(|error| RendererError::CreateFrameResources(error.to_string()))?;
            let descriptor_alignment = resource_heap
                .allocation_alignment(HeapDescriptorType::SampledImage)
                .map_err(|error| RendererError::CreateFrameResources(error.to_string()))?;
            let frames = FrameScheduler::with_descriptor_allocator(
                resource_heap.allocator(),
                descriptor_stride,
                descriptor_alignment,
            )
            .map_err(|error| RendererError::Frame(error.to_string()))?;
            let sampler_capacity = backend
                .descriptor_heap_capacity_bytes(DescriptorHeapKind::Sampler, 1)
                .map_err(|source| RendererError::CreateFrameResources(source.to_string()))?;
            let sampler_heap = backend
                .create_descriptor_heap_with_memory(
                    &DescriptorHeapDescriptor {
                        label: Some("tensor-sampler-descriptor-heap".into()),
                        kind: DescriptorHeapKind::Sampler,
                        descriptor_capacity: sampler_capacity,
                        embedded_samplers: true,
                    },
                    DescriptorHeapMemory::DeviceLocal,
                )
                .map_err(|source| RendererError::CreateFrameResources(source.to_string()))?;
            (resource_heap, sampler_heap, frames)
        };
        #[cfg(feature = "tty")]
        let frame_executor = {
            if let Err(source) = pipeline::validate_shader_modules(&backend) {
                return Err(RendererError::ValidateClientShaders(source.to_string()));
            }
            frame::VulkanFrameExecutor::new(
                &backend,
                resource_heap,
                sampler_heap,
                bootstrap_client_pipeline_format,
            )
            .map_err(|source| RendererError::CreateFrameResources(source.to_string()))?
        };
        let selected_info = SelectedDevice {
            name: selected.candidate.name.clone(),
            api_version: selected.candidate.api_version,
            device_type: selected.candidate.device_type,
            graphics_queue_family,
            primary_node,
            render_node,
            interop: selected.candidate.interop,
            formats: selected.formats.clone(),
            descriptor_heap: selected.candidate.descriptor_heap,
        };
        let client_import_formats = selected_info
            .formats
            .iter()
            .filter(|format| format.supports_client_import())
            .count();
        let output_export_formats = selected_info
            .formats
            .iter()
            .filter(|format| format.supports_output_export())
            .count();
        info!(
            name = selected_info.name,
            api = %selected_info.api_version,
            device_type = ?selected_info.device_type,
            graphics_queue_family,
            primary_node = %selected_info.primary_node,
            render_node = %selected_info.render_node,
            descriptor_heap = true,
            sampler_heap_alignment = selected_info.descriptor_heap.sampler_heap_alignment,
            descriptor_heap_alignment = selected_info.descriptor_heap.resource_heap_alignment,
            sampler_heap_max = selected_info.descriptor_heap.max_sampler_heap_size,
            descriptor_heap_max = selected_info.descriptor_heap.max_resource_heap_size,
            sampler_heap_reserved = selected_info.descriptor_heap.min_sampler_heap_reserved_range_with_embedded,
            descriptor_heap_reserved = selected_info.descriptor_heap.min_resource_heap_reserved_range,
            sampler_descriptor_size = selected_info.descriptor_heap.sampler_descriptor_size,
            sampler_descriptor_alignment = selected_info.descriptor_heap.sampler_descriptor_alignment,
            buffer_descriptor_alignment = selected_info.descriptor_heap.buffer_descriptor_alignment,
            image_descriptor_size = selected_info.descriptor_heap.image_descriptor_size,
            image_descriptor_alignment = selected_info.descriptor_heap.image_descriptor_alignment,
            max_push_data_size = selected_info.descriptor_heap.max_push_data_size,
            max_embedded_samplers = selected_info.descriptor_heap.max_descriptor_heap_embedded_samplers,
            buffer_device_address = true,
            dynamic_rendering = true,
            maintenance5 = true,
            dma_buf = selected_info.interop.dma_buf_memory,
            drm_format_modifier = selected_info.interop.drm_format_modifier,
            foreign_queue_family = selected_info.interop.foreign_queue_family,
            sync_fd = selected_info.interop.sync_fd_semaphore,
            client_import_formats,
            output_export_formats,
            "Vulkanalia renderer device initialized"
        );

        Ok(Self {
            #[cfg(feature = "tty")]
            native_targets,
            #[cfg(feature = "tty")]
            client_images: import::ClientImageCache::default(),
            _owner: VulkanOwner {
                #[cfg(feature = "tty")]
                frame_executor,
                #[cfg(feature = "tty")]
                client_image_allocator,
                backend,
            },
            target,
            selected: selected_info,
            #[cfg(feature = "tty")]
            frames,
            #[cfg(feature = "tty")]
            pending_sync_fds: BTreeMap::new(),
            #[cfg(feature = "tty")]
            client_sync: sync::ClientSyncManager::default(),
        })
    }

    pub(crate) const fn target(&self) -> RendererTarget {
        self.target
    }

    pub(crate) fn selected(&self) -> &SelectedDevice {
        &self.selected
    }

    #[cfg(feature = "tty")]
    pub(crate) fn register_output(
        &mut self,
        target: NativeOutputTarget,
        cursor: Option<NativeCursorTarget>,
    ) -> Result<NativeOutputBuffers, RendererError> {
        for format in std::iter::once(target.format).chain(cursor.map(|cursor| cursor.format)) {
            let supported = self.selected.formats.iter().copied().any(|candidate| {
                candidate.format == format.format
                    && candidate.plane_count == format.plane_count
                    && candidate.supports_output_export()
            });
            if !supported {
                return Err(RendererError::UnsupportedOutputTarget {
                    format: format.format.code,
                    modifier: format.format.modifier.raw(),
                    plane_count: format.plane_count,
                });
            }
        }
        self.refresh_completed()?;
        let buffers = self
            .native_targets
            .register(&self._owner.backend, target, cursor)
            .map_err(|error| RendererError::NativeTarget(error.to_string()))?;
        self.frames
            .register_output(target)
            .map_err(|error| RendererError::Frame(error.to_string()))?;
        Ok(buffers)
    }

    #[cfg(feature = "tty")]
    pub(crate) fn unregister_output(&mut self, output: RenderOutputId) {
        self.frames.unregister_output(output);
        self.native_targets.unregister(output);
        self.pending_sync_fds
            .retain(|(pending_output, _), _| *pending_output != output);
    }

    #[cfg(feature = "tty")]
    pub(crate) fn client_import_formats(&self) -> Vec<tensor_host::DrmFormat> {
        self.selected
            .formats
            .iter()
            .copied()
            .filter(|format| format.plane_count == 1 && format.supports_client_import())
            .map(|format| format.format)
            .collect()
    }

    #[cfg(feature = "tty")]
    pub(crate) fn import_client_dmabuf<F: AsFd>(
        &mut self,
        id: SurfaceBufferId,
        dmabuf: &Dmabuf<F>,
    ) -> Result<(), RendererError> {
        self.refresh_completed()?;
        self.client_images
            .import(id, &self._owner.backend, dmabuf)
            .map_err(|error| RendererError::ClientImport(error.to_string()))
    }

    #[cfg(feature = "tty")]
    pub(crate) fn upload_client_shm(
        &mut self,
        id: SurfaceBufferId,
        size: tensor_util::Size,
        format: Fourcc,
        fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
    ) -> Result<(), RendererError> {
        let completed = self.refresh_completed()?;
        self.client_images
            .upload_shm(
                &self._owner.client_image_allocator,
                import::ShmUploadTarget {
                    id,
                    size,
                    format,
                    completed_timeline: completed,
                },
                fill,
            )
            .map_err(|error| RendererError::ClientImport(error.to_string()))
    }

    #[cfg(feature = "tty")]
    pub(crate) fn import_client_acquire(
        &mut self,
        surface: crate::ecs::SurfaceId,
        fd: OwnedFd,
    ) -> Result<(), RendererError> {
        self.client_sync
            .import_acquire(&self._owner.backend, surface, fd)
            .map_err(|error| RendererError::ClientSync(error.to_string()))
    }

    #[cfg(feature = "tty")]
    pub(crate) fn finish_client_sync(
        &mut self,
        surface: crate::ecs::SurfaceId,
        completed_timeline: u64,
    ) -> ClientReleaseFence {
        self.client_sync.finish(surface, completed_timeline)
    }

    #[cfg(feature = "tty")]
    pub(crate) fn completed_timeline(&self) -> Result<u64, RendererError> {
        self._owner
            .backend
            .completed_timeline()
            .map_err(|source| RendererError::QueryTimeline(source.to_string()))
    }

    #[cfg(feature = "tty")]
    pub(crate) fn refresh_completed(&mut self) -> Result<u64, RendererError> {
        let completed = match self._owner.backend.completed_timeline() {
            Ok(value) => value,
            Err(source) => {
                // Any timeline-query failure makes completion ownership
                // unknowable. Fail closed instead of polling forever or
                // recycling GPU-visible resources after a transient-looking
                // Vulkan error.
                self.frames.mark_device_lost();
                return Err(RendererError::QueryTimeline(source.to_string()));
            }
        };
        self.retire_completed(completed);
        Ok(completed)
    }

    #[cfg(feature = "tty")]
    fn retire_completed(&mut self, completed: u64) {
        self.frames.retire_completed(completed);
        self.native_targets.retire_completed(completed);
        self.client_images.retire_completed(completed);
        self._owner.client_image_allocator.trim();
        self.client_sync.retire_completed(completed);
    }

    #[cfg(feature = "tty")]
    pub(crate) fn release_client_image(&mut self, id: SurfaceBufferId) {
        self.client_images.release(id);
    }

    #[cfg(feature = "tty")]
    pub(crate) fn take_sync_fd(
        &mut self,
        output: RenderOutputId,
        timeline_value: u64,
    ) -> Option<OwnedFd> {
        self.pending_sync_fds.remove(&(output, timeline_value))
    }

    pub(crate) fn output_count(&self) -> usize {
        #[cfg(not(feature = "tty"))]
        return 0;
        #[cfg(feature = "tty")]
        self.frames.output_count()
    }

    #[cfg(feature = "tty")]
    pub(crate) fn next_output_slot(&self, output: RenderOutputId) -> Option<u8> {
        self.frames.next_output_slot(output)
    }

    #[cfg(feature = "tty")]
    pub(crate) fn advance_output_slot(&mut self, output: RenderOutputId) -> Option<u8> {
        self.frames.advance_output_slot(output)
    }

    #[cfg(feature = "tty")]
    pub(crate) fn submit_scene(
        &mut self,
        output: RenderOutputId,
        scene: crate::scene::SceneSnapshot,
        cursors: CursorOverlays,
    ) -> Result<FrameSubmission, RendererError> {
        let completed = self.refresh_completed()?;
        // Tensor's compositor thread owns frame scheduling. Reserve its
        // timeline value from the renderer before descriptor allocation so
        // the product cannot create a second timeline namespace.
        let submission = self
            ._owner
            .backend
            .next_frame()
            .map_err(|source| RendererError::ReserveFrame(source.to_string()))?;
        let frame = self
            .frames
            .prepare_with_cursors_for_timeline(
                output,
                scene,
                cursors,
                completed,
                submission.value(),
            )
            .map_err(|error| RendererError::Frame(error.to_string()))?;
        let client_ids = frame.draw_plan.images();
        debug!(
            output = ?output,
            serial = frame.serial,
            timeline = frame.timeline_value,
            draws = frame.draw_plan.draws().len(),
            unique_client_images = client_ids.len(),
            surface_contents = frame.scene.contents().len(),
            damage_regions = frame.damage.regions().len(),
            "prepared Vulkan scene frame"
        );
        let tracked_surfaces = self.client_sync.tracked_surface_ids(&frame);
        let acquire_waits = self.client_sync.acquire_waits(&tracked_surfaces);
        let image = self
            .native_targets
            .image_info(output, frame.output_slot)
            .ok_or_else(|| {
                let _ = self.frames.abort(&frame);
                RendererError::NativeTarget(format!(
                    "output {output:?} has no native image slot {}",
                    frame.output_slot
                ))
            })?;
        let descriptor_allocation = self
            .frames
            .descriptor_allocation(&frame)
            .map_err(|error| RendererError::Frame(error.to_string()))?;
        let sync_fd = match self._owner.frame_executor.submit(
            &self._owner.backend,
            frame::FrameExecution {
                frame: &frame,
                descriptor_allocation,
                submission,
                output: &image,
                client_image_cache: &self.client_images,
                acquire_waits: &acquire_waits,
                completed_value: completed,
            },
        ) {
            Ok(fd) => fd,
            Err(frame::VulkanFrameError::MissingClientImage(id)) => {
                let _ = self.frames.abort(&frame);
                return Err(RendererError::MissingClientImage(id));
            }
            Err(source) => {
                if source.is_device_lost() {
                    self.frames.mark_device_lost();
                }
                if source.was_submitted() {
                    self.client_images
                        .mark_submitted(client_ids.iter().copied(), frame.timeline_value);
                    self.native_targets.mark_submitted(
                        output,
                        frame.output_slot,
                        frame.timeline_value,
                    );
                    self.client_sync
                        .mark_submitted(&tracked_surfaces, frame.timeline_value, None);
                    if let Err(error) = self.frames.commit(&frame) {
                        self.frames.mark_device_lost();
                        return Err(RendererError::SubmitFrame(format!(
                            "{source:?}; submitted frame state could not commit: {error}"
                        )));
                    }
                } else {
                    let _ = self.frames.abort(&frame);
                }
                return Err(RendererError::SubmitFrame(format!("{source:?}")));
            }
        };
        let (completion, kms_sync_fd) = if tracked_surfaces.is_empty() {
            (None, sync_fd)
        } else {
            let completion = Arc::new(sync_fd);
            let kms_sync_fd = match completion.as_fd().try_clone_to_owned() {
                Ok(fd) => fd,
                Err(error) => {
                    self.client_images
                        .mark_submitted(client_ids.iter().copied(), frame.timeline_value);
                    self.native_targets.mark_submitted(
                        output,
                        frame.output_slot,
                        frame.timeline_value,
                    );
                    self.client_sync.mark_submitted(
                        &tracked_surfaces,
                        frame.timeline_value,
                        Some(completion),
                    );
                    if let Err(commit_error) = self.frames.commit(&frame) {
                        self.frames.mark_device_lost();
                        return Err(RendererError::SubmitFrame(format!(
                            "completion fd duplication failed ({error}); submitted frame state could not commit: {commit_error}"
                        )));
                    }
                    return Err(RendererError::SubmitFrame(format!(
                        "submitted frame completion could not be duplicated for KMS: {error}"
                    )));
                }
            };
            (Some(completion), kms_sync_fd)
        };
        self.client_images
            .mark_submitted(client_ids.iter().copied(), frame.timeline_value);
        self.native_targets
            .mark_submitted(output, frame.output_slot, frame.timeline_value);
        self.client_sync
            .mark_submitted(&tracked_surfaces, frame.timeline_value, completion);
        if let Err(error) = self.frames.commit(&frame) {
            self.frames.mark_device_lost();
            return Err(RendererError::Frame(error.to_string()));
        }
        self.pending_sync_fds
            .insert((output, frame.timeline_value), kms_sync_fd);
        while self
            .pending_sync_fds
            .keys()
            .filter(|(pending_output, _)| *pending_output == output)
            .count()
            > MAX_PENDING_SYNC_FDS_PER_OUTPUT
        {
            let Some(oldest) = self
                .pending_sync_fds
                .keys()
                .find(|(pending_output, _)| *pending_output == output)
                .copied()
            else {
                break;
            };
            self.pending_sync_fds.remove(&oldest);
        }
        Ok(frame)
    }
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        #[cfg(feature = "tty")]
        {
            let _ = self._owner.backend.wait_idle();
            self.client_sync.destroy();
            self.client_images.destroy();
            self.native_targets.destroy();
            self._owner.frame_executor.destroy();
        }
    }
}

struct VulkanOwner {
    #[cfg(feature = "tty")]
    frame_executor: frame::VulkanFrameExecutor,
    #[cfg(feature = "tty")]
    client_image_allocator: MemoryAllocator,
    backend: vulkan_renderer::Device,
}

#[cfg(feature = "tty")]
fn preferred_resource_heap_capacity(properties: DescriptorHeapProperties) -> Option<u64> {
    let alignment = properties.resource_heap_alignment;
    if alignment == 0 || !alignment.is_power_of_two() {
        return None;
    }
    let reserved = align_up(properties.min_resource_heap_reserved_range, alignment)?;
    let available = properties.max_resource_heap_size.checked_sub(reserved)?;
    let requested = DESCRIPTOR_HEAP_BYTES.min(available);
    let capacity = requested & !(alignment - 1);
    (capacity > 0).then_some(capacity)
}

#[cfg(feature = "tty")]
fn align_up(value: u64, alignment: u64) -> Option<u64> {
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
}

#[cfg(feature = "tty")]
fn client_image_allocator_config() -> MemoryAllocatorConfig {
    // Client SHM images are long-lived enough to benefit from retained,
    // persistently mapped storage, but a compositor must not reserve the
    // renderer's large scene-engine pools for a small surface. Empty pools
    // remain bounded at 2 MiB per matching class/type; allocations at that
    // size or above receive exact-size blocks and disappear on trim.
    const MIB: u64 = 1024 * 1024;
    MemoryAllocatorConfig {
        device_block_size: 2 * MIB,
        image_block_size: 2 * MIB,
        upload_block_size: 2 * MIB,
        readback_block_size: 2 * MIB,
        dedicated_threshold: 2 * MIB,
    }
}

fn native_image_usage() -> vk::ImageUsageFlags {
    vk::ImageUsageFlags::COLOR_ATTACHMENT
        | vk::ImageUsageFlags::SAMPLED
        | vk::ImageUsageFlags::TRANSFER_SRC
        | vk::ImageUsageFlags::TRANSFER_DST
}

#[cfg(feature = "tty")]
fn vulkan_format_for_fourcc(fourcc: Fourcc) -> Option<vk::Format> {
    OUTPUT_FORMATS
        .iter()
        .find_map(|(candidate, format)| (*candidate == fourcc).then_some(*format))
}

fn texture_format_for_fourcc(fourcc: Fourcc) -> Option<TextureFormat> {
    match fourcc {
        Fourcc::XRGB8888 | Fourcc::ARGB8888 => Some(TextureFormat::Bgra8Srgb),
        Fourcc::XBGR8888 | Fourcc::ABGR8888 => Some(TextureFormat::Rgba8Srgb),
        Fourcc::XRGB2101010 | Fourcc::ARGB2101010 => Some(TextureFormat::A2R10G10B10UnormPack32),
        Fourcc::XBGR2101010 | Fourcc::ABGR2101010 => Some(TextureFormat::A2B10G10R10UnormPack32),
        _ => None,
    }
}

#[cfg(all(test, feature = "tty"))]
mod tests;

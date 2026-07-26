#![allow(unsafe_code)]

#[cfg(feature = "tty")]
use std::{
    collections::BTreeMap,
    os::fd::{AsFd, OwnedFd},
    sync::Arc,
};

use tensor_host::Fourcc;
use tracing::{debug, info};
use vulkanalia::{
    Device, Entry, Instance, Version,
    loader::{LIBRARY, LibloadingLoader},
    prelude::v1_4::*,
};

#[cfg(feature = "tty")]
use super::{
    CursorOverlay, Dmabuf, FrameScheduler, FrameSubmission, NativeOutputTarget, RenderOutputId,
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
mod heap;
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
pub(crate) use target::NativeOutputBuffer;

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
        let entry = load_entry()?;
        let loader_version = entry.version().map_err(RendererError::LoaderVersion)?;
        if loader_version < target.api_version {
            return Err(RendererError::UnsupportedLoaderVersion {
                required: target.api_version,
                found: loader_version,
            });
        }

        let application = vk::ApplicationInfo::builder()
            .application_name(b"tensor-compositor\0")
            .engine_name(b"tensor-renderer\0")
            .api_version(target.api_version.into());
        let instance_info = vk::InstanceCreateInfo::builder().application_info(&application);
        let instance = unsafe { entry.create_instance(&instance_info, None) }
            .map_err(RendererError::CreateInstance)?;
        let instance = InstanceOwner { entry, instance };

        let probed = probe_devices(&instance.instance)?;
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
                maintenance5_extension = device.maintenance5_extension_available,
                graphics_queue_family = ?device.candidate.graphics_queue_family,
                native_output_formats = device.candidate.native_output_format_count,
                "Vulkan physical device probed"
            );
        }
        let selected = target
            .device
            .select(probed.iter().map(|device| &device.candidate))?;
        let selected = &probed[selected.ordinal];
        #[cfg(feature = "tty")]
        let frame_heap_alignment = selected
            .candidate
            .descriptor_heap
            .resource_heap_alignment
            .max(
                selected
                    .candidate
                    .descriptor_heap
                    .image_descriptor_alignment,
            );
        #[cfg(feature = "tty")]
        let frames = FrameScheduler::new(
            selected
                .candidate
                .descriptor_heap
                .min_resource_heap_reserved_range
                .saturating_add(DESCRIPTOR_HEAP_BYTES)
                .min(selected.candidate.descriptor_heap.max_resource_heap_size),
            frame_heap_alignment,
            selected
                .candidate
                .descriptor_heap
                .min_resource_heap_reserved_range,
            selected.candidate.descriptor_heap.image_descriptor_size,
        )
        .map_err(|error| RendererError::Frame(error.to_string()))?;
        #[cfg(feature = "tty")]
        let frame_heap_layout = frames.layout();
        #[cfg(feature = "tty")]
        let sampler_heap_layout = frame::sampler_heap_layout(selected.candidate.descriptor_heap)
            .map_err(|error| RendererError::Frame(error.to_string()))?;
        #[cfg(feature = "tty")]
        let bootstrap_pipeline_format = selected
            .formats
            .iter()
            .copied()
            .find(|format| format.supports_output_export())
            .and_then(|format| vulkan_format_for_fourcc(format.format.code))
            .ok_or_else(|| {
                RendererError::Frame(
                    "the selected device has no exportable format for the client pipeline".into(),
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
        let device = create_device(
            &instance.instance,
            selected.handle,
            graphics_queue_family,
            selected.maintenance5_extension_available,
        )?;
        #[cfg(feature = "tty")]
        if let Err(source) = pipeline::validate_shader_modules(&device) {
            unsafe { device.destroy_device(None) };
            return Err(RendererError::ValidateClientShaders(source.to_string()));
        }
        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_family, 0) };
        #[cfg(feature = "tty")]
        let frame_executor = match frame::VulkanFrameExecutor::new(
            &instance.instance,
            &device,
            selected.handle,
            graphics_queue_family,
            frame_heap_layout,
            sampler_heap_layout,
            bootstrap_pipeline_format,
        ) {
            Ok(executor) => executor,
            Err(source) => {
                unsafe { device.destroy_device(None) };
                return Err(RendererError::CreateFrameResources(source.to_string()));
            }
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
                device,
                instance,
                _physical_device: selected.handle,
                _graphics_queue: graphics_queue,
                #[cfg(feature = "tty")]
                frame_executor,
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
    ) -> Result<Vec<NativeOutputBuffer>, RendererError> {
        if !self.selected.formats.iter().copied().any(|candidate| {
            candidate.format == target.format.format
                && candidate.plane_count == target.format.plane_count
                && candidate.supports_output_export()
        }) {
            return Err(RendererError::UnsupportedOutputTarget {
                format: target.format.format.code,
                modifier: target.format.format.modifier.raw(),
                plane_count: target.format.plane_count,
            });
        }
        self.refresh_completed()?;
        let buffers = self
            .native_targets
            .register(
                &self._owner.instance.instance,
                &self._owner.device,
                self._owner._physical_device,
                target,
            )
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
            .import(id, &self._owner.device, dmabuf)
            .map_err(|error| RendererError::ClientImport(error.to_string()))
    }

    #[cfg(feature = "tty")]
    pub(crate) fn import_client_acquire(
        &mut self,
        surface: crate::ecs::SurfaceId,
        fd: OwnedFd,
    ) -> Result<(), RendererError> {
        self.client_sync
            .import_acquire(&self._owner.device, surface, fd)
            .map_err(|error| RendererError::ClientSync(error.to_string()))
    }

    #[cfg(feature = "tty")]
    pub(crate) fn finish_client_sync(
        &mut self,
        surface: crate::ecs::SurfaceId,
        completed_timeline: u64,
    ) -> ClientReleaseFence {
        self.client_sync
            .finish(&self._owner.device, surface, completed_timeline)
    }

    #[cfg(feature = "tty")]
    pub(crate) fn completed_timeline(&self) -> Result<u64, RendererError> {
        self._owner
            .frame_executor
            .completed(&self._owner.device)
            .map_err(RendererError::QueryTimeline)
    }

    #[cfg(feature = "tty")]
    pub(crate) fn refresh_completed(&mut self) -> Result<u64, RendererError> {
        let completed = match self._owner.frame_executor.completed(&self._owner.device) {
            Ok(value) => value,
            Err(error) => {
                // Any timeline-query failure makes completion ownership
                // unknowable. Fail closed instead of polling forever or
                // recycling GPU-visible resources after a transient-looking
                // Vulkan error.
                self.frames.mark_device_lost();
                return Err(RendererError::QueryTimeline(error));
            }
        };
        self.retire_completed(completed);
        Ok(completed)
    }

    #[cfg(feature = "tty")]
    fn retire_completed(&mut self, completed: u64) {
        self.frames.retire_completed(completed);
        self.native_targets
            .retire_completed(&self._owner.device, completed);
        self.client_images
            .retire_completed(&self._owner.device, completed);
        self.client_sync
            .retire_completed(&self._owner.device, completed);
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
    pub(crate) fn output_waiting_for_gpu(&self, output: RenderOutputId) -> bool {
        self.frames.output_waiting_for_gpu(output)
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
        cursor: Option<CursorOverlay>,
    ) -> Result<FrameSubmission, RendererError> {
        let completed = self.refresh_completed()?;
        let frame = self
            .frames
            .prepare_with_cursor(output, scene, cursor, completed)
            .map_err(|error| RendererError::Frame(error.to_string()))?;
        let client_ids = frame.draw_plan.images().to_vec();
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
        let client_images = match client_ids
            .iter()
            .map(|id| {
                self.client_images
                    .image_info(*id)
                    .ok_or(RendererError::MissingClientImage(*id))
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(descriptors) => descriptors,
            Err(error) => {
                let _ = self.frames.abort(&frame);
                return Err(error);
            }
        };
        let tracked_surfaces = self.client_sync.tracked_surface_ids(&frame);
        let acquire_semaphores = self.client_sync.wait_semaphores(&tracked_surfaces);
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
        let sync_fd = match self._owner.frame_executor.submit(
            &self._owner.device,
            self._owner._graphics_queue,
            frame::FrameExecution {
                frame: &frame,
                output: &image,
                client_images: &client_images,
                acquire_semaphores: &acquire_semaphores,
                completed_value: completed,
            },
        ) {
            Ok(fd) => fd,
            Err(source) => {
                if matches!(
                    &source,
                    frame::VulkanFrameError::Vulkan(vk::ErrorCode::DEVICE_LOST)
                ) {
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
        unsafe {
            let _ = self._owner.device.device_wait_idle();
            self.client_sync.destroy(&self._owner.device);
        }
        #[cfg(feature = "tty")]
        self.client_images.destroy(&self._owner.device);
        #[cfg(feature = "tty")]
        self.native_targets.destroy(&self._owner.device);
    }
}

struct InstanceOwner {
    entry: Entry,
    instance: Instance,
}

impl Drop for InstanceOwner {
    fn drop(&mut self) {
        unsafe { self.instance.destroy_instance(None) };
        let _ = &self.entry;
    }
}

struct VulkanOwner {
    device: Device,
    instance: InstanceOwner,
    _physical_device: vk::PhysicalDevice,
    _graphics_queue: vk::Queue,
    #[cfg(feature = "tty")]
    frame_executor: frame::VulkanFrameExecutor,
}

impl Drop for VulkanOwner {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            #[cfg(feature = "tty")]
            self.frame_executor.destroy(&self.device);
        }
        unsafe { self.device.destroy_device(None) };
        let _ = &self.instance;
    }
}

fn load_entry() -> Result<Entry, RendererError> {
    // Vulkanalia deliberately exposes loader and dispatch construction as unsafe. The library
    // path is its platform constant and both owners outlive every command loaded from it.
    let loader = unsafe { LibloadingLoader::new(LIBRARY) }
        .map_err(|error| RendererError::LoadLibrary(error.to_string()))?;
    unsafe { Entry::new(loader) }.map_err(|error| RendererError::LoadEntry(error.to_string()))
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

fn create_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    graphics_queue_family: u32,
    maintenance5_extension_available: bool,
) -> Result<Device, RendererError> {
    let priorities = [1.0];
    let queue = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(graphics_queue_family)
        .queue_priorities(&priorities);
    let queues = [queue];
    let mut extensions = vec![
        vk::EXT_DESCRIPTOR_HEAP_EXTENSION.name.as_cstr().as_ptr(),
        vk::KHR_EXTERNAL_MEMORY_FD_EXTENSION.name.as_cstr().as_ptr(),
        vk::EXT_EXTERNAL_MEMORY_DMA_BUF_EXTENSION
            .name
            .as_cstr()
            .as_ptr(),
        vk::EXT_IMAGE_DRM_FORMAT_MODIFIER_EXTENSION
            .name
            .as_cstr()
            .as_ptr(),
        vk::EXT_QUEUE_FAMILY_FOREIGN_EXTENSION
            .name
            .as_cstr()
            .as_ptr(),
        vk::KHR_EXTERNAL_SEMAPHORE_FD_EXTENSION
            .name
            .as_cstr()
            .as_ptr(),
    ];
    if maintenance5_extension_available {
        extensions.push(vk::KHR_MAINTENANCE5_EXTENSION.name.as_cstr().as_ptr());
    }
    let mut descriptor_heap = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::builder()
        .descriptor_heap(true)
        .build();
    let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::builder()
        .buffer_device_address(true)
        .timeline_semaphore(true)
        .build();
    let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::builder()
        .dynamic_rendering(true)
        .build();
    let mut vulkan14 = vk::PhysicalDeviceVulkan14Features::builder()
        .maintenance5(true)
        .build();
    let info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queues)
        .enabled_extension_names(&extensions)
        .push_next(&mut vulkan12)
        .push_next(&mut vulkan13)
        .push_next(&mut vulkan14)
        .push_next(&mut descriptor_heap);
    unsafe { instance.create_device(physical_device, &info, None) }
        .map_err(RendererError::CreateDevice)
}

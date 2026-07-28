use std::collections::BTreeSet;
use std::ffi::CString;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use vulkanalia::{
    Device, Entry, Instance, Version,
    loader::{LIBRARY, LibloadingLoader},
    prelude::v1_4::*,
    vk,
};

use crate::capabilities::{BackendProfile, CoreFeatures, DescriptorHeapLimits, Features, Limits};
use crate::command::{CommandEncoder, CommandEncoderDescriptor};
use crate::frame::FrameToken;
use crate::memory::MemoryTypeInfo;
use crate::queue::{QueueFamilyInfo, QueuePlan};
use crate::roadmap_2026::query_roadmap_2026_device_requirements;
use crate::{Error, Result};

mod retirement;
mod submission;

use retirement::SubmissionRetirement;
use submission::submit_to_graphics_queue;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DevicePreference {
    #[default]
    Discrete,
    Integrated,
    Any,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendConfig {
    pub label: Option<String>,
    pub profile: BackendProfile,
    pub device_preference: DevicePreference,
    /// Window-system extensions, for example `VK_KHR_wayland_surface`.
    pub extra_instance_extensions: Vec<String>,
    /// Feature modules may request extensions beyond the selected profile.
    pub extra_device_extensions: Vec<String>,
    pub required_features: Features,
    pub required_limits: Limits,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            label: None,
            profile: BackendProfile::Vulkan14,
            device_preference: DevicePreference::Discrete,
            extra_instance_extensions: Vec::new(),
            extra_device_extensions: Vec::new(),
            required_features: Features::STANDARD_DEFAULTS,
            required_limits: Limits::downlevel_defaults(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    pub ordinal: usize,
    pub name: String,
    pub api_version: Version,
    pub device_type: vk::PhysicalDeviceType,
    pub vendor_id: u32,
    pub device_id: u32,
    pub features: CoreFeatures,
    pub supported_features: Features,
    pub limits: Limits,
    pub memory_types: Vec<MemoryTypeInfo>,
    pub non_coherent_atom_size: u64,
    pub queues: QueuePlan,
    pub queue_families: Vec<QueueFamilyInfo>,
    pub extensions: BTreeSet<String>,
    pub roadmap_2026_ready: bool,
    pub roadmap_2026_failures: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct DeviceQueues {
    pub graphics: vk::Queue,
    pub compute: vk::Queue,
    pub transfer: vk::Queue,
}

/// Cloneable submission endpoint. Like `wgpu::Queue`, this keeps the logical
/// device alive independently of the `Device` handle returned beside it.
#[derive(Clone)]
pub struct Queue {
    pub(crate) owner: Arc<DeviceOwner>,
    submission_retirement: Arc<SubmissionRetirement>,
}

#[derive(Clone, Copy, Debug)]
pub struct SemaphoreWait {
    pub semaphore: vk::Semaphore,
    /// Zero for a binary semaphore, otherwise the required timeline value.
    pub value: u64,
    pub stages: vk::PipelineStageFlags2,
}

pub struct Backend {
    config: BackendConfig,
    info: DeviceInfo,
    owner: Arc<DeviceOwner>,
    submission_retirement: Arc<SubmissionRetirement>,
}

impl fmt::Debug for Backend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Backend")
            .field("config", &self.config)
            .field("info", &self.info)
            .field("queues", &self.owner.queues)
            .field("command_pool", &self.owner.command_pool)
            .field("timeline", &self.owner.timeline)
            .field(
                "completed_timeline",
                &self.owner.completed_timeline.load(Ordering::Acquire),
            )
            .finish_non_exhaustive()
    }
}

impl Backend {
    pub fn new(mut config: BackendConfig) -> Result<Self> {
        if config.profile == BackendProfile::Roadmap2026 {
            config.required_features |= Features::FIFO_LATEST_READY;
        }
        config.required_features = config.required_features.with_dependencies();
        let entry = load_entry()?;
        let loader_version = entry
            .version()
            .map_err(|source| Error::vulkan("vkEnumerateInstanceVersion", source))?;
        let required_version = config.profile.required_api_version();
        if loader_version < required_version {
            return Err(Error::LoaderVersion {
                required: required_version,
                found: loader_version,
            });
        }

        let instance_extensions = extension_union(
            config.profile.required_instance_extensions(),
            &config.extra_instance_extensions,
        );
        let instance = create_instance(entry, required_version, &instance_extensions)?;
        let candidates = probe_devices(&instance.instance)?;
        if candidates.is_empty() {
            return Err(Error::NoPhysicalDevice);
        }
        let mut feature_extensions = config.extra_device_extensions.clone();
        append_feature_extensions(config.required_features, &mut feature_extensions);
        let required_device_extensions = extension_union(
            config.profile.required_device_extensions(),
            &feature_extensions,
        );
        let (candidate, rejections) = select_device(
            candidates,
            config.profile,
            config.device_preference,
            &required_device_extensions,
            config.required_features,
            config.required_limits,
        );
        let candidate = candidate.ok_or(Error::NoCompatibleDevice(rejections))?;
        Self::from_selected(config, instance, candidate, &required_device_extensions)
    }

    pub(crate) fn from_selected(
        config: BackendConfig,
        instance: Arc<InstanceOwner>,
        candidate: Candidate,
        required_device_extensions: &[String],
    ) -> Result<Self> {
        let device = create_device(
            &instance.instance,
            &candidate,
            required_device_extensions,
            config.required_features,
        )?;
        let queues = DeviceQueues {
            graphics: unsafe { device.get_device_queue(candidate.info.queues.graphics, 0) },
            compute: unsafe { device.get_device_queue(candidate.info.queues.compute, 0) },
            transfer: unsafe { device.get_device_queue(candidate.info.queues.transfer, 0) },
        };
        let command_pool_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(candidate.info.queues.graphics);
        let command_pool = match unsafe { device.create_command_pool(&command_pool_info, None) } {
            Ok(pool) => pool,
            Err(source) => {
                unsafe { device.destroy_device(None) };
                return Err(Error::vulkan("vkCreateCommandPool", source));
            }
        };
        let mut timeline_type = vk::SemaphoreTypeCreateInfo::builder()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0);
        let timeline_info = vk::SemaphoreCreateInfo::builder().push_next(&mut timeline_type);
        let timeline = match unsafe { device.create_semaphore(&timeline_info, None) } {
            Ok(semaphore) => semaphore,
            Err(source) => {
                unsafe {
                    device.destroy_command_pool(command_pool, None);
                    device.destroy_device(None);
                }
                return Err(Error::vulkan("vkCreateSemaphore(timeline)", source));
            }
        };

        let max_push_data_size = candidate.info.limits.descriptor_heap.max_push_data_size;
        let owner = Arc::new(DeviceOwner {
            device,
            instance,
            physical_device: candidate.handle,
            queues,
            max_push_data_size,
            command_pool,
            timeline,
            next_timeline: AtomicU64::new(1),
            completed_timeline: AtomicU64::new(0),
            submit_lock: Mutex::new(()),
            command_pool_lock: Mutex::new(()),
            pending_command_buffers: Mutex::new(Vec::new()),
        });
        let submission_retirement = Arc::new(SubmissionRetirement::new(Arc::clone(&owner)));
        Ok(Self {
            config,
            info: candidate.info,
            owner,
            submission_retirement,
        })
    }

    pub fn config(&self) -> &BackendConfig {
        &self.config
    }

    pub fn label(&self) -> Option<&str> {
        self.config.label.as_deref()
    }

    pub fn device_info(&self) -> &DeviceInfo {
        &self.info
    }

    pub fn features(&self) -> Features {
        self.config.required_features
    }

    pub fn limits(&self) -> Limits {
        self.config.required_limits
    }

    pub fn device(&self) -> &Device {
        &self.owner.device
    }

    pub fn instance(&self) -> &Instance {
        &self.owner.instance.instance
    }

    pub fn queues(&self) -> DeviceQueues {
        self.owner.queues
    }

    pub fn queue(&self) -> Queue {
        Queue {
            owner: Arc::clone(&self.owner),
            submission_retirement: Arc::clone(&self.submission_retirement),
        }
    }

    pub(crate) fn shared_owner(&self) -> Arc<DeviceOwner> {
        Arc::clone(&self.owner)
    }

    pub fn command_pool(&self) -> vk::CommandPool {
        self.owner.command_pool
    }

    pub fn timeline_semaphore(&self) -> vk::Semaphore {
        self.owner.timeline
    }

    pub fn next_frame(&self) -> Result<FrameToken> {
        self.owner.allocate_frame()
    }

    pub fn completed_timeline(&self) -> Result<u64> {
        self.queue().completed_timeline()
    }

    pub fn wait_for(&self, frame: FrameToken, timeout_ns: u64) -> Result<()> {
        self.queue().wait_for(frame, timeout_ns)
    }

    pub fn wait_idle(&self) -> Result<()> {
        self.queue().wait_idle()
    }

    pub fn allocate_primary_command_buffer(&self) -> Result<vk::CommandBuffer> {
        self.owner.allocate_primary_command_buffer()
    }

    /// Creates and begins a primary, one-time-submit command encoder.
    pub fn create_command_encoder(
        &self,
        descriptor: &CommandEncoderDescriptor,
    ) -> Result<CommandEncoder> {
        CommandEncoder::new(Arc::clone(&self.owner), descriptor)
    }

    /// Submits command buffers and signals this backend's timeline. State
    /// owners should commit resource/frame bookkeeping only after this returns
    /// successfully.
    ///
    /// # Safety
    ///
    /// Every command buffer must be executable, belong to this device and its
    /// graphics command-pool family, and remain alive until `frame` completes.
    pub unsafe fn submit_raw(
        &self,
        frame: FrameToken,
        command_buffers: &[vk::CommandBuffer],
        waits: &[SemaphoreWait],
    ) -> Result<()> {
        submit_to_graphics_queue(&self.owner, frame, command_buffers, waits)
    }

    /// The caller must ensure every command buffer has completed before it is
    /// freed. Pair this with the frame timeline value used for submission.
    ///
    /// # Safety
    ///
    /// Each command buffer must have been allocated from this backend's command
    /// pool and must no longer be pending or referenced by any host thread.
    pub unsafe fn free_command_buffers(&self, command_buffers: &[vk::CommandBuffer]) {
        self.owner.free_command_buffers(command_buffers);
    }
}

pub(crate) struct InstanceOwner {
    entry: Entry,
    pub(crate) instance: Instance,
}

impl Drop for InstanceOwner {
    fn drop(&mut self) {
        unsafe { self.instance.destroy_instance(None) };
        let _ = &self.entry;
    }
}

pub(crate) struct DeviceOwner {
    pub(crate) device: Device,
    instance: Arc<InstanceOwner>,
    physical_device: vk::PhysicalDevice,
    queues: DeviceQueues,
    pub(crate) max_push_data_size: u64,
    command_pool: vk::CommandPool,
    timeline: vk::Semaphore,
    next_timeline: AtomicU64,
    completed_timeline: AtomicU64,
    submit_lock: Mutex<()>,
    command_pool_lock: Mutex<()>,
    pending_command_buffers: Mutex<Vec<(u64, Vec<vk::CommandBuffer>)>>,
}

impl DeviceOwner {
    pub(crate) fn instance_owner(&self) -> &Arc<InstanceOwner> {
        &self.instance
    }

    pub(crate) fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    pub(crate) fn timeline(&self) -> vk::Semaphore {
        self.timeline
    }

    fn allocate_frame(&self) -> Result<FrameToken> {
        self.next_timeline
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |next| {
                next.checked_add(1)
            })
            .map(FrameToken::from_value)
            .map_err(|_| Error::TimelineExhausted)
    }

    fn retire_timeline(&self, completed: u64) {
        self.completed_timeline
            .fetch_max(completed, Ordering::AcqRel);
    }

    pub(crate) fn allocate_primary_command_buffer(&self) -> Result<vk::CommandBuffer> {
        let _pool_guard = self
            .command_pool_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        unsafe { self.device.allocate_command_buffers(&info) }
            .map_err(|source| Error::vulkan("vkAllocateCommandBuffers", source))?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Validation("Vulkan returned no command buffer".into()))
    }

    pub(crate) fn free_command_buffers(&self, command_buffers: &[vk::CommandBuffer]) {
        if command_buffers.is_empty() {
            return;
        }
        let _pool_guard = self
            .command_pool_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            self.device
                .free_command_buffers(self.command_pool, command_buffers)
        };
    }

    fn retire_command_buffers_after(
        &self,
        frame: FrameToken,
        command_buffers: Vec<vk::CommandBuffer>,
    ) {
        if command_buffers.is_empty() {
            return;
        }
        self.pending_command_buffers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((frame.value(), command_buffers));
        // A waiter may observe completion between vkQueueSubmit2 returning and
        // this retirement entry being installed. Recheck the cached value so
        // that race cannot strand the command buffers until another poll.
        let completed = self.completed_timeline.load(Ordering::Acquire);
        if completed >= frame.value() {
            self.retire_completed_command_buffers(completed);
        }
    }

    fn retire_completed_command_buffers(&self, completed: u64) {
        let retired = {
            let mut pending = self
                .pending_command_buffers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut retired = Vec::new();
            let mut still_pending = Vec::with_capacity(pending.len());
            for (timeline, command_buffers) in pending.drain(..) {
                if timeline <= completed {
                    retired.extend(command_buffers);
                } else {
                    still_pending.push((timeline, command_buffers));
                }
            }
            *pending = still_pending;
            retired
        };
        self.free_command_buffers(&retired);
    }
}

impl Drop for DeviceOwner {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_semaphore(self.timeline, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
        }
        let _ = &self.instance;
    }
}

#[derive(Clone)]
pub(crate) struct Candidate {
    pub(crate) handle: vk::PhysicalDevice,
    pub(crate) info: DeviceInfo,
}

pub(crate) fn load_entry() -> Result<Entry> {
    let loader = unsafe { LibloadingLoader::new(LIBRARY) }
        .map_err(|error| Error::LoadLibrary(error.to_string()))?;
    unsafe { Entry::new(loader) }.map_err(|error| Error::LoadEntry(error.to_string()))
}

pub(crate) fn create_instance(
    entry: Entry,
    api_version: Version,
    extension_names: &[String],
) -> Result<Arc<InstanceOwner>> {
    let extension_names = c_strings(extension_names)?;
    let extension_pointers = extension_names
        .iter()
        .map(|name| name.as_ptr())
        .collect::<Vec<_>>();
    let application = vk::ApplicationInfo::builder()
        .application_name(b"vulkan-renderer\0")
        .engine_name(b"vulkan-renderer\0")
        .api_version(api_version.into());
    let info = vk::InstanceCreateInfo::builder()
        .application_info(&application)
        .enabled_extension_names(&extension_pointers);
    let instance = unsafe { entry.create_instance(&info, None) }
        .map_err(|source| Error::vulkan("vkCreateInstance", source))?;
    Ok(Arc::new(InstanceOwner { entry, instance }))
}

pub(crate) fn probe_devices(instance: &Instance) -> Result<Vec<Candidate>> {
    let handles = unsafe { instance.enumerate_physical_devices() }
        .map_err(|source| Error::vulkan("vkEnumeratePhysicalDevices", source))?;
    handles
        .into_iter()
        .enumerate()
        .map(|(ordinal, handle)| {
            let properties = unsafe { instance.get_physical_device_properties(handle) };
            let queue_families =
                unsafe { instance.get_physical_device_queue_family_properties(handle) }
                    .into_iter()
                    .enumerate()
                    .map(|(index, family)| QueueFamilyInfo {
                        index: index as u32,
                        queue_count: family.queue_count,
                        flags: family.queue_flags,
                    })
                    .collect::<Vec<_>>();
            let queues = QueuePlan::select(&queue_families).unwrap_or(QueuePlan {
                graphics: u32::MAX,
                compute: u32::MAX,
                transfer: u32::MAX,
            });
            let extensions: BTreeSet<String> =
                unsafe { instance.enumerate_device_extension_properties(handle, None) }
                    .map_err(|source| {
                        Error::vulkan("vkEnumerateDeviceExtensionProperties", source)
                    })?
                    .into_iter()
                    .map(|extension| extension.extension_name.to_string_lossy().into_owned())
                    .collect();
            let (features, descriptor_heap) = query_features(instance, handle, &extensions);
            let extension_names = extensions.iter().cloned().collect::<Vec<_>>();
            let roadmap = query_roadmap_2026_device_requirements(
                instance,
                handle,
                properties.api_version,
                &extension_names,
            );
            let roadmap_2026_ready = roadmap.ready();
            let mut roadmap_2026_failures = Vec::new();
            if !roadmap.api_version_ready {
                roadmap_2026_failures.push("apiVersion".into());
            }
            roadmap_2026_failures.extend(
                roadmap
                    .missing_device_extensions
                    .iter()
                    .map(|name| format!("extension:{name}")),
            );
            roadmap_2026_failures.extend(
                roadmap
                    .missing_core_features
                    .iter()
                    .map(|name| format!("feature:{name}")),
            );
            roadmap_2026_failures.extend(
                roadmap
                    .missing_properties
                    .iter()
                    .map(|name| format!("property:{name}")),
            );
            roadmap_2026_failures.extend(
                roadmap
                    .missing_extension_features
                    .iter()
                    .map(|name| format!("extension-feature:{name}")),
            );
            let supported_features = Features::from_core(features);
            let limits = Limits {
                max_image_dimension_2d: properties.limits.max_image_dimension_2d,
                max_memory_allocation_count: properties.limits.max_memory_allocation_count,
                max_bound_descriptor_sets: properties.limits.max_bound_descriptor_sets,
                max_push_constants_size: properties.limits.max_push_constants_size,
                descriptor_heap,
            };
            let memory_properties =
                unsafe { instance.get_physical_device_memory_properties(handle) };
            let memory_types = (0..memory_properties.memory_type_count)
                .map(|index| {
                    let memory_type = memory_properties.memory_types[index as usize];
                    let heap = memory_properties.memory_heaps[memory_type.heap_index as usize];
                    MemoryTypeInfo {
                        index,
                        heap_index: memory_type.heap_index,
                        properties: memory_type.property_flags,
                        heap_size: heap.size,
                    }
                })
                .collect();
            Ok(Candidate {
                handle,
                info: DeviceInfo {
                    ordinal,
                    name: properties.device_name.to_string_lossy().into_owned(),
                    api_version: Version::from(properties.api_version),
                    device_type: properties.device_type,
                    vendor_id: properties.vendor_id,
                    device_id: properties.device_id,
                    features,
                    supported_features,
                    limits,
                    memory_types,
                    non_coherent_atom_size: properties.limits.non_coherent_atom_size,
                    queues,
                    queue_families,
                    extensions,
                    roadmap_2026_ready,
                    roadmap_2026_failures,
                },
            })
        })
        .collect()
}

fn query_features(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    extensions: &BTreeSet<String>,
) -> (CoreFeatures, DescriptorHeapLimits) {
    let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut vulkan14 = vk::PhysicalDeviceVulkan14Features::default();
    let mut features = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut vulkan12)
        .push_next(&mut vulkan13)
        .push_next(&mut vulkan14);
    unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
    let descriptor_heap_available = extensions.contains("VK_EXT_descriptor_heap");
    let descriptor_heap = if descriptor_heap_available {
        let mut extension = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::default();
        let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
        unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
        extension.descriptor_heap != 0
    } else {
        false
    };
    let present_mode_fifo_latest_ready =
        if extensions.contains("VK_KHR_present_mode_fifo_latest_ready") {
            let mut extension = vk::PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR::default();
            let mut features = vk::PhysicalDeviceFeatures2::builder().push_next(&mut extension);
            unsafe { instance.get_physical_device_features2(physical_device, &mut features) };
            extension.present_mode_fifo_latest_ready != 0
        } else {
            false
        };
    let descriptor_heap_limits = if descriptor_heap_available {
        query_descriptor_heap_limits(instance, physical_device)
    } else {
        DescriptorHeapLimits::default()
    };
    let external_memory_dma_buf = [
        "VK_KHR_external_memory_fd",
        "VK_EXT_external_memory_dma_buf",
        "VK_EXT_image_drm_format_modifier",
        "VK_EXT_queue_family_foreign",
    ]
    .iter()
    .all(|extension| extensions.contains(*extension));
    let external_semaphore_sync_fd = extensions.contains("VK_KHR_external_semaphore_fd")
        && supports_sync_fd_semaphore(instance, physical_device);
    (
        CoreFeatures {
            timeline_semaphore: vulkan12.timeline_semaphore != 0,
            buffer_device_address: vulkan12.buffer_device_address != 0,
            synchronization2: vulkan13.synchronization2 != 0,
            dynamic_rendering: vulkan13.dynamic_rendering != 0,
            maintenance5: vulkan14.maintenance5 != 0,
            maintenance6: vulkan14.maintenance6 != 0,
            dynamic_rendering_local_read: vulkan14.dynamic_rendering_local_read != 0,
            descriptor_heap,
            present_mode_fifo_latest_ready,
            external_memory_dma_buf,
            external_semaphore_sync_fd,
        },
        descriptor_heap_limits,
    )
}

fn supports_sync_fd_semaphore(instance: &Instance, physical_device: vk::PhysicalDevice) -> bool {
    let info = vk::PhysicalDeviceExternalSemaphoreInfo::builder()
        .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
    let mut properties = vk::ExternalSemaphoreProperties::default();
    unsafe {
        instance.get_physical_device_external_semaphore_properties(
            physical_device,
            &info,
            &mut properties,
        )
    };
    properties
        .compatible_handle_types
        .contains(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
        && properties.external_semaphore_features.contains(
            vk::ExternalSemaphoreFeatureFlags::IMPORTABLE
                | vk::ExternalSemaphoreFeatureFlags::EXPORTABLE,
        )
}

fn query_descriptor_heap_limits(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> DescriptorHeapLimits {
    let mut heap = vk::PhysicalDeviceDescriptorHeapPropertiesEXT::default();
    let mut properties = vk::PhysicalDeviceProperties2::builder().push_next(&mut heap);
    unsafe { instance.get_physical_device_properties2(physical_device, &mut properties) };
    DescriptorHeapLimits {
        sampler_heap_alignment: heap.sampler_heap_alignment,
        resource_heap_alignment: heap.resource_heap_alignment,
        max_sampler_heap_size: heap.max_sampler_heap_size,
        max_resource_heap_size: heap.max_resource_heap_size,
        min_sampler_heap_reserved_range: heap.min_sampler_heap_reserved_range,
        min_sampler_heap_reserved_range_with_embedded: heap
            .min_sampler_heap_reserved_range_with_embedded,
        min_resource_heap_reserved_range: heap.min_resource_heap_reserved_range,
        sampler_descriptor_size: heap.sampler_descriptor_size,
        image_descriptor_size: heap.image_descriptor_size,
        buffer_descriptor_size: heap.buffer_descriptor_size,
        sampler_descriptor_alignment: heap.sampler_descriptor_alignment,
        image_descriptor_alignment: heap.image_descriptor_alignment,
        buffer_descriptor_alignment: heap.buffer_descriptor_alignment,
        max_push_data_size: heap.max_push_data_size,
        max_embedded_samplers: heap.max_descriptor_heap_embedded_samplers,
    }
}

pub(crate) fn select_device(
    candidates: Vec<Candidate>,
    profile: BackendProfile,
    preference: DevicePreference,
    required_extensions: &[String],
    required_features: Features,
    required_limits: Limits,
) -> (Option<Candidate>, Vec<String>) {
    let mut eligible = Vec::new();
    let mut rejected = Vec::new();
    for candidate in candidates {
        let mut reasons = candidate.info.features.rejection_reasons(
            candidate.info.api_version,
            profile,
            &candidate.info.extensions,
        );
        if candidate.info.queues.graphics == u32::MAX {
            reasons.push("missing graphics queue".into());
        }
        if profile == BackendProfile::Roadmap2026
            && !candidate.info.features.present_mode_fifo_latest_ready
        {
            reasons.push("missing presentModeFifoLatestReady feature".into());
        }
        if profile == BackendProfile::Roadmap2026 && !candidate.info.roadmap_2026_ready {
            reasons.push(format!(
                "VP_KHR_roadmap_2026 revision 11 failures: {}",
                candidate.info.roadmap_2026_failures.join(", ")
            ));
        }
        let missing_features = required_features.difference(candidate.info.supported_features);
        if missing_features != Features::empty() {
            reasons.push(format!(
                "missing required feature bits {missing_features:?}"
            ));
        }
        if required_features.contains(Features::DESCRIPTOR_HEAP)
            && !candidate.info.limits.descriptor_heap.is_usable()
        {
            reasons.push("VK_EXT_descriptor_heap properties are not usable".into());
        }
        reasons.extend(required_limits.failures_against(candidate.info.limits));
        let missing_extra = required_extensions
            .iter()
            .filter(|required| !candidate.info.extensions.contains(required.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !missing_extra.is_empty() {
            reasons.push(format!(
                "missing requested extensions: {}",
                missing_extra.join(", ")
            ));
        }
        if reasons.is_empty() {
            eligible.push(candidate);
        } else {
            rejected.push(format!("{}: {}", candidate.info.name, reasons.join(" | ")));
        }
    }
    eligible.sort_by_key(|candidate| {
        (
            device_rank(preference, candidate.info.device_type),
            candidate.info.ordinal,
        )
    });
    (eligible.into_iter().next(), rejected)
}

fn create_device(
    instance: &Instance,
    candidate: &Candidate,
    extension_names: &[String],
    required_features: Features,
) -> Result<Device> {
    let priorities = [1.0f32];
    let queue_infos = candidate
        .info
        .queues
        .unique_families()
        .into_iter()
        .map(|family| {
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(family)
                .queue_priorities(&priorities)
                .build()
        })
        .collect::<Vec<_>>();
    let mut enabled_extension_names = extension_names.to_vec();
    if required_features.contains(Features::DESCRIPTOR_HEAP)
        && candidate.info.extensions.contains("VK_KHR_maintenance5")
    {
        enabled_extension_names.push("VK_KHR_maintenance5".into());
        enabled_extension_names.sort();
        enabled_extension_names.dedup();
    }
    let extension_names = c_strings(&enabled_extension_names)?;
    let extension_pointers = extension_names
        .iter()
        .map(|name| name.as_ptr())
        .collect::<Vec<_>>();
    let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::builder()
        .timeline_semaphore(true)
        .buffer_device_address(required_features.contains(Features::BUFFER_DEVICE_ADDRESS))
        .build();
    let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::builder()
        .synchronization2(true)
        .dynamic_rendering(true)
        .build();
    let mut vulkan14 = vk::PhysicalDeviceVulkan14Features::builder()
        .maintenance5(true)
        .maintenance6(required_features.contains(Features::MAINTENANCE6))
        .dynamic_rendering_local_read(
            required_features.contains(Features::DYNAMIC_RENDERING_LOCAL_READ),
        )
        .build();
    let mut descriptor_heap = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::builder()
        .descriptor_heap(required_features.contains(Features::DESCRIPTOR_HEAP))
        .build();
    let mut fifo_latest_ready = vk::PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR::builder()
        .present_mode_fifo_latest_ready(required_features.contains(Features::FIFO_LATEST_READY))
        .build();
    let mut info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&extension_pointers)
        .push_next(&mut vulkan12)
        .push_next(&mut vulkan13)
        .push_next(&mut vulkan14);
    if extension_names
        .iter()
        .any(|name| name.as_bytes() == b"VK_EXT_descriptor_heap")
    {
        info = info.push_next(&mut descriptor_heap);
    }
    if extension_names
        .iter()
        .any(|name| name.as_bytes() == b"VK_KHR_present_mode_fifo_latest_ready")
    {
        info = info.push_next(&mut fifo_latest_ready);
    }
    unsafe { instance.create_device(candidate.handle, &info, None) }
        .map_err(|source| Error::vulkan("vkCreateDevice", source))
}

pub(crate) fn append_feature_extensions(features: Features, extensions: &mut Vec<String>) {
    if features.contains(Features::DESCRIPTOR_HEAP) {
        extensions.push("VK_EXT_descriptor_heap".into());
    }
    if features.contains(Features::FIFO_LATEST_READY) {
        extensions.push("VK_KHR_swapchain".into());
        extensions.push("VK_KHR_present_mode_fifo_latest_ready".into());
    }
    if features.contains(Features::EXTERNAL_MEMORY_DMA_BUF) {
        extensions.push("VK_KHR_external_memory_fd".into());
        extensions.push("VK_EXT_external_memory_dma_buf".into());
        extensions.push("VK_EXT_image_drm_format_modifier".into());
        extensions.push("VK_EXT_queue_family_foreign".into());
    }
    if features.contains(Features::EXTERNAL_SEMAPHORE_SYNC_FD) {
        extensions.push("VK_KHR_external_semaphore_fd".into());
    }
}

pub(crate) fn extension_union(required: &[&str], extra: &[String]) -> Vec<String> {
    required
        .iter()
        .map(|extension| (*extension).to_owned())
        .chain(extra.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn c_strings(names: &[String]) -> Result<Vec<CString>> {
    names
        .iter()
        .map(|name| {
            CString::new(name.as_str()).map_err(|_| {
                Error::NoCompatibleDevice(vec![format!(
                    "extension name contains an interior NUL: {name:?}"
                )])
            })
        })
        .collect()
}

fn device_rank(preference: DevicePreference, device_type: vk::PhysicalDeviceType) -> u8 {
    let discrete = device_type == vk::PhysicalDeviceType::DISCRETE_GPU;
    let integrated = device_type == vk::PhysicalDeviceType::INTEGRATED_GPU;
    match preference {
        DevicePreference::Discrete if discrete => 0,
        DevicePreference::Discrete if integrated => 1,
        DevicePreference::Integrated if integrated => 0,
        DevicePreference::Integrated if discrete => 1,
        DevicePreference::Any if discrete || integrated => 0,
        _ if device_type == vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
        _ if device_type == vk::PhysicalDeviceType::CPU => 4,
        _ => 3,
    }
}

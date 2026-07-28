use std::collections::BTreeSet;
use std::ffi::CString;
use std::fmt;
use std::sync::Arc;

use vulkanalia::{
    Device, Entry, Instance, Version,
    loader::{LIBRARY, LibloadingLoader},
    prelude::v1_4::*,
    vk,
};

use crate::capabilities::{BackendProfile, CoreFeatures, DescriptorHeapLimits, Features, Limits};
use crate::frame::{FrameClock, FrameToken};
use crate::queue::{QueueFamilyInfo, QueuePlan};
use crate::roadmap_2026::query_roadmap_2026_device_requirements;
use crate::{Error, Result};

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
            required_features: Features::VULKAN14_RENDERER_BASELINE,
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
    pub queues: QueuePlan,
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
    owner: Arc<DeviceOwner>,
}

impl fmt::Debug for Queue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Queue")
            .field("graphics", &self.owner.queues.graphics)
            .field("timeline", &self.owner.timeline)
            .finish_non_exhaustive()
    }
}

impl Queue {
    pub fn raw(&self) -> vk::Queue {
        self.owner.queues.graphics
    }

    pub fn timeline_semaphore(&self) -> vk::Semaphore {
        self.owner.timeline
    }

    pub fn submit(
        &self,
        frame: FrameToken,
        command_buffers: &[vk::CommandBuffer],
        waits: &[SemaphoreWait],
    ) -> Result<()> {
        submit_to_graphics_queue(&self.owner, frame, command_buffers, waits)
    }
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
    frame_clock: FrameClock,
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
            .field("frame_clock", &self.frame_clock)
            .finish_non_exhaustive()
    }
}

impl Backend {
    pub fn new(mut config: BackendConfig) -> Result<Self> {
        if config.profile == BackendProfile::Roadmap2026 {
            config.required_features |= Features::FIFO_LATEST_READY;
        }
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

        Ok(Self {
            config,
            info: candidate.info,
            owner: Arc::new(DeviceOwner {
                device,
                instance,
                queues,
                command_pool,
                timeline,
            }),
            frame_clock: FrameClock::default(),
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
        }
    }

    pub fn command_pool(&self) -> vk::CommandPool {
        self.owner.command_pool
    }

    pub fn timeline_semaphore(&self) -> vk::Semaphore {
        self.owner.timeline
    }

    pub fn next_frame(&mut self) -> Result<FrameToken> {
        self.frame_clock.allocate()
    }

    pub fn completed_timeline(&mut self) -> Result<u64> {
        let completed = unsafe {
            self.owner
                .device
                .get_semaphore_counter_value(self.owner.timeline)
        }
        .map_err(|source| Error::vulkan("vkGetSemaphoreCounterValue", source))?;
        self.frame_clock.retire(completed);
        Ok(completed)
    }

    pub fn wait_for(&mut self, frame: FrameToken, timeout_ns: u64) -> Result<()> {
        let semaphores = [self.owner.timeline];
        let values = [frame.value()];
        let wait = vk::SemaphoreWaitInfo::builder()
            .semaphores(&semaphores)
            .values(&values);
        unsafe { self.owner.device.wait_semaphores(&wait, timeout_ns) }
            .map_err(|source| Error::vulkan("vkWaitSemaphores", source))?;
        self.frame_clock.retire(frame.value());
        Ok(())
    }

    pub fn allocate_primary_command_buffer(&self) -> Result<vk::CommandBuffer> {
        let info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.owner.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        unsafe { self.owner.device.allocate_command_buffers(&info) }
            .map_err(|source| Error::vulkan("vkAllocateCommandBuffers", source))?
            .into_iter()
            .next()
            .ok_or_else(|| {
                Error::NoCompatibleDevice(vec!["Vulkan returned no command buffer".into()])
            })
    }

    /// Submits command buffers and signals this backend's timeline. State
    /// owners should commit resource/frame bookkeeping only after this returns
    /// successfully.
    pub fn submit(
        &self,
        frame: FrameToken,
        command_buffers: &[vk::CommandBuffer],
        waits: &[SemaphoreWait],
    ) -> Result<()> {
        submit_to_graphics_queue(&self.owner, frame, command_buffers, waits)
    }

    /// The caller must ensure every command buffer has completed before it is
    /// freed. Pair this with the frame timeline value used for submission.
    pub unsafe fn free_command_buffers(&self, command_buffers: &[vk::CommandBuffer]) {
        unsafe {
            self.owner
                .device
                .free_command_buffers(self.owner.command_pool, command_buffers)
        };
    }
}

fn submit_to_graphics_queue(
    owner: &DeviceOwner,
    frame: FrameToken,
    command_buffers: &[vk::CommandBuffer],
    waits: &[SemaphoreWait],
) -> Result<()> {
    let wait_infos = waits
        .iter()
        .map(|wait| {
            vk::SemaphoreSubmitInfo::builder()
                .semaphore(wait.semaphore)
                .value(wait.value)
                .stage_mask(wait.stages)
                .build()
        })
        .collect::<Vec<_>>();
    let command_infos = command_buffers
        .iter()
        .copied()
        .map(|command_buffer| {
            vk::CommandBufferSubmitInfo::builder()
                .command_buffer(command_buffer)
                .build()
        })
        .collect::<Vec<_>>();
    let signals = [vk::SemaphoreSubmitInfo::builder()
        .semaphore(owner.timeline)
        .value(frame.value())
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .build()];
    let submissions = [vk::SubmitInfo2::builder()
        .wait_semaphore_infos(&wait_infos)
        .command_buffer_infos(&command_infos)
        .signal_semaphore_infos(&signals)
        .build()];
    unsafe {
        owner
            .device
            .queue_submit2(owner.queues.graphics, &submissions, vk::Fence::null())
    }
    .map_err(|source| Error::vulkan("vkQueueSubmit2", source))
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

struct DeviceOwner {
    device: Device,
    instance: Arc<InstanceOwner>,
    queues: DeviceQueues,
    command_pool: vk::CommandPool,
    timeline: vk::Semaphore,
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
                    queues,
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
        },
        descriptor_heap_limits,
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
    let extension_names = c_strings(extension_names)?;
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
        extensions.push("VK_KHR_present_mode_fifo_latest_ready".into());
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

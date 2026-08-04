use std::collections::BTreeSet;
use std::ffi::CString;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use vulkanalia::{
    Device, Entry,
    loader::{LIBRARY, LibloadingLoader},
    prelude::v1_4::*,
    vk,
};

use crate::capabilities::{
    BackendProfile, CoreFeatures, DeviceProperties, Features, Limits, PipelineBinaryProperties,
};
use crate::command::{CommandEncoder, CommandEncoderDescriptor};
use crate::frame::FrameToken;
use crate::memory::MemoryTypeInfo;
use crate::queue::{QueueFamilyInfo, QueuePlan};
use crate::video::{VideoDecodeCodecs, VideoDecodeDevice, VideoDecodeRequirements};
use crate::{
    ApiVersion, DeviceType, Error, Result, TimestampQuerySet, TimestampQuerySetDescriptor,
};

mod device;
mod owner;
mod probe;
mod retirement;
mod submission;

use device::create_device;
#[cfg(feature = "ffmpeg-vulkan-decode")]
use device::enabled_device_extensions;
pub(crate) use owner::{DeviceOwner, InstanceOwner};
pub(crate) use probe::probe_devices;
use retirement::SubmissionRetirement;
use submission::submit_to_graphics_queue;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DevicePreference {
    #[default]
    Discrete,
    Integrated,
    Any,
}

/// Stable PCI identity reported by `VK_EXT_pci_bus_info` when available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciAddress {
    pub domain: u32,
    pub bus: u32,
    pub device: u32,
    pub function: u32,
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
    /// Optional exact Vulkan Video profiles owned by this logical device.
    pub video_decode: Option<VideoDecodeRequirements>,
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
            video_decode: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    pub ordinal: usize,
    pub name: String,
    pub api_version: ApiVersion,
    pub device_type: DeviceType,
    pub vendor_id: u32,
    pub device_id: u32,
    pub driver_version: u32,
    pub device_uuid: [u8; vk::UUID_SIZE],
    pub driver_uuid: [u8; vk::UUID_SIZE],
    pub pci_address: Option<PciAddress>,
    pub features: CoreFeatures,
    pub supported_features: Features,
    pub properties: DeviceProperties,
    pub pipeline_binary_properties: PipelineBinaryProperties,
    pub limits: Limits,
    pub memory_types: Vec<MemoryTypeInfo>,
    pub non_coherent_atom_size: u64,
    pub queues: QueuePlan,
    pub queue_families: Vec<QueueFamilyInfo>,
    pub supported_video_decode_profiles: VideoDecodeCodecs,
    pub extensions: BTreeSet<String>,
    pub roadmap_2026_ready: bool,
    pub roadmap_2026_failures: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct DeviceQueues {
    pub graphics_family: u32,
    pub graphics: vk::Queue,
    pub compute: vk::Queue,
    pub transfer: vk::Queue,
    _video_decode: Option<vk::Queue>,
}

/// Borrowed native command context for a product capability that has not yet
/// grown a typed encoder in `vulkan-renderer`.
///
/// The renderer still owns the loader, instance, logical device, queues,
/// command-pool lifetime, and capability gate. This context intentionally
/// exposes no ownership or destruction APIs: callers may record a genuinely
/// product-specific Vulkan command stream, but cannot create a second device
/// owner or bypass the renderer's selected-adapter contract.
#[derive(Clone, Copy)]
pub struct NativeDevice<'a> {
    owner: &'a DeviceOwner,
}

impl fmt::Debug for NativeDevice<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeDevice")
            .field("physical_device", &self.owner.physical_device)
            .field("queues", &self.owner.queues)
            .finish_non_exhaustive()
    }
}

impl<'a> NativeDevice<'a> {
    /// Native logical-device dispatch table owned by this renderer instance.
    pub fn device(self) -> &'a Device {
        &self.owner.device
    }

    /// Native Vulkan instance associated with this logical device.
    pub fn instance(self) -> &'a vulkanalia::Instance {
        &self.owner.instance.instance
    }

    /// Physical device selected and validated by the shared renderer.
    pub const fn physical_device(self) -> vk::PhysicalDevice {
        self.owner.physical_device
    }

    /// Queue endpoints created by the shared renderer for this device.
    pub const fn queues(self) -> DeviceQueues {
        self.owner.queues
    }

    /// Graphics queue family selected by the shared renderer.
    pub const fn graphics_queue_family(self) -> u32 {
        self.owner.queues.graphics_family
    }
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
    pub(crate) semaphore: vk::Semaphore,
    /// Zero for a binary semaphore, otherwise the required timeline value.
    pub(crate) value: u64,
    pub(crate) stages: vk::PipelineStageFlags2,
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
        validate_no_untyped_video_extensions(&config.extra_device_extensions)?;
        if config.profile == BackendProfile::Roadmap2026 {
            config.required_features |= Features::FIFO_LATEST_READY;
        }
        config.required_features = config.required_features.with_dependencies();
        let entry = load_entry()?;
        let loader_version = ApiVersion::from_vk(
            entry
                .version()
                .map_err(|source| Error::vulkan("vkEnumerateInstanceVersion", source))?,
        );
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
        append_video_decode_extensions(config.video_decode, &mut feature_extensions);
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
            config.video_decode,
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
        #[cfg(feature = "ffmpeg-vulkan-decode")]
        let enabled_device_extensions = enabled_device_extensions(
            &candidate,
            required_device_extensions,
            config.required_features,
        );
        let queues = DeviceQueues {
            graphics_family: candidate.info.queues.graphics,
            graphics: unsafe { device.get_device_queue(candidate.info.queues.graphics, 0) },
            compute: unsafe { device.get_device_queue(candidate.info.queues.compute, 0) },
            transfer: unsafe { device.get_device_queue(candidate.info.queues.transfer, 0) },
            _video_decode: candidate
                .info
                .queues
                .video_decode
                .map(|family| unsafe { device.get_device_queue(family, 0) }),
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
            #[cfg(feature = "ffmpeg-vulkan-decode")]
            graphics_queue_family: candidate.info.queues.graphics,
            enabled_features: config.required_features,
            properties: candidate.info.properties,
            limits: candidate.info.limits,
            #[cfg(feature = "ffmpeg-vulkan-decode")]
            enabled_device_extensions,
            max_push_data_size,
            command_pool,
            timeline,
            next_timeline: AtomicU64::new(1),
            completed_timeline: AtomicU64::new(0),
            submit_lock: Mutex::new(()),
            command_pool_lock: Mutex::new(()),
            retained_command_buffers: Mutex::new(Vec::new()),
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

    pub(crate) fn device(&self) -> &Device {
        &self.owner.device
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

    /// Borrows the renderer-owned native command context for an advanced
    /// product stream. Prefer typed renderer encoders whenever they cover the
    /// operation; this boundary exists for real product-specific passes while
    /// their reusable primitive is being upstreamed.
    pub fn native_device(&self) -> NativeDevice<'_> {
        NativeDevice { owner: &self.owner }
    }

    /// Returns the opaque Vulkan Video endpoint requested with this device.
    pub fn video_decode_device(&self) -> Option<VideoDecodeDevice> {
        let family_index = self.info.queues.video_decode?;
        let family = self
            .info
            .queue_families
            .iter()
            .find(|family| family.index == family_index)?;
        Some(VideoDecodeDevice::new(
            Arc::clone(&self.owner),
            self.config.video_decode?,
            family_index,
            family.flags,
            family.video_decode_operations,
        ))
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

    /// Creates a renderer-owned timestamp query set for the selected graphics queue.
    pub fn create_timestamp_query_set(
        &self,
        descriptor: &TimestampQuerySetDescriptor,
    ) -> Result<TimestampQuerySet> {
        TimestampQuerySet::new(Arc::clone(&self.owner), descriptor)
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
    api_version: ApiVersion,
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
        .api_version(api_version.to_raw());
    let info = vk::InstanceCreateInfo::builder()
        .application_info(&application)
        .enabled_extension_names(&extension_pointers);
    let instance = unsafe { entry.create_instance(&info, None) }
        .map_err(|source| Error::vulkan("vkCreateInstance", source))?;
    Ok(Arc::new(InstanceOwner {
        entry,
        instance,
        #[cfg(feature = "ffmpeg-vulkan-decode")]
        enabled_extensions: extension_names
            .iter()
            .map(|name| name.to_string_lossy().into_owned())
            .collect(),
    }))
}

pub(crate) fn select_device(
    candidates: Vec<Candidate>,
    profile: BackendProfile,
    preference: DevicePreference,
    required_extensions: &[String],
    required_features: Features,
    required_limits: Limits,
    video_decode: Option<VideoDecodeRequirements>,
) -> (Option<Candidate>, Vec<String>) {
    let mut eligible = Vec::new();
    let mut rejected = Vec::new();
    for mut candidate in candidates {
        let mut reasons = candidate.info.features.rejection_reasons(
            candidate.info.api_version,
            profile,
            &candidate.info.extensions,
        );
        if candidate.info.queues.graphics == u32::MAX {
            reasons.push("missing graphics queue".into());
        }
        if let Some(requirements) = video_decode {
            let missing_profiles = requirements
                .codecs()
                .difference(candidate.info.supported_video_decode_profiles);
            if !missing_profiles.is_empty() {
                reasons.push(format!(
                    "missing Vulkan Video decode profiles: {}",
                    missing_profiles.labels().join(", ")
                ));
            }
            if let Some(queues) = candidate
                .info
                .queues
                .require_video_decode(&candidate.info.queue_families, requirements)
            {
                candidate.info.queues = queues;
            } else {
                reasons.push(
                    "no queue family supports all requested video decode codec operations".into(),
                );
            }
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

pub(crate) fn append_feature_extensions(features: Features, extensions: &mut Vec<String>) {
    if features.contains(Features::DESCRIPTOR_HEAP) {
        extensions.push("VK_EXT_descriptor_heap".into());
    }
    if features.contains(Features::FIFO_LATEST_READY) {
        extensions.push("VK_KHR_swapchain".into());
        extensions.push("VK_KHR_present_mode_fifo_latest_ready".into());
    }
    if features.contains(Features::PIPELINE_BINARIES) {
        extensions.push("VK_KHR_pipeline_binary".into());
    }
    for (required, extension) in [
        (
            Features::SHADER_UNTYPED_POINTERS,
            "VK_KHR_shader_untyped_pointers",
        ),
        (Features::PRESENT_ID2, "VK_KHR_present_id2"),
        (Features::PRESENT_WAIT2, "VK_KHR_present_wait2"),
        (
            Features::SWAPCHAIN_MAINTENANCE1,
            "VK_KHR_swapchain_maintenance1",
        ),
        (Features::ADVANCED_BLEND, "VK_EXT_blend_operation_advanced"),
        (
            Features::MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED,
            "VK_EXT_multisampled_render_to_single_sampled",
        ),
        (Features::MAINTENANCE7, "VK_KHR_maintenance7"),
        (Features::MAINTENANCE8, "VK_KHR_maintenance8"),
        (Features::MAINTENANCE9, "VK_KHR_maintenance9"),
        (Features::MAINTENANCE10, "VK_KHR_maintenance10"),
    ] {
        if features.contains(required) {
            extensions.push(extension.into());
        }
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

pub(crate) fn append_video_decode_extensions(
    requirements: Option<VideoDecodeRequirements>,
    extensions: &mut Vec<String>,
) {
    if let Some(requirements) = requirements {
        extensions.extend(
            requirements
                .required_extensions()
                .into_iter()
                .map(str::to_owned),
        );
    }
}

pub(crate) fn validate_no_untyped_video_extensions(extensions: &[String]) -> Result<()> {
    let untyped = extensions
        .iter()
        .filter(|extension| extension.starts_with("VK_KHR_video_"))
        .cloned()
        .collect::<Vec<_>>();
    if untyped.is_empty() {
        Ok(())
    } else {
        Err(Error::Validation(format!(
            "Vulkan Video extensions must be requested through typed video requirements: {}",
            untyped.join(", ")
        )))
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

fn device_rank(preference: DevicePreference, device_type: DeviceType) -> u8 {
    let discrete = device_type == DeviceType::Discrete;
    let integrated = device_type == DeviceType::Integrated;
    match preference {
        DevicePreference::Discrete if discrete => 0,
        DevicePreference::Discrete if integrated => 1,
        DevicePreference::Integrated if integrated => 0,
        DevicePreference::Integrated if discrete => 1,
        DevicePreference::Any if discrete || integrated => 0,
        _ if device_type == DeviceType::Virtual => 2,
        _ if device_type == DeviceType::Cpu => 4,
        _ => 3,
    }
}

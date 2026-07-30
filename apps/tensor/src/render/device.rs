use std::{
    fmt, fs,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    str::FromStr,
};

use thiserror::Error;
use vulkan_renderer::vulkanalia::{Version, vk};

use super::NativeInteropCapabilities;

/// Linux `major()` for a `dev_t` without binding device ranking to rustix types.
#[inline]
pub(super) const fn major_dev(dev: u64) -> u32 {
    // glibc: ((dev >> 8) & 0xfff) | ((u32)(dev >> 32) & !0xfff)
    (((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff)) as u32
}

/// Linux `minor()` for a `dev_t`.
#[inline]
pub(super) const fn minor_dev(dev: u64) -> u32 {
    // glibc: ((dev & 0xff) | ((u32)(dev >> 12) & !0xff))
    ((dev & 0xff) | ((dev >> 12) & !0xff)) as u32
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GpuPreference {
    #[default]
    Discrete,
    Integrated,
    Any,
}

impl GpuPreference {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Discrete => "discrete",
            Self::Integrated => "integrated",
            Self::Any => "any",
        }
    }
}

impl FromStr for GpuPreference {
    type Err = ParseGpuPreferenceError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "discrete" => Ok(Self::Discrete),
            "integrated" => Ok(Self::Integrated),
            "any" => Ok(Self::Any),
            _ => Err(ParseGpuPreferenceError(value.to_owned())),
        }
    }
}

#[derive(Debug, Error)]
#[error("unknown GPU preference '{0}'; expected discrete, integrated, or any")]
pub struct ParseGpuPreferenceError(String);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DrmNodeId {
    major: u32,
    minor: u32,
}

impl DrmNodeId {
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    pub fn from_path(path: &Path) -> Result<Self, DrmNodeError> {
        let metadata = fs::metadata(path).map_err(|source| DrmNodeError::Read {
            path: path.to_owned(),
            source,
        })?;
        if !metadata.file_type().is_char_device() {
            return Err(DrmNodeError::NotCharacterDevice(path.to_owned()));
        }
        let device = metadata.rdev();
        Ok(Self::new(major_dev(device), minor_dev(device)))
    }

    pub const fn major(self) -> u32 {
        self.major
    }

    pub const fn minor(self) -> u32 {
        self.minor
    }
}

impl fmt::Display for DrmNodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.major, self.minor)
    }
}

#[derive(Debug, Error)]
pub enum DrmNodeError {
    #[error("failed to inspect DRM node {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("configured DRM node {0} is not a character device")]
    NotCharacterDevice(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrmDeviceIdentity {
    pub primary: Option<DrmNodeId>,
    pub render: Option<DrmNodeId>,
}

impl DrmDeviceIdentity {
    pub const fn new(primary: Option<DrmNodeId>, render: Option<DrmNodeId>) -> Self {
        Self { primary, render }
    }

    const fn matches(self, node: DrmNodeId) -> bool {
        matches!(self.primary, Some(primary) if same_node(primary, node))
            || matches!(self.render, Some(render) if same_node(render, node))
    }

    pub(crate) const fn node_pair(self) -> Option<(DrmNodeId, DrmNodeId)> {
        match (self.primary, self.render) {
            (Some(primary), Some(render)) => Some((primary, render)),
            _ => None,
        }
    }
}

const fn same_node(left: DrmNodeId, right: DrmNodeId) -> bool {
    left.major == right.major && left.minor == right.minor
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceCandidate {
    pub ordinal: usize,
    pub name: String,
    pub device_type: vk::PhysicalDeviceType,
    pub api_version: Version,
    pub descriptor_heap_supported: bool,
    pub descriptor_heap: DescriptorHeapProperties,
    pub buffer_device_address_supported: bool,
    pub timeline_semaphore_supported: bool,
    pub dynamic_rendering_supported: bool,
    pub maintenance5_supported: bool,
    pub graphics_queue_family: Option<u32>,
    pub drm: Option<DrmDeviceIdentity>,
    pub interop: NativeInteropCapabilities,
    pub native_output_format_count: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DescriptorHeapProperties {
    pub sampler_heap_alignment: u64,
    pub resource_heap_alignment: u64,
    pub max_sampler_heap_size: u64,
    pub max_resource_heap_size: u64,
    pub min_sampler_heap_reserved_range: u64,
    pub min_sampler_heap_reserved_range_with_embedded: u64,
    pub min_resource_heap_reserved_range: u64,
    pub sampler_descriptor_size: u64,
    pub buffer_descriptor_alignment: u64,
    pub image_descriptor_size: u64,
    pub sampler_descriptor_alignment: u64,
    pub image_descriptor_alignment: u64,
    pub max_push_data_size: u64,
    pub max_descriptor_heap_embedded_samplers: u32,
}

impl DescriptorHeapProperties {
    pub const REQUIRED_DRAW_PUSH_DATA_SIZE: u64 = 64;

    pub const fn is_usable(self) -> bool {
        let sampler_reserved = if self.min_sampler_heap_reserved_range
            > self.min_sampler_heap_reserved_range_with_embedded
        {
            self.min_sampler_heap_reserved_range
        } else {
            self.min_sampler_heap_reserved_range_with_embedded
        };
        self.sampler_heap_alignment.is_power_of_two()
            && self.resource_heap_alignment.is_power_of_two()
            && self.sampler_descriptor_alignment.is_power_of_two()
            && self.buffer_descriptor_alignment.is_power_of_two()
            && self.image_descriptor_alignment.is_power_of_two()
            && self.sampler_descriptor_size > 0
            && self.max_resource_heap_size
                > self
                    .min_resource_heap_reserved_range
                    .saturating_add(self.image_descriptor_alignment.saturating_mul(2))
            && heap_range_fits(
                self.max_sampler_heap_size,
                sampler_reserved,
                self.sampler_heap_alignment,
            )
            && self.image_descriptor_size > 0
            && self.max_push_data_size >= Self::REQUIRED_DRAW_PUSH_DATA_SIZE
            && self.max_descriptor_heap_embedded_samplers > 0
    }
}

const fn heap_range_fits(maximum: u64, reserved: u64, alignment: u64) -> bool {
    if alignment == 0 || !alignment.is_power_of_two() {
        return false;
    }
    let requested = if reserved == 0 { 1 } else { reserved };
    let remainder = requested % alignment;
    let padding = (alignment - remainder) % alignment;
    requested <= maximum && padding <= maximum - requested
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceSelector {
    preference: GpuPreference,
    requested_drm_node: Option<DrmNodeId>,
}

impl DeviceSelector {
    pub const fn new(preference: GpuPreference) -> Self {
        Self {
            preference,
            requested_drm_node: None,
        }
    }

    pub const fn with_drm_node(mut self, node: Option<DrmNodeId>) -> Self {
        self.requested_drm_node = node;
        self
    }

    pub const fn preference(self) -> GpuPreference {
        self.preference
    }

    pub fn select<'a>(
        self,
        candidates: impl IntoIterator<Item = &'a DeviceCandidate>,
    ) -> Result<&'a DeviceCandidate, DeviceSelectionError> {
        let candidates = candidates.into_iter().collect::<Vec<_>>();
        let candidates = if let Some(requested) = self.requested_drm_node {
            let matching = candidates
                .into_iter()
                .filter(|candidate| candidate.drm.is_some_and(|drm| drm.matches(requested)))
                .collect::<Vec<_>>();
            if matching.is_empty() {
                return Err(DeviceSelectionError::DrmNodeNotFound(requested));
            }
            matching
        } else {
            candidates
        };
        if !candidates
            .iter()
            .any(|candidate| candidate.descriptor_heap_supported)
        {
            return Err(DeviceSelectionError::MissingDescriptorHeap);
        }
        if !candidates.iter().any(|candidate| {
            candidate.descriptor_heap_supported && candidate.buffer_device_address_supported
        }) {
            return Err(DeviceSelectionError::MissingBufferDeviceAddress);
        }
        if !candidates.iter().any(|candidate| {
            candidate.descriptor_heap_supported && candidate.timeline_semaphore_supported
        }) {
            return Err(DeviceSelectionError::MissingTimelineSemaphore);
        }
        if !candidates.iter().any(|candidate| {
            candidate.descriptor_heap_supported
                && candidate.buffer_device_address_supported
                && candidate.timeline_semaphore_supported
                && candidate.descriptor_heap.is_usable()
        }) {
            return Err(DeviceSelectionError::InvalidDescriptorHeapProperties);
        }
        if !candidates.iter().any(|candidate| {
            candidate.descriptor_heap_supported
                && candidate.buffer_device_address_supported
                && candidate.timeline_semaphore_supported
                && candidate.api_version >= Version::V1_4_0
        }) {
            return Err(DeviceSelectionError::VulkanTooOld);
        }
        if !candidates.iter().any(|candidate| {
            candidate.descriptor_heap_supported
                && candidate.buffer_device_address_supported
                && candidate.timeline_semaphore_supported
                && candidate.descriptor_heap.is_usable()
                && candidate.api_version >= Version::V1_4_0
                && candidate.dynamic_rendering_supported
        }) {
            return Err(DeviceSelectionError::MissingDynamicRendering);
        }
        if !candidates
            .iter()
            .any(|candidate| descriptor_heap_core(candidate))
        {
            return Err(DeviceSelectionError::MissingMaintenance5);
        }
        if !candidates.iter().any(|candidate| {
            descriptor_heap_core(candidate) && candidate.graphics_queue_family.is_some()
        }) {
            return Err(DeviceSelectionError::MissingGraphicsQueue);
        }
        if !candidates.iter().any(|candidate| {
            descriptor_heap_core(candidate)
                && candidate.graphics_queue_family.is_some()
                && candidate
                    .drm
                    .and_then(DrmDeviceIdentity::node_pair)
                    .is_some()
        }) {
            return Err(DeviceSelectionError::MissingDrmNodePair);
        }
        if !candidates
            .iter()
            .any(|candidate| native_base(candidate) && candidate.interop.external_memory_fd)
        {
            return Err(DeviceSelectionError::MissingExternalMemoryFd);
        }
        if !candidates.iter().any(|candidate| {
            native_base(candidate)
                && candidate.interop.external_memory_fd
                && candidate.interop.dma_buf_memory
        }) {
            return Err(DeviceSelectionError::MissingDmaBufMemory);
        }
        if !candidates.iter().any(|candidate| {
            native_base(candidate)
                && candidate.interop.external_memory_fd
                && candidate.interop.dma_buf_memory
                && candidate.interop.drm_format_modifier
        }) {
            return Err(DeviceSelectionError::MissingDrmFormatModifier);
        }
        if !candidates.iter().any(|candidate| {
            native_base(candidate)
                && candidate.interop.external_memory_fd
                && candidate.interop.dma_buf_memory
                && candidate.interop.drm_format_modifier
                && candidate.interop.foreign_queue_family
        }) {
            return Err(DeviceSelectionError::MissingForeignQueueFamily);
        }
        if !candidates.iter().any(|candidate| {
            native_base(candidate)
                && candidate.interop.external_memory_fd
                && candidate.interop.dma_buf_memory
                && candidate.interop.drm_format_modifier
                && candidate.interop.foreign_queue_family
                && candidate.interop.external_semaphore_fd
        }) {
            return Err(DeviceSelectionError::MissingExternalSemaphoreFd);
        }
        if !candidates
            .iter()
            .any(|candidate| native_base(candidate) && candidate.interop.is_complete())
        {
            return Err(DeviceSelectionError::MissingSyncFdSemaphore);
        }
        if !candidates.iter().any(|candidate| {
            native_base(candidate)
                && candidate.interop.is_complete()
                && candidate.native_output_format_count > 0
        }) {
            return Err(DeviceSelectionError::MissingNativeOutputFormat);
        }

        candidates
            .into_iter()
            .filter(|candidate| descriptor_heap_core(candidate))
            .filter(|candidate| candidate.graphics_queue_family.is_some())
            .filter(|candidate| {
                candidate
                    .drm
                    .and_then(DrmDeviceIdentity::node_pair)
                    .is_some()
            })
            .filter(|candidate| candidate.interop.is_complete())
            .filter(|candidate| candidate.native_output_format_count > 0)
            .min_by_key(|candidate| (self.rank(candidate.device_type), candidate.ordinal))
            .ok_or(DeviceSelectionError::MissingNativeOutputFormat)
    }

    fn rank(self, device_type: vk::PhysicalDeviceType) -> u8 {
        match self.preference {
            GpuPreference::Discrete => match device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => 0,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
                vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                vk::PhysicalDeviceType::OTHER => 3,
                vk::PhysicalDeviceType::CPU => 4,
                _ => 5,
            },
            GpuPreference::Integrated => match device_type {
                vk::PhysicalDeviceType::INTEGRATED_GPU => 0,
                vk::PhysicalDeviceType::DISCRETE_GPU => 1,
                vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                vk::PhysicalDeviceType::OTHER => 3,
                vk::PhysicalDeviceType::CPU => 4,
                _ => 5,
            },
            GpuPreference::Any => match device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU | vk::PhysicalDeviceType::INTEGRATED_GPU => 0,
                vk::PhysicalDeviceType::VIRTUAL_GPU => 1,
                vk::PhysicalDeviceType::OTHER => 2,
                vk::PhysicalDeviceType::CPU => 3,
                _ => 4,
            },
        }
    }
}

fn descriptor_heap_core(candidate: &DeviceCandidate) -> bool {
    candidate.descriptor_heap_supported
        && candidate.buffer_device_address_supported
        && candidate.timeline_semaphore_supported
        && candidate.dynamic_rendering_supported
        && candidate.maintenance5_supported
        && candidate.descriptor_heap.is_usable()
        && candidate.api_version >= Version::V1_4_0
}

fn native_base(candidate: &DeviceCandidate) -> bool {
    descriptor_heap_core(candidate)
        && candidate.graphics_queue_family.is_some()
        && candidate
            .drm
            .and_then(DrmDeviceIdentity::node_pair)
            .is_some()
}

#[derive(Debug, Eq, Error, PartialEq)]
pub enum DeviceSelectionError {
    #[error("no Vulkan device supports the required VK_EXT_descriptor_heap feature")]
    MissingDescriptorHeap,
    #[error("no descriptor-heap Vulkan device supports timeline semaphores")]
    MissingTimelineSemaphore,
    #[error("no descriptor-heap Vulkan device supports buffer device addresses")]
    MissingBufferDeviceAddress,
    #[error("no descriptor-heap Vulkan device exposes usable resource-heap limits")]
    InvalidDescriptorHeapProperties,
    #[error("no descriptor-heap Vulkan device supports Vulkan 1.4")]
    VulkanTooOld,
    #[error("no Vulkan 1.4 descriptor-heap device supports maintenance5")]
    MissingMaintenance5,
    #[error("no Vulkan 1.4 descriptor-heap device supports dynamic rendering")]
    MissingDynamicRendering,
    #[error("no Vulkan 1.4 descriptor-heap device exposes a graphics queue")]
    MissingGraphicsQueue,
    #[error("configured DRM node {0} does not identify a Vulkan physical device")]
    DrmNodeNotFound(DrmNodeId),
    #[error("no eligible Vulkan device exposes a complete DRM primary/render node pair")]
    MissingDrmNodePair,
    #[error("no eligible Vulkan device supports VK_KHR_external_memory_fd")]
    MissingExternalMemoryFd,
    #[error("no eligible Vulkan device supports VK_EXT_external_memory_dma_buf")]
    MissingDmaBufMemory,
    #[error("no eligible Vulkan device supports VK_EXT_image_drm_format_modifier")]
    MissingDrmFormatModifier,
    #[error("no eligible Vulkan device supports VK_EXT_queue_family_foreign")]
    MissingForeignQueueFamily,
    #[error("no eligible Vulkan device supports VK_KHR_external_semaphore_fd")]
    MissingExternalSemaphoreFd,
    #[error("no eligible Vulkan device can import and export binary SYNC_FD semaphores")]
    MissingSyncFdSemaphore,
    #[error("no eligible Vulkan device exposes a renderable and dma-buf-exportable DRM modifier")]
    MissingNativeOutputFormat,
}

#[cfg(test)]
mod tests;

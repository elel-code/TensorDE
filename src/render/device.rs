use std::{
    fmt, fs,
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    str::FromStr,
};

use thiserror::Error;
use vulkanalia::{Version, vk};

use super::NativeInteropCapabilities;

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
        Ok(Self::new(libc::major(device), libc::minor(device)))
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
                && candidate.api_version >= Version::V1_4_0
                && candidate.graphics_queue_family.is_some()
        }) {
            return Err(DeviceSelectionError::MissingGraphicsQueue);
        }
        if !candidates.iter().any(|candidate| {
            candidate.descriptor_heap_supported
                && candidate.buffer_device_address_supported
                && candidate.timeline_semaphore_supported
                && candidate.api_version >= Version::V1_4_0
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
            .filter(|candidate| candidate.descriptor_heap_supported)
            .filter(|candidate| candidate.buffer_device_address_supported)
            .filter(|candidate| candidate.timeline_semaphore_supported)
            .filter(|candidate| candidate.descriptor_heap.is_usable())
            .filter(|candidate| candidate.api_version >= Version::V1_4_0)
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

fn native_base(candidate: &DeviceCandidate) -> bool {
    candidate.descriptor_heap_supported
        && candidate.buffer_device_address_supported
        && candidate.timeline_semaphore_supported
        && candidate.descriptor_heap.is_usable()
        && candidate.api_version >= Version::V1_4_0
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
mod tests {
    use super::*;

    fn required_interop() -> NativeInteropCapabilities {
        NativeInteropCapabilities {
            external_memory_fd: true,
            dma_buf_memory: true,
            drm_format_modifier: true,
            foreign_queue_family: true,
            external_semaphore_fd: true,
            sync_fd_semaphore: true,
        }
    }

    fn candidate(
        ordinal: usize,
        device_type: vk::PhysicalDeviceType,
        heap: bool,
    ) -> DeviceCandidate {
        DeviceCandidate {
            ordinal,
            name: format!("device-{ordinal}"),
            device_type,
            api_version: Version::V1_4_0,
            descriptor_heap_supported: heap,
            descriptor_heap: DescriptorHeapProperties {
                sampler_heap_alignment: 32,
                resource_heap_alignment: 32,
                max_sampler_heap_size: 4096,
                max_resource_heap_size: 16 * 1024 * 1024,
                min_sampler_heap_reserved_range: 0,
                min_sampler_heap_reserved_range_with_embedded: 32,
                min_resource_heap_reserved_range: 0,
                sampler_descriptor_size: 32,
                buffer_descriptor_alignment: 32,
                image_descriptor_size: 32,
                sampler_descriptor_alignment: 32,
                image_descriptor_alignment: 32,
                max_push_data_size: 128,
                max_descriptor_heap_embedded_samplers: 8,
            },
            buffer_device_address_supported: true,
            timeline_semaphore_supported: true,
            graphics_queue_family: Some(0),
            drm: Some(DrmDeviceIdentity::new(
                Some(DrmNodeId::new(226, ordinal as u32)),
                Some(DrmNodeId::new(226, 128 + ordinal as u32)),
            )),
            interop: required_interop(),
            native_output_format_count: 1,
        }
    }

    #[test]
    fn default_prefers_discrete_gpu_with_heap() {
        let candidates = [
            candidate(0, vk::PhysicalDeviceType::CPU, true),
            candidate(1, vk::PhysicalDeviceType::DISCRETE_GPU, true),
            candidate(2, vk::PhysicalDeviceType::INTEGRATED_GPU, true),
        ];

        assert_eq!(
            DeviceSelector::new(GpuPreference::Discrete)
                .select(&candidates)
                .unwrap()
                .ordinal,
            1
        );
    }

    #[test]
    fn unsupported_heap_devices_are_never_selected() {
        let candidates = [
            candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, false),
            candidate(1, vk::PhysicalDeviceType::CPU, false),
        ];

        assert!(matches!(
            DeviceSelector::new(GpuPreference::Any).select(&candidates),
            Err(DeviceSelectionError::MissingDescriptorHeap)
        ));
    }

    #[test]
    fn timeline_semaphores_are_required_for_native_frame_scheduling() {
        let mut candidate = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
        candidate.timeline_semaphore_supported = false;

        assert!(matches!(
            DeviceSelector::new(GpuPreference::Any).select([&candidate]),
            Err(DeviceSelectionError::MissingTimelineSemaphore)
        ));
    }

    #[test]
    fn buffer_device_address_is_required_for_descriptor_heap_binding() {
        let mut candidate = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
        candidate.buffer_device_address_supported = false;

        assert!(matches!(
            DeviceSelector::new(GpuPreference::Any).select([&candidate]),
            Err(DeviceSelectionError::MissingBufferDeviceAddress)
        ));
    }

    #[test]
    fn unusable_descriptor_heap_limits_are_rejected() {
        let mut candidate = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
        candidate.descriptor_heap.max_resource_heap_size = 0;

        assert!(matches!(
            DeviceSelector::new(GpuPreference::Any).select([&candidate]),
            Err(DeviceSelectionError::InvalidDescriptorHeapProperties)
        ));
    }

    #[test]
    fn descriptor_heap_draw_push_and_embedded_sampler_limits_are_required() {
        let mut candidate = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
        candidate.descriptor_heap.max_push_data_size = 32;
        assert!(matches!(
            DeviceSelector::new(GpuPreference::Any).select([&candidate]),
            Err(DeviceSelectionError::InvalidDescriptorHeapProperties)
        ));

        candidate.descriptor_heap.max_push_data_size = 128;
        candidate
            .descriptor_heap
            .max_descriptor_heap_embedded_samplers = 0;
        assert!(matches!(
            DeviceSelector::new(GpuPreference::Any).select([&candidate]),
            Err(DeviceSelectionError::InvalidDescriptorHeapProperties)
        ));

        candidate
            .descriptor_heap
            .max_descriptor_heap_embedded_samplers = 8;
        candidate
            .descriptor_heap
            .min_sampler_heap_reserved_range_with_embedded = 33;
        candidate.descriptor_heap.max_sampler_heap_size = 40;
        assert!(matches!(
            DeviceSelector::new(GpuPreference::Any).select([&candidate]),
            Err(DeviceSelectionError::InvalidDescriptorHeapProperties)
        ));
    }

    #[test]
    fn older_vulkan_devices_are_rejected_before_ranking() {
        let mut discrete = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
        discrete.api_version = Version::V1_3_0;
        let integrated = candidate(1, vk::PhysicalDeviceType::INTEGRATED_GPU, true);
        let candidates = [discrete, integrated];

        assert_eq!(
            DeviceSelector::new(GpuPreference::Discrete)
                .select(&candidates)
                .unwrap()
                .ordinal,
            1
        );
    }

    #[test]
    fn reports_when_all_descriptor_heap_devices_are_too_old() {
        let mut candidate = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
        candidate.api_version = Version::V1_3_0;

        assert!(matches!(
            DeviceSelector::new(GpuPreference::Discrete).select([&candidate]),
            Err(DeviceSelectionError::VulkanTooOld)
        ));
    }

    #[test]
    fn graphics_queue_is_a_required_renderer_capability() {
        let mut candidate = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
        candidate.graphics_queue_family = None;

        assert!(matches!(
            DeviceSelector::new(GpuPreference::Any).select([&candidate]),
            Err(DeviceSelectionError::MissingGraphicsQueue)
        ));
    }

    #[test]
    fn configured_drm_node_overrides_gpu_type_ranking() {
        let discrete = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
        let integrated = candidate(1, vk::PhysicalDeviceType::INTEGRATED_GPU, true);
        let requested = integrated.drm.unwrap().render.unwrap();

        let selected = DeviceSelector::new(GpuPreference::Discrete)
            .with_drm_node(Some(requested))
            .select([&discrete, &integrated])
            .unwrap();

        assert_eq!(selected.ordinal, integrated.ordinal);
    }

    #[test]
    fn rejects_a_drm_node_without_a_vulkan_device() {
        let candidate = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
        let requested = DrmNodeId::new(226, 191);

        assert!(matches!(
            DeviceSelector::new(GpuPreference::Discrete)
                .with_drm_node(Some(requested))
                .select([&candidate]),
            Err(DeviceSelectionError::DrmNodeNotFound(node)) if node == requested
        ));
    }

    #[test]
    fn drm_primary_and_render_nodes_are_both_required() {
        let mut candidate = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
        candidate.drm = Some(DrmDeviceIdentity::new(Some(DrmNodeId::new(226, 0)), None));

        assert!(matches!(
            DeviceSelector::new(GpuPreference::Discrete).select([&candidate]),
            Err(DeviceSelectionError::MissingDrmNodePair)
        ));
    }

    #[test]
    fn configured_node_path_must_be_a_character_device() {
        assert!(matches!(
            DrmNodeId::from_path(Path::new("Cargo.toml")),
            Err(DrmNodeError::NotCharacterDevice(_))
        ));
    }

    #[test]
    fn missing_configured_node_is_reported_at_selection_boundary() {
        assert!(matches!(
            DrmNodeId::from_path(Path::new("/definitely/missing/tensor-drm-node")),
            Err(DrmNodeError::Read { .. })
        ));
    }

    #[test]
    fn reports_the_first_missing_native_interop_capability() {
        let required = required_interop();
        let cases = [
            (
                NativeInteropCapabilities {
                    external_memory_fd: false,
                    ..required
                },
                DeviceSelectionError::MissingExternalMemoryFd,
            ),
            (
                NativeInteropCapabilities {
                    dma_buf_memory: false,
                    ..required
                },
                DeviceSelectionError::MissingDmaBufMemory,
            ),
            (
                NativeInteropCapabilities {
                    drm_format_modifier: false,
                    ..required
                },
                DeviceSelectionError::MissingDrmFormatModifier,
            ),
            (
                NativeInteropCapabilities {
                    foreign_queue_family: false,
                    ..required
                },
                DeviceSelectionError::MissingForeignQueueFamily,
            ),
            (
                NativeInteropCapabilities {
                    external_semaphore_fd: false,
                    ..required
                },
                DeviceSelectionError::MissingExternalSemaphoreFd,
            ),
            (
                NativeInteropCapabilities {
                    sync_fd_semaphore: false,
                    ..required
                },
                DeviceSelectionError::MissingSyncFdSemaphore,
            ),
        ];

        for (interop, expected) in cases {
            let mut candidate = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
            candidate.interop = interop;
            let error = DeviceSelector::new(GpuPreference::Discrete)
                .select([&candidate])
                .unwrap_err();
            assert_eq!(error, expected);
        }
    }

    #[test]
    fn device_without_an_exportable_output_format_is_not_selected() {
        let mut candidate = candidate(0, vk::PhysicalDeviceType::DISCRETE_GPU, true);
        candidate.native_output_format_count = 0;

        assert_eq!(
            DeviceSelector::new(GpuPreference::Discrete)
                .select([&candidate])
                .unwrap_err(),
            DeviceSelectionError::MissingNativeOutputFormat
        );
    }
}

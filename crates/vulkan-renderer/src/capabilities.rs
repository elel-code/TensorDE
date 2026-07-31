use std::collections::BTreeSet;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};

use vulkanalia::Version;

pub const ROADMAP_2026_PROFILE_NAME: &str = "VP_KHR_roadmap_2026";
pub const ROADMAP_2026_PROFILE_REVISION: u32 = 11;
pub const ROADMAP_2026_API_VERSION: Version = Version::new(1, 4, 328);
pub const ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS: &[&str] = &[
    "VK_KHR_surface",
    "VK_KHR_get_surface_capabilities2",
    "VK_KHR_surface_maintenance1",
];
pub const STANDARD_REQUIRED_INSTANCE_EXTENSIONS: &[&str] = &["VK_KHR_surface"];

/// Khronos `VP_KHR_roadmap_2026`, revision 11 (2026-01-28).
pub const ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS: &[&str] = &[
    "VK_KHR_global_priority",
    "VK_KHR_load_store_op_none",
    "VK_KHR_shader_quad_control",
    "VK_KHR_shader_maximal_reconvergence",
    "VK_KHR_shader_subgroup_uniform_control_flow",
    "VK_KHR_map_memory2",
    "VK_KHR_dynamic_rendering",
    "VK_KHR_shader_subgroup_rotate",
    "VK_KHR_shader_float_controls2",
    "VK_KHR_shader_expect_assume",
    "VK_KHR_line_rasterization",
    "VK_KHR_vertex_attribute_divisor",
    "VK_KHR_index_type_uint8",
    "VK_KHR_maintenance5",
    "VK_KHR_dynamic_rendering_local_read",
    "VK_KHR_push_descriptor",
    "VK_KHR_robustness2",
    "VK_KHR_pipeline_binary",
    "VK_KHR_fragment_shading_rate",
    "VK_KHR_shader_clock",
    "VK_KHR_workgroup_memory_explicit_layout",
    "VK_KHR_compute_shader_derivatives",
    "VK_KHR_maintenance7",
    "VK_KHR_maintenance8",
    "VK_KHR_maintenance9",
    "VK_KHR_depth_clamp_zero_one",
    "VK_KHR_copy_memory_indirect",
    "VK_KHR_shader_untyped_pointers",
    "VK_KHR_swapchain",
    "VK_KHR_present_mode_fifo_latest_ready",
    "VK_KHR_present_id2",
    "VK_KHR_present_wait2",
    "VK_KHR_swapchain_maintenance1",
    "VK_KHR_cooperative_matrix",
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackendProfile {
    /// Portable Vulkan 1.4 renderer core used for device creation.
    #[default]
    Vulkan14,
    /// Exact Khronos 2026 capability gate. Device creation still enables only
    /// the core features used by this crate; feature-specific modules extend
    /// the device chain when they consume the corresponding capability.
    Roadmap2026,
}

impl BackendProfile {
    pub const fn required_api_version(self) -> Version {
        match self {
            Self::Vulkan14 => Version::V1_4_0,
            Self::Roadmap2026 => ROADMAP_2026_API_VERSION,
        }
    }

    pub const fn required_device_extensions(self) -> &'static [&'static str] {
        match self {
            Self::Vulkan14 => &[],
            Self::Roadmap2026 => ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS,
        }
    }

    pub const fn required_instance_extensions(self) -> &'static [&'static str] {
        match self {
            Self::Vulkan14 => STANDARD_REQUIRED_INSTANCE_EXTENSIONS,
            Self::Roadmap2026 => ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CoreFeatures {
    pub timeline_semaphore: bool,
    pub buffer_device_address: bool,
    pub synchronization2: bool,
    pub dynamic_rendering: bool,
    pub maintenance5: bool,
    pub maintenance6: bool,
    pub dynamic_rendering_local_read: bool,
    pub descriptor_heap: bool,
    pub pipeline_binaries: bool,
    pub present_mode_fifo_latest_ready: bool,
    pub external_memory_dma_buf: bool,
    pub external_semaphore_sync_fd: bool,
}

impl CoreFeatures {
    pub const fn renderer_ready(self) -> bool {
        self.timeline_semaphore
            && self.synchronization2
            && self.dynamic_rendering
            && self.maintenance5
    }

    pub(crate) fn rejection_reasons(
        self,
        version: Version,
        profile: BackendProfile,
        extensions: &BTreeSet<String>,
    ) -> Vec<String> {
        let mut reasons = Vec::new();
        if version < profile.required_api_version() {
            reasons.push(format!(
                "API {version} is below {}",
                profile.required_api_version()
            ));
        }
        for (available, name) in [
            (self.timeline_semaphore, "timelineSemaphore"),
            (self.synchronization2, "synchronization2"),
            (self.dynamic_rendering, "dynamicRendering"),
            (self.maintenance5, "maintenance5"),
        ] {
            if !available {
                reasons.push(format!("missing {name}"));
            }
        }
        let missing = profile
            .required_device_extensions()
            .iter()
            .copied()
            .filter(|required| !extensions.contains(*required))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            reasons.push(format!("missing extensions: {}", missing.join(", ")));
        }
        reasons
    }
}

/// A WebGPU-style set of features which may be required in a device request.
///
/// Availability on an adapter and enablement on a device are distinct. A
/// request fails if any required bit is not advertised by the adapter.
#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
pub struct Features(u64);

impl Features {
    pub const TIMELINE_SEMAPHORE: Self = Self(1 << 0);
    pub const BUFFER_DEVICE_ADDRESS: Self = Self(1 << 1);
    pub const SYNCHRONIZATION2: Self = Self(1 << 2);
    pub const DYNAMIC_RENDERING: Self = Self(1 << 3);
    pub const MAINTENANCE5: Self = Self(1 << 4);
    pub const MAINTENANCE6: Self = Self(1 << 5);
    pub const DYNAMIC_RENDERING_LOCAL_READ: Self = Self(1 << 6);
    pub const DESCRIPTOR_HEAP: Self = Self(1 << 7);
    pub const FIFO_LATEST_READY: Self = Self(1 << 8);
    pub const EXTERNAL_MEMORY_DMA_BUF: Self = Self(1 << 9);
    pub const EXTERNAL_SEMAPHORE_SYNC_FD: Self = Self(1 << 10);
    pub const PIPELINE_BINARIES: Self = Self(1 << 11);

    pub const VULKAN14_RENDERER_BASELINE: Self = Self(
        Self::TIMELINE_SEMAPHORE.0
            | Self::BUFFER_DEVICE_ADDRESS.0
            | Self::SYNCHRONIZATION2.0
            | Self::DYNAMIC_RENDERING.0
            | Self::MAINTENANCE5.0,
    );

    /// Default contract of this renderer. Descriptor heaps are the sole
    /// shader-resource binding model and FIFO latest-ready is the preferred
    /// low-latency FIFO presentation path.
    pub const STANDARD_DEFAULTS: Self = Self(
        Self::VULKAN14_RENDERER_BASELINE.0
            | Self::DESCRIPTOR_HEAP.0
            | Self::FIFO_LATEST_READY.0
            | Self::PIPELINE_BINARIES.0,
    );

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Expands feature dependencies that Vulkan requires to be enabled on the
    /// logical device. The expanded set is the device's reported contract.
    pub const fn with_dependencies(self) -> Self {
        if self.contains(Self::DESCRIPTOR_HEAP) {
            Self(self.0 | Self::BUFFER_DEVICE_ADDRESS.0 | Self::MAINTENANCE5.0)
        } else {
            self
        }
    }

    pub const fn from_core(features: CoreFeatures) -> Self {
        let mut bits = 0;
        if features.timeline_semaphore {
            bits |= Self::TIMELINE_SEMAPHORE.0;
        }
        if features.buffer_device_address {
            bits |= Self::BUFFER_DEVICE_ADDRESS.0;
        }
        if features.synchronization2 {
            bits |= Self::SYNCHRONIZATION2.0;
        }
        if features.dynamic_rendering {
            bits |= Self::DYNAMIC_RENDERING.0;
        }
        if features.maintenance5 {
            bits |= Self::MAINTENANCE5.0;
        }
        if features.maintenance6 {
            bits |= Self::MAINTENANCE6.0;
        }
        if features.dynamic_rendering_local_read {
            bits |= Self::DYNAMIC_RENDERING_LOCAL_READ.0;
        }
        if features.descriptor_heap {
            bits |= Self::DESCRIPTOR_HEAP.0;
        }
        if features.pipeline_binaries {
            bits |= Self::PIPELINE_BINARIES.0;
        }
        if features.present_mode_fifo_latest_ready {
            bits |= Self::FIFO_LATEST_READY.0;
        }
        if features.external_memory_dma_buf {
            bits |= Self::EXTERNAL_MEMORY_DMA_BUF.0;
        }
        if features.external_semaphore_sync_fd {
            bits |= Self::EXTERNAL_SEMAPHORE_SYNC_FD.0;
        }
        Self(bits)
    }
}

/// Device behavior exposed by `VK_KHR_pipeline_binary`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PipelineBinaryProperties {
    pub internal_cache: bool,
    pub internal_cache_control: bool,
    pub prefers_internal_cache: bool,
    pub precompiled_internal_cache: bool,
    pub compressed_data: bool,
}

impl std::fmt::Debug for Features {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("Features")
            .field(&format_args!("{:#x}", self.0))
            .finish()
    }
}

impl BitOr for Features {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Features {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for Features {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for Features {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

/// Adapter limits used by strict device-request validation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Limits {
    pub max_image_dimension_2d: u32,
    pub max_memory_allocation_count: u32,
    pub max_bound_descriptor_sets: u32,
    pub max_push_constants_size: u32,
    pub descriptor_heap: DescriptorHeapLimits,
}

impl Limits {
    pub const fn downlevel_defaults() -> Self {
        Self {
            max_image_dimension_2d: 8_192,
            max_memory_allocation_count: 4_096,
            max_bound_descriptor_sets: 4,
            max_push_constants_size: 128,
            descriptor_heap: DescriptorHeapLimits::default_required(),
        }
    }

    pub fn failures_against(self, supported: Self) -> Vec<String> {
        let mut failures = Vec::new();
        for (name, required, available) in [
            (
                "max_image_dimension_2d",
                self.max_image_dimension_2d as u64,
                supported.max_image_dimension_2d as u64,
            ),
            (
                "max_memory_allocation_count",
                self.max_memory_allocation_count as u64,
                supported.max_memory_allocation_count as u64,
            ),
            (
                "max_bound_descriptor_sets",
                self.max_bound_descriptor_sets as u64,
                supported.max_bound_descriptor_sets as u64,
            ),
            (
                "max_push_constants_size",
                self.max_push_constants_size as u64,
                supported.max_push_constants_size as u64,
            ),
        ] {
            if required > available {
                failures.push(format!(
                    "{name} requires {required}, adapter exposes {available}"
                ));
            }
        }
        failures.extend(
            self.descriptor_heap
                .failures_against(supported.descriptor_heap),
        );
        failures
    }
}

/// Limits and alignment rules exposed by `VK_EXT_descriptor_heap`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DescriptorHeapLimits {
    pub sampler_heap_alignment: u64,
    pub resource_heap_alignment: u64,
    pub max_sampler_heap_size: u64,
    pub max_resource_heap_size: u64,
    pub min_sampler_heap_reserved_range: u64,
    pub min_sampler_heap_reserved_range_with_embedded: u64,
    pub min_resource_heap_reserved_range: u64,
    pub sampler_descriptor_size: u64,
    pub image_descriptor_size: u64,
    pub buffer_descriptor_size: u64,
    pub sampler_descriptor_alignment: u64,
    pub image_descriptor_alignment: u64,
    pub buffer_descriptor_alignment: u64,
    pub max_push_data_size: u64,
    pub max_embedded_samplers: u32,
}

impl DescriptorHeapLimits {
    pub const fn default_required() -> Self {
        Self::zero()
    }

    const fn zero() -> Self {
        Self {
            sampler_heap_alignment: 0,
            resource_heap_alignment: 0,
            max_sampler_heap_size: 0,
            max_resource_heap_size: 0,
            min_sampler_heap_reserved_range: 0,
            min_sampler_heap_reserved_range_with_embedded: 0,
            min_resource_heap_reserved_range: 0,
            sampler_descriptor_size: 0,
            image_descriptor_size: 0,
            buffer_descriptor_size: 0,
            sampler_descriptor_alignment: 0,
            image_descriptor_alignment: 0,
            buffer_descriptor_alignment: 0,
            max_push_data_size: 0,
            max_embedded_samplers: 0,
        }
    }

    /// Exact resource-array stride emitted by Slang's
    /// `-spirv-unified-descriptor-heap-stride` ABI.
    ///
    /// `VK_EXT_descriptor_heap` requires descriptor sizes to be powers of two
    /// and each descriptor alignment to be a power of two no larger than its
    /// matching size. Under those requirements the larger descriptor size is
    /// aligned for both image and buffer descriptors without host-side
    /// rounding that would diverge from SPIR-V `ArrayStride`.
    pub const fn unified_resource_descriptor_stride(self) -> Option<u64> {
        if !descriptor_layout_is_valid(self.image_descriptor_size, self.image_descriptor_alignment)
            || !descriptor_layout_is_valid(
                self.buffer_descriptor_size,
                self.buffer_descriptor_alignment,
            )
        {
            return None;
        }
        Some(max_u64(
            self.image_descriptor_size,
            self.buffer_descriptor_size,
        ))
    }

    /// Exact sampler-array stride emitted by Slang's descriptor-heap ABI.
    pub const fn sampler_descriptor_stride(self) -> Option<u64> {
        if descriptor_layout_is_valid(
            self.sampler_descriptor_size,
            self.sampler_descriptor_alignment,
        ) {
            Some(self.sampler_descriptor_size)
        } else {
            None
        }
    }

    /// Whether the advertised heap can hold real descriptors after the
    /// implementation-reserved ranges while obeying every power-of-two
    /// alignment constraint.
    pub const fn is_usable(self) -> bool {
        self.sampler_heap_alignment.is_power_of_two()
            && self.resource_heap_alignment.is_power_of_two()
            && self.sampler_descriptor_stride().is_some()
            && self.unified_resource_descriptor_stride().is_some()
            && self.max_push_data_size > 0
            && self.max_embedded_samplers > 0
            && range_has_payload(
                self.max_sampler_heap_size,
                max_u64(
                    self.min_sampler_heap_reserved_range,
                    self.min_sampler_heap_reserved_range_with_embedded,
                ),
                self.sampler_heap_alignment,
            )
            && range_has_payload(
                self.max_resource_heap_size,
                self.min_resource_heap_reserved_range,
                self.resource_heap_alignment,
            )
    }

    fn failures_against(self, supported: Self) -> Vec<String> {
        let mut failures = Vec::new();
        for (name, required, available) in [
            (
                "max_sampler_heap_size",
                self.max_sampler_heap_size,
                supported.max_sampler_heap_size,
            ),
            (
                "max_resource_heap_size",
                self.max_resource_heap_size,
                supported.max_resource_heap_size,
            ),
            (
                "max_push_data_size",
                self.max_push_data_size,
                supported.max_push_data_size,
            ),
            (
                "max_embedded_samplers",
                self.max_embedded_samplers as u64,
                supported.max_embedded_samplers as u64,
            ),
        ] {
            if required > available {
                failures.push(format!(
                    "descriptor_heap.{name} requires {required}, adapter exposes {available}"
                ));
            }
        }
        for (name, required, available) in [
            (
                "sampler_heap_alignment",
                self.sampler_heap_alignment,
                supported.sampler_heap_alignment,
            ),
            (
                "resource_heap_alignment",
                self.resource_heap_alignment,
                supported.resource_heap_alignment,
            ),
            (
                "sampler_descriptor_alignment",
                self.sampler_descriptor_alignment,
                supported.sampler_descriptor_alignment,
            ),
            (
                "image_descriptor_alignment",
                self.image_descriptor_alignment,
                supported.image_descriptor_alignment,
            ),
            (
                "buffer_descriptor_alignment",
                self.buffer_descriptor_alignment,
                supported.buffer_descriptor_alignment,
            ),
        ] {
            if required != 0 && (available == 0 || available > required) {
                failures.push(format!("descriptor_heap.{name} requires alignment <= {required}, adapter exposes {available}"));
            }
        }
        failures
    }
}

const fn descriptor_layout_is_valid(size: u64, alignment: u64) -> bool {
    size.is_power_of_two() && alignment.is_power_of_two() && alignment <= size
}

const fn max_u64(left: u64, right: u64) -> u64 {
    if left > right { left } else { right }
}

const fn range_has_payload(maximum: u64, reserved: u64, alignment: u64) -> bool {
    if alignment == 0 || !alignment.is_power_of_two() || reserved >= maximum {
        return false;
    }
    let remainder = reserved % alignment;
    let padding = (alignment - remainder) % alignment;
    reserved.saturating_add(padding) < maximum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vulkan_14_core_requires_submission_and_dynamic_rendering_features() {
        let ready = CoreFeatures {
            timeline_semaphore: true,
            synchronization2: true,
            dynamic_rendering: true,
            maintenance5: true,
            ..CoreFeatures::default()
        };
        assert!(ready.renderer_ready());
        assert!(
            ready
                .rejection_reasons(Version::V1_4_0, BackendProfile::Vulkan14, &BTreeSet::new())
                .is_empty()
        );
    }

    #[test]
    fn roadmap_profile_uses_the_published_revision_11_api_patch() {
        assert_eq!(ROADMAP_2026_PROFILE_REVISION, 11);
        assert_eq!(ROADMAP_2026_API_VERSION, Version::new(1, 4, 328));
        assert!(ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS.contains(&"VK_KHR_pipeline_binary"));
    }

    #[test]
    fn descriptor_heap_requires_payload_beside_aligned_reserved_ranges() {
        let usable = DescriptorHeapLimits {
            sampler_heap_alignment: 32,
            resource_heap_alignment: 64,
            max_sampler_heap_size: 4096,
            max_resource_heap_size: 8192,
            min_sampler_heap_reserved_range: 64,
            min_sampler_heap_reserved_range_with_embedded: 96,
            min_resource_heap_reserved_range: 128,
            sampler_descriptor_size: 32,
            image_descriptor_size: 32,
            buffer_descriptor_size: 32,
            sampler_descriptor_alignment: 32,
            image_descriptor_alignment: 32,
            buffer_descriptor_alignment: 32,
            max_push_data_size: 64,
            max_embedded_samplers: 1,
        };
        assert!(usable.is_usable());
        assert_eq!(usable.unified_resource_descriptor_stride(), Some(32));
        assert_eq!(usable.sampler_descriptor_stride(), Some(32));
        assert!(
            !DescriptorHeapLimits {
                max_resource_heap_size: 128,
                ..usable
            }
            .is_usable()
        );
        assert!(
            !DescriptorHeapLimits {
                resource_heap_alignment: 48,
                ..usable
            }
            .is_usable()
        );
        assert!(
            !DescriptorHeapLimits {
                image_descriptor_size: 24,
                ..usable
            }
            .is_usable()
        );
        assert_eq!(
            DescriptorHeapLimits {
                image_descriptor_size: 32,
                image_descriptor_alignment: 8,
                buffer_descriptor_size: 16,
                buffer_descriptor_alignment: 16,
                ..usable
            }
            .unified_resource_descriptor_stride(),
            Some(32)
        );
    }
}

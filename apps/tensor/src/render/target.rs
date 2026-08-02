use vulkan_renderer::{ApiVersion, ROADMAP_2026_API_VERSION};

use super::device::{DeviceSelector, DrmNodeId, GpuPreference};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererTarget {
    pub api_version: ApiVersion,
    pub descriptor_heap: DescriptorHeapTarget,
    pub device: DeviceSelector,
}

impl Default for RendererTarget {
    fn default() -> Self {
        Self {
            api_version: ROADMAP_2026_API_VERSION,
            descriptor_heap: DescriptorHeapTarget::Required,
            device: DeviceSelector::new(GpuPreference::default()),
        }
    }
}

impl RendererTarget {
    pub const fn with_device(preference: GpuPreference, drm_node: Option<DrmNodeId>) -> Self {
        Self {
            api_version: ROADMAP_2026_API_VERSION,
            descriptor_heap: DescriptorHeapTarget::Required,
            device: DeviceSelector::new(preference).with_drm_node(drm_node),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorHeapTarget {
    /// `VK_EXT_descriptor_heap` is the initial renderer contract.
    Required,
}

impl DescriptorHeapTarget {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Required => "descriptor heap (VK_EXT_descriptor_heap, required)",
        }
    }
}

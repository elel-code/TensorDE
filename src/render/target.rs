use vulkanalia::{Version, vk};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererTarget {
    pub api_version: Version,
    pub descriptor_heap: DescriptorHeapTarget,
}

impl Default for RendererTarget {
    fn default() -> Self {
        Self {
            api_version: Version::V1_4_0,
            descriptor_heap: DescriptorHeapTarget::Required,
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

    #[allow(dead_code)]
    pub fn extension(self) -> vk::ExtensionName {
        match self {
            Self::Required => vk::EXT_DESCRIPTOR_HEAP_EXTENSION.name,
        }
    }
}

use std::fmt;

use vulkanalia::vk;

/// Intended CPU/GPU access pattern. It affects memory-type ranking but never
/// weakens the hard Vulkan memory-property requirements.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryLocation {
    /// GPU-only resources. Host-visible types are allowed only when they are
    /// also device-local and no better type exists.
    Device,
    /// Persistently mapped upload/staging resources.
    Upload,
    /// GPU-to-CPU readback resources, preferring host-cached memory.
    Readback,
}

/// One Vulkan physical-device memory type in a stable, testable form.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryTypeInfo {
    pub index: u32,
    pub heap_index: u32,
    pub properties: vk::MemoryPropertyFlags,
    pub heap_size: u64,
}

/// Requirements returned by `vkGet*MemoryRequirements`, plus the device's
/// non-coherent atom size needed by mapped-range flushing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AllocationRequirements {
    pub size: u64,
    pub alignment: u64,
    pub memory_type_bits: u32,
    pub non_coherent_atom_size: u64,
}

/// Selected memory type and aligned allocation size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryPlan {
    pub memory_type_index: u32,
    pub heap_index: u32,
    pub properties: vk::MemoryPropertyFlags,
    pub allocation_size: u64,
    pub alignment: u64,
    pub flush_atom_size: Option<u64>,
    pub invalidate_atom_size: Option<u64>,
}

impl MemoryPlan {
    pub const fn host_visible(self) -> bool {
        self.properties
            .contains(vk::MemoryPropertyFlags::HOST_VISIBLE)
    }

    pub const fn host_coherent(self) -> bool {
        self.properties
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
    }
}

/// Pure memory-type selector. Keeping policy separate from Vulkan allocation
/// makes device choice and allocator behavior deterministic and testable.
#[derive(Clone, Debug, Default)]
pub struct MemoryTypeSelector {
    types: Vec<MemoryTypeInfo>,
}

impl MemoryTypeSelector {
    pub fn new(types: impl IntoIterator<Item = MemoryTypeInfo>) -> Self {
        Self {
            types: types.into_iter().collect(),
        }
    }

    pub fn select(
        &self,
        requirements: AllocationRequirements,
        location: MemoryLocation,
    ) -> Result<MemoryPlan, MemoryPlanError> {
        if requirements.size == 0 {
            return Err(MemoryPlanError::ZeroSize);
        }
        if requirements.alignment == 0 || !requirements.alignment.is_power_of_two() {
            return Err(MemoryPlanError::InvalidAlignment(requirements.alignment));
        }
        let allocation_size = align_up(requirements.size, requirements.alignment)
            .ok_or(MemoryPlanError::SizeOverflow)?;
        let required = match location {
            MemoryLocation::Device => vk::MemoryPropertyFlags::DEVICE_LOCAL,
            MemoryLocation::Upload | MemoryLocation::Readback => {
                vk::MemoryPropertyFlags::HOST_VISIBLE
            }
        };
        let selected = self
            .types
            .iter()
            .copied()
            .filter(|memory| requirements.memory_type_bits & (1 << memory.index) != 0)
            .filter(|memory| memory.properties.contains(required))
            .filter(|memory| {
                !memory
                    .properties
                    .contains(vk::MemoryPropertyFlags::LAZILY_ALLOCATED)
            })
            .filter(|memory| memory.heap_size >= allocation_size)
            .min_by_key(|memory| memory_score(*memory, location))
            .ok_or(MemoryPlanError::NoCompatibleMemoryType {
                memory_type_bits: requirements.memory_type_bits,
                location,
            })?;
        let non_coherent = selected
            .properties
            .contains(vk::MemoryPropertyFlags::HOST_VISIBLE)
            && !selected
                .properties
                .contains(vk::MemoryPropertyFlags::HOST_COHERENT);
        let atom = non_coherent.then_some(requirements.non_coherent_atom_size.max(1));
        Ok(MemoryPlan {
            memory_type_index: selected.index,
            heap_index: selected.heap_index,
            properties: selected.properties,
            allocation_size,
            alignment: requirements.alignment,
            flush_atom_size: atom,
            invalidate_atom_size: atom,
        })
    }
}

fn memory_score(memory: MemoryTypeInfo, location: MemoryLocation) -> (u8, u8, u8, u32) {
    let flags = memory.properties;
    let coherent = flags.contains(vk::MemoryPropertyFlags::HOST_COHERENT);
    let cached = flags.contains(vk::MemoryPropertyFlags::HOST_CACHED);
    let device_local = flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL);
    match location {
        MemoryLocation::Device => (
            flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE) as u8,
            !device_local as u8,
            0,
            memory.index,
        ),
        MemoryLocation::Upload => (
            !coherent as u8,
            !device_local as u8,
            cached as u8,
            memory.index,
        ),
        MemoryLocation::Readback => (
            !cached as u8,
            !coherent as u8,
            !device_local as u8,
            memory.index,
        ),
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let mask = alignment.checked_sub(1)?;
    value.checked_add(mask).map(|value| value & !mask)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryPlanError {
    ZeroSize,
    InvalidAlignment(u64),
    SizeOverflow,
    NoCompatibleMemoryType {
        memory_type_bits: u32,
        location: MemoryLocation,
    },
}

impl fmt::Display for MemoryPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for MemoryPlanError {}

/// Backend-neutral buffer creation contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BufferDescriptor {
    pub label: Option<String>,
    pub size: u64,
    pub usage: vk::BufferUsageFlags,
    pub memory: MemoryLocation,
}

impl BufferDescriptor {
    pub fn validate(&self) -> Result<(), ResourceDescriptorError> {
        if self.size == 0 {
            return Err(ResourceDescriptorError::ZeroBufferSize);
        }
        if self.usage.is_empty() {
            return Err(ResourceDescriptorError::EmptyBufferUsage);
        }
        Ok(())
    }
}

/// Backend-neutral image creation contract. Surface/swapchain images are
/// imported through a separate ownership path and do not use this descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageDescriptor {
    pub label: Option<String>,
    pub image_type: vk::ImageType,
    pub format: vk::Format,
    pub extent: vk::Extent3D,
    pub mip_levels: u32,
    pub array_layers: u32,
    pub samples: vk::SampleCountFlags,
    pub tiling: vk::ImageTiling,
    pub usage: vk::ImageUsageFlags,
    pub memory: MemoryLocation,
}

impl ImageDescriptor {
    pub fn validate(&self) -> Result<(), ResourceDescriptorError> {
        if self.extent.width == 0 || self.extent.height == 0 || self.extent.depth == 0 {
            return Err(ResourceDescriptorError::ZeroImageExtent);
        }
        if self.mip_levels == 0 {
            return Err(ResourceDescriptorError::ZeroMipLevels);
        }
        if self.array_layers == 0 {
            return Err(ResourceDescriptorError::ZeroArrayLayers);
        }
        if self.samples.is_empty() || !self.samples.bits().is_power_of_two() {
            return Err(ResourceDescriptorError::InvalidSampleCount);
        }
        if self.usage.is_empty() {
            return Err(ResourceDescriptorError::EmptyImageUsage);
        }
        if self.memory != MemoryLocation::Device && self.tiling == vk::ImageTiling::OPTIMAL {
            return Err(ResourceDescriptorError::HostVisibleOptimalImage);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceDescriptorError {
    ZeroBufferSize,
    EmptyBufferUsage,
    ZeroImageExtent,
    ZeroMipLevels,
    ZeroArrayLayers,
    InvalidSampleCount,
    EmptyImageUsage,
    HostVisibleOptimalImage,
}

impl fmt::Display for ResourceDescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ResourceDescriptorError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(index: u32, properties: vk::MemoryPropertyFlags) -> MemoryTypeInfo {
        MemoryTypeInfo {
            index,
            heap_index: index,
            properties,
            heap_size: 1 << 30,
        }
    }

    #[test]
    fn device_memory_avoids_host_visible_bar_when_vram_exists() {
        let selector = MemoryTypeSelector::new([
            memory(
                0,
                vk::MemoryPropertyFlags::DEVICE_LOCAL | vk::MemoryPropertyFlags::HOST_VISIBLE,
            ),
            memory(1, vk::MemoryPropertyFlags::DEVICE_LOCAL),
        ]);
        let plan = selector
            .select(
                AllocationRequirements {
                    size: 1000,
                    alignment: 256,
                    memory_type_bits: 0b11,
                    non_coherent_atom_size: 256,
                },
                MemoryLocation::Device,
            )
            .unwrap();
        assert_eq!(plan.memory_type_index, 1);
        assert_eq!(plan.allocation_size, 1024);
    }

    #[test]
    fn upload_prefers_coherent_and_readback_prefers_cached() {
        let selector = MemoryTypeSelector::new([
            memory(0, vk::MemoryPropertyFlags::HOST_VISIBLE),
            memory(
                1,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            ),
            memory(
                2,
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::HOST_CACHED,
            ),
        ]);
        let requirements = AllocationRequirements {
            size: 4096,
            alignment: 64,
            memory_type_bits: 0b111,
            non_coherent_atom_size: 256,
        };
        assert_eq!(
            selector
                .select(requirements, MemoryLocation::Upload)
                .unwrap()
                .memory_type_index,
            1
        );
        assert_eq!(
            selector
                .select(requirements, MemoryLocation::Readback)
                .unwrap()
                .memory_type_index,
            2
        );
    }

    #[test]
    fn non_coherent_mapping_exposes_flush_and_invalidate_atoms() {
        let selector = MemoryTypeSelector::new([memory(0, vk::MemoryPropertyFlags::HOST_VISIBLE)]);
        let plan = selector
            .select(
                AllocationRequirements {
                    size: 1,
                    alignment: 1,
                    memory_type_bits: 1,
                    non_coherent_atom_size: 256,
                },
                MemoryLocation::Readback,
            )
            .unwrap();
        assert_eq!(plan.flush_atom_size, Some(256));
        assert_eq!(plan.invalidate_atom_size, Some(256));
    }

    #[test]
    fn resource_descriptors_reject_cpu_visible_optimal_images() {
        let image = ImageDescriptor {
            label: None,
            image_type: vk::ImageType::_2D,
            format: vk::Format::R8G8B8A8_UNORM,
            extent: vk::Extent3D {
                width: 64,
                height: 64,
                depth: 1,
            },
            mip_levels: 1,
            array_layers: 1,
            samples: vk::SampleCountFlags::_1,
            tiling: vk::ImageTiling::OPTIMAL,
            usage: vk::ImageUsageFlags::SAMPLED,
            memory: MemoryLocation::Upload,
        };
        assert_eq!(
            image.validate(),
            Err(ResourceDescriptorError::HostVisibleOptimalImage)
        );
    }
}

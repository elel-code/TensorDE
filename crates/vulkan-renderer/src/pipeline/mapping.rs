use std::ffi::CStr;
use std::fmt;

use vulkanalia::vk::{self, HasBuilder};

use crate::ShaderModule;

/// Heap offsets used by `HEAP_WITH_CONSTANT_OFFSET` shader mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstantOffsetMapping {
    pub resource_heap_offset: u32,
    pub resource_array_stride: u32,
    pub sampler_heap_offset: u32,
    pub sampler_array_stride: u32,
}

/// Maps a SPIR-V descriptor set/binding range directly to descriptor heaps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShaderBindingMapping {
    pub descriptor_set: u32,
    pub first_binding: u32,
    pub binding_count: u32,
    pub resource_mask: vk::SpirvResourceTypeFlagsEXT,
    pub constant_offset: ConstantOffsetMapping,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShaderBindingMap {
    mappings: Vec<ShaderBindingMapping>,
}

impl ShaderBindingMap {
    pub fn new(mut mappings: Vec<ShaderBindingMapping>) -> Result<Self, ShaderBindingMapError> {
        for mapping in &mappings {
            if mapping.binding_count == 0 {
                return Err(ShaderBindingMapError::ZeroBindingCount {
                    descriptor_set: mapping.descriptor_set,
                    first_binding: mapping.first_binding,
                });
            }
            if mapping.resource_mask.is_empty() {
                return Err(ShaderBindingMapError::EmptyResourceMask {
                    descriptor_set: mapping.descriptor_set,
                    first_binding: mapping.first_binding,
                });
            }
            mapping
                .first_binding
                .checked_add(mapping.binding_count)
                .ok_or(ShaderBindingMapError::BindingRangeOverflow {
                    descriptor_set: mapping.descriptor_set,
                    first_binding: mapping.first_binding,
                    binding_count: mapping.binding_count,
                })?;
        }
        mappings.sort_by_key(|mapping| (mapping.descriptor_set, mapping.first_binding));
        for pair in mappings.windows(2) {
            let previous = pair[0];
            let next = pair[1];
            if previous.descriptor_set == next.descriptor_set
                && previous.first_binding + previous.binding_count > next.first_binding
            {
                return Err(ShaderBindingMapError::OverlappingBindings {
                    descriptor_set: next.descriptor_set,
                    first_binding: next.first_binding,
                });
            }
        }
        Ok(Self { mappings })
    }

    pub fn mappings(&self) -> &[ShaderBindingMapping] {
        &self.mappings
    }

    /// Builds Vulkan's borrowed pNext chain only for the duration of `use_stage`.
    /// This prevents mapping pointers from escaping their backing vectors.
    pub fn with_stage_create_info<T>(
        &self,
        stage: vk::ShaderStageFlags,
        module: &ShaderModule,
        entry_point: &CStr,
        use_stage: impl FnOnce(&vk::PipelineShaderStageCreateInfo) -> T,
    ) -> Result<T, ShaderBindingMapError> {
        if stage.is_empty() || stage.bits().count_ones() != 1 {
            return Err(ShaderBindingMapError::InvalidShaderStage(stage));
        }
        let mappings = self
            .mappings
            .iter()
            .copied()
            .map(to_vk_mapping)
            .collect::<Vec<_>>();
        let base = vk::PipelineShaderStageCreateInfo::builder()
            .stage(stage)
            .module(module.raw())
            .name(entry_point.to_bytes_with_nul());
        if mappings.is_empty() {
            let stage = base.build();
            return Ok(use_stage(&stage));
        }
        let mut mapping_info = vk::ShaderDescriptorSetAndBindingMappingInfoEXT::builder()
            .mappings(&mappings)
            .build();
        let stage = base.push_next(&mut mapping_info).build();
        Ok(use_stage(&stage))
    }
}

fn to_vk_mapping(mapping: ShaderBindingMapping) -> vk::DescriptorSetAndBindingMappingEXT {
    let source = vk::DescriptorMappingSourceConstantOffsetEXT::builder()
        .heap_offset(mapping.constant_offset.resource_heap_offset)
        .heap_array_stride(mapping.constant_offset.resource_array_stride)
        .sampler_heap_offset(mapping.constant_offset.sampler_heap_offset)
        .sampler_heap_array_stride(mapping.constant_offset.sampler_array_stride)
        .build();
    vk::DescriptorSetAndBindingMappingEXT::builder()
        .descriptor_set(mapping.descriptor_set)
        .first_binding(mapping.first_binding)
        .binding_count(mapping.binding_count)
        .resource_mask(mapping.resource_mask)
        .source(vk::DescriptorMappingSourceEXT::HEAP_WITH_CONSTANT_OFFSET)
        .source_data(vk::DescriptorMappingSourceDataEXT {
            constant_offset: source,
        })
        .build()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderBindingMapError {
    ZeroBindingCount {
        descriptor_set: u32,
        first_binding: u32,
    },
    EmptyResourceMask {
        descriptor_set: u32,
        first_binding: u32,
    },
    BindingRangeOverflow {
        descriptor_set: u32,
        first_binding: u32,
        binding_count: u32,
    },
    OverlappingBindings {
        descriptor_set: u32,
        first_binding: u32,
    },
    InvalidShaderStage(vk::ShaderStageFlags),
}

impl fmt::Display for ShaderBindingMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ShaderBindingMapError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(first_binding: u32, binding_count: u32) -> ShaderBindingMapping {
        ShaderBindingMapping {
            descriptor_set: 0,
            first_binding,
            binding_count,
            resource_mask: vk::SpirvResourceTypeFlagsEXT::UNIFORM_BUFFER,
            constant_offset: ConstantOffsetMapping {
                resource_heap_offset: first_binding * 32,
                resource_array_stride: 32,
                sampler_heap_offset: 0,
                sampler_array_stride: 0,
            },
        }
    }

    #[test]
    fn mapping_order_is_canonical_and_overlap_is_rejected() {
        let map = ShaderBindingMap::new(vec![mapping(4, 1), mapping(1, 2)]).unwrap();
        assert_eq!(map.mappings()[0].first_binding, 1);
        assert_eq!(map.mappings()[1].first_binding, 4);
        assert_eq!(
            ShaderBindingMap::new(vec![mapping(1, 2), mapping(2, 1)]),
            Err(ShaderBindingMapError::OverlappingBindings {
                descriptor_set: 0,
                first_binding: 2,
            })
        );
    }

    #[test]
    fn zero_count_and_empty_resource_mask_are_rejected() {
        assert!(matches!(
            ShaderBindingMap::new(vec![mapping(0, 0)]),
            Err(ShaderBindingMapError::ZeroBindingCount { .. })
        ));
        let mut empty = mapping(0, 1);
        empty.resource_mask = vk::SpirvResourceTypeFlagsEXT::empty();
        assert!(matches!(
            ShaderBindingMap::new(vec![empty]),
            Err(ShaderBindingMapError::EmptyResourceMask { .. })
        ));
    }
}

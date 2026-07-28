use std::ffi::CStr;
use std::fmt;

use vulkanalia::vk::{self, HasBuilder};

use crate::{DescriptorHeapLimits, ShaderModule};

/// Heap offsets used by `HEAP_WITH_CONSTANT_OFFSET` shader mapping.
///
/// `heap_offset` addresses the heap selected by the mapped SPIR-V resource:
/// the sampler heap for `OpTypeSampler`, otherwise the resource heap. The
/// `sampler_heap_*` fields are only the sampler half of an
/// `OpTypeSampledImage` (combined sampled image), not the offset of a separate
/// sampler binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstantOffsetMapping {
    pub heap_offset: u32,
    pub heap_array_stride: u32,
    pub sampler_heap_offset: u32,
    pub sampler_heap_array_stride: u32,
}

/// Heap mapping indexed by one `u32` stored in descriptor-heap push data.
///
/// The descriptor byte offset is `heap_offset + index * heap_index_stride +
/// shader_index * heap_array_stride`. The standard deliberately uses separate
/// image and sampler declarations; combined sampled images are rejected here
/// rather than silently filling their second heap calculation with zeros.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PushIndexMapping {
    pub heap_offset: u32,
    pub push_offset: u32,
    pub heap_index_stride: u32,
    pub heap_array_stride: u32,
}

/// Heap mapping indexed through a GPU address stored in descriptor-heap push
/// data.
///
/// `push_offset` selects an aligned `VkDeviceAddress`; `address_offset` then
/// selects a `u32` index in that immutable in-flight memory. The index is
/// applied using the same byte-stride formula as [`PushIndexMapping`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndirectIndexMapping {
    pub heap_offset: u32,
    pub push_offset: u32,
    pub address_offset: u32,
    pub heap_index_stride: u32,
    pub heap_array_stride: u32,
}

/// Exactly one mapping source for a SPIR-V binding range.
///
/// This enum mirrors Vulkan's tagged `source`/`sourceData` pair, preventing a
/// caller from constructing a source tag with the wrong union member.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderBindingSource {
    ConstantOffset(ConstantOffsetMapping),
    PushIndex(PushIndexMapping),
    IndirectIndex(IndirectIndexMapping),
}

/// Maps a SPIR-V descriptor set/binding range directly to descriptor heaps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShaderBindingMapping {
    pub descriptor_set: u32,
    pub first_binding: u32,
    pub binding_count: u32,
    pub resource_mask: vk::SpirvResourceTypeFlagsEXT,
    pub source: ShaderBindingSource,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShaderBindingMap {
    mappings: Vec<ShaderBindingMapping>,
}

impl ShaderBindingMap {
    pub fn new(mut mappings: Vec<ShaderBindingMapping>) -> Result<Self, ShaderBindingMapError> {
        for mapping in &mappings {
            validate_mapping(mapping)?;
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

    /// Validates device-dependent descriptor alignment and push-data limits.
    /// Pipeline creation calls this automatically for the selected adapter.
    pub fn validate_for_device(
        &self,
        limits: DescriptorHeapLimits,
    ) -> Result<(), ShaderBindingMapError> {
        for mapping in &self.mappings {
            validate_source_for_device(mapping, limits)?;
        }
        Ok(())
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

fn validate_mapping(mapping: &ShaderBindingMapping) -> Result<(), ShaderBindingMapError> {
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

    match mapping.source {
        ShaderBindingSource::ConstantOffset(_) => {}
        ShaderBindingSource::PushIndex(source) => {
            reject_combined_dynamic_mapping(mapping)?;
            require_aligned("push index", "push_offset", source.push_offset, 4)?;
        }
        ShaderBindingSource::IndirectIndex(source) => {
            reject_combined_dynamic_mapping(mapping)?;
            require_aligned("indirect index", "push_offset", source.push_offset, 8)?;
            require_aligned("indirect index", "address_offset", source.address_offset, 4)?;
        }
    }
    Ok(())
}

fn reject_combined_dynamic_mapping(
    mapping: &ShaderBindingMapping,
) -> Result<(), ShaderBindingMapError> {
    if mapping
        .resource_mask
        .contains(vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE)
    {
        return Err(
            ShaderBindingMapError::CombinedSampledImageNeedsSamplerMapping {
                descriptor_set: mapping.descriptor_set,
                first_binding: mapping.first_binding,
            },
        );
    }
    Ok(())
}

fn validate_source_for_device(
    mapping: &ShaderBindingMapping,
    limits: DescriptorHeapLimits,
) -> Result<(), ShaderBindingMapError> {
    match mapping.source {
        ShaderBindingSource::ConstantOffset(source) => {
            validate_primary_heap_alignment(
                mapping.resource_mask,
                source.heap_offset,
                source.heap_array_stride,
                limits,
            )?;
            if mapping
                .resource_mask
                .contains(vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE)
            {
                require_heap_aligned(
                    "sampler_heap_offset",
                    source.sampler_heap_offset,
                    limits.sampler_descriptor_alignment,
                )?;
                require_heap_aligned(
                    "sampler_heap_array_stride",
                    source.sampler_heap_array_stride,
                    limits.sampler_descriptor_alignment,
                )?;
            }
        }
        ShaderBindingSource::PushIndex(source) => {
            validate_push_range(
                "push_offset",
                source.push_offset,
                4,
                limits.max_push_data_size,
            )?;
            validate_primary_heap_alignment(
                mapping.resource_mask,
                source.heap_offset,
                source.heap_array_stride,
                limits,
            )?;
        }
        ShaderBindingSource::IndirectIndex(source) => {
            validate_push_range(
                "push_offset",
                source.push_offset,
                8,
                limits.max_push_data_size,
            )?;
            validate_primary_heap_alignment(
                mapping.resource_mask,
                source.heap_offset,
                source.heap_array_stride,
                limits,
            )?;
        }
    }
    Ok(())
}

fn validate_primary_heap_alignment(
    resource_mask: vk::SpirvResourceTypeFlagsEXT,
    heap_offset: u32,
    heap_array_stride: u32,
    limits: DescriptorHeapLimits,
) -> Result<(), ShaderBindingMapError> {
    let image_mask = vk::SpirvResourceTypeFlagsEXT::SAMPLED_IMAGE
        | vk::SpirvResourceTypeFlagsEXT::READ_ONLY_IMAGE
        | vk::SpirvResourceTypeFlagsEXT::READ_WRITE_IMAGE
        | vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE;
    let buffer_mask = vk::SpirvResourceTypeFlagsEXT::UNIFORM_BUFFER
        | vk::SpirvResourceTypeFlagsEXT::READ_ONLY_STORAGE_BUFFER
        | vk::SpirvResourceTypeFlagsEXT::READ_WRITE_STORAGE_BUFFER
        | vk::SpirvResourceTypeFlagsEXT::ACCELERATION_STRUCTURE;
    for alignment in [
        resource_mask
            .intersects(image_mask)
            .then_some(limits.image_descriptor_alignment),
        resource_mask
            .intersects(buffer_mask)
            .then_some(limits.buffer_descriptor_alignment),
        resource_mask
            .contains(vk::SpirvResourceTypeFlagsEXT::SAMPLER)
            .then_some(limits.sampler_descriptor_alignment),
    ]
    .into_iter()
    .flatten()
    {
        require_heap_aligned("heap_offset", heap_offset, alignment)?;
        require_heap_aligned("heap_array_stride", heap_array_stride, alignment)?;
    }
    Ok(())
}

fn require_aligned(
    source: &'static str,
    field: &'static str,
    offset: u32,
    alignment: u32,
) -> Result<(), ShaderBindingMapError> {
    if !offset.is_multiple_of(alignment) {
        return Err(ShaderBindingMapError::MisalignedSourceOffset {
            source,
            field,
            offset,
            required_alignment: alignment,
        });
    }
    Ok(())
}

fn require_heap_aligned(
    field: &'static str,
    value: u32,
    alignment: u64,
) -> Result<(), ShaderBindingMapError> {
    if alignment == 0 || !u64::from(value).is_multiple_of(alignment) {
        return Err(ShaderBindingMapError::MisalignedHeapValue {
            field,
            value,
            required_alignment: alignment,
        });
    }
    Ok(())
}

fn validate_push_range(
    field: &'static str,
    offset: u32,
    size: u32,
    maximum: u64,
) -> Result<(), ShaderBindingMapError> {
    if u64::from(offset) + u64::from(size) > maximum {
        return Err(ShaderBindingMapError::PushDataRangeExceedsLimit {
            field,
            offset,
            size,
            maximum,
        });
    }
    Ok(())
}

fn to_vk_mapping(mapping: ShaderBindingMapping) -> vk::DescriptorSetAndBindingMappingEXT {
    let (source, source_data) = match mapping.source {
        ShaderBindingSource::ConstantOffset(mapping) => (
            vk::DescriptorMappingSourceEXT::HEAP_WITH_CONSTANT_OFFSET,
            vk::DescriptorMappingSourceDataEXT {
                constant_offset: vk::DescriptorMappingSourceConstantOffsetEXT::builder()
                    .heap_offset(mapping.heap_offset)
                    .heap_array_stride(mapping.heap_array_stride)
                    .sampler_heap_offset(mapping.sampler_heap_offset)
                    .sampler_heap_array_stride(mapping.sampler_heap_array_stride)
                    .build(),
            },
        ),
        ShaderBindingSource::PushIndex(mapping) => (
            vk::DescriptorMappingSourceEXT::HEAP_WITH_PUSH_INDEX,
            vk::DescriptorMappingSourceDataEXT {
                push_index: vk::DescriptorMappingSourcePushIndexEXT::builder()
                    .heap_offset(mapping.heap_offset)
                    .push_offset(mapping.push_offset)
                    .heap_index_stride(mapping.heap_index_stride)
                    .heap_array_stride(mapping.heap_array_stride)
                    .build(),
            },
        ),
        ShaderBindingSource::IndirectIndex(mapping) => (
            vk::DescriptorMappingSourceEXT::HEAP_WITH_INDIRECT_INDEX,
            vk::DescriptorMappingSourceDataEXT {
                indirect_index: vk::DescriptorMappingSourceIndirectIndexEXT::builder()
                    .heap_offset(mapping.heap_offset)
                    .push_offset(mapping.push_offset)
                    .address_offset(mapping.address_offset)
                    .heap_index_stride(mapping.heap_index_stride)
                    .heap_array_stride(mapping.heap_array_stride)
                    .build(),
            },
        ),
    };
    vk::DescriptorSetAndBindingMappingEXT::builder()
        .descriptor_set(mapping.descriptor_set)
        .first_binding(mapping.first_binding)
        .binding_count(mapping.binding_count)
        .resource_mask(mapping.resource_mask)
        .source(source)
        .source_data(source_data)
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
    CombinedSampledImageNeedsSamplerMapping {
        descriptor_set: u32,
        first_binding: u32,
    },
    MisalignedSourceOffset {
        source: &'static str,
        field: &'static str,
        offset: u32,
        required_alignment: u32,
    },
    MisalignedHeapValue {
        field: &'static str,
        value: u32,
        required_alignment: u64,
    },
    PushDataRangeExceedsLimit {
        field: &'static str,
        offset: u32,
        size: u32,
        maximum: u64,
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

    fn constant_mapping(first_binding: u32, binding_count: u32) -> ShaderBindingMapping {
        ShaderBindingMapping {
            descriptor_set: 0,
            first_binding,
            binding_count,
            resource_mask: vk::SpirvResourceTypeFlagsEXT::UNIFORM_BUFFER,
            source: ShaderBindingSource::ConstantOffset(ConstantOffsetMapping {
                heap_offset: first_binding * 32,
                heap_array_stride: 32,
                sampler_heap_offset: 0,
                sampler_heap_array_stride: 0,
            }),
        }
    }

    fn limits() -> DescriptorHeapLimits {
        DescriptorHeapLimits {
            sampler_descriptor_alignment: 16,
            image_descriptor_alignment: 32,
            buffer_descriptor_alignment: 16,
            max_push_data_size: 64,
            ..DescriptorHeapLimits::default()
        }
    }

    #[test]
    fn mapping_order_is_canonical_and_overlap_is_rejected() {
        let map =
            ShaderBindingMap::new(vec![constant_mapping(4, 1), constant_mapping(1, 2)]).unwrap();
        assert_eq!(map.mappings()[0].first_binding, 1);
        assert_eq!(map.mappings()[1].first_binding, 4);
        assert_eq!(
            ShaderBindingMap::new(vec![constant_mapping(1, 2), constant_mapping(2, 1)]),
            Err(ShaderBindingMapError::OverlappingBindings {
                descriptor_set: 0,
                first_binding: 2,
            })
        );
    }

    #[test]
    fn zero_count_and_empty_resource_mask_are_rejected() {
        assert!(matches!(
            ShaderBindingMap::new(vec![constant_mapping(0, 0)]),
            Err(ShaderBindingMapError::ZeroBindingCount { .. })
        ));
        let mut empty = constant_mapping(0, 1);
        empty.resource_mask = vk::SpirvResourceTypeFlagsEXT::empty();
        assert!(matches!(
            ShaderBindingMap::new(vec![empty]),
            Err(ShaderBindingMapError::EmptyResourceMask { .. })
        ));
    }

    #[test]
    fn push_index_obeys_u32_alignment_and_device_limit() {
        let mapping = |push_offset| ShaderBindingMapping {
            descriptor_set: 0,
            first_binding: 0,
            binding_count: 1,
            resource_mask: vk::SpirvResourceTypeFlagsEXT::SAMPLED_IMAGE,
            source: ShaderBindingSource::PushIndex(PushIndexMapping {
                heap_offset: 0,
                push_offset,
                heap_index_stride: 32,
                heap_array_stride: 0,
            }),
        };
        assert!(ShaderBindingMap::new(vec![mapping(2)]).is_err());
        let map = ShaderBindingMap::new(vec![mapping(60)]).unwrap();
        assert!(map.validate_for_device(limits()).is_ok());
        assert!(
            ShaderBindingMap::new(vec![mapping(64)])
                .unwrap()
                .validate_for_device(limits())
                .is_err()
        );
    }

    #[test]
    fn indirect_index_obeys_address_alignment_and_device_limit() {
        let mapping = |push_offset, address_offset| ShaderBindingMapping {
            descriptor_set: 1,
            first_binding: 2,
            binding_count: 1,
            resource_mask: vk::SpirvResourceTypeFlagsEXT::UNIFORM_BUFFER,
            source: ShaderBindingSource::IndirectIndex(IndirectIndexMapping {
                heap_offset: 16,
                push_offset,
                address_offset,
                heap_index_stride: 16,
                heap_array_stride: 0,
            }),
        };
        assert!(ShaderBindingMap::new(vec![mapping(4, 0)]).is_err());
        assert!(ShaderBindingMap::new(vec![mapping(8, 2)]).is_err());
        assert!(
            ShaderBindingMap::new(vec![mapping(56, 4)])
                .unwrap()
                .validate_for_device(limits())
                .is_ok()
        );
        assert!(
            ShaderBindingMap::new(vec![mapping(64, 4)])
                .unwrap()
                .validate_for_device(limits())
                .is_err()
        );
    }

    #[test]
    fn device_descriptor_alignment_is_checked_by_resource_class() {
        let image = ShaderBindingMapping {
            descriptor_set: 0,
            first_binding: 0,
            binding_count: 1,
            resource_mask: vk::SpirvResourceTypeFlagsEXT::SAMPLED_IMAGE,
            source: ShaderBindingSource::ConstantOffset(ConstantOffsetMapping {
                heap_offset: 16,
                heap_array_stride: 0,
                sampler_heap_offset: 0,
                sampler_heap_array_stride: 0,
            }),
        };
        assert!(
            ShaderBindingMap::new(vec![image])
                .unwrap()
                .validate_for_device(limits())
                .is_err()
        );
    }

    #[test]
    fn safe_dynamic_mapping_rejects_implicit_combined_sampler_fields() {
        let mapping = ShaderBindingMapping {
            descriptor_set: 0,
            first_binding: 0,
            binding_count: 1,
            resource_mask: vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE,
            source: ShaderBindingSource::PushIndex(PushIndexMapping {
                heap_offset: 0,
                push_offset: 0,
                heap_index_stride: 32,
                heap_array_stride: 0,
            }),
        };
        assert!(matches!(
            ShaderBindingMap::new(vec![mapping]),
            Err(ShaderBindingMapError::CombinedSampledImageNeedsSamplerMapping { .. })
        ));
    }

    #[test]
    fn tagged_sources_select_the_matching_vulkan_union_member() {
        let push = to_vk_mapping(ShaderBindingMapping {
            descriptor_set: 2,
            first_binding: 4,
            binding_count: 1,
            resource_mask: vk::SpirvResourceTypeFlagsEXT::SAMPLED_IMAGE,
            source: ShaderBindingSource::PushIndex(PushIndexMapping {
                heap_offset: 32,
                push_offset: 12,
                heap_index_stride: 32,
                heap_array_stride: 64,
            }),
        });
        assert_eq!(
            push.source,
            vk::DescriptorMappingSourceEXT::HEAP_WITH_PUSH_INDEX
        );
        // SAFETY: `source` identifies `push_index` as the active union member.
        let push_source = unsafe { push.source_data.push_index };
        assert_eq!(push_source.heap_offset, 32);
        assert_eq!(push_source.push_offset, 12);
        assert_eq!(push_source.heap_index_stride, 32);
        assert_eq!(push_source.heap_array_stride, 64);

        let indirect = to_vk_mapping(ShaderBindingMapping {
            source: ShaderBindingSource::IndirectIndex(IndirectIndexMapping {
                heap_offset: 64,
                push_offset: 16,
                address_offset: 20,
                heap_index_stride: 16,
                heap_array_stride: 0,
            }),
            ..constant_mapping(0, 1)
        });
        assert_eq!(
            indirect.source,
            vk::DescriptorMappingSourceEXT::HEAP_WITH_INDIRECT_INDEX
        );
        // SAFETY: `source` identifies `indirect_index` as the active member.
        let indirect_source = unsafe { indirect.source_data.indirect_index };
        assert_eq!(indirect_source.push_offset, 16);
        assert_eq!(indirect_source.address_offset, 20);
    }
}

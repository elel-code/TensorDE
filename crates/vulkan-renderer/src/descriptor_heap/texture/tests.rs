use super::*;

fn binding(image_offset: u64, sampler_offset: u64) -> SampledTextureBinding {
    SampledTextureBinding {
        image: SampledImageBinding {
            image: DescriptorAllocation {
                range: image_offset..image_offset + 32,
                allocator_id: 1,
            },
        },
        sampler: SamplerBinding {
            sampler: DescriptorAllocation {
                range: sampler_offset..sampler_offset + 16,
                allocator_id: 2,
            },
        },
    }
}

#[test]
fn sampled_texture_maps_separate_image_and_sampler_heap_offsets() {
    let map = binding(256, 64)
        .shader_binding_map(SampledTextureShaderBindings::new(2, 3, 7))
        .unwrap();
    assert_eq!(map.mappings().len(), 2);
    assert_eq!(map.mappings()[0].descriptor_set, 2);
    assert_eq!(map.mappings()[0].first_binding, 3);
    assert_eq!(
        map.mappings()[0].resource_mask,
        vk::SpirvResourceTypeFlagsEXT::SAMPLED_IMAGE
    );
    assert_eq!(
        map.mappings()[0].source,
        ShaderBindingSource::ConstantOffset(ConstantOffsetMapping {
            heap_offset: 256,
            heap_array_stride: 0,
            sampler_heap_offset: 0,
            sampler_heap_array_stride: 0,
        })
    );
    assert_eq!(map.mappings()[1].first_binding, 7);
    assert_eq!(
        map.mappings()[1].resource_mask,
        vk::SpirvResourceTypeFlagsEXT::SAMPLER
    );
    assert_eq!(
        map.mappings()[1].source,
        ShaderBindingSource::ConstantOffset(ConstantOffsetMapping {
            heap_offset: 64,
            heap_array_stride: 0,
            sampler_heap_offset: 0,
            sampler_heap_array_stride: 0,
        })
    );
}

#[test]
fn sampled_texture_rejects_overlapping_bindings_and_truncating_offsets() {
    let texture_binding = binding(0, 16);
    assert!(
        texture_binding
            .shader_binding_map(SampledTextureShaderBindings::new(0, 1, 1))
            .is_err()
    );
    assert!(
        binding(u64::from(u32::MAX) + 1, 16)
            .shader_binding_map(SampledTextureShaderBindings::new(0, 0, 1))
            .is_err()
    );
}

#[test]
fn sampled_texture_push_index_map_keeps_pipeline_independent_of_heap_slot() {
    let map = SampledTextureShaderBindings::new(2, 3, 7)
        .push_index_shader_binding_map(0, 4)
        .unwrap();
    assert_eq!(
        map.mappings()[0].source,
        ShaderBindingSource::PushIndex(PushIndexMapping {
            heap_offset: 0,
            push_offset: 0,
            heap_index_stride: 1,
            heap_array_stride: 0,
        })
    );
    assert_eq!(
        map.mappings()[1].source,
        ShaderBindingSource::PushIndex(PushIndexMapping {
            heap_offset: 0,
            push_offset: 4,
            heap_index_stride: 1,
            heap_array_stride: 0,
        })
    );
    assert_eq!(
        binding(256, 64).push_index_heap_offsets().unwrap(),
        SampledTextureHeapOffsets {
            image: 256,
            sampler: 64,
        }
    );
}

#[test]
fn standard_sampler_is_linear_clamp_without_anisotropy() {
    let sampler = SamplerDescriptor::linear_clamp().to_vk().unwrap();
    assert_eq!(sampler.mag_filter, vk::Filter::LINEAR);
    assert_eq!(sampler.min_filter, vk::Filter::LINEAR);
    assert_eq!(sampler.mipmap_mode, vk::SamplerMipmapMode::LINEAR);
    assert_eq!(
        sampler.address_mode_u,
        vk::SamplerAddressMode::CLAMP_TO_EDGE
    );
    assert_eq!(sampler.anisotropy_enable, vk::FALSE);
    assert_eq!(sampler.unnormalized_coordinates, vk::FALSE);
}

#[test]
fn standard_sampler_rejects_invalid_lod_ranges() {
    assert!(
        SamplerDescriptor {
            lod_min_clamp: 4.0,
            lod_max_clamp: 2.0,
            ..SamplerDescriptor::default()
        }
        .to_vk()
        .is_err()
    );
    assert!(
        SamplerDescriptor {
            mip_lod_bias: f32::NAN,
            ..SamplerDescriptor::default()
        }
        .to_vk()
        .is_err()
    );
}

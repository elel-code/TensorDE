use super::*;

#[path = "descriptor_writes/input_attachment.rs"]
mod input_attachment;
#[path = "descriptor_writes/storage_buffer.rs"]
mod storage_buffer;

pub(in crate::renderer::native_vulkan) use input_attachment::{
    native_vulkan_vulkanalia_descriptor_heap_resource_input_attachment_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_input_attachment_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_mixed_input_attachment_binding_mapping,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_input_attachment,
};
pub(in crate::renderer::native_vulkan) use storage_buffer::native_vulkan_vulkanalia_write_descriptor_heap_resource_storage_buffer;

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_write_descriptor_heap_uniform_buffer(
    device: &Device,
    resources: &mut VulkanaliaDescriptorHeapUniformBufferResources,
    buffer_index: usize,
    device_address: vk::DeviceAddress,
    range: u64,
) -> Result<(), String> {
    if device_address == 0 {
        return Err("descriptor heap uniform buffer requires non-zero device address".to_owned());
    }
    if range == 0 {
        return Err("descriptor heap uniform buffer requires non-zero range".to_owned());
    }
    let resource_offset = *resources
        .plan
        .buffer_descriptor_offsets
        .get(buffer_index)
        .ok_or_else(|| {
            format!("descriptor heap uniform buffer index {buffer_index} has no resource offset")
        })?;
    let descriptor_size = resources.plan.buffer_descriptor_size;
    let address_range = vk::DeviceAddressRangeEXT::builder()
        .address(device_address)
        .size(range)
        .build();
    let resource_info = vk::ResourceDescriptorInfoEXT::builder()
        .type_(vk::DescriptorType::UNIFORM_BUFFER)
        .data(vk::ResourceDescriptorDataEXT {
            address_range: &address_range,
        })
        .build();
    let resource_range = heap_host_address_range(
        &resources.resource_heap,
        resource_offset,
        descriptor_size,
        "uniform-buffer-resource-heap",
    )?;

    unsafe {
        device
            .write_resource_descriptors_ext(&[resource_info], &[resource_range])
            .map_err(|err| {
                format!("vkWriteResourceDescriptorsEXT(vulkanalia uniform buffer): {err:?}")
            })?;
    }
    flush_descriptor_heap_buffer(
        device,
        &resources.resource_heap,
        resource_offset,
        descriptor_size,
    )?;

    resources.snapshot.resource_descriptor_written = true;
    resources.snapshot.zero_copy_gate =
        "uniform buffer heap descriptor points at retained GPU uniform records";
    Ok(())
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer(
    device: &Device,
    resources: &mut VulkanaliaDescriptorHeapResourceResources,
    resource_descriptor_index: usize,
    device_address: vk::DeviceAddress,
    range: u64,
) -> Result<(), String> {
    if device_address == 0 {
        return Err(
            "descriptor heap mixed uniform buffer requires non-zero device address".to_owned(),
        );
    }
    if range == 0 {
        return Err("descriptor heap mixed uniform buffer requires non-zero range".to_owned());
    }
    validate_mixed_resource_descriptor_kind(
        resources,
        resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
    )?;
    let resource_offset = mixed_resource_descriptor_offset(resources, resource_descriptor_index)?;
    let descriptor_size = resources.plan.buffer_descriptor_size;
    let address_range = vk::DeviceAddressRangeEXT::builder()
        .address(device_address)
        .size(range)
        .build();
    let resource_info = vk::ResourceDescriptorInfoEXT::builder()
        .type_(vk::DescriptorType::UNIFORM_BUFFER)
        .data(vk::ResourceDescriptorDataEXT {
            address_range: &address_range,
        })
        .build();
    let resource_range = heap_host_address_range(
        &resources.resource_heap,
        resource_offset,
        descriptor_size,
        "mixed-resource-heap",
    )?;

    unsafe {
        device
            .write_resource_descriptors_ext(&[resource_info], &[resource_range])
            .map_err(|err| {
                format!("vkWriteResourceDescriptorsEXT(vulkanalia mixed uniform buffer): {err:?}")
            })?;
    }
    flush_descriptor_heap_buffer(
        device,
        &resources.resource_heap,
        resource_offset,
        descriptor_size,
    )?;

    resources.snapshot.resource_descriptors_written = resources
        .snapshot
        .resource_descriptors_written
        .saturating_add(1);
    Ok(())
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler(
    device: &Device,
    resources: &mut VulkanaliaDescriptorHeapResourceResources,
    resource_descriptor_index: usize,
    sampler_descriptor_index: usize,
    image_view_info: &vk::ImageViewCreateInfo,
    image_layout: vk::ImageLayout,
    sampler_info: &vk::SamplerCreateInfo,
) -> Result<(), String> {
    validate_mixed_resource_descriptor_kind(
        resources,
        resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
    )?;
    let sampler_heap = resources
        .sampler_heap
        .as_ref()
        .ok_or_else(|| "descriptor heap mixed image sampler requires a sampler heap".to_owned())?;
    let resource_offset = mixed_resource_descriptor_offset(resources, resource_descriptor_index)?;
    let sampler_offset = *resources
        .plan
        .sampler_descriptor_offsets
        .get(sampler_descriptor_index)
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed sampler descriptor index {sampler_descriptor_index} has no sampler offset"
            )
        })?;
    let image_descriptor_size = resources.plan.image_descriptor_size;
    let sampler_descriptor_size = resources.plan.sampler_descriptor_size;
    let image_descriptor = vk::ImageDescriptorInfoEXT::builder()
        .view(image_view_info)
        .layout(image_layout)
        .build();
    let resource_info = vk::ResourceDescriptorInfoEXT::builder()
        .type_(vk::DescriptorType::SAMPLED_IMAGE)
        .data(vk::ResourceDescriptorDataEXT {
            image: &image_descriptor,
        })
        .build();
    let resource_range = heap_host_address_range(
        &resources.resource_heap,
        resource_offset,
        image_descriptor_size,
        "mixed-resource-heap",
    )?;
    let sampler_range = heap_host_address_range(
        sampler_heap,
        sampler_offset,
        sampler_descriptor_size,
        "mixed-sampler-heap",
    )?;

    unsafe {
        device
            .write_resource_descriptors_ext(&[resource_info], &[resource_range])
            .map_err(|err| {
                format!("vkWriteResourceDescriptorsEXT(vulkanalia mixed sampled image): {err:?}")
            })?;
        device
            .write_sampler_descriptors_ext(&[*sampler_info], &[sampler_range])
            .map_err(|err| {
                format!("vkWriteSamplerDescriptorsEXT(vulkanalia mixed sampler): {err:?}")
            })?;
    }
    flush_descriptor_heap_buffer(
        device,
        &resources.resource_heap,
        resource_offset,
        image_descriptor_size,
    )?;
    flush_descriptor_heap_buffer(
        device,
        sampler_heap,
        sampler_offset,
        sampler_descriptor_size,
    )?;

    resources.snapshot.resource_descriptors_written = resources
        .snapshot
        .resource_descriptors_written
        .saturating_add(1);
    resources.snapshot.sampler_descriptors_written = resources
        .snapshot
        .sampler_descriptors_written
        .saturating_add(1);
    Ok(())
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    image_index: usize,
) -> Result<NativeVulkanDescriptorHeapShaderBindingMapping, String> {
    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
        plan,
        0,
        image_index,
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    binding: u32,
    image_index: usize,
) -> Result<NativeVulkanDescriptorHeapShaderBindingMapping, String> {
    let heap_offset = descriptor_offset_u32(&plan.image_descriptor_offsets, image_index, "image")?;
    let sampler_heap_offset =
        descriptor_offset_u32(&plan.sampler_descriptor_offsets, image_index, "sampler")?;
    let heap_array_stride = u32::try_from(plan.image_descriptor_stride)
        .map_err(|_| "descriptor heap image stride exceeds u32".to_owned())?;
    let sampler_heap_array_stride = u32::try_from(plan.sampler_descriptor_stride)
        .map_err(|_| "descriptor heap sampler stride exceeds u32".to_owned())?;
    let source = vk::DescriptorMappingSourceConstantOffsetEXT::builder()
        .heap_offset(heap_offset)
        .heap_array_stride(heap_array_stride)
        .sampler_heap_offset(sampler_heap_offset)
        .sampler_heap_array_stride(sampler_heap_array_stride)
        .build();

    Ok(
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping(
            binding,
            vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE,
            vk::DescriptorMappingSourceDataEXT {
                constant_offset: source,
            },
        ),
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_binding_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanSnapshot,
    binding: u32,
    buffer_index: usize,
) -> Result<NativeVulkanDescriptorHeapShaderBindingMapping, String> {
    let heap_offset = descriptor_offset_u32(
        &plan.buffer_descriptor_offsets,
        buffer_index,
        "uniform-buffer",
    )?;
    let heap_array_stride = u32::try_from(plan.buffer_descriptor_stride)
        .map_err(|_| "descriptor heap uniform-buffer stride exceeds u32".to_owned())?;
    let source = vk::DescriptorMappingSourceConstantOffsetEXT::builder()
        .heap_offset(heap_offset)
        .heap_array_stride(heap_array_stride)
        .sampler_heap_offset(0)
        .sampler_heap_array_stride(0)
        .build();

    Ok(
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping(
            binding,
            vk::SpirvResourceTypeFlagsEXT::UNIFORM_BUFFER,
            vk::DescriptorMappingSourceDataEXT {
                constant_offset: source,
            },
        ),
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_uniform_buffer_binding_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    binding: u32,
    resource_descriptor_index: usize,
) -> Result<NativeVulkanDescriptorHeapShaderBindingMapping, String> {
    validate_mixed_plan_descriptor_kind(
        plan,
        resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
    )?;
    let heap_offset = descriptor_offset_u32(
        &plan.resource_descriptor_offsets,
        resource_descriptor_index,
        "mixed-uniform-buffer",
    )?;
    let heap_array_stride = u32::try_from(plan.buffer_descriptor_stride)
        .map_err(|_| "descriptor heap mixed uniform-buffer stride exceeds u32".to_owned())?;
    let source = vk::DescriptorMappingSourceConstantOffsetEXT::builder()
        .heap_offset(heap_offset)
        .heap_array_stride(heap_array_stride)
        .sampler_heap_offset(0)
        .sampler_heap_array_stride(0)
        .build();

    Ok(
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping(
            binding,
            vk::SpirvResourceTypeFlagsEXT::UNIFORM_BUFFER,
            vk::DescriptorMappingSourceDataEXT {
                constant_offset: source,
            },
        ),
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    binding: u32,
    base_resource_descriptor_index: usize,
    resource_descriptor_index: usize,
) -> Result<NativeVulkanDescriptorHeapShaderBindingMapping, String> {
    validate_mixed_plan_descriptor_kind(
        plan,
        base_resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
    )?;
    validate_mixed_plan_descriptor_kind(
        plan,
        resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
    )?;
    let base_heap_offset = *plan
        .resource_descriptor_offsets
        .get(base_resource_descriptor_index)
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed base uniform descriptor index {base_resource_descriptor_index} has no resource offset"
            )
        })?;
    let base_heap_offset = align_down(base_heap_offset, plan.resource_heap_alignment);
    let heap_offset = plan
        .resource_descriptor_offsets
        .get(resource_descriptor_index)
        .copied()
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed uniform descriptor index {resource_descriptor_index} has no resource offset"
            )
        })?
        .checked_sub(base_heap_offset)
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed uniform descriptor index {resource_descriptor_index} precedes heap-slice base {base_resource_descriptor_index}"
            )
        })?;
    let heap_offset = u32::try_from(heap_offset)
        .map_err(|_| "descriptor heap mixed relative uniform offset exceeds u32".to_owned())?;
    let heap_array_stride = u32::try_from(plan.buffer_descriptor_stride)
        .map_err(|_| "descriptor heap mixed uniform-buffer stride exceeds u32".to_owned())?;
    let source = vk::DescriptorMappingSourceConstantOffsetEXT::builder()
        .heap_offset(heap_offset)
        .heap_array_stride(heap_array_stride)
        .sampler_heap_offset(0)
        .sampler_heap_array_stride(0)
        .build();

    Ok(
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping(
            binding,
            vk::SpirvResourceTypeFlagsEXT::UNIFORM_BUFFER,
            vk::DescriptorMappingSourceDataEXT {
                constant_offset: source,
            },
        ),
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_combined_image_sampler_binding_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    binding: u32,
    resource_descriptor_index: usize,
    sampler_descriptor_index: usize,
) -> Result<NativeVulkanDescriptorHeapShaderBindingMapping, String> {
    validate_mixed_plan_descriptor_kind(
        plan,
        resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
    )?;
    let heap_offset = descriptor_offset_u32(
        &plan.resource_descriptor_offsets,
        resource_descriptor_index,
        "mixed-sampled-image",
    )?;
    let sampler_heap_offset = descriptor_offset_u32(
        &plan.sampler_descriptor_offsets,
        sampler_descriptor_index,
        "mixed-sampler",
    )?;
    let heap_array_stride = u32::try_from(plan.image_descriptor_stride)
        .map_err(|_| "descriptor heap mixed image stride exceeds u32".to_owned())?;
    let sampler_heap_array_stride = u32::try_from(plan.sampler_descriptor_stride)
        .map_err(|_| "descriptor heap mixed sampler stride exceeds u32".to_owned())?;
    let source = vk::DescriptorMappingSourceConstantOffsetEXT::builder()
        .heap_offset(heap_offset)
        .heap_array_stride(heap_array_stride)
        .sampler_heap_offset(sampler_heap_offset)
        .sampler_heap_array_stride(sampler_heap_array_stride)
        .build();

    Ok(
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping(
            binding,
            vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE,
            vk::DescriptorMappingSourceDataEXT {
                constant_offset: source,
            },
        ),
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    binding: u32,
    base_resource_descriptor_index: usize,
    resource_descriptor_index: usize,
    base_sampler_descriptor_index: usize,
    sampler_descriptor_index: usize,
) -> Result<NativeVulkanDescriptorHeapShaderBindingMapping, String> {
    validate_mixed_plan_descriptor_kind(
        plan,
        base_resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
    )?;
    validate_mixed_plan_descriptor_kind(
        plan,
        resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
    )?;
    let base_heap_offset = *plan
        .resource_descriptor_offsets
        .get(base_resource_descriptor_index)
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed base resource descriptor index {base_resource_descriptor_index} has no resource offset"
            )
        })?;
    let base_heap_offset = align_down(base_heap_offset, plan.resource_heap_alignment);
    let heap_offset = plan
        .resource_descriptor_offsets
        .get(resource_descriptor_index)
        .copied()
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed sampled image index {resource_descriptor_index} has no resource offset"
            )
        })?
        .checked_sub(base_heap_offset)
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed sampled image index {resource_descriptor_index} precedes heap-slice base {base_resource_descriptor_index}"
            )
        })?;
    let base_sampler_heap_offset = *plan
        .sampler_descriptor_offsets
        .get(base_sampler_descriptor_index)
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed base sampler descriptor index {base_sampler_descriptor_index} has no sampler offset"
            )
        })?;
    let base_sampler_heap_offset =
        align_down(base_sampler_heap_offset, plan.sampler_heap_alignment);
    let sampler_heap_offset = plan
        .sampler_descriptor_offsets
        .get(sampler_descriptor_index)
        .copied()
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed sampler descriptor index {sampler_descriptor_index} has no sampler offset"
            )
        })?
        .checked_sub(base_sampler_heap_offset)
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed sampler descriptor index {sampler_descriptor_index} precedes sampler-set base {base_sampler_descriptor_index}"
            )
        })?;
    let heap_offset = u32::try_from(heap_offset)
        .map_err(|_| "descriptor heap mixed relative image offset exceeds u32".to_owned())?;
    let sampler_heap_offset = u32::try_from(sampler_heap_offset)
        .map_err(|_| "descriptor heap mixed relative sampler offset exceeds u32".to_owned())?;
    let heap_array_stride = u32::try_from(plan.image_descriptor_stride)
        .map_err(|_| "descriptor heap mixed image stride exceeds u32".to_owned())?;
    let sampler_heap_array_stride = u32::try_from(plan.sampler_descriptor_stride)
        .map_err(|_| "descriptor heap mixed sampler stride exceeds u32".to_owned())?;
    let source = vk::DescriptorMappingSourceConstantOffsetEXT::builder()
        .heap_offset(heap_offset)
        .heap_array_stride(heap_array_stride)
        .sampler_heap_offset(sampler_heap_offset)
        .sampler_heap_array_stride(sampler_heap_array_stride)
        .build();

    Ok(
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping(
            binding,
            vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE,
            vk::DescriptorMappingSourceDataEXT {
                constant_offset: source,
            },
        ),
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_relative_sampled_image_binding_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    binding: u32,
    base_resource_descriptor_index: usize,
    resource_descriptor_index: usize,
    base_sampler_descriptor_index: usize,
    sampler_descriptor_index: usize,
) -> Result<NativeVulkanDescriptorHeapShaderBindingMapping, String> {
    validate_mixed_plan_descriptor_kind(
        plan,
        base_resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
    )?;
    validate_mixed_plan_descriptor_kind(
        plan,
        resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
    )?;
    let base_heap_offset = *plan
        .resource_descriptor_offsets
        .get(base_resource_descriptor_index)
        .ok_or_else(|| {
            format!(
                "descriptor heap sampled-image base resource descriptor index {base_resource_descriptor_index} has no resource offset"
            )
        })?;
    let base_heap_offset = align_down(base_heap_offset, plan.resource_heap_alignment);
    let heap_offset = plan
        .resource_descriptor_offsets
        .get(resource_descriptor_index)
        .copied()
        .ok_or_else(|| {
            format!(
                "descriptor heap sampled-image descriptor index {resource_descriptor_index} has no resource offset"
            )
        })?
        .checked_sub(base_heap_offset)
        .ok_or_else(|| {
            format!(
                "descriptor heap sampled-image descriptor index {resource_descriptor_index} precedes heap-slice base {base_resource_descriptor_index}"
            )
        })?;
    let base_sampler_heap_offset = *plan
        .sampler_descriptor_offsets
        .get(base_sampler_descriptor_index)
        .ok_or_else(|| {
            format!(
                "descriptor heap sampled-image base sampler descriptor index {base_sampler_descriptor_index} has no sampler offset"
            )
        })?;
    let base_sampler_heap_offset =
        align_down(base_sampler_heap_offset, plan.sampler_heap_alignment);
    let sampler_heap_offset = plan
        .sampler_descriptor_offsets
        .get(sampler_descriptor_index)
        .copied()
        .ok_or_else(|| {
            format!(
                "descriptor heap sampled-image sampler descriptor index {sampler_descriptor_index} has no sampler offset"
            )
        })?
        .checked_sub(base_sampler_heap_offset)
        .ok_or_else(|| {
            format!(
                "descriptor heap sampled-image sampler descriptor index {sampler_descriptor_index} precedes sampler-set base {base_sampler_descriptor_index}"
            )
        })?;
    let heap_offset = u32::try_from(heap_offset).map_err(|_| {
        "descriptor heap sampled-image relative image offset exceeds u32".to_owned()
    })?;
    let sampler_heap_offset = u32::try_from(sampler_heap_offset).map_err(|_| {
        "descriptor heap sampled-image relative sampler offset exceeds u32".to_owned()
    })?;
    let heap_array_stride = u32::try_from(plan.image_descriptor_stride)
        .map_err(|_| "descriptor heap sampled-image stride exceeds u32".to_owned())?;
    let sampler_heap_array_stride = u32::try_from(plan.sampler_descriptor_stride)
        .map_err(|_| "descriptor heap sampled-image sampler stride exceeds u32".to_owned())?;
    let source = vk::DescriptorMappingSourceConstantOffsetEXT::builder()
        .heap_offset(heap_offset)
        .heap_array_stride(heap_array_stride)
        .sampler_heap_offset(sampler_heap_offset)
        .sampler_heap_array_stride(sampler_heap_array_stride)
        .build();

    Ok(
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping(
            binding,
            vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE,
            vk::DescriptorMappingSourceDataEXT {
                constant_offset: source,
            },
        ),
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_bind_info(
    resources: &VulkanaliaDescriptorHeapImageSamplerResources,
) -> vk::BindHeapInfoEXT {
    vk::BindHeapInfoEXT::builder()
        .heap_range(
            vk::DeviceAddressRangeEXT::builder()
                .address(resources.resource_heap.device_address)
                .size(resources.resource_heap.snapshot.requested_bytes)
                .build(),
        )
        .reserved_range_offset(resources.plan.resource_heap_reserved_range_offset)
        .reserved_range_size(resources.plan.resource_heap_reserved_range_size)
        .build()
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info(
    resources: &VulkanaliaDescriptorHeapResourceResources,
) -> vk::BindHeapInfoEXT {
    vk::BindHeapInfoEXT::builder()
        .heap_range(
            vk::DeviceAddressRangeEXT::builder()
                .address(resources.resource_heap.device_address)
                .size(resources.resource_heap.snapshot.requested_bytes)
                .build(),
        )
        .reserved_range_offset(resources.plan.resource_heap_reserved_range_offset)
        .reserved_range_size(resources.plan.resource_heap_reserved_range_size)
        .build()
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor(
    resources: &VulkanaliaDescriptorHeapResourceResources,
    resource_descriptor_index: usize,
) -> Result<vk::BindHeapInfoEXT, String> {
    let descriptor_offset = mixed_resource_descriptor_offset(resources, resource_descriptor_index)?;
    descriptor_heap_indexed_bind_info(
        &resources.resource_heap,
        resources.plan.resource_heap_reserved_range_offset,
        resources.plan.resource_heap_reserved_range_size,
        descriptor_offset,
        resources.plan.resource_heap_alignment,
        "mixed-resource",
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_resource_bind_info(
    resources: &VulkanaliaDescriptorHeapUniformBufferResources,
) -> vk::BindHeapInfoEXT {
    vk::BindHeapInfoEXT::builder()
        .heap_range(
            vk::DeviceAddressRangeEXT::builder()
                .address(resources.resource_heap.device_address)
                .size(resources.resource_heap.snapshot.requested_bytes)
                .build(),
        )
        .reserved_range_offset(resources.plan.resource_heap_reserved_range_offset)
        .reserved_range_size(resources.plan.resource_heap_reserved_range_size)
        .build()
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info(
    resources: &VulkanaliaDescriptorHeapResourceResources,
) -> Result<vk::BindHeapInfoEXT, String> {
    let sampler_heap = resources
        .sampler_heap
        .as_ref()
        .ok_or_else(|| "descriptor heap mixed sampler heap is not resident".to_owned())?;
    Ok(vk::BindHeapInfoEXT::builder()
        .heap_range(
            vk::DeviceAddressRangeEXT::builder()
                .address(sampler_heap.device_address)
                .size(sampler_heap.snapshot.requested_bytes)
                .build(),
        )
        .reserved_range_offset(resources.plan.sampler_heap_reserved_range_offset)
        .reserved_range_size(resources.plan.sampler_heap_reserved_range_size)
        .build())
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor(
    resources: &VulkanaliaDescriptorHeapResourceResources,
    sampler_descriptor_index: usize,
) -> Result<vk::BindHeapInfoEXT, String> {
    let sampler_heap = resources
        .sampler_heap
        .as_ref()
        .ok_or_else(|| "descriptor heap mixed sampler heap is not resident".to_owned())?;
    let descriptor_offset = *resources
        .plan
        .sampler_descriptor_offsets
        .get(sampler_descriptor_index)
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed sampler descriptor index {sampler_descriptor_index} has no sampler offset"
            )
        })?;
    descriptor_heap_indexed_bind_info(
        sampler_heap,
        resources.plan.sampler_heap_reserved_range_offset,
        resources.plan.sampler_heap_reserved_range_size,
        descriptor_offset,
        resources.plan.sampler_heap_alignment,
        "mixed-sampler",
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_resource_bind_info_for_buffer(
    resources: &VulkanaliaDescriptorHeapUniformBufferResources,
    buffer_index: usize,
) -> Result<vk::BindHeapInfoEXT, String> {
    let descriptor_offset = *resources
        .plan
        .buffer_descriptor_offsets
        .get(buffer_index)
        .ok_or_else(|| {
            format!("descriptor heap uniform buffer index {buffer_index} has no resource offset")
        })?;
    descriptor_heap_indexed_bind_info(
        &resources.resource_heap,
        resources.plan.resource_heap_reserved_range_offset,
        resources.plan.resource_heap_reserved_range_size,
        descriptor_offset,
        resources.plan.resource_heap_alignment,
        "uniform-buffer-resource",
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_bind_info_for_image(
    resources: &VulkanaliaDescriptorHeapImageSamplerResources,
    image_index: usize,
) -> Result<vk::BindHeapInfoEXT, String> {
    let descriptor_offset = *resources
        .plan
        .image_descriptor_offsets
        .get(image_index)
        .ok_or_else(|| format!("descriptor heap image index {image_index} has no image offset"))?;
    descriptor_heap_indexed_bind_info(
        &resources.resource_heap,
        resources.plan.resource_heap_reserved_range_offset,
        resources.plan.resource_heap_reserved_range_size,
        descriptor_offset,
        resources.plan.resource_heap_alignment,
        "resource",
    )
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info(
    resources: &VulkanaliaDescriptorHeapImageSamplerResources,
) -> vk::BindHeapInfoEXT {
    vk::BindHeapInfoEXT::builder()
        .heap_range(
            vk::DeviceAddressRangeEXT::builder()
                .address(resources.sampler_heap.device_address)
                .size(resources.sampler_heap.snapshot.requested_bytes)
                .build(),
        )
        .reserved_range_offset(resources.plan.sampler_heap_reserved_range_offset)
        .reserved_range_size(resources.plan.sampler_heap_reserved_range_size)
        .build()
}

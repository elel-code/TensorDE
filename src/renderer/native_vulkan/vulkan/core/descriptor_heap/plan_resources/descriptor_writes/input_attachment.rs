//! Vulkan descriptor-heap input-attachment writes and shader mappings.
//!
//! This module deliberately owns only the input-attachment resource contract.
//! It does not open a sampler path or infer a local read from a shader name.

use super::super::*;

/// Writes an input-attachment image descriptor into the mixed resource heap.
///
/// Input attachments are image resources, not sampled images: no sampler
/// descriptor is allocated or touched. The caller owns the dynamic-rendering
/// layout contract and must pass `VK_IMAGE_LAYOUT_RENDERING_LOCAL_READ` when
/// this descriptor participates in a local-read rendering scope.
pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_write_descriptor_heap_resource_input_attachment(
    device: &Device,
    resources: &mut VulkanaliaDescriptorHeapResourceResources,
    resource_descriptor_index: usize,
    image_view_info: &vk::ImageViewCreateInfo,
    image_layout: vk::ImageLayout,
) -> Result<(), String> {
    validate_mixed_resource_descriptor_kind(
        resources,
        resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::InputAttachment,
    )?;
    let resource_offset = mixed_resource_descriptor_offset(resources, resource_descriptor_index)?;
    let image_descriptor_size = resources.plan.image_descriptor_size;
    let image_descriptor = vk::ImageDescriptorInfoEXT::builder()
        .view(image_view_info)
        .layout(image_layout)
        .build();
    let resource_info = vk::ResourceDescriptorInfoEXT::builder()
        .type_(vk::DescriptorType::INPUT_ATTACHMENT)
        .data(vk::ResourceDescriptorDataEXT {
            image: &image_descriptor,
        })
        .build();
    let resource_range = heap_host_address_range(
        &resources.resource_heap,
        resource_offset,
        image_descriptor_size,
        "mixed-resource-input-attachment-heap",
    )?;

    unsafe {
        device
            .write_resource_descriptors_ext(&[resource_info], &[resource_range])
            .map_err(|err| {
                format!(
                    "vkWriteResourceDescriptorsEXT(vulkanalia input attachment): {err:?}"
                )
            })?;
    }
    flush_descriptor_heap_buffer(
        device,
        &resources.resource_heap,
        resource_offset,
        image_descriptor_size,
    )?;
    resources.snapshot.resource_descriptors_written = resources
        .snapshot
        .resource_descriptors_written
        .saturating_add(1);
    Ok(())
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_input_attachment_binding_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    binding: u32,
    resource_descriptor_index: usize,
) -> Result<NativeVulkanDescriptorHeapShaderBindingMapping, String> {
    validate_mixed_plan_descriptor_kind(
        plan,
        resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::InputAttachment,
    )?;
    let heap_offset = descriptor_offset_u32(
        &plan.resource_descriptor_offsets,
        resource_descriptor_index,
        "input-attachment",
    )?;
    let heap_array_stride = u32::try_from(plan.image_descriptor_stride)
        .map_err(|_| "descriptor heap input-attachment stride exceeds u32".to_owned())?;
    let source = vk::DescriptorMappingSourceConstantOffsetEXT::builder()
        .heap_offset(heap_offset)
        .heap_array_stride(heap_array_stride)
        .sampler_heap_offset(0)
        .sampler_heap_array_stride(0)
        .build();
    Ok(native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping(
        binding,
        vk::SpirvResourceTypeFlagsEXT::READ_ONLY_IMAGE,
        vk::DescriptorMappingSourceDataEXT {
            constant_offset: source,
        },
    ))
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_relative_input_attachment_binding_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    binding: u32,
    base_resource_descriptor_index: usize,
    resource_descriptor_index: usize,
) -> Result<NativeVulkanDescriptorHeapShaderBindingMapping, String> {
    validate_mixed_plan_descriptor_kind(
        plan,
        base_resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::InputAttachment,
    )?;
    validate_mixed_plan_descriptor_kind(
        plan,
        resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::InputAttachment,
    )?;
    let base_heap_offset = *plan
        .resource_descriptor_offsets
        .get(base_resource_descriptor_index)
        .ok_or_else(|| {
            format!(
                "descriptor heap input-attachment base resource descriptor index {base_resource_descriptor_index} has no resource offset"
            )
        })?;
    let base_heap_offset = align_down(base_heap_offset, plan.resource_heap_alignment);
    let heap_offset = plan
        .resource_descriptor_offsets
        .get(resource_descriptor_index)
        .copied()
        .ok_or_else(|| {
            format!(
                "descriptor heap input-attachment resource descriptor index {resource_descriptor_index} has no resource offset"
            )
        })?
        .checked_sub(base_heap_offset)
        .ok_or_else(|| {
            format!(
                "descriptor heap input-attachment descriptor index {resource_descriptor_index} precedes heap-slice base {base_resource_descriptor_index}"
            )
        })?;
    let heap_offset = u32::try_from(heap_offset)
        .map_err(|_| "descriptor heap input-attachment relative image offset exceeds u32".to_owned())?;
    let heap_array_stride = u32::try_from(plan.image_descriptor_stride)
        .map_err(|_| "descriptor heap input-attachment stride exceeds u32".to_owned())?;
    let source = vk::DescriptorMappingSourceConstantOffsetEXT::builder()
        .heap_offset(heap_offset)
        .heap_array_stride(heap_array_stride)
        .sampler_heap_offset(0)
        .sampler_heap_array_stride(0)
        .build();
    Ok(native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping(
        binding,
        vk::SpirvResourceTypeFlagsEXT::READ_ONLY_IMAGE,
        vk::DescriptorMappingSourceDataEXT {
            constant_offset: source,
        },
    ))
}

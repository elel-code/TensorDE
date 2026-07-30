//! Native storage-buffer descriptor writes for the mixed resource heap.

use super::*;

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_write_descriptor_heap_resource_storage_buffer(
    device: &Device,
    resources: &mut VulkanaliaDescriptorHeapResourceResources,
    resource_descriptor_index: usize,
    device_address: vk::DeviceAddress,
    range: u64,
) -> Result<(), String> {
    if device_address == 0 {
        return Err(
            "descriptor heap mixed storage buffer requires non-zero device address".to_owned(),
        );
    }
    if range == 0 {
        return Err("descriptor heap mixed storage buffer requires non-zero range".to_owned());
    }
    validate_mixed_resource_descriptor_kind(
        resources,
        resource_descriptor_index,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
    )?;
    let resource_offset = mixed_resource_descriptor_offset(resources, resource_descriptor_index)?;
    let descriptor_size = resources.plan.buffer_descriptor_size;
    let address_range = vk::DeviceAddressRangeEXT::builder()
        .address(device_address)
        .size(range)
        .build();
    let resource_info = vk::ResourceDescriptorInfoEXT::builder()
        .type_(vk::DescriptorType::STORAGE_BUFFER)
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
                format!("vkWriteResourceDescriptorsEXT(vulkanalia mixed storage buffer): {err:?}")
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


pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info_for_image(
    resources: &VulkanaliaDescriptorHeapImageSamplerResources,
    image_index: usize,
) -> Result<vk::BindHeapInfoEXT, String> {
    let descriptor_offset = *resources
        .plan
        .sampler_descriptor_offsets
        .get(image_index)
        .ok_or_else(|| {
            format!("descriptor heap image index {image_index} has no sampler offset")
        })?;
    descriptor_heap_indexed_bind_info(
        &resources.sampler_heap,
        resources.plan.sampler_heap_reserved_range_offset,
        resources.plan.sampler_heap_reserved_range_size,
        descriptor_offset,
        resources.plan.sampler_heap_alignment,
        "sampler",
    )
}

fn descriptor_heap_indexed_bind_info(
    heap: &VulkanaliaDescriptorHeapBuffer,
    reserved_range_offset: u64,
    reserved_range_size: u64,
    descriptor_offset: u64,
    heap_range_alignment: u64,
    role: &'static str,
) -> Result<vk::BindHeapInfoEXT, String> {
    if descriptor_offset > reserved_range_offset {
        return Err(format!(
            "{role} descriptor offset {descriptor_offset} exceeds reserved range offset {reserved_range_offset}"
        ));
    }
    let heap_range_offset = align_down(descriptor_offset, heap_range_alignment);
    let heap_size = heap
        .snapshot
        .requested_bytes
        .checked_sub(heap_range_offset)
        .ok_or_else(|| format!("{role} descriptor offset exceeds heap size"))?;
    let address = heap
        .device_address
        .checked_add(heap_range_offset)
        .ok_or_else(|| format!("{role} descriptor heap device address overflows"))?;
    Ok(vk::BindHeapInfoEXT::builder()
        .heap_range(
            vk::DeviceAddressRangeEXT::builder()
                .address(address)
                .size(heap_size)
                .build(),
        )
        .reserved_range_offset(reserved_range_offset - heap_range_offset)
        .reserved_range_size(reserved_range_size)
        .build())
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
    device: &Device,
    resources: VulkanaliaDescriptorHeapImageSamplerResources,
) {
    native_vulkan_vulkanalia_destroy_descriptor_heap_buffer(device, resources.sampler_heap);
    native_vulkan_vulkanalia_destroy_descriptor_heap_buffer(device, resources.resource_heap);
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_destroy_descriptor_heap_uniform_buffer_resources(
    device: &Device,
    resources: VulkanaliaDescriptorHeapUniformBufferResources,
) {
    native_vulkan_vulkanalia_destroy_descriptor_heap_buffer(device, resources.resource_heap);
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
    device: &Device,
    resources: VulkanaliaDescriptorHeapResourceResources,
) {
    if let Some(sampler_heap) = resources.sampler_heap {
        native_vulkan_vulkanalia_destroy_descriptor_heap_buffer(device, sampler_heap);
    }
    native_vulkan_vulkanalia_destroy_descriptor_heap_buffer(device, resources.resource_heap);
}

fn mixed_resource_descriptor_offset(
    resources: &VulkanaliaDescriptorHeapResourceResources,
    resource_descriptor_index: usize,
) -> Result<u64, String> {
    resources
        .plan
        .resource_descriptor_offsets
        .get(resource_descriptor_index)
        .copied()
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed resource descriptor index {resource_descriptor_index} has no resource offset"
            )
        })
}

fn validate_mixed_resource_descriptor_kind(
    resources: &VulkanaliaDescriptorHeapResourceResources,
    resource_descriptor_index: usize,
    expected: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
) -> Result<(), String> {
    validate_mixed_plan_descriptor_kind(&resources.plan, resource_descriptor_index, expected)
}

fn validate_mixed_plan_descriptor_kind(
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    resource_descriptor_index: usize,
    expected: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
) -> Result<(), String> {
    let actual = plan
        .resource_descriptor_kinds
        .get(resource_descriptor_index)
        .copied()
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed resource descriptor index {resource_descriptor_index} has no descriptor kind"
            )
        })?;
    if actual != expected {
        return Err(format!(
            "descriptor heap mixed resource descriptor index {resource_descriptor_index} has kind {:?}, expected {:?}",
            actual, expected
        ));
    }
    Ok(())
}

fn create_descriptor_heap_buffer(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    role: &'static str,
    requested_bytes: u64,
) -> Result<VulkanaliaDescriptorHeapBuffer, String> {
    if requested_bytes == 0 {
        return Err(format!("{role} descriptor heap requires non-zero size"));
    }

    let usage =
        vk::BufferUsageFlags::DESCRIPTOR_HEAP_EXT | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
    let create_info = vk::BufferCreateInfo::builder()
        .size(requested_bytes)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&create_info, None) }
        .map_err(|err| format!("vkCreateBuffer(vulkanalia {role} descriptor heap): {err:?}"))?;

    let result = (|| -> Result<VulkanaliaDescriptorHeapBuffer, String> {
        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let memory_type = descriptor_heap_memory_type_index(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
            HOST_VISIBLE_COHERENT_DEVICE_LOCAL_MEMORY_FLAG_BITS,
        )
        .or_else(|| {
            descriptor_heap_memory_type_index(
                &memory_type_candidates,
                memory_requirements.memory_type_bits,
                HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS,
            )
        })
        .or_else(|| {
            descriptor_heap_memory_type_index(
                &memory_type_candidates,
                memory_requirements.memory_type_bits,
                HOST_VISIBLE_MEMORY_FLAG_BITS,
            )
        })
        .ok_or_else(|| {
            format!(
                "{role} descriptor heap has no host-visible memory type for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let mut allocate_flags = vk::MemoryAllocateFlagsInfo::builder()
            .flags(vk::MemoryAllocateFlags::DEVICE_ADDRESS)
            .build();
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type.index)
            .push_next(&mut allocate_flags);
        let memory = unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|err| {
            format!("vkAllocateMemory(vulkanalia {role} descriptor heap): {err:?}")
        })?;

        let label = format!("{role} descriptor heap");
        if let Err(err) =
            native_vulkan_vulkanalia_bind_buffer_memory2(device, buffer, memory, 0, &label)
        {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(err);
        }

        let mapped_ptr = match native_vulkan_vulkanalia_map_memory2(
            device,
            memory,
            0,
            memory_requirements.size,
            vk::MemoryMapFlags::empty(),
            &label,
        ) {
            Ok(mapped_ptr) => mapped_ptr,
            Err(err) => {
                unsafe {
                    device.free_memory(memory, None);
                }
                return Err(err);
            }
        };
        let address_info = vk::BufferDeviceAddressInfo::builder()
            .buffer(buffer)
            .build();
        let device_address = unsafe { device.get_buffer_device_address(&address_info) };
        let host_coherent = memory_type.property_flags_bits
            & vk::MemoryPropertyFlags::HOST_COHERENT.bits()
            == vk::MemoryPropertyFlags::HOST_COHERENT.bits();

        Ok(VulkanaliaDescriptorHeapBuffer {
            buffer,
            memory,
            mapped_ptr,
            mapped_size: memory_requirements.size,
            device_address,
            host_coherent,
            snapshot: NativeVulkanVulkanaliaDescriptorHeapBufferSnapshot {
                role,
                buffer_created: true,
                memory_bound: true,
                mapped: true,
                device_address_nonzero: device_address != 0,
                requested_bytes,
                memory_size: memory_requirements.size,
                memory_alignment: memory_requirements.alignment,
                memory_type_bits: memory_requirements.memory_type_bits,
                selected_memory_type_index: memory_type.index,
                selected_memory_property_flags: memory_property_flag_labels(
                    memory_type.property_flags_bits,
                ),
                usage_flags: buffer_usage_flag_labels(usage),
                host_coherent,
            },
        })
    })();

    if result.is_err() {
        unsafe {
            device.destroy_buffer(buffer, None);
        }
    }
    result
}

fn native_vulkan_vulkanalia_destroy_descriptor_heap_buffer(
    device: &Device,
    buffer: VulkanaliaDescriptorHeapBuffer,
) {
    let _ = native_vulkan_vulkanalia_unmap_memory2(device, buffer.memory, buffer.snapshot.role);
    unsafe {
        device.destroy_buffer(buffer.buffer, None);
        device.free_memory(buffer.memory, None);
    }
}

fn heap_host_address_range(
    buffer: &VulkanaliaDescriptorHeapBuffer,
    offset: u64,
    size: u64,
    role: &'static str,
) -> Result<vk::HostAddressRangeEXT, String> {
    let end = offset
        .checked_add(size)
        .ok_or_else(|| format!("{role} descriptor range overflows"))?;
    if end > buffer.mapped_size {
        return Err(format!(
            "{role} descriptor range {offset}..{end} exceeds mapped size {}",
            buffer.mapped_size
        ));
    }
    let offset_usize =
        usize::try_from(offset).map_err(|_| format!("{role} descriptor offset exceeds usize"))?;
    let size_usize =
        usize::try_from(size).map_err(|_| format!("{role} descriptor size exceeds usize"))?;
    let address = unsafe { buffer.mapped_ptr.cast::<u8>().add(offset_usize) };
    Ok(vk::HostAddressRangeEXT {
        address: address.cast(),
        size: size_usize,
    })
}

fn flush_descriptor_heap_buffer(
    device: &Device,
    buffer: &VulkanaliaDescriptorHeapBuffer,
    offset: u64,
    size: u64,
) -> Result<(), String> {
    if buffer.host_coherent {
        return Ok(());
    }
    let range = vk::MappedMemoryRange::builder()
        .memory(buffer.memory)
        .offset(offset)
        .size(size)
        .build();
    unsafe { device.flush_mapped_memory_ranges(&[range]) }
        .map_err(|err| format!("vkFlushMappedMemoryRanges(vulkanalia descriptor heap): {err:?}"))
}

fn descriptor_offset_u32(offsets: &[u64], index: usize, role: &'static str) -> Result<u32, String> {
    let offset = *offsets
        .get(index)
        .ok_or_else(|| format!("descriptor heap {role} index {index} has no offset"))?;
    u32::try_from(offset).map_err(|_| format!("descriptor heap offset {offset} exceeds u32"))
}

fn descriptor_heap_memory_type_index(
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
    allowed_memory_type_bits: u32,
    required_property_flags: u32,
) -> Option<NativeVulkanVulkanaliaMemoryTypeCandidate> {
    memory_types.iter().copied().find(|candidate| {
        let allowed = allowed_memory_type_bits & (1u32 << candidate.index) != 0;
        allowed
            && candidate.property_flags_bits & required_property_flags == required_property_flags
    })
}

fn buffer_usage_flag_labels(flags: vk::BufferUsageFlags) -> Vec<&'static str> {
    [
        (
            vk::BufferUsageFlags::DESCRIPTOR_HEAP_EXT.bits(),
            "descriptor-heap-ext",
        ),
        (
            vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS.bits(),
            "shader-device-address",
        ),
        (
            vk::BufferUsageFlags::RESOURCE_DESCRIPTOR_BUFFER_EXT.bits(),
            "resource-descriptor-buffer-ext",
        ),
        (
            vk::BufferUsageFlags::SAMPLER_DESCRIPTOR_BUFFER_EXT.bits(),
            "sampler-descriptor-buffer-ext",
        ),
    ]
    .iter()
    .filter_map(|(bit, label)| {
        if flags.bits() & bit == *bit {
            Some(*label)
        } else {
            None
        }
    })
    .collect()
}

fn memory_property_flag_labels(flags: u32) -> Vec<&'static str> {
    [
        (vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(), "device-local"),
        (vk::MemoryPropertyFlags::HOST_VISIBLE.bits(), "host-visible"),
        (
            vk::MemoryPropertyFlags::HOST_COHERENT.bits(),
            "host-coherent",
        ),
        (vk::MemoryPropertyFlags::HOST_CACHED.bits(), "host-cached"),
        (vk::MemoryPropertyFlags::PROTECTED.bits(), "protected"),
    ]
    .iter()
    .filter_map(|(bit, label)| {
        if flags & bit == *bit {
            Some(*label)
        } else {
            None
        }
    })
    .collect()
}

fn descriptor_offsets(count: usize, stride: u64) -> Vec<u64> {
    (0..count)
        .map(|index| (index as u64).saturating_mul(stride))
        .collect()
}

fn descriptor_heap_bytes(count: usize, stride: u64, heap_alignment: u64) -> u64 {
    align_up((count as u64).saturating_mul(stride), heap_alignment)
}

fn mixed_resource_descriptor_offsets(
    resource_descriptors: &[NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind],
    image_descriptor_alignment: u64,
    image_descriptor_stride: u64,
    buffer_descriptor_alignment: u64,
    buffer_descriptor_stride: u64,
) -> Vec<u64> {
    let mut cursor = 0u64;
    let mut offsets = Vec::with_capacity(resource_descriptors.len());
    for kind in resource_descriptors {
        let alignment = resource_descriptor_alignment_for_kind(
            *kind,
            image_descriptor_alignment,
            buffer_descriptor_alignment,
        );
        let stride = resource_descriptor_stride_for_kind(
            *kind,
            image_descriptor_stride,
            buffer_descriptor_stride,
        );
        cursor = align_up(cursor, alignment);
        offsets.push(cursor);
        cursor = cursor.saturating_add(stride);
    }
    offsets
}

fn resource_descriptor_alignment_for_kind(
    kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    image_descriptor_alignment: u64,
    buffer_descriptor_alignment: u64,
) -> u64 {
    match kind {
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage => {
            image_descriptor_alignment
        }
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer => {
            buffer_descriptor_alignment
        }
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer => {
            buffer_descriptor_alignment
        }
    }
}

fn resource_descriptor_stride_for_kind(
    kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    image_descriptor_stride: u64,
    buffer_descriptor_stride: u64,
) -> u64 {
    match kind {
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage => {
            image_descriptor_stride
        }
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer => {
            buffer_descriptor_stride
        }
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer => {
            buffer_descriptor_stride
        }
    }
}

fn aligned_descriptor_stride(descriptor_size: u64, descriptor_alignment: u64) -> u64 {
    align_up(descriptor_size, descriptor_alignment)
}

fn align_up(value: u64, alignment: u64) -> u64 {
    if alignment <= 1 {
        return value;
    }
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value.saturating_add(alignment - remainder)
    }
}

fn align_down(value: u64, alignment: u64) -> u64 {
    if alignment <= 1 {
        return value;
    }
    value - (value % alignment)
}

#[cfg(test)]
mod tests;

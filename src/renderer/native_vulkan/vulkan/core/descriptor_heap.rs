use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder};

use super::features::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot;
use super::memory::{
    native_vulkan_vulkanalia_bind_buffer_memory2, native_vulkan_vulkanalia_map_memory2,
    native_vulkan_vulkanalia_unmap_memory2,
};
use super::video_session::{
    NativeVulkanVulkanaliaMemoryTypeCandidate, native_vulkan_vulkanalia_memory_type_candidates,
};

const HOST_VISIBLE_COHERENT_DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 =
    vk::MemoryPropertyFlags::HOST_VISIBLE.bits()
        | vk::MemoryPropertyFlags::HOST_COHERENT.bits()
        | vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();
const HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS: u32 =
    vk::MemoryPropertyFlags::HOST_VISIBLE.bits() | vk::MemoryPropertyFlags::HOST_COHERENT.bits();
const HOST_VISIBLE_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::HOST_VISIBLE.bits();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
    pub image_count: usize,
    pub properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub descriptor_model: &'static str,
    pub backend_ready: bool,
    pub blocking_reason: Option<&'static str>,
    pub image_count: usize,
    pub resource_heap_alignment: u64,
    pub sampler_heap_alignment: u64,
    pub image_descriptor_size: u64,
    pub sampler_descriptor_size: u64,
    pub image_descriptor_stride: u64,
    pub sampler_descriptor_stride: u64,
    pub resource_heap_bytes: u64,
    pub sampler_heap_bytes: u64,
    pub resource_heap_reserved_range_offset: u64,
    pub resource_heap_reserved_range_size: u64,
    pub sampler_heap_reserved_range_offset: u64,
    pub sampler_heap_reserved_range_size: u64,
    pub image_descriptor_offsets: Vec<u64>,
    pub sampler_descriptor_offsets: Vec<u64>,
    pub max_resource_heap_size: u64,
    pub max_sampler_heap_size: u64,
    pub command_order: Vec<&'static str>,
    pub next_gate: &'static str,
    pub primary_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDescriptorHeapBufferSnapshot {
    pub role: &'static str,
    pub buffer_created: bool,
    pub memory_bound: bool,
    pub mapped: bool,
    pub device_address_nonzero: bool,
    pub requested_bytes: u64,
    pub memory_size: u64,
    pub memory_alignment: u64,
    pub memory_type_bits: u32,
    pub selected_memory_type_index: u32,
    pub selected_memory_property_flags: Vec<&'static str>,
    pub usage_flags: Vec<&'static str>,
    pub host_coherent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDescriptorHeapImageSamplerResourceSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub descriptor_model: &'static str,
    pub resource_heap: NativeVulkanVulkanaliaDescriptorHeapBufferSnapshot,
    pub sampler_heap: NativeVulkanVulkanaliaDescriptorHeapBufferSnapshot,
    pub resource_descriptor_written: bool,
    pub sampler_descriptor_written: bool,
    pub shader_mapping_source: &'static str,
    pub shader_resource_mask: &'static str,
    pub command_order: Vec<&'static str>,
    pub zero_copy_gate: &'static str,
    pub primary_reference: &'static str,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaDescriptorHeapBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped_ptr: *mut std::ffi::c_void,
    mapped_size: u64,
    device_address: vk::DeviceAddress,
    host_coherent: bool,
    snapshot: NativeVulkanVulkanaliaDescriptorHeapBufferSnapshot,
}

// The mapped pointer moves with the owning Vulkan resource and is not shared.
unsafe impl Send for VulkanaliaDescriptorHeapBuffer {}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaDescriptorHeapImageSamplerResources
{
    pub(in crate::renderer::native_vulkan::vulkan) resource_heap: VulkanaliaDescriptorHeapBuffer,
    pub(in crate::renderer::native_vulkan::vulkan) sampler_heap: VulkanaliaDescriptorHeapBuffer,
    pub(in crate::renderer::native_vulkan::vulkan) plan:
        NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaDescriptorHeapImageSamplerResourceSnapshot,
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
    input: NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput,
) -> NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot {
    let properties = input.properties;
    let image_descriptor_stride = aligned_descriptor_stride(
        properties.image_descriptor_size,
        properties.image_descriptor_alignment,
    );
    let sampler_descriptor_stride = aligned_descriptor_stride(
        properties.sampler_descriptor_size,
        properties.sampler_descriptor_alignment,
    );
    let resource_descriptor_region_bytes = descriptor_heap_bytes(
        input.image_count,
        image_descriptor_stride,
        properties.resource_heap_alignment,
    );
    // VK_EXT_descriptor_heap requires the resource heap bind to declare a reserved range
    // of at least minResourceHeapReservedRange (VUID-vkCmdBindResourceHeapEXT-pBindInfo-11233).
    // Keep the application descriptors at the front of the heap (offsets unchanged) and
    // place the driver-reserved range immediately after them, growing the buffer to cover both.
    let resource_heap_reserved_range_offset = align_up(
        resource_descriptor_region_bytes,
        properties.resource_heap_alignment,
    );
    let resource_heap_reserved_range_size = align_up(
        properties.min_resource_heap_reserved_range,
        properties.resource_heap_alignment,
    );
    let resource_heap_bytes =
        resource_heap_reserved_range_offset.saturating_add(resource_heap_reserved_range_size);
    let sampler_descriptor_region_bytes = descriptor_heap_bytes(
        input.image_count,
        sampler_descriptor_stride,
        properties.sampler_heap_alignment,
    );
    let sampler_heap_reserved_range_offset = align_up(
        sampler_descriptor_region_bytes,
        properties.sampler_heap_alignment,
    );
    let sampler_heap_reserved_range_size = align_up(
        properties.min_sampler_heap_reserved_range,
        properties.sampler_heap_alignment,
    );
    let sampler_heap_bytes =
        sampler_heap_reserved_range_offset.saturating_add(sampler_heap_reserved_range_size);
    let descriptor_sizes_ready = properties.image_descriptor_size > 0
        && properties.sampler_descriptor_size > 0
        && image_descriptor_stride > 0
        && sampler_descriptor_stride > 0;
    let resource_heap_fits = properties.max_resource_heap_size == 0
        || resource_heap_bytes <= properties.max_resource_heap_size;
    let sampler_heap_fits = properties.max_sampler_heap_size == 0
        || sampler_heap_bytes <= properties.max_sampler_heap_size;
    let backend_ready =
        input.image_count > 0 && descriptor_sizes_ready && resource_heap_fits && sampler_heap_fits;
    let blocking_reason = if input.image_count == 0 {
        Some("no-sampled-images")
    } else if !descriptor_sizes_ready {
        Some("descriptor-heap-descriptor-sizes-unavailable")
    } else if !resource_heap_fits {
        Some("resource-heap-range-too-small")
    } else if !sampler_heap_fits {
        Some("sampler-heap-range-too-small")
    } else {
        None
    };

    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot {
        binding: "vulkanalia",
        route: "descriptor-heap-image-sampler-plan",
        descriptor_model: "VK_EXT_descriptor_heap",
        backend_ready,
        blocking_reason,
        image_count: input.image_count,
        resource_heap_alignment: properties.resource_heap_alignment,
        sampler_heap_alignment: properties.sampler_heap_alignment,
        image_descriptor_size: properties.image_descriptor_size,
        sampler_descriptor_size: properties.sampler_descriptor_size,
        image_descriptor_stride,
        sampler_descriptor_stride,
        resource_heap_bytes,
        sampler_heap_bytes,
        resource_heap_reserved_range_offset,
        resource_heap_reserved_range_size,
        sampler_heap_reserved_range_offset,
        sampler_heap_reserved_range_size,
        image_descriptor_offsets: descriptor_offsets(input.image_count, image_descriptor_stride),
        sampler_descriptor_offsets: descriptor_offsets(
            input.image_count,
            sampler_descriptor_stride,
        ),
        max_resource_heap_size: properties.max_resource_heap_size,
        max_sampler_heap_size: properties.max_sampler_heap_size,
        command_order: if backend_ready {
            vec![
                "create_device_addressable_resource_heap_buffer",
                "create_device_addressable_sampler_heap_buffer",
                "write_image_descriptors_into_resource_heap",
                "write_sampler_descriptors_into_sampler_heap",
                "cmd_bind_resource_heap_ext",
                "cmd_bind_sampler_heap_ext",
                "draw_with_heap_descriptor_mapping",
            ]
        } else {
            vec!["wait_for_descriptor_heap_capabilities"]
        },
        next_gate: "allocate retained descriptor heap buffers and replace scene/video per-resource descriptor pools with heap offsets",
        primary_reference: "VK_EXT_descriptor_heap device-addressable resource/sampler heaps; FFmpeg-style retained frame lifetime keeps descriptor writes tied to resource lifetime",
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_descriptor_heap_image_sampler_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
) -> Result<VulkanaliaDescriptorHeapImageSamplerResources, String> {
    if !plan.backend_ready {
        return Err(format!(
            "descriptor heap image/sampler resources require ready plan: {:?}",
            plan.blocking_reason
        ));
    }

    let resource_heap = create_descriptor_heap_buffer(
        device,
        memory_properties,
        "resource-heap",
        plan.resource_heap_bytes,
    )?;
    let sampler_heap = match create_descriptor_heap_buffer(
        device,
        memory_properties,
        "sampler-heap",
        plan.sampler_heap_bytes,
    ) {
        Ok(sampler_heap) => sampler_heap,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_descriptor_heap_buffer(device, resource_heap);
            return Err(err);
        }
    };

    Ok(VulkanaliaDescriptorHeapImageSamplerResources {
        plan: plan.clone(),
        snapshot: NativeVulkanVulkanaliaDescriptorHeapImageSamplerResourceSnapshot {
            binding: "vulkanalia",
            route: "descriptor-heap-image-sampler-retained-resource",
            descriptor_model: "VK_EXT_descriptor_heap",
            resource_heap: resource_heap.snapshot.clone(),
            sampler_heap: sampler_heap.snapshot.clone(),
            resource_descriptor_written: false,
            sampler_descriptor_written: false,
            shader_mapping_source: "heap-with-constant-offset",
            shader_resource_mask: "combined-sampled-image",
            command_order: vec![
                "create_device_addressable_resource_heap_buffer",
                "create_device_addressable_sampler_heap_buffer",
                "write_resource_descriptors_ext",
                "write_sampler_descriptors_ext",
                "cmd_bind_resource_heap_ext",
                "cmd_bind_sampler_heap_ext",
                "draw_with_descriptor_heap_mapping",
            ],
            zero_copy_gate: "decoded VkImage remains retained; descriptor heap only binds the image/sampler, so no CPU pixel copy is introduced",
            primary_reference: plan.primary_reference,
        },
        resource_heap,
        sampler_heap,
    })
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_write_descriptor_heap_image_sampler(
    device: &Device,
    resources: &mut VulkanaliaDescriptorHeapImageSamplerResources,
    image_index: usize,
    image_view_info: &vk::ImageViewCreateInfo,
    image_layout: vk::ImageLayout,
    sampler_info: &vk::SamplerCreateInfo,
) -> Result<(), String> {
    let resource_offset = *resources
        .plan
        .image_descriptor_offsets
        .get(image_index)
        .ok_or_else(|| format!("descriptor heap image index {image_index} has no image offset"))?;
    let sampler_offset = *resources
        .plan
        .sampler_descriptor_offsets
        .get(image_index)
        .ok_or_else(|| {
            format!("descriptor heap image index {image_index} has no sampler offset")
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
        "resource-heap",
    )?;
    let sampler_range = heap_host_address_range(
        &resources.sampler_heap,
        sampler_offset,
        sampler_descriptor_size,
        "sampler-heap",
    )?;

    unsafe {
        device
            .write_resource_descriptors_ext(&[resource_info], &[resource_range])
            .map_err(|err| format!("vkWriteResourceDescriptorsEXT(vulkanalia): {err:?}"))?;
        device
            .write_sampler_descriptors_ext(&[*sampler_info], &[sampler_range])
            .map_err(|err| format!("vkWriteSamplerDescriptorsEXT(vulkanalia): {err:?}"))?;
    }
    flush_descriptor_heap_buffer(
        device,
        &resources.resource_heap,
        resource_offset,
        image_descriptor_size,
    )?;
    flush_descriptor_heap_buffer(
        device,
        &resources.sampler_heap,
        sampler_offset,
        sampler_descriptor_size,
    )?;

    resources.snapshot.resource_descriptor_written = true;
    resources.snapshot.sampler_descriptor_written = true;
    resources.snapshot.zero_copy_gate = if image_index == 0 {
        "video present heap descriptor points at the retained decoded image layer; next step is command-buffer heap bind"
    } else {
        "scene/video heap descriptor points at a retained sampled image slot; next step is indexed heap binding"
    };
    Ok(())
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    image_index: usize,
) -> Result<vk::DescriptorSetAndBindingMappingEXT, String> {
    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
        plan,
        0,
        image_index,
    )
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
    plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    binding: u32,
    image_index: usize,
) -> Result<vk::DescriptorSetAndBindingMappingEXT, String> {
    let heap_offset = descriptor_offset_u32(&plan.image_descriptor_offsets, image_index)?;
    let sampler_heap_offset = descriptor_offset_u32(&plan.sampler_descriptor_offsets, image_index)?;
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

    Ok(vk::DescriptorSetAndBindingMappingEXT::builder()
        .descriptor_set(0)
        .first_binding(binding)
        .binding_count(1)
        .resource_mask(vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE)
        .source(vk::DescriptorMappingSourceEXT::HEAP_WITH_CONSTANT_OFFSET)
        .source_data(vk::DescriptorMappingSourceDataEXT {
            constant_offset: source,
        })
        .build())
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_bind_info(
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

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info(
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

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
    device: &Device,
    resources: VulkanaliaDescriptorHeapImageSamplerResources,
) {
    native_vulkan_vulkanalia_destroy_descriptor_heap_buffer(device, resources.sampler_heap);
    native_vulkan_vulkanalia_destroy_descriptor_heap_buffer(device, resources.resource_heap);
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

fn descriptor_offset_u32(offsets: &[u64], index: usize) -> Result<u32, String> {
    let offset = *offsets
        .get(index)
        .ok_or_else(|| format!("descriptor heap image index {index} has no offset"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_sampler_plan_aligns_offsets_and_heap_ranges() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
            NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
                image_count: 3,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 32,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 96,
                    max_sampler_heap_size: 2048,
                    min_sampler_heap_reserved_range: 48,
                    image_descriptor_size: 24,
                    sampler_descriptor_size: 16,
                    image_descriptor_alignment: 32,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        assert!(snapshot.backend_ready);
        assert_eq!(snapshot.descriptor_model, "VK_EXT_descriptor_heap");
        assert_eq!(snapshot.image_descriptor_stride, 32);
        assert_eq!(snapshot.sampler_descriptor_stride, 16);
        assert_eq!(snapshot.resource_heap_reserved_range_offset, 128);
        assert_eq!(snapshot.resource_heap_reserved_range_size, 128);
        assert_eq!(snapshot.sampler_heap_reserved_range_offset, 64);
        assert_eq!(snapshot.sampler_heap_reserved_range_size, 64);
        assert_eq!(snapshot.resource_heap_bytes, 256);
        assert_eq!(snapshot.sampler_heap_bytes, 128);
        assert_eq!(snapshot.image_descriptor_offsets, vec![0, 32, 64]);
        assert_eq!(snapshot.sampler_descriptor_offsets, vec![0, 16, 32]);
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_bind_resource_heap_ext")
        );
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_bind_sampler_heap_ext")
        );
    }

    #[test]
    fn image_sampler_plan_blocks_when_descriptor_sizes_are_missing() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
            NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
                image_count: 1,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            },
        );

        assert!(!snapshot.backend_ready);
        assert_eq!(
            snapshot.blocking_reason,
            Some("descriptor-heap-descriptor-sizes-unavailable")
        );
        assert_eq!(
            snapshot.command_order,
            vec!["wait_for_descriptor_heap_capabilities"]
        );
    }

    #[test]
    fn video_present_plane_plan_uses_one_descriptor_pair_per_plane() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
            NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
                image_count: 2,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 64,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    max_sampler_heap_size: 4096,
                    image_descriptor_size: 32,
                    sampler_descriptor_size: 16,
                    image_descriptor_alignment: 32,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        assert!(snapshot.backend_ready);
        assert_eq!(snapshot.image_count, 2);
        assert_eq!(snapshot.image_descriptor_offsets, vec![0, 32]);
        assert_eq!(snapshot.sampler_descriptor_offsets, vec![0, 16]);
        assert!(snapshot.resource_heap_bytes >= snapshot.image_descriptor_size);
        assert!(snapshot.sampler_heap_bytes >= snapshot.sampler_descriptor_size);
        assert!(
            snapshot
                .primary_reference
                .contains("FFmpeg-style retained frame lifetime")
        );
    }

    #[test]
    fn combined_image_sampler_mapping_uses_constant_heap_offsets() {
        let snapshot = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
            NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
                image_count: 2,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 64,
                    max_resource_heap_size: 4096,
                    min_resource_heap_reserved_range: 0,
                    max_sampler_heap_size: 4096,
                    image_descriptor_size: 32,
                    sampler_descriptor_size: 16,
                    image_descriptor_alignment: 32,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );

        let mapping =
            native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_mapping(&snapshot, 1)
                .expect("mapping should fit u32 offsets");

        assert_eq!(mapping.descriptor_set, 0);
        assert_eq!(mapping.first_binding, 0);
        assert_eq!(mapping.binding_count, 1);
        assert_eq!(
            mapping.resource_mask,
            vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE
        );
        assert_eq!(
            mapping.source,
            vk::DescriptorMappingSourceEXT::HEAP_WITH_CONSTANT_OFFSET
        );
        unsafe {
            assert_eq!(mapping.source_data.constant_offset.heap_offset, 32);
            assert_eq!(mapping.source_data.constant_offset.sampler_heap_offset, 16);
        }
    }
}

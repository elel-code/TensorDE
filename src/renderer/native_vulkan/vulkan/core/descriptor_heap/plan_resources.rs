use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr;

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
const SHADER_HEAP_BINDING_MAPPING_STYPE: i32 = 1000135005;
const SHADER_HEAP_BINDING_MAPPING_INFO_STYPE: i32 = 1000135006;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanDescriptorHeapShaderBindingMapping {
    s_type: vk::StructureType,
    next: *const c_void,
    pub heap_table: u32,
    pub first_binding: u32,
    pub binding_count: u32,
    pub resource_mask: vk::SpirvResourceTypeFlagsEXT,
    pub source: vk::DescriptorMappingSourceEXT,
    pub source_data: vk::DescriptorMappingSourceDataEXT,
}

#[repr(C)]
#[derive(Debug)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanDescriptorHeapShaderBindingMappingInfo<'a>
{
    s_type: vk::StructureType,
    next: *const c_void,
    mapping_count: u32,
    mappings: *const NativeVulkanDescriptorHeapShaderBindingMapping,
    _marker: PhantomData<&'a [NativeVulkanDescriptorHeapShaderBindingMapping]>,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(
    mappings: &[NativeVulkanDescriptorHeapShaderBindingMapping],
) -> Result<NativeVulkanDescriptorHeapShaderBindingMappingInfo<'_>, String> {
    let mapping_count = u32::try_from(mappings.len())
        .map_err(|_| "descriptor heap shader binding mapping count exceeds u32".to_owned())?;
    Ok(NativeVulkanDescriptorHeapShaderBindingMappingInfo {
        s_type: native_vulkan_vulkanalia_descriptor_heap_structure_type(
            SHADER_HEAP_BINDING_MAPPING_INFO_STYPE,
        ),
        next: ptr::null(),
        mapping_count,
        mappings: mappings.as_ptr(),
        _marker: PhantomData,
    })
}

fn native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping(
    first_binding: u32,
    resource_mask: vk::SpirvResourceTypeFlagsEXT,
    source_data: vk::DescriptorMappingSourceDataEXT,
) -> NativeVulkanDescriptorHeapShaderBindingMapping {
    NativeVulkanDescriptorHeapShaderBindingMapping {
        s_type: native_vulkan_vulkanalia_descriptor_heap_structure_type(
            SHADER_HEAP_BINDING_MAPPING_STYPE,
        ),
        next: ptr::null(),
        heap_table: 0,
        first_binding,
        binding_count: 1,
        resource_mask,
        source: vk::DescriptorMappingSourceEXT::HEAP_WITH_CONSTANT_OFFSET,
        source_data,
    }
}

fn native_vulkan_vulkanalia_descriptor_heap_structure_type(value: i32) -> vk::StructureType {
    unsafe { std::mem::transmute::<i32, vk::StructureType>(value) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
    pub image_count: usize,
    pub properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanInput {
    pub buffer_count: usize,
    pub properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind {
    SampledImage,
    UniformBuffer,
    StorageBuffer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
    pub resource_descriptors: Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    pub sampler_count: usize,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub descriptor_model: &'static str,
    pub backend_ready: bool,
    pub blocking_reason: Option<&'static str>,
    pub buffer_count: usize,
    pub resource_heap_alignment: u64,
    pub buffer_descriptor_size: u64,
    pub buffer_descriptor_stride: u64,
    pub resource_heap_bytes: u64,
    pub resource_heap_reserved_range_offset: u64,
    pub resource_heap_reserved_range_size: u64,
    pub buffer_descriptor_offsets: Vec<u64>,
    pub max_resource_heap_size: u64,
    pub command_order: Vec<&'static str>,
    pub next_gate: &'static str,
    pub primary_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub descriptor_model: &'static str,
    pub backend_ready: bool,
    pub blocking_reason: Option<&'static str>,
    pub resource_descriptor_count: usize,
    pub sampled_image_count: usize,
    pub uniform_buffer_count: usize,
    pub storage_buffer_count: usize,
    pub sampler_count: usize,
    pub resource_descriptor_kinds: Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    pub resource_descriptor_offsets: Vec<u64>,
    pub sampler_descriptor_offsets: Vec<u64>,
    pub resource_heap_alignment: u64,
    pub sampler_heap_alignment: u64,
    pub image_descriptor_size: u64,
    pub image_descriptor_stride: u64,
    pub buffer_descriptor_size: u64,
    pub buffer_descriptor_stride: u64,
    pub sampler_descriptor_size: u64,
    pub sampler_descriptor_stride: u64,
    pub resource_heap_bytes: u64,
    pub sampler_heap_bytes: u64,
    pub resource_heap_reserved_range_offset: u64,
    pub resource_heap_reserved_range_size: u64,
    pub sampler_heap_reserved_range_offset: u64,
    pub sampler_heap_reserved_range_size: u64,
    pub max_resource_heap_size: u64,
    pub max_sampler_heap_size: u64,
    pub command_order: Vec<&'static str>,
    pub next_gate: &'static str,
    pub primary_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDescriptorHeapUniformBufferResourceSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub descriptor_model: &'static str,
    pub resource_heap: NativeVulkanVulkanaliaDescriptorHeapBufferSnapshot,
    pub resource_descriptor_written: bool,
    pub shader_mapping_source: &'static str,
    pub shader_resource_mask: &'static str,
    pub command_order: Vec<&'static str>,
    pub zero_copy_gate: &'static str,
    pub primary_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDescriptorHeapResourceResourceSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub descriptor_model: &'static str,
    pub resource_heap: NativeVulkanVulkanaliaDescriptorHeapBufferSnapshot,
    pub sampler_heap: Option<NativeVulkanVulkanaliaDescriptorHeapBufferSnapshot>,
    pub resource_descriptors_written: usize,
    pub sampler_descriptors_written: usize,
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

pub(in crate::renderer::native_vulkan) struct VulkanaliaDescriptorHeapImageSamplerResources {
    pub(in crate::renderer::native_vulkan::vulkan) resource_heap: VulkanaliaDescriptorHeapBuffer,
    pub(in crate::renderer::native_vulkan::vulkan) sampler_heap: VulkanaliaDescriptorHeapBuffer,
    pub(in crate::renderer::native_vulkan::vulkan) plan:
        NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaDescriptorHeapImageSamplerResourceSnapshot,
}

pub(in crate::renderer::native_vulkan) struct VulkanaliaDescriptorHeapUniformBufferResources {
    pub(in crate::renderer::native_vulkan::vulkan) resource_heap: VulkanaliaDescriptorHeapBuffer,
    pub(in crate::renderer::native_vulkan::vulkan) plan:
        NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanSnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaDescriptorHeapUniformBufferResourceSnapshot,
}

pub(in crate::renderer::native_vulkan) struct VulkanaliaDescriptorHeapResourceResources {
    pub(in crate::renderer::native_vulkan::vulkan) resource_heap: VulkanaliaDescriptorHeapBuffer,
    pub(in crate::renderer::native_vulkan::vulkan) sampler_heap:
        Option<VulkanaliaDescriptorHeapBuffer>,
    pub(in crate::renderer::native_vulkan::vulkan) plan:
        NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaDescriptorHeapResourceResourceSnapshot,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
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
        next_gate: "allocate retained descriptor heap buffers and replace scene/video legacy pooled binding allocators with heap offsets",
        primary_reference: "VK_EXT_descriptor_heap device-addressable resource/sampler heaps; FFmpeg-style retained frame lifetime keeps descriptor writes tied to resource lifetime",
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_uniform_buffer_plan(
    input: NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanInput,
) -> NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanSnapshot {
    let properties = input.properties;
    let buffer_descriptor_stride = aligned_descriptor_stride(
        properties.buffer_descriptor_size,
        properties.buffer_descriptor_alignment,
    );
    let resource_descriptor_region_bytes = descriptor_heap_bytes(
        input.buffer_count,
        buffer_descriptor_stride,
        properties.resource_heap_alignment,
    );
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
    let descriptor_sizes_ready =
        properties.buffer_descriptor_size > 0 && buffer_descriptor_stride > 0;
    let resource_heap_fits = properties.max_resource_heap_size == 0
        || resource_heap_bytes <= properties.max_resource_heap_size;
    let backend_ready = input.buffer_count > 0 && descriptor_sizes_ready && resource_heap_fits;
    let blocking_reason = if input.buffer_count == 0 {
        Some("no-uniform-buffers")
    } else if !descriptor_sizes_ready {
        Some("descriptor-heap-buffer-descriptor-size-unavailable")
    } else if !resource_heap_fits {
        Some("resource-heap-range-too-small")
    } else {
        None
    };

    NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanSnapshot {
        binding: "vulkanalia",
        route: "descriptor-heap-uniform-buffer-plan",
        descriptor_model: "VK_EXT_descriptor_heap",
        backend_ready,
        blocking_reason,
        buffer_count: input.buffer_count,
        resource_heap_alignment: properties.resource_heap_alignment,
        buffer_descriptor_size: properties.buffer_descriptor_size,
        buffer_descriptor_stride,
        resource_heap_bytes,
        resource_heap_reserved_range_offset,
        resource_heap_reserved_range_size,
        buffer_descriptor_offsets: descriptor_offsets(input.buffer_count, buffer_descriptor_stride),
        max_resource_heap_size: properties.max_resource_heap_size,
        command_order: if backend_ready {
            vec![
                "create_device_addressable_resource_heap_buffer",
                "write_uniform_buffer_descriptors_into_resource_heap",
                "cmd_bind_resource_heap_ext",
                "draw_with_uniform_buffer_heap_mapping",
            ]
        } else {
            vec!["wait_for_descriptor_heap_capabilities"]
        },
        next_gate: "bind confirmed constant-buffer records through descriptor heap resource offsets",
        primary_reference: "VK_EXT_descriptor_heap uniform-buffer resource heap; WE dynamic constant-buffer commits must be lowered only after slot semantics are confirmed",
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_plan(
    input: NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
) -> NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot {
    let properties = input.properties;
    let image_descriptor_stride = aligned_descriptor_stride(
        properties.image_descriptor_size,
        properties.image_descriptor_alignment,
    );
    let buffer_descriptor_stride = aligned_descriptor_stride(
        properties.buffer_descriptor_size,
        properties.buffer_descriptor_alignment,
    );
    let sampler_descriptor_stride = aligned_descriptor_stride(
        properties.sampler_descriptor_size,
        properties.sampler_descriptor_alignment,
    );

    let sampled_image_count = input
        .resource_descriptors
        .iter()
        .filter(|kind| {
            **kind == NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage
        })
        .count();
    let uniform_buffer_count = input
        .resource_descriptors
        .iter()
        .filter(|kind| {
            **kind == NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer
        })
        .count();
    let storage_buffer_count = input
        .resource_descriptors
        .iter()
        .filter(|kind| {
            **kind == NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer
        })
        .count();
    let resource_descriptor_offsets = mixed_resource_descriptor_offsets(
        &input.resource_descriptors,
        properties.image_descriptor_alignment,
        image_descriptor_stride,
        properties.buffer_descriptor_alignment,
        buffer_descriptor_stride,
    );
    let resource_descriptor_region_bytes =
        resource_descriptor_offsets
            .last()
            .copied()
            .map_or(0, |last_offset| {
                let last_kind = input.resource_descriptors.last().copied().unwrap_or(
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                );
                last_offset.saturating_add(resource_descriptor_stride_for_kind(
                    last_kind,
                    image_descriptor_stride,
                    buffer_descriptor_stride,
                ))
            });
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
        input.sampler_count,
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

    let resource_descriptor_sizes_ready =
        input.resource_descriptors.iter().all(|kind| match kind {
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage => {
                properties.image_descriptor_size > 0 && image_descriptor_stride > 0
            }
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer => {
                properties.buffer_descriptor_size > 0 && buffer_descriptor_stride > 0
            }
            NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer => {
                properties.buffer_descriptor_size > 0 && buffer_descriptor_stride > 0
            }
        });
    let sampler_descriptor_sizes_ready = input.sampler_count == 0
        || (properties.sampler_descriptor_size > 0 && sampler_descriptor_stride > 0);
    let resource_heap_fits = properties.max_resource_heap_size == 0
        || resource_heap_bytes <= properties.max_resource_heap_size;
    let sampler_heap_fits = properties.max_sampler_heap_size == 0
        || sampler_heap_bytes <= properties.max_sampler_heap_size;
    let sampler_count_matches_images = input.sampler_count == sampled_image_count;
    let backend_ready = !input.resource_descriptors.is_empty()
        && resource_descriptor_sizes_ready
        && sampler_descriptor_sizes_ready
        && resource_heap_fits
        && sampler_heap_fits
        && sampler_count_matches_images;
    let blocking_reason = if input.resource_descriptors.is_empty() {
        Some("no-resource-descriptors")
    } else if !sampler_count_matches_images {
        Some("sampler-count-must-match-sampled-image-count")
    } else if !resource_descriptor_sizes_ready {
        Some("descriptor-heap-resource-descriptor-sizes-unavailable")
    } else if !sampler_descriptor_sizes_ready {
        Some("descriptor-heap-sampler-descriptor-size-unavailable")
    } else if !resource_heap_fits {
        Some("resource-heap-range-too-small")
    } else if !sampler_heap_fits {
        Some("sampler-heap-range-too-small")
    } else {
        None
    };

    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot {
        binding: "vulkanalia",
        route: "descriptor-heap-mixed-resource-plan",
        descriptor_model: "VK_EXT_descriptor_heap",
        backend_ready,
        blocking_reason,
        resource_descriptor_count: input.resource_descriptors.len(),
        sampled_image_count,
        uniform_buffer_count,
        storage_buffer_count,
        sampler_count: input.sampler_count,
        resource_descriptor_kinds: input.resource_descriptors,
        resource_descriptor_offsets,
        sampler_descriptor_offsets: descriptor_offsets(
            input.sampler_count,
            sampler_descriptor_stride,
        ),
        resource_heap_alignment: properties.resource_heap_alignment,
        sampler_heap_alignment: properties.sampler_heap_alignment,
        image_descriptor_size: properties.image_descriptor_size,
        image_descriptor_stride,
        buffer_descriptor_size: properties.buffer_descriptor_size,
        buffer_descriptor_stride,
        sampler_descriptor_size: properties.sampler_descriptor_size,
        sampler_descriptor_stride,
        resource_heap_bytes,
        sampler_heap_bytes,
        resource_heap_reserved_range_offset,
        resource_heap_reserved_range_size,
        sampler_heap_reserved_range_offset,
        sampler_heap_reserved_range_size,
        max_resource_heap_size: properties.max_resource_heap_size,
        max_sampler_heap_size: properties.max_sampler_heap_size,
        command_order: if backend_ready {
            vec![
                "pack_draw_heap_slices",
                "create_device_addressable_resource_heap_buffer",
                "create_device_addressable_sampler_heap_buffer",
                "write_uniform_buffer_descriptors_into_resource_heap",
                "write_image_descriptors_into_same_resource_heap",
                "write_sampler_descriptors_into_sampler_heap",
                "cmd_bind_resource_heap_ext_once_per_draw_heap_slice",
                "cmd_bind_sampler_heap_ext_once_per_draw_heap_slice",
            ]
        } else {
            vec!["wait_for_descriptor_heap_capabilities"]
        },
        next_gate: "bind scene draw heap slices containing WE constant buffers and textures",
        primary_reference: "VK_EXT_descriptor_heap mixed resource heap; WE PSSetConstantBuffers(slot=3) and g_TextureN sampled images must share the draw heap slice",
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_create_descriptor_heap_image_sampler_resources(
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_create_descriptor_heap_uniform_buffer_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    plan: &NativeVulkanVulkanaliaDescriptorHeapUniformBufferPlanSnapshot,
) -> Result<VulkanaliaDescriptorHeapUniformBufferResources, String> {
    if !plan.backend_ready {
        return Err(format!(
            "descriptor heap uniform-buffer resources require ready plan: {:?}",
            plan.blocking_reason
        ));
    }

    let resource_heap = create_descriptor_heap_buffer(
        device,
        memory_properties,
        "uniform-buffer-resource-heap",
        plan.resource_heap_bytes,
    )?;

    Ok(VulkanaliaDescriptorHeapUniformBufferResources {
        plan: plan.clone(),
        snapshot: NativeVulkanVulkanaliaDescriptorHeapUniformBufferResourceSnapshot {
            binding: "vulkanalia",
            route: "descriptor-heap-uniform-buffer-retained-resource",
            descriptor_model: "VK_EXT_descriptor_heap",
            resource_heap: resource_heap.snapshot.clone(),
            resource_descriptor_written: false,
            shader_mapping_source: "heap-with-constant-offset",
            shader_resource_mask: "uniform-buffer",
            command_order: vec![
                "create_device_addressable_resource_heap_buffer",
                "write_uniform_buffer_resource_descriptors_ext",
                "cmd_bind_resource_heap_ext",
                "draw_with_descriptor_heap_uniform_mapping",
            ],
            zero_copy_gate: "uniform payload remains in a retained GPU buffer; descriptor heap only binds device address ranges",
            primary_reference: plan.primary_reference,
        },
        resource_heap,
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_create_descriptor_heap_resource_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
) -> Result<VulkanaliaDescriptorHeapResourceResources, String> {
    if !plan.backend_ready {
        return Err(format!(
            "descriptor heap mixed resources require ready plan: {:?}",
            plan.blocking_reason
        ));
    }

    let resource_heap = create_descriptor_heap_buffer(
        device,
        memory_properties,
        "mixed-resource-heap",
        plan.resource_heap_bytes,
    )?;
    let sampler_heap = if plan.sampler_count == 0 {
        None
    } else {
        match create_descriptor_heap_buffer(
            device,
            memory_properties,
            "mixed-sampler-heap",
            plan.sampler_heap_bytes,
        ) {
            Ok(sampler_heap) => Some(sampler_heap),
            Err(err) => {
                native_vulkan_vulkanalia_destroy_descriptor_heap_buffer(device, resource_heap);
                return Err(err);
            }
        }
    };

    Ok(VulkanaliaDescriptorHeapResourceResources {
        snapshot: NativeVulkanVulkanaliaDescriptorHeapResourceResourceSnapshot {
            binding: "vulkanalia",
            route: "descriptor-heap-mixed-resource-retained-resource",
            descriptor_model: "VK_EXT_descriptor_heap",
            resource_heap: resource_heap.snapshot.clone(),
            sampler_heap: sampler_heap.as_ref().map(|heap| heap.snapshot.clone()),
            resource_descriptors_written: 0,
            sampler_descriptors_written: 0,
            shader_mapping_source: "heap-with-constant-offset",
            shader_resource_mask: "uniform-buffer|combined-sampled-image",
            command_order: vec![
                "create_device_addressable_resource_heap_buffer",
                "create_device_addressable_sampler_heap_buffer_when_sampled_images_exist",
                "write_uniform_buffer_descriptors_into_resource_heap",
                "write_sampled_image_descriptors_into_same_resource_heap",
                "write_sampler_descriptors_into_sampler_heap",
                "cmd_bind_resource_heap_ext",
                "cmd_bind_sampler_heap_ext_when_sampled_images_exist",
                "draw_with_mixed_resource_heap_mapping",
            ],
            zero_copy_gate: "uniform payload and sampled images remain retained GPU resources; descriptor heap only binds device address ranges and image/sampler handles",
            primary_reference: plan.primary_reference,
        },
        plan: plan.clone(),
        resource_heap,
        sampler_heap,
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_write_descriptor_heap_image_sampler(
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_descriptor_heap_resource_relative_storage_buffer_binding_mapping(
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
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::StorageBuffer,
    )?;
    let base_heap_offset = *plan
        .resource_descriptor_offsets
        .get(base_resource_descriptor_index)
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed base storage descriptor index {base_resource_descriptor_index} has no resource offset"
            )
        })?;
    let base_heap_offset = align_down(base_heap_offset, plan.resource_heap_alignment);
    let heap_offset = plan
        .resource_descriptor_offsets
        .get(resource_descriptor_index)
        .copied()
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed storage descriptor index {resource_descriptor_index} has no resource offset"
            )
        })?
        .checked_sub(base_heap_offset)
        .ok_or_else(|| {
            format!(
                "descriptor heap mixed storage descriptor index {resource_descriptor_index} precedes heap-slice base {base_resource_descriptor_index}"
            )
        })?;
    let heap_offset = u32::try_from(heap_offset)
        .map_err(|_| "descriptor heap mixed relative storage offset exceeds u32".to_owned())?;
    let heap_array_stride = u32::try_from(plan.buffer_descriptor_stride)
        .map_err(|_| "descriptor heap mixed storage-buffer stride exceeds u32".to_owned())?;
    let source = vk::DescriptorMappingSourceConstantOffsetEXT::builder()
        .heap_offset(heap_offset)
        .heap_array_stride(heap_array_stride)
        .sampler_heap_offset(0)
        .sampler_heap_array_stride(0)
        .build();

    Ok(
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping(
            binding,
            vk::SpirvResourceTypeFlagsEXT::READ_ONLY_STORAGE_BUFFER,
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

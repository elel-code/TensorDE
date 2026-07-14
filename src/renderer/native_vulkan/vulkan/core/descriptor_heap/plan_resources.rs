use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder};

#[path = "plan_resources/descriptor_writes.rs"]
mod descriptor_writes;

pub(in crate::renderer::native_vulkan) use descriptor_writes::*;

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

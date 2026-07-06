//! Generic Vulkan buffer allocation helpers for scene resource residency.
//!
//! References:
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/storage/`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::memory::{
    native_vulkan_vulkanalia_bind_buffer_memory2, native_vulkan_vulkanalia_map_memory2,
    native_vulkan_vulkanalia_unmap_memory2,
};
use super::video_session::{
    NativeVulkanVulkanaliaMemoryTypeCandidate, native_vulkan_vulkanalia_memory_type_candidates,
};

const HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS: u32 =
    vk::MemoryPropertyFlags::HOST_VISIBLE.bits() | vk::MemoryPropertyFlags::HOST_COHERENT.bits();
const HOST_VISIBLE_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::HOST_VISIBLE.bits();
const DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanVulkanaliaBufferMemoryPreference {
    HostUpload,
    DeviceLocal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaBufferSnapshot {
    pub role: &'static str,
    pub buffer_created: bool,
    pub memory_bound: bool,
    pub mapped: bool,
    pub requested_bytes: u64,
    pub memory_size: u64,
    pub memory_alignment: u64,
    pub memory_type_bits: u32,
    pub selected_memory_type_index: u32,
    pub selected_memory_property_flags: Vec<&'static str>,
    pub usage_flags: Vec<&'static str>,
    pub host_coherent: bool,
    pub payload_uploaded: bool,
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanVulkanaliaBuffer {
    pub(in crate::renderer::native_vulkan) buffer: vk::Buffer,
    pub(in crate::renderer::native_vulkan) memory: vk::DeviceMemory,
    pub(in crate::renderer::native_vulkan) snapshot: NativeVulkanVulkanaliaBufferSnapshot,
}

unsafe impl Send for NativeVulkanVulkanaliaBuffer {}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_create_buffer(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    role: &'static str,
    requested_bytes: u64,
    usage: vk::BufferUsageFlags,
    memory_preference: NativeVulkanVulkanaliaBufferMemoryPreference,
    upload_payload: Option<&[u8]>,
) -> Result<NativeVulkanVulkanaliaBuffer, String> {
    let size = requested_bytes.max(1);
    if let Some(payload) = upload_payload
        && payload.len() as u64 > size
    {
        return Err(format!(
            "{role} upload payload {} exceeds requested buffer size {size}",
            payload.len()
        ));
    }

    let create_info = vk::BufferCreateInfo::builder()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&create_info, None) }
        .map_err(|err| format!("vkCreateBuffer(vulkanalia {role}): {err:?}"))?;

    let result = (|| -> Result<NativeVulkanVulkanaliaBuffer, String> {
        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let memory_type = native_vulkan_vulkanalia_buffer_memory_type(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
            memory_preference,
        )
        .ok_or_else(|| {
            format!(
                "{role} buffer has no matching memory type for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;

        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type.index);
        let memory = unsafe { device.allocate_memory(&allocation_info, None) }
            .map_err(|err| format!("vkAllocateMemory(vulkanalia {role}): {err:?}"))?;

        if let Err(err) =
            native_vulkan_vulkanalia_bind_buffer_memory2(device, buffer, memory, 0, role)
        {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(err);
        }

        let host_visible = memory_type.property_flags_bits & HOST_VISIBLE_MEMORY_FLAG_BITS
            == HOST_VISIBLE_MEMORY_FLAG_BITS;
        let host_coherent = memory_type.property_flags_bits
            & vk::MemoryPropertyFlags::HOST_COHERENT.bits()
            == vk::MemoryPropertyFlags::HOST_COHERENT.bits();
        let payload_uploaded = if let Some(payload) = upload_payload {
            if !host_visible {
                unsafe {
                    device.free_memory(memory, None);
                }
                return Err(format!("{role} upload requested non-host-visible memory"));
            }
            let mapped_ptr = native_vulkan_vulkanalia_map_memory2(
                device,
                memory,
                0,
                memory_requirements.size,
                vk::MemoryMapFlags::empty(),
                role,
            )?;
            unsafe {
                std::ptr::copy_nonoverlapping(payload.as_ptr(), mapped_ptr.cast(), payload.len());
            }
            native_vulkan_vulkanalia_unmap_memory2(device, memory, role)?;
            true
        } else {
            false
        };

        Ok(NativeVulkanVulkanaliaBuffer {
            buffer,
            memory,
            snapshot: NativeVulkanVulkanaliaBufferSnapshot {
                role,
                buffer_created: true,
                memory_bound: true,
                mapped: false,
                requested_bytes: size,
                memory_size: memory_requirements.size,
                memory_alignment: memory_requirements.alignment,
                memory_type_bits: memory_requirements.memory_type_bits,
                selected_memory_type_index: memory_type.index,
                selected_memory_property_flags: memory_property_flag_labels(
                    memory_type.property_flags_bits,
                ),
                usage_flags: buffer_usage_flag_labels(usage),
                host_coherent,
                payload_uploaded,
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_destroy_buffer(
    device: &Device,
    buffer: NativeVulkanVulkanaliaBuffer,
) {
    unsafe {
        device.destroy_buffer(buffer.buffer, None);
        device.free_memory(buffer.memory, None);
    }
}

fn native_vulkan_vulkanalia_buffer_memory_type(
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
    allowed_memory_type_bits: u32,
    memory_preference: NativeVulkanVulkanaliaBufferMemoryPreference,
) -> Option<NativeVulkanVulkanaliaMemoryTypeCandidate> {
    match memory_preference {
        NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload => {
            native_vulkan_vulkanalia_buffer_memory_type_matching(
                memory_types,
                allowed_memory_type_bits,
                HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS,
            )
            .or_else(|| {
                native_vulkan_vulkanalia_buffer_memory_type_matching(
                    memory_types,
                    allowed_memory_type_bits,
                    HOST_VISIBLE_MEMORY_FLAG_BITS,
                )
            })
        }
        NativeVulkanVulkanaliaBufferMemoryPreference::DeviceLocal => {
            native_vulkan_vulkanalia_buffer_memory_type_matching(
                memory_types,
                allowed_memory_type_bits,
                DEVICE_LOCAL_MEMORY_FLAG_BITS,
            )
            .or_else(|| {
                native_vulkan_vulkanalia_buffer_memory_type_matching(
                    memory_types,
                    allowed_memory_type_bits,
                    HOST_VISIBLE_MEMORY_FLAG_BITS,
                )
            })
        }
    }
}

fn native_vulkan_vulkanalia_buffer_memory_type_matching(
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
    allowed_memory_type_bits: u32,
    required_property_flags: u32,
) -> Option<NativeVulkanVulkanaliaMemoryTypeCandidate> {
    memory_types.iter().copied().find(|candidate| {
        let allowed = candidate.index < u32::BITS
            && allowed_memory_type_bits & (1u32 << candidate.index) != 0;
        let properties_match =
            candidate.property_flags_bits & required_property_flags == required_property_flags;
        allowed && properties_match
    })
}

fn buffer_usage_flag_labels(flags: vk::BufferUsageFlags) -> Vec<&'static str> {
    [
        (vk::BufferUsageFlags::TRANSFER_SRC.bits(), "transfer-src"),
        (vk::BufferUsageFlags::TRANSFER_DST.bits(), "transfer-dst"),
        (
            vk::BufferUsageFlags::UNIFORM_BUFFER.bits(),
            "uniform-buffer",
        ),
        (
            vk::BufferUsageFlags::STORAGE_BUFFER.bits(),
            "storage-buffer",
        ),
        (vk::BufferUsageFlags::INDEX_BUFFER.bits(), "index-buffer"),
        (vk::BufferUsageFlags::VERTEX_BUFFER.bits(), "vertex-buffer"),
        (
            vk::BufferUsageFlags::INDIRECT_BUFFER.bits(),
            "indirect-buffer",
        ),
    ]
    .into_iter()
    .filter_map(|(bit, label)| (flags.bits() & bit == bit).then_some(label))
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
    .into_iter()
    .filter_map(|(bit, label)| (flags & bit == bit).then_some(label))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_upload_memory_prefers_coherent_then_host_visible() {
        let memory_types = vec![
            memory_type_candidate(0, vk::MemoryPropertyFlags::DEVICE_LOCAL),
            memory_type_candidate(1, vk::MemoryPropertyFlags::HOST_VISIBLE),
            memory_type_candidate(
                2,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            ),
        ];

        let selected = native_vulkan_vulkanalia_buffer_memory_type(
            &memory_types,
            0b111,
            NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
        )
        .expect("host visible memory type");
        assert_eq!(selected.index, 2);
    }

    #[test]
    fn device_local_memory_falls_back_to_host_visible() {
        let memory_types = vec![memory_type_candidate(
            1,
            vk::MemoryPropertyFlags::HOST_VISIBLE,
        )];

        let selected = native_vulkan_vulkanalia_buffer_memory_type(
            &memory_types,
            0b10,
            NativeVulkanVulkanaliaBufferMemoryPreference::DeviceLocal,
        )
        .expect("fallback host visible memory type");
        assert_eq!(selected.index, 1);
    }

    fn memory_type_candidate(
        index: u32,
        property_flags: vk::MemoryPropertyFlags,
    ) -> NativeVulkanVulkanaliaMemoryTypeCandidate {
        NativeVulkanVulkanaliaMemoryTypeCandidate {
            index,
            property_flags_bits: property_flags.bits(),
        }
    }
}

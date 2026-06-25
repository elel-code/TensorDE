use std::ptr;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::video_session::{
    NativeVulkanVulkanaliaMemoryTypeCandidate, native_vulkan_vulkanalia_memory_type_candidates,
};

const HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS: u32 =
    vk::MemoryPropertyFlags::HOST_VISIBLE.bits() | vk::MemoryPropertyFlags::HOST_COHERENT.bits();
const HOST_VISIBLE_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::HOST_VISIBLE.bits();

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionBitstreamBufferSmokeSnapshot {
    pub buffer_created: bool,
    pub memory_bound: bool,
    pub mapped: bool,
    pub flushed: bool,
    pub buffer: NativeVulkanVulkanaliaVideoSessionBitstreamBufferSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionBitstreamBufferSnapshot {
    pub requested_size: u64,
    pub size: u64,
    pub min_size_alignment: u64,
    pub usage_flags: Vec<&'static str>,
    pub memory_size: u64,
    pub memory_alignment: u64,
    pub memory_type_bits: u32,
    pub selected_memory_type_index: u32,
    pub selected_memory_property_flags: Vec<&'static str>,
    pub mapped_write_bytes: u64,
    pub mapped_write_source: &'static str,
    pub mapped_write_hash: Option<u64>,
    pub host_visible: bool,
    pub host_coherent: bool,
    pub keep_mapped: bool,
}

pub(super) struct VulkanaliaVideoSessionBitstreamBuffer {
    pub(super) buffer: vk::Buffer,
    pub(super) memory: vk::DeviceMemory,
    pub(super) mapped_ptr: Option<*mut std::ffi::c_void>,
    pub(super) snapshot: NativeVulkanVulkanaliaVideoSessionBitstreamBufferSnapshot,
}

pub(super) fn native_vulkan_vulkanalia_smoke_create_video_session_bitstream_buffer(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    profile_info: &vk::VideoProfileInfoKHR,
    requested_size: u64,
    min_size_alignment: u64,
    write_payload: Option<&[u8]>,
    keep_mapped: bool,
) -> Result<NativeVulkanVulkanaliaVideoSessionBitstreamBufferSmokeSnapshot, String> {
    let buffer = native_vulkan_vulkanalia_create_video_session_bitstream_buffer(
        device,
        memory_properties,
        profile_info,
        requested_size,
        min_size_alignment,
        write_payload,
        keep_mapped,
    )?;
    let snapshot = NativeVulkanVulkanaliaVideoSessionBitstreamBufferSmokeSnapshot {
        buffer_created: true,
        memory_bound: true,
        mapped: true,
        flushed: !buffer.snapshot.host_coherent,
        buffer: buffer.snapshot.clone(),
    };
    native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer(device, buffer);
    Ok(snapshot)
}

pub(super) fn native_vulkan_vulkanalia_create_video_session_bitstream_buffer(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    profile_info: &vk::VideoProfileInfoKHR,
    requested_size: u64,
    min_size_alignment: u64,
    write_payload: Option<&[u8]>,
    keep_mapped: bool,
) -> Result<VulkanaliaVideoSessionBitstreamBuffer, String> {
    let size = native_vulkan_vulkanalia_align_up(requested_size.max(1), min_size_alignment.max(1));
    let usage = vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR;
    let profiles = [*profile_info];
    let mut profile_list_info = vk::VideoProfileListInfoKHR::builder()
        .profiles(&profiles)
        .build();
    let create_info = vk::BufferCreateInfo::builder()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .push_next(&mut profile_list_info);
    let buffer = unsafe { device.create_buffer(&create_info, None) }
        .map_err(|err| format!("vkCreateBuffer(vulkanalia video bitstream): {err:?}"))?;

    let mut buffer_destroyed = false;
    let result = (|| -> Result<VulkanaliaVideoSessionBitstreamBuffer, String> {
        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let memory_type = native_vulkan_vulkanalia_bitstream_memory_type_index(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
            HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS,
        )
        .or_else(|| {
            native_vulkan_vulkanalia_bitstream_memory_type_index(
                &memory_type_candidates,
                memory_requirements.memory_type_bits,
                HOST_VISIBLE_MEMORY_FLAG_BITS,
            )
        })
        .ok_or_else(|| {
            format!(
                "video bitstream buffer has no host-visible memory type for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type.index);
        let memory = unsafe { device.allocate_memory(&allocation_info, None) }
            .map_err(|err| format!("vkAllocateMemory(vulkanalia video bitstream): {err:?}"))?;

        if let Err(err) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.destroy_buffer(buffer, None);
                buffer_destroyed = true;
                device.free_memory(memory, None);
            }
            return Err(format!(
                "vkBindBufferMemory(vulkanalia video bitstream): {err:?}"
            ));
        }

        let mapped_write_bytes = if keep_mapped {
            memory_requirements.size
        } else {
            write_payload
                .map(|payload| payload.len() as u64)
                .unwrap_or_else(|| size.min(256))
        };
        let map = match unsafe {
            device.map_memory(memory, 0, mapped_write_bytes, vk::MemoryMapFlags::empty())
        } {
            Ok(map) => map,
            Err(err) => {
                unsafe {
                    device.destroy_buffer(buffer, None);
                    buffer_destroyed = true;
                    device.free_memory(memory, None);
                }
                return Err(format!("vkMapMemory(vulkanalia video bitstream): {err:?}"));
            }
        };
        if let Some(payload) = write_payload {
            unsafe {
                ptr::copy_nonoverlapping(payload.as_ptr(), map.cast::<u8>(), payload.len());
            }
        } else {
            unsafe {
                ptr::write_bytes(map.cast::<u8>(), 0, mapped_write_bytes as usize);
            }
        }

        let host_coherent = memory_type.property_flags_bits
            & vk::MemoryPropertyFlags::HOST_COHERENT.bits()
            == vk::MemoryPropertyFlags::HOST_COHERENT.bits();
        if !host_coherent {
            let range = vk::MappedMemoryRange::builder()
                .memory(memory)
                .offset(0)
                .size(mapped_write_bytes)
                .build();
            if let Err(err) = unsafe { device.flush_mapped_memory_ranges(&[range]) } {
                unsafe {
                    device.unmap_memory(memory);
                    device.destroy_buffer(buffer, None);
                    buffer_destroyed = true;
                    device.free_memory(memory, None);
                }
                return Err(format!(
                    "vkFlushMappedMemoryRanges(vulkanalia video bitstream): {err:?}"
                ));
            }
        }
        let mapped_ptr = if keep_mapped {
            Some(map)
        } else {
            unsafe {
                device.unmap_memory(memory);
            }
            None
        };

        Ok(VulkanaliaVideoSessionBitstreamBuffer {
            buffer,
            memory,
            mapped_ptr,
            snapshot: NativeVulkanVulkanaliaVideoSessionBitstreamBufferSnapshot {
                requested_size,
                size,
                min_size_alignment,
                usage_flags: buffer_usage_flag_labels(usage),
                memory_size: memory_requirements.size,
                memory_alignment: memory_requirements.alignment,
                memory_type_bits: memory_requirements.memory_type_bits,
                selected_memory_type_index: memory_type.index,
                selected_memory_property_flags: memory_property_flag_labels(
                    memory_type.property_flags_bits,
                ),
                mapped_write_bytes,
                mapped_write_source: if keep_mapped {
                    "persistent-mapped-reusable-slot"
                } else if write_payload.is_some() {
                    "extracted-encoded-video-unit"
                } else {
                    "zero-fill-smoke-pattern"
                },
                mapped_write_hash: write_payload.map(stable_byte_hash),
                host_visible: memory_type.property_flags_bits
                    & vk::MemoryPropertyFlags::HOST_VISIBLE.bits()
                    == vk::MemoryPropertyFlags::HOST_VISIBLE.bits(),
                host_coherent,
                keep_mapped,
            },
        })
    })();

    if result.is_err() && !buffer_destroyed {
        unsafe {
            device.destroy_buffer(buffer, None);
        }
    }
    result
}

pub(super) fn native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer(
    device: &Device,
    buffer: VulkanaliaVideoSessionBitstreamBuffer,
) {
    unsafe {
        if buffer.mapped_ptr.is_some() {
            device.unmap_memory(buffer.memory);
        }
        device.destroy_buffer(buffer.buffer, None);
        device.free_memory(buffer.memory, None);
    }
}

pub(super) fn native_vulkan_vulkanalia_write_video_session_bitstream_payload(
    device: &Device,
    buffer: &VulkanaliaVideoSessionBitstreamBuffer,
    payload: &[u8],
    min_size_alignment: u64,
) -> Result<u64, String> {
    if payload.is_empty() {
        return Err("Vulkanalia streaming bitstream payload cannot be empty".to_owned());
    }
    let mapped_ptr = buffer.mapped_ptr.ok_or_else(|| {
        "Vulkanalia streaming bitstream buffer is not persistently mapped".to_owned()
    })?;
    let src_buffer_range =
        native_vulkan_vulkanalia_align_up(payload.len() as u64, min_size_alignment.max(1));
    if src_buffer_range > buffer.snapshot.size {
        return Err(format!(
            "Vulkanalia streaming bitstream payload range {src_buffer_range} exceeds buffer size {}",
            buffer.snapshot.size
        ));
    }
    unsafe {
        ptr::copy_nonoverlapping(payload.as_ptr(), mapped_ptr.cast::<u8>(), payload.len());
        if src_buffer_range as usize > payload.len() {
            ptr::write_bytes(
                mapped_ptr.cast::<u8>().add(payload.len()),
                0,
                src_buffer_range as usize - payload.len(),
            );
        }
    }
    if !buffer.snapshot.host_coherent {
        let range = vk::MappedMemoryRange::builder()
            .memory(buffer.memory)
            .offset(0)
            .size(src_buffer_range)
            .build();
        unsafe {
            device.flush_mapped_memory_ranges(&[range]).map_err(|err| {
                format!("vkFlushMappedMemoryRanges(vulkanalia streaming bitstream): {err:?}")
            })?;
        }
    }
    Ok(src_buffer_range)
}

fn native_vulkan_vulkanalia_bitstream_memory_type_index(
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
    allowed_memory_type_bits: u32,
    required_property_flags_bits: u32,
) -> Option<NativeVulkanVulkanaliaMemoryTypeCandidate> {
    memory_types.iter().copied().find(|candidate| {
        let allowed = candidate.index < u32::BITS
            && allowed_memory_type_bits & (1u32 << candidate.index) != 0;
        let properties_match = candidate.property_flags_bits & required_property_flags_bits
            == required_property_flags_bits;
        allowed && properties_match
    })
}

fn native_vulkan_vulkanalia_align_up(value: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    value
        .checked_add(alignment.saturating_sub(1))
        .map(|aligned| aligned / alignment * alignment)
        .unwrap_or(value)
}

fn buffer_usage_flag_labels(flags: vk::BufferUsageFlags) -> Vec<&'static str> {
    [
        (vk::BufferUsageFlags::TRANSFER_SRC, "transfer-src"),
        (vk::BufferUsageFlags::TRANSFER_DST, "transfer-dst"),
        (
            vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER,
            "uniform-texel-buffer",
        ),
        (
            vk::BufferUsageFlags::STORAGE_TEXEL_BUFFER,
            "storage-texel-buffer",
        ),
        (vk::BufferUsageFlags::UNIFORM_BUFFER, "uniform-buffer"),
        (vk::BufferUsageFlags::STORAGE_BUFFER, "storage-buffer"),
        (vk::BufferUsageFlags::INDEX_BUFFER, "index-buffer"),
        (vk::BufferUsageFlags::VERTEX_BUFFER, "vertex-buffer"),
        (vk::BufferUsageFlags::INDIRECT_BUFFER, "indirect-buffer"),
        (
            vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR,
            "video-decode-src",
        ),
    ]
    .into_iter()
    .filter_map(|(flag, label)| flags.contains(flag).then_some(label))
    .collect()
}

fn memory_property_flag_labels(bits: u32) -> Vec<&'static str> {
    [
        (vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(), "device-local"),
        (vk::MemoryPropertyFlags::HOST_VISIBLE.bits(), "host-visible"),
        (
            vk::MemoryPropertyFlags::HOST_COHERENT.bits(),
            "host-coherent",
        ),
        (vk::MemoryPropertyFlags::HOST_CACHED.bits(), "host-cached"),
        (
            vk::MemoryPropertyFlags::LAZILY_ALLOCATED.bits(),
            "lazily-allocated",
        ),
        (vk::MemoryPropertyFlags::PROTECTED.bits(), "protected"),
    ]
    .into_iter()
    .filter_map(|(flag_bits, label)| (bits & flag_bits == flag_bits).then_some(label))
    .collect()
}

fn stable_byte_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitstream_buffer_usage_labels_cover_decode_src() {
        let labels = buffer_usage_flag_labels(vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR);

        assert_eq!(labels, vec!["video-decode-src"]);
    }

    #[test]
    fn bitstream_memory_type_selection_prefers_host_visible_coherent() {
        let memory_types = vec![
            NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: 0,
                property_flags_bits: vk::MemoryPropertyFlags::HOST_VISIBLE.bits(),
            },
            NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: 1,
                property_flags_bits: vk::MemoryPropertyFlags::HOST_VISIBLE.bits()
                    | vk::MemoryPropertyFlags::HOST_COHERENT.bits(),
            },
        ];

        let selected = native_vulkan_vulkanalia_bitstream_memory_type_index(
            &memory_types,
            0b11,
            HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS,
        )
        .expect("host-visible coherent memory type");

        assert_eq!(selected.index, 1);
    }

    #[test]
    fn bitstream_alignment_matches_min_size_alignment() {
        assert_eq!(native_vulkan_vulkanalia_align_up(257, 256), 512);
        assert_eq!(native_vulkan_vulkanalia_align_up(256, 256), 256);
        assert_eq!(native_vulkan_vulkanalia_align_up(1, 0), 1);
    }

    #[test]
    fn bitstream_hash_matches_native_vulkan_fnv1a() {
        assert_eq!(stable_byte_hash(b"gilder"), 0x94b7_2934_9cd9_5876);
    }
}

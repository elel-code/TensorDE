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
    pub device_address_nonzero: bool,
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
    pub(in crate::renderer::native_vulkan) device_address: vk::DeviceAddress,
    pub(in crate::renderer::native_vulkan) snapshot: NativeVulkanVulkanaliaBufferSnapshot,
}

unsafe impl Send for NativeVulkanVulkanaliaBuffer {}

pub(in crate::renderer::native_vulkan) struct NativeVulkanVulkanaliaRecordedBufferUpload {
    pub(in crate::renderer::native_vulkan) target: NativeVulkanVulkanaliaBuffer,
    pub(in crate::renderer::native_vulkan) staging: Option<NativeVulkanVulkanaliaBuffer>,
    pub(in crate::renderer::native_vulkan) copy_recorded: bool,
}

unsafe impl Send for NativeVulkanVulkanaliaRecordedBufferUpload {}

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

        let wants_device_address = usage.contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS);
        let mut allocate_flags = vk::MemoryAllocateFlagsInfo::builder()
            .flags(vk::MemoryAllocateFlags::DEVICE_ADDRESS)
            .build();
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type.index);
        let allocation_info = if wants_device_address {
            allocation_info.push_next(&mut allocate_flags)
        } else {
            allocation_info
        };
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
            let flush_result = if host_coherent {
                Ok(())
            } else {
                let range = vk::MappedMemoryRange::builder()
                    .memory(memory)
                    .offset(0)
                    .size(vk::WHOLE_SIZE)
                    .build();
                unsafe { device.flush_mapped_memory_ranges(&[range]) }.map_err(|err| {
                    format!("vkFlushMappedMemoryRanges(vulkanalia {role} initial upload): {err:?}")
                })
            };
            let unmap_result = native_vulkan_vulkanalia_unmap_memory2(device, memory, role);
            flush_result?;
            unmap_result?;
            true
        } else {
            false
        };
        let device_address = if wants_device_address {
            let address_info = vk::BufferDeviceAddressInfo::builder()
                .buffer(buffer)
                .build();
            unsafe { device.get_buffer_device_address(&address_info) }
        } else {
            0
        };

        Ok(NativeVulkanVulkanaliaBuffer {
            buffer,
            memory,
            device_address,
            snapshot: NativeVulkanVulkanaliaBufferSnapshot {
                role,
                buffer_created: true,
                memory_bound: true,
                mapped: false,
                device_address_nonzero: device_address != 0,
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

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_write_host_buffer(
    device: &Device,
    buffer: &NativeVulkanVulkanaliaBuffer,
    payload: &[u8],
) -> Result<(), String> {
    let role = buffer.snapshot.role;
    if payload.len() as u64 > buffer.snapshot.requested_bytes {
        return Err(format!(
            "{role} update payload {} exceeds requested buffer size {}",
            payload.len(),
            buffer.snapshot.requested_bytes
        ));
    }
    if !buffer
        .snapshot
        .selected_memory_property_flags
        .contains(&"host-visible")
    {
        return Err(format!("{role} update requires host-visible memory"));
    }
    let mapped_ptr = native_vulkan_vulkanalia_map_memory2(
        device,
        buffer.memory,
        0,
        buffer.snapshot.memory_size,
        vk::MemoryMapFlags::empty(),
        role,
    )?;
    unsafe {
        std::ptr::copy_nonoverlapping(payload.as_ptr(), mapped_ptr.cast(), payload.len());
    }
    let flush_result = if buffer.snapshot.host_coherent {
        Ok(())
    } else {
        let range = vk::MappedMemoryRange::builder()
            .memory(buffer.memory)
            .offset(0)
            .size(vk::WHOLE_SIZE)
            .build();
        unsafe { device.flush_mapped_memory_ranges(&[range]) }
            .map_err(|err| format!("vkFlushMappedMemoryRanges(vulkanalia {role}): {err:?}"))
    };
    let unmap_result = native_vulkan_vulkanalia_unmap_memory2(device, buffer.memory, role);
    flush_result?;
    unmap_result
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_read_host_buffer(
    device: &Device,
    buffer: &NativeVulkanVulkanaliaBuffer,
    byte_count: u64,
) -> Result<Vec<u8>, String> {
    let role = buffer.snapshot.role;
    if byte_count > buffer.snapshot.requested_bytes {
        return Err(format!(
            "{role} read byte count {byte_count} exceeds requested buffer size {}",
            buffer.snapshot.requested_bytes
        ));
    }
    if !buffer
        .snapshot
        .selected_memory_property_flags
        .contains(&"host-visible")
    {
        return Err(format!("{role} read requires host-visible memory"));
    }
    let byte_count = usize::try_from(byte_count)
        .map_err(|_| format!("{role} read byte count does not fit host address space"))?;
    let mapped_ptr = native_vulkan_vulkanalia_map_memory2(
        device,
        buffer.memory,
        0,
        buffer.snapshot.memory_size,
        vk::MemoryMapFlags::empty(),
        role,
    )?;
    let invalidate_result = if buffer.snapshot.host_coherent {
        Ok(())
    } else {
        let range = vk::MappedMemoryRange::builder()
            .memory(buffer.memory)
            .offset(0)
            .size(vk::WHOLE_SIZE)
            .build();
        unsafe { device.invalidate_mapped_memory_ranges(&[range]) }
            .map_err(|err| format!("vkInvalidateMappedMemoryRanges(vulkanalia {role}): {err:?}"))
    };
    let payload = invalidate_result.as_ref().ok().map(|()| unsafe {
        std::slice::from_raw_parts(mapped_ptr.cast::<u8>(), byte_count).to_vec()
    });
    let unmap_result = native_vulkan_vulkanalia_unmap_memory2(device, buffer.memory, role);
    invalidate_result?;
    unmap_result?;
    payload.ok_or_else(|| format!("{role} readback payload was not produced"))
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_buffer: vk::CommandBuffer,
    role: &'static str,
    requested_bytes: u64,
    usage: vk::BufferUsageFlags,
    upload_payload: &[u8],
) -> Result<NativeVulkanVulkanaliaRecordedBufferUpload, String> {
    if upload_payload.is_empty() {
        let target = native_vulkan_vulkanalia_create_buffer(
            device,
            memory_properties,
            role,
            requested_bytes,
            usage | vk::BufferUsageFlags::TRANSFER_DST,
            NativeVulkanVulkanaliaBufferMemoryPreference::DeviceLocal,
            None,
        )?;
        return Ok(NativeVulkanVulkanaliaRecordedBufferUpload {
            target,
            staging: None,
            copy_recorded: false,
        });
    }

    let mut target = Some(native_vulkan_vulkanalia_create_buffer(
        device,
        memory_properties,
        role,
        requested_bytes,
        usage | vk::BufferUsageFlags::TRANSFER_DST,
        NativeVulkanVulkanaliaBufferMemoryPreference::DeviceLocal,
        None,
    )?);

    let staging = match native_vulkan_vulkanalia_create_buffer(
        device,
        memory_properties,
        "scene-recorded-staging-upload",
        upload_payload.len() as u64,
        vk::BufferUsageFlags::TRANSFER_SRC,
        NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
        Some(upload_payload),
    ) {
        Ok(staging) => staging,
        Err(err) => {
            if let Some(target) = target.take() {
                native_vulkan_vulkanalia_destroy_buffer(device, target);
            }
            return Err(err);
        }
    };

    let mut target = target
        .take()
        .ok_or_else(|| format!("vulkanalia {role} recorded upload lost retained buffer"))?;
    let result = native_vulkan_vulkanalia_record_buffer_upload_copy(
        device,
        command_buffer,
        &staging,
        &target,
        upload_payload.len() as u64,
        usage,
        role,
    );

    match result {
        Ok(()) => {
            target.snapshot.payload_uploaded = true;
            Ok(NativeVulkanVulkanaliaRecordedBufferUpload {
                target,
                staging: Some(staging),
                copy_recorded: true,
            })
        }
        Err(err) => {
            native_vulkan_vulkanalia_destroy_buffer(device, staging);
            native_vulkan_vulkanalia_destroy_buffer(device, target);
            Err(err)
        }
    }
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

fn native_vulkan_vulkanalia_record_buffer_upload_copy(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    staging: &NativeVulkanVulkanaliaBuffer,
    target: &NativeVulkanVulkanaliaBuffer,
    copy_bytes: u64,
    usage: vk::BufferUsageFlags,
    role: &'static str,
) -> Result<(), String> {
    if copy_bytes == 0 {
        return Err(format!(
            "{role} recorded staging upload requires non-zero copy size"
        ));
    }
    if copy_bytes > target.snapshot.requested_bytes {
        return Err(format!(
            "{role} recorded staging upload copy size {copy_bytes} exceeds target buffer size {}",
            target.snapshot.requested_bytes
        ));
    }
    if copy_bytes > staging.snapshot.requested_bytes {
        return Err(format!(
            "{role} recorded staging upload copy size {copy_bytes} exceeds staging buffer size {}",
            staging.snapshot.requested_bytes
        ));
    }

    let copy_region = vk::BufferCopy::builder()
        .src_offset(0)
        .dst_offset(0)
        .size(copy_bytes)
        .build();
    let barriers = [vk::BufferMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(recorded_upload_dst_stage_mask(usage))
        .dst_access_mask(recorded_upload_dst_access_mask(usage))
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(target.buffer)
        .offset(0)
        .size(copy_bytes)
        .build()];
    let dependency = vk::DependencyInfo::builder()
        .buffer_memory_barriers(&barriers)
        .build();
    unsafe {
        device.cmd_copy_buffer(
            command_buffer,
            staging.buffer,
            target.buffer,
            &[copy_region],
        );
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }
    Ok(())
}

fn recorded_upload_dst_stage_mask(usage: vk::BufferUsageFlags) -> vk::PipelineStageFlags2 {
    let mut stages = vk::PipelineStageFlags2::empty();
    if usage.contains(vk::BufferUsageFlags::VERTEX_BUFFER)
        || usage.contains(vk::BufferUsageFlags::INDEX_BUFFER)
    {
        stages |= vk::PipelineStageFlags2::VERTEX_INPUT;
    }
    if usage.contains(vk::BufferUsageFlags::UNIFORM_BUFFER)
        || usage.contains(vk::BufferUsageFlags::STORAGE_BUFFER)
    {
        stages |= vk::PipelineStageFlags2::VERTEX_SHADER
            | vk::PipelineStageFlags2::FRAGMENT_SHADER
            | vk::PipelineStageFlags2::COMPUTE_SHADER;
    }
    if usage.contains(vk::BufferUsageFlags::INDIRECT_BUFFER) {
        stages |= vk::PipelineStageFlags2::DRAW_INDIRECT;
    }
    if stages.is_empty() {
        vk::PipelineStageFlags2::ALL_COMMANDS
    } else {
        stages
    }
}

fn recorded_upload_dst_access_mask(usage: vk::BufferUsageFlags) -> vk::AccessFlags2 {
    let mut access = vk::AccessFlags2::empty();
    if usage.contains(vk::BufferUsageFlags::VERTEX_BUFFER) {
        access |= vk::AccessFlags2::VERTEX_ATTRIBUTE_READ;
    }
    if usage.contains(vk::BufferUsageFlags::INDEX_BUFFER) {
        access |= vk::AccessFlags2::INDEX_READ;
    }
    if usage.contains(vk::BufferUsageFlags::UNIFORM_BUFFER)
        || usage.contains(vk::BufferUsageFlags::STORAGE_BUFFER)
    {
        access |= vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::SHADER_WRITE;
    }
    if usage.contains(vk::BufferUsageFlags::INDIRECT_BUFFER) {
        access |= vk::AccessFlags2::INDIRECT_COMMAND_READ;
    }
    if access.is_empty() {
        vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE
    } else {
        access
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
        (
            vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS.bits(),
            "shader-device-address",
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

    #[test]
    fn recorded_upload_barrier_covers_vertex_and_index_consumers() {
        let usage = vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::INDEX_BUFFER;

        let stages = recorded_upload_dst_stage_mask(usage);
        let access = recorded_upload_dst_access_mask(usage);

        assert!(stages.contains(vk::PipelineStageFlags2::VERTEX_INPUT));
        assert!(access.contains(vk::AccessFlags2::VERTEX_ATTRIBUTE_READ));
        assert!(access.contains(vk::AccessFlags2::INDEX_READ));
    }

    #[test]
    fn recorded_upload_barrier_covers_storage_and_indirect_consumers() {
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER;

        let stages = recorded_upload_dst_stage_mask(usage);
        let access = recorded_upload_dst_access_mask(usage);

        assert!(stages.contains(vk::PipelineStageFlags2::VERTEX_SHADER));
        assert!(stages.contains(vk::PipelineStageFlags2::COMPUTE_SHADER));
        assert!(stages.contains(vk::PipelineStageFlags2::DRAW_INDIRECT));
        assert!(access.contains(vk::AccessFlags2::SHADER_READ));
        assert!(access.contains(vk::AccessFlags2::SHADER_WRITE));
        assert!(access.contains(vk::AccessFlags2::INDIRECT_COMMAND_READ));
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

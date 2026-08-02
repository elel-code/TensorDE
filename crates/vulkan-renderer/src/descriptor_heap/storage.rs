use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use crate::backend::DeviceOwner;
use crate::{
    AllocationRequirements, DescriptorHeapLimits, Error, MemoryLocation, MemoryTypeInfo,
    MemoryTypeSelector, Result,
};

use super::{DescriptorHeap, DescriptorHeapAllocator, DescriptorHeapDescriptor};

/// Cold-path inputs needed to realize one already-created descriptor-heap
/// buffer. Keeping placement inputs together makes future heap memory policies
/// explicit without turning the creation boundary into an argument list whose
/// ordering is easy to misuse.
pub(super) struct DescriptorHeapStorageRequest<'a> {
    pub(super) owner: Arc<DeviceOwner>,
    pub(super) buffer: vk::Buffer,
    pub(super) descriptor: &'a DescriptorHeapDescriptor,
    pub(super) allocator: DescriptorHeapAllocator,
    pub(super) limits: DescriptorHeapLimits,
    pub(super) memory_types: &'a [MemoryTypeInfo],
    pub(super) non_coherent_atom_size: u64,
    pub(super) memory: DescriptorHeapMemory,
}

/// Memory placement for one descriptor heap.
///
/// Both forms expose the same direct-heap ABI. `HostVisible` is useful for
/// small, cold descriptor tables; `DeviceLocal` keeps shader descriptor reads
/// in device-local memory and uses a retained upload buffer for explicit
/// transfer commands.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DescriptorHeapMemory {
    #[default]
    HostVisible,
    DeviceLocal,
}

pub(super) fn create_descriptor_heap_memory(
    request: DescriptorHeapStorageRequest<'_>,
) -> Result<DescriptorHeap> {
    let DescriptorHeapStorageRequest {
        owner,
        buffer,
        descriptor,
        allocator,
        limits,
        memory_types,
        non_coherent_atom_size,
        memory,
    } = request;
    let requirements = unsafe { owner.device.get_buffer_memory_requirements(buffer) };
    let target_location = match memory {
        DescriptorHeapMemory::HostVisible => MemoryLocation::Upload,
        DescriptorHeapMemory::DeviceLocal => MemoryLocation::Device,
    };
    let target_plan = MemoryTypeSelector::new(memory_types.iter().copied())
        .select(
            AllocationRequirements {
                size: requirements.size,
                alignment: requirements.alignment,
                memory_type_bits: requirements.memory_type_bits,
                non_coherent_atom_size,
            },
            target_location,
        )
        .map_err(|error| Error::Validation(error.to_string()))?;
    let mut flags = vk::MemoryAllocateFlagsInfo::builder()
        .flags(vk::MemoryAllocateFlags::DEVICE_ADDRESS)
        .build();
    let allocate = vk::MemoryAllocateInfo::builder()
        .allocation_size(target_plan.allocation_size)
        .memory_type_index(target_plan.memory_type_index)
        .push_next(&mut flags);
    let target_memory = unsafe { owner.device.allocate_memory(&allocate, None) }
        .map_err(|source| Error::vulkan("vkAllocateMemory(descriptor heap)", source))?;
    let result = (|| {
        let bind = vk::BindBufferMemoryInfo::builder()
            .buffer(buffer)
            .memory(target_memory)
            .memory_offset(0)
            .build();
        unsafe { owner.device.bind_buffer_memory2(&[bind]) }
            .map_err(|source| Error::vulkan("vkBindBufferMemory2(descriptor heap)", source))?;
        let address_info = vk::BufferDeviceAddressInfo::builder().buffer(buffer);
        let device_address = unsafe { owner.device.get_buffer_device_address(&address_info) };
        if device_address == 0 || device_address % allocator.alignment() != 0 {
            return Err(Error::Validation(format!(
                "descriptor heap address {device_address:#x} does not satisfy alignment {}",
                allocator.alignment()
            )));
        }
        match memory {
            DescriptorHeapMemory::HostVisible => {
                let map = vk::MemoryMapInfo::builder()
                    .memory(target_memory)
                    .offset(0)
                    .size(target_plan.allocation_size)
                    .flags(vk::MemoryMapFlags::empty());
                let mapped = unsafe { owner.device.map_memory2(&map) }
                    .map_err(|source| Error::vulkan("vkMapMemory2(descriptor heap)", source))?;
                Ok(DescriptorHeap {
                    owner: Arc::clone(&owner),
                    label: descriptor.label.clone(),
                    kind: descriptor.kind,
                    memory,
                    buffer,
                    target_memory,
                    mapped_memory: target_memory,
                    staging_buffer: None,
                    staging_memory: None,
                    mapped_address: mapped as usize,
                    mapped_size: target_plan.allocation_size,
                    device_address,
                    reserved_range_offset: allocator.reserved_range_offset(),
                    reserved_range_size: allocator.reserved_range_size(),
                    host_coherent: target_plan.host_coherent(),
                    non_coherent_atom_size: non_coherent_atom_size.max(1),
                    limits,
                    allocator,
                    write_lock: std::sync::Mutex::new(()),
                })
            }
            DescriptorHeapMemory::DeviceLocal => create_device_local_heap(
                Arc::clone(&owner),
                buffer,
                target_memory,
                descriptor,
                allocator,
                limits,
                memory_types,
                non_coherent_atom_size,
                device_address,
            ),
        }
    })();
    if result.is_err() {
        unsafe { owner.device.free_memory(target_memory, None) };
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn create_device_local_heap(
    owner: Arc<DeviceOwner>,
    buffer: vk::Buffer,
    target_memory: vk::DeviceMemory,
    descriptor: &DescriptorHeapDescriptor,
    allocator: DescriptorHeapAllocator,
    limits: DescriptorHeapLimits,
    memory_types: &[MemoryTypeInfo],
    non_coherent_atom_size: u64,
    device_address: vk::DeviceAddress,
) -> Result<DescriptorHeap> {
    let create = vk::BufferCreateInfo::builder()
        .size(allocator.heap_size())
        .usage(vk::BufferUsageFlags::TRANSFER_SRC)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let staging_buffer = unsafe { owner.device.create_buffer(&create, None) }
        .map_err(|source| Error::vulkan("vkCreateBuffer(descriptor heap staging)", source))?;
    let result = (|| {
        let requirements = unsafe { owner.device.get_buffer_memory_requirements(staging_buffer) };
        let plan = MemoryTypeSelector::new(memory_types.iter().copied())
            .select(
                AllocationRequirements {
                    size: requirements.size,
                    alignment: requirements.alignment,
                    memory_type_bits: requirements.memory_type_bits,
                    non_coherent_atom_size,
                },
                MemoryLocation::Upload,
            )
            .map_err(|error| Error::Validation(error.to_string()))?;
        let allocate = vk::MemoryAllocateInfo::builder()
            .allocation_size(plan.allocation_size)
            .memory_type_index(plan.memory_type_index);
        let staging_memory = unsafe { owner.device.allocate_memory(&allocate, None) }
            .map_err(|source| Error::vulkan("vkAllocateMemory(descriptor heap staging)", source))?;
        let staging = (|| {
            let bind = vk::BindBufferMemoryInfo::builder()
                .buffer(staging_buffer)
                .memory(staging_memory)
                .memory_offset(0)
                .build();
            unsafe { owner.device.bind_buffer_memory2(&[bind]) }.map_err(|source| {
                Error::vulkan("vkBindBufferMemory2(descriptor heap staging)", source)
            })?;
            let map = vk::MemoryMapInfo::builder()
                .memory(staging_memory)
                .offset(0)
                .size(plan.allocation_size)
                .flags(vk::MemoryMapFlags::empty());
            let mapped = unsafe { owner.device.map_memory2(&map) }
                .map_err(|source| Error::vulkan("vkMapMemory2(descriptor heap staging)", source))?;
            Ok(DescriptorHeap {
                owner: Arc::clone(&owner),
                label: descriptor.label.clone(),
                kind: descriptor.kind,
                memory: DescriptorHeapMemory::DeviceLocal,
                buffer,
                target_memory,
                mapped_memory: staging_memory,
                staging_buffer: Some(staging_buffer),
                staging_memory: Some(staging_memory),
                mapped_address: mapped as usize,
                mapped_size: plan.allocation_size,
                device_address,
                reserved_range_offset: allocator.reserved_range_offset(),
                reserved_range_size: allocator.reserved_range_size(),
                host_coherent: plan.host_coherent(),
                non_coherent_atom_size: non_coherent_atom_size.max(1),
                limits,
                allocator,
                write_lock: std::sync::Mutex::new(()),
            })
        })();
        if staging.is_err() {
            unsafe { owner.device.free_memory(staging_memory, None) };
        }
        staging
    })();
    if result.is_err() {
        unsafe { owner.device.destroy_buffer(staging_buffer, None) };
    }
    result
}

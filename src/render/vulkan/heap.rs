#![allow(unsafe_code)]

use std::{ptr, slice};

use thiserror::Error;
use vulkanalia::vk::{
    DeviceV1_0, DeviceV1_2, DeviceV1_3, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder,
    InstanceV1_0,
};
use vulkanalia::{Device, Instance, vk};

use crate::render::frame::{DescriptorHeapLayout, HeapAllocation};

/// The Vulkan resource heap is a device-address range backed by a buffer with
/// `VK_BUFFER_USAGE_DESCRIPTOR_HEAP_BIT_EXT`. Descriptor bytes are generated
/// into a persistently mapped staging buffer and copied into the device-local
/// heap during frame recording.
pub(super) struct DescriptorHeapResource {
    heap_buffer: vk::Buffer,
    heap_memory: vk::DeviceMemory,
    heap_address: vk::DeviceAddress,
    heap_size: u64,
    reserved_range: u64,
    descriptor_size: u64,
    descriptor_stride: u64,
    staging_buffer: vk::Buffer,
    staging_memory: vk::DeviceMemory,
    staging_mapping: *mut u8,
    staging_coherent: bool,
}

impl DescriptorHeapResource {
    pub(super) fn new(
        instance: &Instance,
        device: &Device,
        physical_device: vk::PhysicalDevice,
        layout: DescriptorHeapLayout,
    ) -> Result<Self, DescriptorHeapError> {
        validate_layout(layout)?;
        // Resolve every arithmetic failure before creating a Vulkan object.  A
        // fallible expression in the final struct literal would otherwise
        // return after the heap and staging resources had already been
        // allocated, leaking them on an extreme (but testable) layout.
        let descriptor_stride = align_up(layout.descriptor_size, layout.alignment)
            .ok_or(DescriptorHeapError::DescriptorRangeOverflow)?;
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };

        let heap_buffer = create_buffer(
            device,
            layout.capacity,
            vk::BufferUsageFlags::DESCRIPTOR_HEAP_EXT
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::TRANSFER_DST,
        )
        .map_err(DescriptorHeapError::CreateHeapBuffer)?;
        let heap_requirements = unsafe { device.get_buffer_memory_requirements(heap_buffer) };
        let heap_memory_type = select_memory_type(
            &memory_properties,
            heap_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::empty(),
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .ok_or(DescriptorHeapError::NoHeapMemoryType)?;
        let mut address_flags = vk::MemoryAllocateFlagsInfo::builder()
            .flags(vk::MemoryAllocateFlags::DEVICE_ADDRESS)
            .build();
        let heap_allocate = vk::MemoryAllocateInfo::builder()
            .allocation_size(heap_requirements.size)
            .memory_type_index(heap_memory_type)
            .push_next(&mut address_flags);
        let heap_memory = match unsafe { device.allocate_memory(&heap_allocate, None) } {
            Ok(memory) => memory,
            Err(source) => {
                unsafe { device.destroy_buffer(heap_buffer, None) };
                return Err(DescriptorHeapError::AllocateHeapMemory(source));
            }
        };
        if let Err(source) = unsafe { device.bind_buffer_memory(heap_buffer, heap_memory, 0) } {
            unsafe {
                device.free_memory(heap_memory, None);
                device.destroy_buffer(heap_buffer, None);
            }
            return Err(DescriptorHeapError::BindHeapMemory(source));
        }
        let address_info = vk::BufferDeviceAddressInfo::builder().buffer(heap_buffer);
        let heap_address = unsafe { device.get_buffer_device_address(&address_info) };
        if heap_address == 0 || heap_address % layout.alignment != 0 {
            unsafe {
                device.destroy_buffer(heap_buffer, None);
                device.free_memory(heap_memory, None);
            }
            return Err(DescriptorHeapError::InvalidHeapAddress {
                address: heap_address,
                alignment: layout.alignment,
            });
        }

        let staging_buffer =
            match create_buffer(device, layout.capacity, vk::BufferUsageFlags::TRANSFER_SRC) {
                Ok(buffer) => buffer,
                Err(source) => {
                    unsafe {
                        device.destroy_buffer(heap_buffer, None);
                        device.free_memory(heap_memory, None);
                    }
                    return Err(DescriptorHeapError::CreateStagingBuffer(source));
                }
            };
        let staging_requirements = unsafe { device.get_buffer_memory_requirements(staging_buffer) };
        let Some((staging_memory_type, staging_coherent)) =
            select_host_memory_type(&memory_properties, staging_requirements.memory_type_bits)
        else {
            unsafe {
                device.destroy_buffer(staging_buffer, None);
                device.destroy_buffer(heap_buffer, None);
                device.free_memory(heap_memory, None);
            }
            return Err(DescriptorHeapError::NoStagingMemoryType);
        };
        let staging_allocate = vk::MemoryAllocateInfo::builder()
            .allocation_size(staging_requirements.size)
            .memory_type_index(staging_memory_type);
        let staging_memory = match unsafe { device.allocate_memory(&staging_allocate, None) } {
            Ok(memory) => memory,
            Err(source) => {
                unsafe {
                    device.destroy_buffer(staging_buffer, None);
                    device.destroy_buffer(heap_buffer, None);
                    device.free_memory(heap_memory, None);
                }
                return Err(DescriptorHeapError::AllocateStagingMemory(source));
            }
        };
        if let Err(source) = unsafe { device.bind_buffer_memory(staging_buffer, staging_memory, 0) }
        {
            unsafe {
                device.free_memory(staging_memory, None);
                device.destroy_buffer(staging_buffer, None);
                device.destroy_buffer(heap_buffer, None);
                device.free_memory(heap_memory, None);
            }
            return Err(DescriptorHeapError::BindStagingMemory(source));
        }
        let staging_mapping = match unsafe {
            device.map_memory(
                staging_memory,
                0,
                vk::WHOLE_SIZE,
                vk::MemoryMapFlags::empty(),
            )
        } {
            Ok(mapping) => mapping.cast::<u8>(),
            Err(source) => {
                unsafe {
                    device.destroy_buffer(staging_buffer, None);
                    device.free_memory(staging_memory, None);
                    device.destroy_buffer(heap_buffer, None);
                    device.free_memory(heap_memory, None);
                }
                return Err(DescriptorHeapError::MapStagingMemory(source));
            }
        };

        Ok(Self {
            heap_buffer,
            heap_memory,
            heap_address,
            heap_size: layout.capacity,
            reserved_range: layout.reserved_range,
            descriptor_size: layout.descriptor_size,
            descriptor_stride,
            staging_buffer,
            staging_memory,
            staging_mapping,
            staging_coherent,
        })
    }

    pub(super) fn prepare_image_descriptors(
        &self,
        device: &Device,
        allocation: HeapAllocation,
        view_infos: &[vk::ImageViewCreateInfo],
    ) -> Result<(), DescriptorHeapError> {
        if view_infos.is_empty() {
            return Err(DescriptorHeapError::NoImageDescriptors);
        }
        let offset = usize::try_from(allocation.offset)
            .map_err(|_| DescriptorHeapError::DescriptorRangeOverflow)?;
        let size = usize::try_from(allocation.size)
            .map_err(|_| DescriptorHeapError::DescriptorRangeOverflow)?;
        let descriptor_size = usize::try_from(self.descriptor_size)
            .map_err(|_| DescriptorHeapError::DescriptorRangeOverflow)?;
        let descriptor_count = u64::try_from(view_infos.len())
            .map_err(|_| DescriptorHeapError::DescriptorRangeOverflow)?;
        let required_size = self
            .descriptor_stride
            .checked_mul(descriptor_count)
            .ok_or(DescriptorHeapError::DescriptorRangeOverflow)?;
        if descriptor_size == 0
            || descriptor_size > size
            || required_size > allocation.size
            || allocation.offset > self.heap_size
            || allocation.size > self.heap_size.saturating_sub(allocation.offset)
        {
            return Err(DescriptorHeapError::DescriptorRangeInvalid {
                offset: allocation.offset,
                size: allocation.size,
                capacity: self.heap_size,
            });
        }

        // Clear the complete allocation so every descriptor slot has defined bytes even
        // before scene resources are added to the frame graph.
        unsafe {
            ptr::write_bytes(self.staging_mapping.add(offset), 0, size);
        }
        let images = view_infos
            .iter()
            .map(|view_info| {
                vk::ImageDescriptorInfoEXT::builder()
                    .view(view_info)
                    .layout(vk::ImageLayout::GENERAL)
                    .build()
            })
            .collect::<Vec<_>>();
        let resources = images
            .iter()
            .map(|image| {
                vk::ResourceDescriptorInfoEXT::builder()
                    .type_(vk::DescriptorType::SAMPLED_IMAGE)
                    .data(vk::ResourceDescriptorDataEXT {
                        image: ptr::from_ref(image),
                    })
                    .build()
            })
            .collect::<Vec<_>>();
        let destinations = (0..view_infos.len())
            .map(|index| {
                let byte_offset = self
                    .descriptor_stride
                    .checked_mul(u64::try_from(index).unwrap_or(u64::MAX))
                    .and_then(|value| value.checked_add(allocation.offset))
                    .ok_or(DescriptorHeapError::DescriptorRangeOverflow)?;
                let byte_offset = usize::try_from(byte_offset)
                    .map_err(|_| DescriptorHeapError::DescriptorRangeOverflow)?;
                Ok(vk::HostAddressRangeEXT {
                    address: unsafe { self.staging_mapping.add(byte_offset).cast() },
                    size: descriptor_size,
                })
            })
            .collect::<Result<Vec<_>, DescriptorHeapError>>()?;
        unsafe {
            device
                .write_resource_descriptors_ext(&resources, &destinations)
                .map_err(DescriptorHeapError::WriteDescriptor)?;
        }
        if !self.staging_coherent {
            let range = vk::MappedMemoryRange::builder()
                .memory(self.staging_memory)
                .offset(0)
                .size(vk::WHOLE_SIZE)
                .build();
            unsafe {
                device
                    .flush_mapped_memory_ranges(slice::from_ref(&range))
                    .map_err(DescriptorHeapError::FlushStaging)?;
            }
        }
        Ok(())
    }

    pub(super) unsafe fn record_copy_and_bind(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        allocation: HeapAllocation,
    ) {
        let heap_range = vk::DeviceAddressRangeEXT::builder()
            .address(self.heap_address)
            .size(self.heap_size)
            .build();
        let bind_info = vk::BindHeapInfoEXT::builder()
            .heap_range(heap_range)
            .reserved_range_offset(0)
            .reserved_range_size(self.reserved_range)
            .build();
        unsafe { device.cmd_bind_resource_heap_ext(command_buffer, &bind_info) };

        let host_barrier = vk::MemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::HOST)
            .src_access_mask(vk::AccessFlags2::HOST_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .build();
        let host_dependency = vk::DependencyInfo::builder()
            .memory_barriers(slice::from_ref(&host_barrier))
            .build();
        unsafe { device.cmd_pipeline_barrier2(command_buffer, &host_dependency) };

        let copy = vk::BufferCopy::builder()
            .src_offset(allocation.offset)
            .dst_offset(allocation.offset)
            .size(allocation.size)
            .build();
        unsafe {
            device.cmd_copy_buffer(
                command_buffer,
                self.staging_buffer,
                self.heap_buffer,
                slice::from_ref(&copy),
            );
        }

        let heap_barrier = vk::MemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_access_mask(
                vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::RESOURCE_HEAP_READ_EXT,
            )
            .build();
        let heap_dependency = vk::DependencyInfo::builder()
            .memory_barriers(slice::from_ref(&heap_barrier))
            .build();
        unsafe { device.cmd_pipeline_barrier2(command_buffer, &heap_dependency) };
    }

    pub(super) unsafe fn destroy(&mut self, device: &Device) {
        unsafe {
            device.unmap_memory(self.staging_memory);
            device.destroy_buffer(self.staging_buffer, None);
            device.free_memory(self.staging_memory, None);
            device.destroy_buffer(self.heap_buffer, None);
            device.free_memory(self.heap_memory, None);
        }
    }
}

fn validate_layout(layout: DescriptorHeapLayout) -> Result<(), DescriptorHeapError> {
    if layout.capacity == 0
        || layout.capacity > vk::WHOLE_SIZE - 1
        || !layout.alignment.is_power_of_two()
        || layout.reserved_range >= layout.capacity
        || layout.descriptor_size == 0
    {
        return Err(DescriptorHeapError::InvalidLayout(layout));
    }
    Ok(())
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let remainder = value % alignment;
    value.checked_add((alignment - remainder) % alignment)
}

fn create_buffer(
    device: &Device,
    size: u64,
    usage: vk::BufferUsageFlags,
) -> Result<vk::Buffer, vk::ErrorCode> {
    let info = vk::BufferCreateInfo::builder()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    unsafe { device.create_buffer(&info, None) }
}

fn select_memory_type(
    properties: &vk::PhysicalDeviceMemoryProperties,
    compatible_bits: u32,
    required: vk::MemoryPropertyFlags,
    preferred: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let count = usize::try_from(properties.memory_type_count).ok()?;
    let compatible = properties.memory_types[..count]
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            u32::try_from(*index)
                .ok()
                .and_then(|index| 1u32.checked_shl(index))
                .is_some_and(|bit| compatible_bits & bit != 0)
        })
        .filter(|(_, memory)| memory.property_flags.contains(required));
    compatible
        .clone()
        .find(|(_, memory)| memory.property_flags.contains(preferred))
        .or_else(|| compatible.into_iter().next())
        .and_then(|(index, _)| u32::try_from(index).ok())
}

fn select_host_memory_type(
    properties: &vk::PhysicalDeviceMemoryProperties,
    compatible_bits: u32,
) -> Option<(u32, bool)> {
    let count = usize::try_from(properties.memory_type_count).ok()?;
    let compatible =
        properties.memory_types[..count]
            .iter()
            .enumerate()
            .filter(|(index, memory)| {
                let bit = u32::try_from(*index)
                    .ok()
                    .and_then(|index| 1u32.checked_shl(index));
                bit.is_some_and(|bit| compatible_bits & bit != 0)
                    && memory
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::HOST_VISIBLE)
            });
    compatible
        .clone()
        .find(|(_, memory)| {
            memory
                .property_flags
                .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
        })
        .or_else(|| compatible.into_iter().next())
        .map(|(index, memory)| {
            (
                u32::try_from(index).expect("Vulkan memory type count fits in u32"),
                memory
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_COHERENT),
            )
        })
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(super) enum DescriptorHeapError {
    #[error("descriptor heap layout is invalid: {0:?}")]
    InvalidLayout(DescriptorHeapLayout),
    #[error("failed to create the descriptor heap buffer: {0:?}")]
    CreateHeapBuffer(vk::ErrorCode),
    #[error("no compatible memory type exists for the descriptor heap")]
    NoHeapMemoryType,
    #[error("failed to allocate descriptor heap memory: {0:?}")]
    AllocateHeapMemory(vk::ErrorCode),
    #[error("failed to bind descriptor heap memory: {0:?}")]
    BindHeapMemory(vk::ErrorCode),
    #[error(
        "descriptor heap buffer returned an invalid device address {address:#x} (alignment {alignment})"
    )]
    InvalidHeapAddress { address: u64, alignment: u64 },
    #[error("failed to create the descriptor staging buffer: {0:?}")]
    CreateStagingBuffer(vk::ErrorCode),
    #[error("no host-visible memory type exists for descriptor staging")]
    NoStagingMemoryType,
    #[error("failed to allocate descriptor staging memory: {0:?}")]
    AllocateStagingMemory(vk::ErrorCode),
    #[error("failed to bind descriptor staging memory: {0:?}")]
    BindStagingMemory(vk::ErrorCode),
    #[error("failed to map descriptor staging memory: {0:?}")]
    MapStagingMemory(vk::ErrorCode),
    #[error("descriptor allocation range overflows the host address space")]
    DescriptorRangeOverflow,
    #[error(
        "descriptor allocation range exceeds heap capacity {capacity} (offset {offset}, size {size})"
    )]
    DescriptorRangeInvalid {
        offset: u64,
        size: u64,
        capacity: u64,
    },
    #[error("failed to encode an image descriptor: {0:?}")]
    WriteDescriptor(vk::ErrorCode),
    #[error("at least one image descriptor is required for a frame")]
    NoImageDescriptors,
    #[error("failed to flush descriptor staging memory: {0:?}")]
    FlushStaging(vk::ErrorCode),
}

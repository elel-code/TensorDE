use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use vulkanalia::{prelude::v1_4::*, vk};

use crate::backend::DeviceOwner;
use crate::{
    AllocationRequirements, Backend, DescriptorHeapLimits, Error, Features, FrameToken,
    MemoryLocation, MemoryTypeSelector, Result as BackendResult,
};

mod write;

/// Byte range inside a descriptor heap buffer.
#[derive(Debug, Eq, PartialEq)]
pub struct DescriptorAllocation {
    range: Range<u64>,
    allocator_id: u64,
}

impl DescriptorAllocation {
    pub const fn offset(&self) -> u64 {
        self.range.start
    }

    pub const fn size(&self) -> u64 {
        self.range.end - self.range.start
    }

    pub const fn range(&self) -> &Range<u64> {
        &self.range
    }
}

/// Bounded first-fit allocator for one sampler or resource descriptor heap.
///
/// The implementation-reserved suffix is never returned. The resulting
/// `reserved_range_offset` and `reserved_range_size` are passed directly to
/// `VkBindHeapInfoEXT`. Freed descriptor ranges enter a retirement list and
/// become reusable only after the renderer timeline completes the frame.
#[derive(Debug)]
pub struct DescriptorHeapAllocator {
    id: u64,
    heap_size: u64,
    alignment: u64,
    reserved_range_offset: u64,
    reserved_range_size: u64,
    free: Vec<Range<u64>>,
    retired: Vec<(u64, Range<u64>)>,
}

impl DescriptorHeapAllocator {
    pub fn new(
        heap_size: u64,
        minimum_reserved_range: u64,
        alignment: u64,
    ) -> Result<Self, DescriptorHeapError> {
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(DescriptorHeapError::InvalidAlignment(alignment));
        }
        let id = next_allocator_id()?;
        let reserved_range_size = align_up(minimum_reserved_range, alignment)
            .ok_or(DescriptorHeapError::RangeOverflow)?;
        if reserved_range_size >= heap_size {
            return Err(DescriptorHeapError::ReservedRangeConsumesHeap {
                heap_size,
                minimum_reserved_range,
                alignment,
            });
        }
        let reserved_range_offset = align_down(heap_size - reserved_range_size, alignment);
        if reserved_range_offset == 0 {
            return Err(DescriptorHeapError::ReservedRangeConsumesHeap {
                heap_size,
                minimum_reserved_range,
                alignment,
            });
        }
        Ok(Self {
            id,
            heap_size,
            alignment,
            reserved_range_offset,
            reserved_range_size: heap_size - reserved_range_offset,
            free: std::iter::once(0..reserved_range_offset).collect(),
            retired: Vec::new(),
        })
    }

    pub const fn heap_size(&self) -> u64 {
        self.heap_size
    }

    pub const fn reserved_range_offset(&self) -> u64 {
        self.reserved_range_offset
    }

    pub const fn reserved_range_size(&self) -> u64 {
        self.reserved_range_size
    }

    pub const fn alignment(&self) -> u64 {
        self.alignment
    }

    pub fn allocate(
        &mut self,
        size: u64,
        descriptor_alignment: u64,
    ) -> Result<DescriptorAllocation, DescriptorHeapError> {
        if size == 0 {
            return Err(DescriptorHeapError::ZeroSize);
        }
        if descriptor_alignment == 0 || !descriptor_alignment.is_power_of_two() {
            return Err(DescriptorHeapError::InvalidAlignment(descriptor_alignment));
        }
        let alignment = self.alignment.max(descriptor_alignment);
        for index in 0..self.free.len() {
            let free = self.free[index].clone();
            let Some(start) = align_up(free.start, alignment) else {
                continue;
            };
            let Some(end) = start.checked_add(size) else {
                continue;
            };
            if end > free.end {
                continue;
            }
            self.free.remove(index);
            if free.start < start {
                self.free.push(free.start..start);
            }
            if end < free.end {
                self.free.push(end..free.end);
            }
            self.normalize_free_ranges();
            return Ok(DescriptorAllocation {
                range: start..end,
                allocator_id: self.id,
            });
        }
        Err(DescriptorHeapError::OutOfMemory {
            requested: size,
            available: self.available_bytes(),
        })
    }

    pub fn retire(
        &mut self,
        allocation: DescriptorAllocation,
        after: FrameToken,
    ) -> Result<(), DescriptorHeapError> {
        if !self.owns(&allocation) {
            return Err(DescriptorHeapError::WrongAllocator);
        }
        debug_assert!(allocation.range.end <= self.reserved_range_offset);
        self.retired.push((after.value(), allocation.range));
        Ok(())
    }

    pub fn reclaim(&mut self, completed_timeline: u64) -> usize {
        let mut pending = Vec::with_capacity(self.retired.len());
        let mut reclaimed = 0;
        for (timeline, range) in self.retired.drain(..) {
            if timeline <= completed_timeline {
                self.free.push(range);
                reclaimed += 1;
            } else {
                pending.push((timeline, range));
            }
        }
        self.retired = pending;
        if reclaimed > 0 {
            self.normalize_free_ranges();
        }
        reclaimed
    }

    pub fn available_bytes(&self) -> u64 {
        self.free.iter().map(|range| range.end - range.start).sum()
    }

    pub fn pending_retirements(&self) -> usize {
        self.retired.len()
    }

    const fn owns(&self, allocation: &DescriptorAllocation) -> bool {
        allocation.allocator_id == self.id
    }

    fn normalize_free_ranges(&mut self) {
        self.free.sort_by_key(|range| range.start);
        let mut merged = Vec::<Range<u64>>::with_capacity(self.free.len());
        for range in self.free.drain(..) {
            if let Some(previous) = merged.last_mut()
                && range.start <= previous.end
            {
                previous.end = previous.end.max(range.end);
                continue;
            }
            merged.push(range);
        }
        self.free = merged;
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let mask = alignment.checked_sub(1)?;
    value.checked_add(mask).map(|value| value & !mask)
}

const fn align_down(value: u64, alignment: u64) -> u64 {
    value & !(alignment - 1)
}

static NEXT_ALLOCATOR_ID: AtomicU64 = AtomicU64::new(1);

fn next_allocator_id() -> Result<u64, DescriptorHeapError> {
    NEXT_ALLOCATOR_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| DescriptorHeapError::RangeOverflow)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorHeapError {
    InvalidAlignment(u64),
    ZeroSize,
    RangeOverflow,
    WrongHeapKind,
    WrongAllocator,
    ReservedRangeConsumesHeap {
        heap_size: u64,
        minimum_reserved_range: u64,
        alignment: u64,
    },
    OutOfMemory {
        requested: u64,
        available: u64,
    },
}

impl fmt::Display for DescriptorHeapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for DescriptorHeapError {}

/// Selects which of the two `VK_EXT_descriptor_heap` binding points a heap
/// occupies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorHeapKind {
    Resource,
    Sampler,
}

/// A host-written, GPU-addressable descriptor heap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptorHeapDescriptor {
    pub label: Option<String>,
    pub kind: DescriptorHeapKind,
    /// Bytes available for application descriptors, excluding the aligned
    /// implementation-reserved range appended by the backend.
    pub descriptor_capacity: u64,
    /// Uses the larger sampler reserved range required by embedded samplers.
    pub embedded_samplers: bool,
}

/// Descriptor representation used to size, align, and validate allocations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeapDescriptorType {
    SampledImage,
    StorageImage,
    InputAttachment,
    UniformBuffer,
    StorageBuffer,
    Sampler,
}

/// Retained Vulkan descriptor-heap buffer and its persistently mapped memory.
pub struct DescriptorHeap {
    owner: Arc<DeviceOwner>,
    label: Option<String>,
    kind: DescriptorHeapKind,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped_address: usize,
    mapped_size: u64,
    device_address: vk::DeviceAddress,
    reserved_range_offset: u64,
    reserved_range_size: u64,
    host_coherent: bool,
    non_coherent_atom_size: u64,
    limits: DescriptorHeapLimits,
    allocator: Mutex<DescriptorHeapAllocator>,
    write_lock: Mutex<()>,
}

impl fmt::Debug for DescriptorHeap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DescriptorHeap")
            .field("label", &self.label)
            .field("kind", &self.kind)
            .field("buffer", &self.buffer)
            .field("mapped_size", &self.mapped_size)
            .field("device_address", &self.device_address)
            .field("reserved_range_offset", &self.reserved_range_offset)
            .field("reserved_range_size", &self.reserved_range_size)
            .field("host_coherent", &self.host_coherent)
            .finish_non_exhaustive()
    }
}

impl Backend {
    /// Creates the sole standard shader-resource binding storage model.
    pub fn create_descriptor_heap(
        &self,
        descriptor: &DescriptorHeapDescriptor,
    ) -> BackendResult<DescriptorHeap> {
        if !self.features().contains(Features::DESCRIPTOR_HEAP) {
            return Err(Error::Validation(
                "Device did not enable Features::DESCRIPTOR_HEAP".into(),
            ));
        }
        if descriptor.descriptor_capacity == 0 {
            return Err(Error::Validation(
                "descriptor heap capacity must be non-zero".into(),
            ));
        }
        if descriptor.kind == DescriptorHeapKind::Resource && descriptor.embedded_samplers {
            return Err(Error::Validation(
                "embedded_samplers is valid only for sampler heaps".into(),
            ));
        }

        let limits = self.device_info().limits.descriptor_heap;
        let (alignment, maximum, minimum_reserved) = match descriptor.kind {
            DescriptorHeapKind::Resource => (
                limits.resource_heap_alignment,
                limits.max_resource_heap_size,
                limits.min_resource_heap_reserved_range,
            ),
            DescriptorHeapKind::Sampler => (
                limits.sampler_heap_alignment,
                limits.max_sampler_heap_size,
                if descriptor.embedded_samplers {
                    limits.min_sampler_heap_reserved_range_with_embedded
                } else {
                    limits.min_sampler_heap_reserved_range
                },
            ),
        };
        let payload_size = align_up(descriptor.descriptor_capacity, alignment)
            .ok_or_else(|| Error::Validation("descriptor heap payload size overflows".into()))?;
        let reserved_size = align_up(minimum_reserved, alignment)
            .ok_or_else(|| Error::Validation("descriptor heap reserved size overflows".into()))?;
        let heap_size = payload_size
            .checked_add(reserved_size)
            .ok_or_else(|| Error::Validation("descriptor heap total size overflows".into()))?;
        if maximum != 0 && heap_size > maximum {
            return Err(Error::Validation(format!(
                "descriptor heap size {heap_size} exceeds device maximum {maximum}"
            )));
        }
        let allocator = DescriptorHeapAllocator::new(heap_size, reserved_size, alignment)
            .map_err(|error| Error::Validation(error.to_string()))?;

        let owner = self.shared_owner();
        let usage =
            vk::BufferUsageFlags::DESCRIPTOR_HEAP_EXT | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        let create = vk::BufferCreateInfo::builder()
            .size(heap_size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { owner.device.create_buffer(&create, None) }
            .map_err(|source| Error::vulkan("vkCreateBuffer(descriptor heap)", source))?;
        match create_descriptor_heap_memory(
            Arc::clone(&owner),
            buffer,
            descriptor,
            allocator,
            limits,
            self.device_info().memory_types.iter().copied(),
            self.device_info().non_coherent_atom_size,
        ) {
            Ok(heap) => Ok(heap),
            Err(error) => {
                unsafe { owner.device.destroy_buffer(buffer, None) };
                Err(error)
            }
        }
    }
}

fn create_descriptor_heap_memory(
    owner: Arc<DeviceOwner>,
    buffer: vk::Buffer,
    descriptor: &DescriptorHeapDescriptor,
    allocator: DescriptorHeapAllocator,
    limits: DescriptorHeapLimits,
    memory_types: impl IntoIterator<Item = crate::MemoryTypeInfo>,
    non_coherent_atom_size: u64,
) -> BackendResult<DescriptorHeap> {
    let requirements = unsafe { owner.device.get_buffer_memory_requirements(buffer) };
    let plan = MemoryTypeSelector::new(memory_types)
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
    let mut flags = vk::MemoryAllocateFlagsInfo::builder()
        .flags(vk::MemoryAllocateFlags::DEVICE_ADDRESS)
        .build();
    let allocate = vk::MemoryAllocateInfo::builder()
        .allocation_size(plan.allocation_size)
        .memory_type_index(plan.memory_type_index)
        .push_next(&mut flags);
    let memory = unsafe { owner.device.allocate_memory(&allocate, None) }
        .map_err(|source| Error::vulkan("vkAllocateMemory(descriptor heap)", source))?;
    let result = (|| {
        let bind = vk::BindBufferMemoryInfo::builder()
            .buffer(buffer)
            .memory(memory)
            .memory_offset(0)
            .build();
        unsafe { owner.device.bind_buffer_memory2(&[bind]) }
            .map_err(|source| Error::vulkan("vkBindBufferMemory2(descriptor heap)", source))?;
        let map = vk::MemoryMapInfo::builder()
            .memory(memory)
            .offset(0)
            .size(plan.allocation_size)
            .flags(vk::MemoryMapFlags::empty());
        let mapped = unsafe { owner.device.map_memory2(&map) }
            .map_err(|source| Error::vulkan("vkMapMemory2(descriptor heap)", source))?;
        let address_info = vk::BufferDeviceAddressInfo::builder().buffer(buffer);
        let device_address = unsafe { owner.device.get_buffer_device_address(&address_info) };
        if device_address == 0 || device_address % allocator.alignment() != 0 {
            let unmap = vk::MemoryUnmapInfo::builder().memory(memory);
            let _ = unsafe { owner.device.unmap_memory2(&unmap) };
            return Err(Error::Validation(format!(
                "descriptor heap address {device_address:#x} does not satisfy alignment {}",
                allocator.alignment()
            )));
        }
        Ok(DescriptorHeap {
            owner: Arc::clone(&owner),
            label: descriptor.label.clone(),
            kind: descriptor.kind,
            buffer,
            memory,
            mapped_address: mapped as usize,
            mapped_size: plan.allocation_size,
            device_address,
            reserved_range_offset: allocator.reserved_range_offset(),
            reserved_range_size: allocator.reserved_range_size(),
            host_coherent: plan.host_coherent(),
            non_coherent_atom_size: non_coherent_atom_size.max(1),
            limits,
            allocator: Mutex::new(allocator),
            write_lock: Mutex::new(()),
        })
    })();
    if result.is_err() {
        unsafe { owner.device.free_memory(memory, None) };
    }
    result
}

impl DescriptorHeap {
    pub const fn kind(&self) -> DescriptorHeapKind {
        self.kind
    }

    pub const fn raw_buffer(&self) -> vk::Buffer {
        self.buffer
    }

    pub const fn device_address(&self) -> vk::DeviceAddress {
        self.device_address
    }

    pub const fn reserved_range_offset(&self) -> u64 {
        self.reserved_range_offset
    }

    pub const fn reserved_range_size(&self) -> u64 {
        self.reserved_range_size
    }

    pub fn allocate(
        &self,
        descriptor_type: HeapDescriptorType,
    ) -> std::result::Result<DescriptorAllocation, DescriptorHeapError> {
        let (size, alignment, expected_heap) = self.descriptor_layout(descriptor_type);
        if self.kind != expected_heap {
            return Err(DescriptorHeapError::WrongHeapKind);
        }
        self.allocator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .allocate(size, alignment)
    }

    pub fn retire(
        &self,
        allocation: DescriptorAllocation,
        after: FrameToken,
    ) -> std::result::Result<(), DescriptorHeapError> {
        self.allocator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retire(allocation, after)
    }

    pub fn reclaim(&self, completed_timeline: u64) -> usize {
        self.allocator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .reclaim(completed_timeline)
    }

    pub fn bind_info(&self) -> vk::BindHeapInfoEXT {
        vk::BindHeapInfoEXT::builder()
            .heap_range(
                vk::DeviceAddressRangeEXT::builder()
                    .address(self.device_address)
                    .size(self.reserved_range_offset + self.reserved_range_size)
                    .build(),
            )
            .reserved_range_offset(self.reserved_range_offset)
            .reserved_range_size(self.reserved_range_size)
            .build()
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }

    fn descriptor_layout(
        &self,
        descriptor_type: HeapDescriptorType,
    ) -> (u64, u64, DescriptorHeapKind) {
        let limits = self.limits;
        match descriptor_type {
            HeapDescriptorType::SampledImage
            | HeapDescriptorType::StorageImage
            | HeapDescriptorType::InputAttachment => (
                limits.image_descriptor_size,
                limits.image_descriptor_alignment,
                DescriptorHeapKind::Resource,
            ),
            HeapDescriptorType::UniformBuffer | HeapDescriptorType::StorageBuffer => (
                limits.buffer_descriptor_size,
                limits.buffer_descriptor_alignment,
                DescriptorHeapKind::Resource,
            ),
            HeapDescriptorType::Sampler => (
                limits.sampler_descriptor_size,
                limits.sampler_descriptor_alignment,
                DescriptorHeapKind::Sampler,
            ),
        }
    }
}

impl Drop for DescriptorHeap {
    fn drop(&mut self) {
        let unmap = vk::MemoryUnmapInfo::builder().memory(self.memory);
        unsafe {
            let _ = self.owner.device.unmap_memory2(&unmap);
            self.owner.device.destroy_buffer(self.buffer, None);
            self.owner.device.free_memory(self.memory, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::FrameClock;

    use super::*;

    #[test]
    fn reserved_suffix_and_descriptor_alignment_are_never_violated() {
        let mut heap = DescriptorHeapAllocator::new(1024, 65, 64).unwrap();
        let allocation = heap.allocate(32, 128).unwrap();
        assert_eq!(heap.reserved_range_offset(), 896);
        assert_eq!(heap.reserved_range_size(), 128);
        assert_eq!(allocation.offset(), 0);
        assert_eq!(allocation.offset() % 128, 0);
    }

    #[test]
    fn range_is_reused_only_after_timeline_completion() {
        let mut clock = FrameClock::default();
        let frame = clock.allocate().unwrap();
        let mut heap = DescriptorHeapAllocator::new(256, 64, 64).unwrap();
        let allocation = heap.allocate(192, 64).unwrap();
        heap.retire(allocation, frame).unwrap();
        assert!(matches!(
            heap.allocate(64, 64),
            Err(DescriptorHeapError::OutOfMemory { .. })
        ));
        assert_eq!(heap.reclaim(frame.value() - 1), 0);
        assert_eq!(heap.reclaim(frame.value()), 1);
        assert_eq!(heap.allocate(192, 64).unwrap().range(), &(0..192));
    }

    #[test]
    fn fragmented_ranges_merge_after_retirement() {
        let mut clock = FrameClock::default();
        let frame = clock.allocate().unwrap();
        let mut heap = DescriptorHeapAllocator::new(512, 64, 64).unwrap();
        let first = heap.allocate(64, 64).unwrap();
        let second = heap.allocate(64, 64).unwrap();
        heap.retire(first, frame).unwrap();
        heap.retire(second, frame).unwrap();
        heap.reclaim(frame.value());
        assert_eq!(heap.allocate(128, 64).unwrap().range(), &(0..128));
    }

    #[test]
    fn allocation_cannot_be_retired_by_an_unrelated_allocator() {
        let frame = FrameClock::default().allocate().unwrap();
        let mut first = DescriptorHeapAllocator::new(256, 64, 64).unwrap();
        let mut second = DescriptorHeapAllocator::new(256, 64, 64).unwrap();
        let allocation = first.allocate(64, 64).unwrap();
        assert_eq!(
            second.retire(allocation, frame),
            Err(DescriptorHeapError::WrongAllocator)
        );
    }
}

use std::fmt;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use vulkanalia::{prelude::v1_4::*, vk};

use crate::backend::DeviceOwner;
use crate::{
    AllocationRequirements, Backend, BufferDescriptor, Error, MemoryLocation, MemoryPlan,
    MemoryTypeInfo, MemoryTypeSelector, Result,
};

mod image;

pub use image::{Image, ImageView, ImageViewDescriptor};

/// Block sizes and the cutoff for isolated large allocations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryAllocatorConfig {
    pub device_block_size: u64,
    pub image_block_size: u64,
    pub upload_block_size: u64,
    pub readback_block_size: u64,
    pub dedicated_threshold: u64,
}

impl Default for MemoryAllocatorConfig {
    fn default() -> Self {
        Self {
            device_block_size: 64 * 1024 * 1024,
            image_block_size: 128 * 1024 * 1024,
            upload_block_size: 16 * 1024 * 1024,
            readback_block_size: 16 * 1024 * 1024,
            dedicated_threshold: 32 * 1024 * 1024,
        }
    }
}

/// Reusable device-memory block allocator. Buffers of the same memory type and
/// access class share allocations; large buffers receive an isolated block.
#[derive(Clone)]
pub struct MemoryAllocator {
    owner: Arc<DeviceOwner>,
    memory_types: Arc<[MemoryTypeInfo]>,
    non_coherent_atom_size: u64,
    config: MemoryAllocatorConfig,
    blocks: Arc<Mutex<Vec<Arc<MemoryBlock>>>>,
}

impl fmt::Debug for MemoryAllocator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MemoryAllocator")
            .field("config", &self.config)
            .field(
                "block_count",
                &self
                    .blocks
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .len(),
            )
            .finish_non_exhaustive()
    }
}

impl Backend {
    pub fn create_memory_allocator(
        &self,
        config: MemoryAllocatorConfig,
    ) -> Result<MemoryAllocator> {
        if config.device_block_size == 0
            || config.image_block_size == 0
            || config.upload_block_size == 0
            || config.readback_block_size == 0
            || config.dedicated_threshold == 0
        {
            return Err(Error::Validation(
                "memory allocator block sizes and dedicated threshold must be non-zero".into(),
            ));
        }
        Ok(MemoryAllocator {
            owner: self.shared_owner(),
            memory_types: self.device_info().memory_types.clone().into(),
            non_coherent_atom_size: self.device_info().non_coherent_atom_size.max(1),
            config,
            blocks: Arc::new(Mutex::new(Vec::new())),
        })
    }
}

impl MemoryAllocator {
    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }

    pub fn create_buffer(&self, descriptor: &BufferDescriptor) -> Result<Buffer> {
        descriptor
            .validate()
            .map_err(|error| Error::Validation(error.to_string()))?;
        let create = vk::BufferCreateInfo::builder()
            .size(descriptor.size)
            .usage(descriptor.usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { self.owner.device.create_buffer(&create, None) }
            .map_err(|source| Error::vulkan("vkCreateBuffer", source))?;
        match self.bind_buffer(buffer, descriptor) {
            Ok(buffer) => Ok(buffer),
            Err(error) => {
                unsafe { self.owner.device.destroy_buffer(buffer, None) };
                Err(error)
            }
        }
    }

    fn bind_buffer(&self, buffer: vk::Buffer, descriptor: &BufferDescriptor) -> Result<Buffer> {
        let (requirements, dedicated_requirements) =
            buffer_memory_requirements(&self.owner, buffer);
        let selection = MemoryTypeSelector::new(self.memory_types.iter().copied())
            .select(
                AllocationRequirements {
                    size: requirements.size,
                    alignment: requirements.alignment,
                    memory_type_bits: requirements.memory_type_bits,
                    non_coherent_atom_size: self.non_coherent_atom_size,
                },
                descriptor.memory,
            )
            .map_err(|error| Error::Validation(error.to_string()))?;
        let dedicated = requirements.size >= self.config.dedicated_threshold
            || dedicated_requirements.prefers_dedicated_allocation != 0
            || dedicated_requirements.requires_dedicated_allocation != 0;
        let mut blocks = self
            .blocks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !dedicated {
            for block in blocks.iter() {
                if block.compatible(
                    MemoryClass::Buffer,
                    descriptor.memory,
                    selection.memory_type_index,
                ) && let Some(range) = block.allocate(requirements.size, requirements.alignment)
                {
                    return self.finish_buffer(buffer, descriptor, Arc::clone(block), range);
                }
            }
        }

        let configured = match descriptor.memory {
            MemoryLocation::Device => self.config.device_block_size,
            MemoryLocation::Upload => self.config.upload_block_size,
            MemoryLocation::Readback => self.config.readback_block_size,
        };
        let desired_block_size = if dedicated {
            align_up(requirements.size, requirements.alignment)
        } else {
            align_up(configured.max(requirements.size), requirements.alignment)
        }
        .ok_or_else(|| Error::Validation("memory block size overflows".into()))?;
        let heap_size = self
            .memory_types
            .iter()
            .find(|memory| memory.index == selection.memory_type_index)
            .map(|memory| memory.heap_size)
            .ok_or_else(|| Error::Validation("selected memory type disappeared".into()))?;
        let block_size = if desired_block_size <= heap_size {
            desired_block_size
        } else {
            align_up(requirements.size, requirements.alignment).ok_or_else(|| {
                Error::Validation("minimum Vulkan memory allocation size overflows".into())
            })?
        };
        let block = Arc::new(MemoryBlock::new(
            Arc::clone(&self.owner),
            MemoryClass::Buffer,
            descriptor.memory,
            selection,
            block_size,
            dedicated,
            dedicated.then_some(DedicatedResource::Buffer(buffer)),
        )?);
        let range = block
            .allocate(requirements.size, requirements.alignment)
            .ok_or_else(|| {
                Error::Validation("new memory block cannot satisfy allocation".into())
            })?;
        blocks.push(Arc::clone(&block));
        self.finish_buffer(buffer, descriptor, block, range)
    }

    fn finish_buffer(
        &self,
        buffer: vk::Buffer,
        descriptor: &BufferDescriptor,
        block: Arc<MemoryBlock>,
        range: Range<u64>,
    ) -> Result<Buffer> {
        let bind = vk::BindBufferMemoryInfo::builder()
            .buffer(buffer)
            .memory(block.memory)
            .memory_offset(range.start)
            .build();
        if let Err(source) = unsafe { self.owner.device.bind_buffer_memory2(&[bind]) } {
            block.release(range);
            return Err(Error::vulkan("vkBindBufferMemory2", source));
        }
        let device_address = if descriptor
            .usage
            .contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS)
        {
            let info = vk::BufferDeviceAddressInfo::builder().buffer(buffer);
            let address = unsafe { self.owner.device.get_buffer_device_address(&info) };
            if address == 0 {
                block.release(range);
                return Err(Error::Validation(
                    "SHADER_DEVICE_ADDRESS buffer returned a zero address".into(),
                ));
            }
            Some(address)
        } else {
            None
        };
        Ok(Buffer {
            inner: Arc::new(BufferInner {
                owner: Arc::clone(&self.owner),
                block,
                range: Some(range),
                handle: buffer,
                size: descriptor.size,
                usage: descriptor.usage,
                memory: descriptor.memory,
                device_address,
                label: descriptor.label.clone(),
            }),
        })
    }

    /// Drops completely unused blocks. Live buffers keep their blocks alive;
    /// one empty pooled block per class/type is retained to avoid allocation
    /// churn.
    pub fn trim(&self) -> usize {
        let mut blocks = self
            .blocks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut retained_empty = Vec::<(MemoryClass, MemoryLocation, u32)>::new();
        let before = blocks.len();
        blocks.retain(|block| {
            if Arc::strong_count(block) != 1 || !block.is_empty() {
                return true;
            }
            let key = (block.class, block.location, block.memory_type_index);
            if block.dedicated || retained_empty.contains(&key) {
                false
            } else {
                retained_empty.push(key);
                true
            }
        });
        before - blocks.len()
    }

    pub fn block_count(&self) -> usize {
        self.blocks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

struct MemoryBlock {
    owner: Arc<DeviceOwner>,
    memory: vk::DeviceMemory,
    size: u64,
    class: MemoryClass,
    location: MemoryLocation,
    memory_type_index: u32,
    host_coherent: bool,
    non_coherent_atom_size: u64,
    mapped_address: Option<usize>,
    dedicated: bool,
    ranges: Mutex<RangeAllocator>,
}

impl fmt::Debug for MemoryBlock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MemoryBlock")
            .field("size", &self.size)
            .field("class", &self.class)
            .field("location", &self.location)
            .field("memory_type_index", &self.memory_type_index)
            .field("dedicated", &self.dedicated)
            .finish_non_exhaustive()
    }
}

impl MemoryBlock {
    fn new(
        owner: Arc<DeviceOwner>,
        class: MemoryClass,
        location: MemoryLocation,
        plan: MemoryPlan,
        size: u64,
        dedicated: bool,
        dedicated_resource: Option<DedicatedResource>,
    ) -> Result<Self> {
        let mut flags = vk::MemoryAllocateFlagsInfo::builder()
            .flags(vk::MemoryAllocateFlags::DEVICE_ADDRESS)
            .build();
        let mut dedicated_info = dedicated_resource.map(|resource| match resource {
            DedicatedResource::Buffer(buffer) => vk::MemoryDedicatedAllocateInfo::builder()
                .buffer(buffer)
                .build(),
            DedicatedResource::Image(image) => vk::MemoryDedicatedAllocateInfo::builder()
                .image(image)
                .build(),
        });
        let mut allocate = vk::MemoryAllocateInfo::builder()
            .allocation_size(size)
            .memory_type_index(plan.memory_type_index)
            .push_next(&mut flags);
        if let Some(dedicated_info) = &mut dedicated_info {
            allocate = allocate.push_next(dedicated_info);
        }
        let memory = unsafe { owner.device.allocate_memory(&allocate, None) }
            .map_err(|source| Error::vulkan("vkAllocateMemory(block)", source))?;
        let mapped_address = if plan.host_visible() {
            let map = vk::MemoryMapInfo::builder()
                .memory(memory)
                .offset(0)
                .size(size)
                .flags(vk::MemoryMapFlags::empty());
            match unsafe { owner.device.map_memory2(&map) } {
                Ok(pointer) => Some(pointer as usize),
                Err(source) => {
                    unsafe { owner.device.free_memory(memory, None) };
                    return Err(Error::vulkan("vkMapMemory2(block)", source));
                }
            }
        } else {
            None
        };
        Ok(Self {
            owner,
            memory,
            size,
            class,
            location,
            memory_type_index: plan.memory_type_index,
            host_coherent: plan.host_coherent(),
            non_coherent_atom_size: plan.flush_atom_size.unwrap_or(1),
            mapped_address,
            dedicated,
            ranges: Mutex::new(RangeAllocator::new(size)),
        })
    }

    fn compatible(
        &self,
        class: MemoryClass,
        location: MemoryLocation,
        memory_type_index: u32,
    ) -> bool {
        !self.dedicated
            && self.class == class
            && self.location == location
            && self.memory_type_index == memory_type_index
    }

    fn allocate(&self, size: u64, alignment: u64) -> Option<Range<u64>> {
        self.ranges
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .allocate(size, alignment)
    }

    fn release(&self, range: Range<u64>) {
        self.ranges
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .release(range);
    }

    fn is_empty(&self) -> bool {
        self.ranges
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .available()
            == self.size
    }

    fn mapped_range(&self, offset: u64, size: usize) -> Result<*mut u8> {
        let base = self
            .mapped_address
            .ok_or_else(|| Error::Validation("buffer memory is not host visible".into()))?;
        let offset = usize::try_from(offset)
            .map_err(|_| Error::Validation("mapped offset exceeds usize".into()))?;
        let end = offset
            .checked_add(size)
            .ok_or_else(|| Error::Validation("mapped range overflows".into()))?;
        if end > self.size as usize {
            return Err(Error::Validation(
                "mapped range exceeds memory block".into(),
            ));
        }
        Ok((base as *mut u8).wrapping_add(offset))
    }

    fn flush(&self, offset: u64, size: u64) -> Result<()> {
        if self.host_coherent {
            return Ok(());
        }
        self.flush_or_invalidate(offset, size, true)
    }

    fn invalidate(&self, offset: u64, size: u64) -> Result<()> {
        if self.host_coherent {
            return Ok(());
        }
        self.flush_or_invalidate(offset, size, false)
    }

    fn flush_or_invalidate(&self, offset: u64, size: u64, flush: bool) -> Result<()> {
        let atom = self.non_coherent_atom_size;
        let start = align_down(offset, atom);
        let end = offset
            .checked_add(size)
            .and_then(|end| align_up(end, atom))
            .ok_or_else(|| Error::Validation("mapped synchronization range overflows".into()))?;
        let range = vk::MappedMemoryRange::builder()
            .memory(self.memory)
            .offset(start)
            .size(if end <= self.size {
                end - start
            } else {
                vk::WHOLE_SIZE
            })
            .build();
        if flush {
            unsafe { self.owner.device.flush_mapped_memory_ranges(&[range]) }
                .map_err(|source| Error::vulkan("vkFlushMappedMemoryRanges", source))
        } else {
            unsafe { self.owner.device.invalidate_mapped_memory_ranges(&[range]) }
                .map_err(|source| Error::vulkan("vkInvalidateMappedMemoryRanges", source))
        }
    }
}

impl Drop for MemoryBlock {
    fn drop(&mut self) {
        unsafe {
            if self.mapped_address.is_some() {
                let unmap = vk::MemoryUnmapInfo::builder().memory(self.memory);
                let _ = self.owner.device.unmap_memory2(&unmap);
            }
            self.owner.device.free_memory(self.memory, None);
        }
    }
}

/// Cloneable ownership handle for one Vulkan buffer.
///
/// The underlying buffer and allocation range are released after the final
/// host owner or in-flight submission lease is dropped.
#[derive(Clone)]
pub struct Buffer {
    inner: Arc<BufferInner>,
}

struct BufferInner {
    owner: Arc<DeviceOwner>,
    block: Arc<MemoryBlock>,
    range: Option<Range<u64>>,
    handle: vk::Buffer,
    size: u64,
    usage: vk::BufferUsageFlags,
    memory: MemoryLocation,
    device_address: Option<vk::DeviceAddress>,
    label: Option<String>,
}

impl fmt::Debug for Buffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Buffer")
            .field("label", &self.inner.label)
            .field("handle", &self.inner.handle)
            .field("size", &self.inner.size)
            .field("usage", &self.inner.usage)
            .field("memory", &self.inner.memory)
            .field("device_address", &self.inner.device_address)
            .finish_non_exhaustive()
    }
}

impl Buffer {
    pub fn raw(&self) -> vk::Buffer {
        self.inner.handle
    }

    pub fn size(&self) -> u64 {
        self.inner.size
    }

    pub fn usage(&self) -> vk::BufferUsageFlags {
        self.inner.usage
    }

    pub fn memory_location(&self) -> MemoryLocation {
        self.inner.memory
    }

    pub fn device_address(&self) -> Option<vk::DeviceAddress> {
        self.inner.device_address
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.inner.owner, owner)
    }

    /// Copies bytes into upload-visible memory and flushes when required.
    ///
    /// # Safety
    ///
    /// The target range must not be read or written by the GPU concurrently.
    pub unsafe fn write(&self, offset: u64, data: &[u8]) -> Result<()> {
        if self.inner.memory != MemoryLocation::Upload {
            return Err(Error::Validation(
                "CPU writes require MemoryLocation::Upload".into(),
            ));
        }
        let size = u64::try_from(data.len())
            .map_err(|_| Error::Validation("write size exceeds u64".into()))?;
        let end = offset
            .checked_add(size)
            .ok_or_else(|| Error::Validation("buffer write range overflows".into()))?;
        if end > self.inner.size {
            return Err(Error::Validation("buffer write exceeds buffer size".into()));
        }
        let allocation_offset =
            self.inner.range.as_ref().expect("live buffer range").start + offset;
        let destination = self
            .inner
            .block
            .mapped_range(allocation_offset, data.len())?;
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), destination, data.len()) };
        self.inner.block.flush(allocation_offset, size)
    }

    /// Invalidates and copies bytes from readback-visible memory.
    ///
    /// # Safety
    ///
    /// All GPU writes to the source range must have completed.
    pub unsafe fn read(&self, offset: u64, destination: &mut [u8]) -> Result<()> {
        if self.inner.memory != MemoryLocation::Readback {
            return Err(Error::Validation(
                "CPU reads require MemoryLocation::Readback".into(),
            ));
        }
        let size = u64::try_from(destination.len())
            .map_err(|_| Error::Validation("read size exceeds u64".into()))?;
        let end = offset
            .checked_add(size)
            .ok_or_else(|| Error::Validation("buffer read range overflows".into()))?;
        if end > self.inner.size {
            return Err(Error::Validation("buffer read exceeds buffer size".into()));
        }
        let allocation_offset =
            self.inner.range.as_ref().expect("live buffer range").start + offset;
        self.inner.block.invalidate(allocation_offset, size)?;
        let source = self
            .inner
            .block
            .mapped_range(allocation_offset, destination.len())?;
        unsafe {
            std::ptr::copy_nonoverlapping(source, destination.as_mut_ptr(), destination.len())
        };
        Ok(())
    }
}

impl crate::SubmissionResource for Buffer {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.inner))
    }
}

impl Drop for BufferInner {
    fn drop(&mut self) {
        unsafe { self.owner.device.destroy_buffer(self.handle, None) };
        if let Some(range) = self.range.take() {
            self.block.release(range);
        }
    }
}

#[derive(Debug)]
struct RangeAllocator {
    size: u64,
    free: Vec<Range<u64>>,
}

impl RangeAllocator {
    fn new(size: u64) -> Self {
        Self {
            size,
            free: std::iter::once(0..size).collect(),
        }
    }

    fn allocate(&mut self, size: u64, alignment: u64) -> Option<Range<u64>> {
        for index in 0..self.free.len() {
            let free = self.free[index].clone();
            let start = align_up(free.start, alignment)?;
            let end = start.checked_add(size)?;
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
            self.normalize();
            return Some(start..end);
        }
        None
    }

    fn release(&mut self, range: Range<u64>) {
        debug_assert!(range.start < range.end && range.end <= self.size);
        self.free.push(range);
        self.normalize();
    }

    fn available(&self) -> u64 {
        self.free.iter().map(|range| range.end - range.start).sum()
    }

    fn normalize(&mut self) {
        self.free.sort_by_key(|range| range.start);
        let mut merged = Vec::<Range<u64>>::with_capacity(self.free.len());
        for range in self.free.drain(..) {
            if let Some(previous) = merged.last_mut()
                && range.start <= previous.end
            {
                previous.end = previous.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        self.free = merged;
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return None;
    }
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
}

const fn align_down(value: u64, alignment: u64) -> u64 {
    value & !(alignment - 1)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MemoryClass {
    Buffer,
    LinearImage,
    OptimalImage,
}

#[derive(Clone, Copy, Debug)]
enum DedicatedResource {
    Buffer(vk::Buffer),
    Image(vk::Image),
}

fn buffer_memory_requirements(
    owner: &DeviceOwner,
    buffer: vk::Buffer,
) -> (vk::MemoryRequirements, vk::MemoryDedicatedRequirements) {
    let info = vk::BufferMemoryRequirementsInfo2::builder().buffer(buffer);
    let mut dedicated = vk::MemoryDedicatedRequirements::default();
    let mut requirements = vk::MemoryRequirements2::builder().push_next(&mut dedicated);
    unsafe {
        owner
            .device
            .get_buffer_memory_requirements2(&info, &mut requirements)
    };
    (requirements.memory_requirements, dedicated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_allocator_reuses_and_merges_suballocations() {
        let mut allocator = RangeAllocator::new(1024);
        let first = allocator.allocate(100, 64).unwrap();
        let second = allocator.allocate(100, 256).unwrap();
        assert_eq!(first, 0..100);
        assert_eq!(second, 256..356);
        allocator.release(first);
        allocator.release(second);
        assert_eq!(allocator.available(), 1024);
        assert_eq!(allocator.allocate(1024, 1).unwrap(), 0..1024);
    }
}

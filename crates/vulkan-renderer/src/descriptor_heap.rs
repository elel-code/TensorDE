use std::fmt;
use std::sync::{Arc, Mutex};

use vulkanalia::{prelude::v1_4::*, vk};

use crate::backend::DeviceOwner;
use crate::{Backend, DescriptorHeapLimits, Error, Features, FrameToken, Result as BackendResult};

mod allocator;
mod binding;
mod index;
mod storage;
mod texture;
mod upload;
mod write;

use allocator::align_up;
use storage::DescriptorHeapStorageRequest;

pub use allocator::{
    DescriptorAllocation, DescriptorHeapAllocator, DescriptorHeapError, DescriptorHeapUploadRange,
};
pub use binding::{
    BufferDescriptorBinding, BufferDescriptorKind, DescriptorSlotKind,
    DynamicExternalImageDescriptorBinding, ImageDescriptorBinding, ImageDescriptorKind,
    ReservedDescriptorBinding,
};
pub use index::descriptor_heap_element_index;
pub use storage::DescriptorHeapMemory;
pub use texture::{
    SampledImageBinding, SampledTextureBinding, SampledTextureHeapIndices,
    SampledTextureHeapOffsets, SampledTextureShaderBindings, SamplerAddressMode, SamplerBinding,
    SamplerBorderColor, SamplerCompareFunction, SamplerDescriptor, SamplerFilterMode,
};
pub use upload::DescriptorHeapUploadBatch;
pub use write::{SampledImageDescriptor, SampledImageDescriptorWriteBatch};

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
    memory: DescriptorHeapMemory,
    buffer: vk::Buffer,
    target_memory: vk::DeviceMemory,
    mapped_memory: vk::DeviceMemory,
    staging_buffer: Option<vk::Buffer>,
    staging_memory: Option<vk::DeviceMemory>,
    mapped_address: usize,
    mapped_size: u64,
    device_address: vk::DeviceAddress,
    reserved_range_offset: u64,
    reserved_range_size: u64,
    host_coherent: bool,
    non_coherent_atom_size: u64,
    limits: DescriptorHeapLimits,
    allocator: DescriptorHeapAllocator,
    write_lock: Mutex<()>,
}

impl fmt::Debug for DescriptorHeap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DescriptorHeap")
            .field("label", &self.label)
            .field("kind", &self.kind)
            .field("memory", &self.memory)
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
    /// Returns the application payload bytes required for `descriptor_count`
    /// direct-heap elements of `kind`.
    ///
    /// Resource heaps use the unified image/buffer stride emitted by Slang;
    /// sampler heaps use their independent sampler stride. The implementation
    /// reserved suffix is appended by [`Self::create_descriptor_heap`].
    pub fn descriptor_heap_capacity_bytes(
        &self,
        kind: DescriptorHeapKind,
        descriptor_count: u64,
    ) -> BackendResult<u64> {
        if descriptor_count == 0 {
            return Err(Error::Validation(
                "descriptor heap element count must be non-zero".into(),
            ));
        }
        let limits = self.device_info().limits.descriptor_heap;
        let stride = match kind {
            DescriptorHeapKind::Resource => limits.unified_resource_descriptor_stride(),
            DescriptorHeapKind::Sampler => limits.sampler_descriptor_stride(),
        }
        .ok_or_else(|| {
            Error::Validation(format!(
                "{kind:?} descriptor limits do not satisfy the Slang heap ABI"
            ))
        })?;
        stride
            .checked_mul(descriptor_count)
            .ok_or_else(|| Error::Validation("descriptor heap capacity overflows".into()))
    }

    /// Creates the sole standard shader-resource binding storage model.
    pub fn create_descriptor_heap(
        &self,
        descriptor: &DescriptorHeapDescriptor,
    ) -> BackendResult<DescriptorHeap> {
        self.create_descriptor_heap_with_memory(descriptor, DescriptorHeapMemory::HostVisible)
    }

    /// Creates a descriptor heap with explicit host-visible or device-local
    /// placement.
    ///
    /// `HostVisible` is appropriate for cold tables. `DeviceLocal` retains a
    /// mapped transfer source and requires its explicit upload recording API
    /// before shader access; it avoids treating PCIe-visible upload memory as
    /// the steady-state shader descriptor store on discrete GPUs.
    pub fn create_descriptor_heap_with_memory(
        &self,
        descriptor: &DescriptorHeapDescriptor,
        memory: DescriptorHeapMemory,
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
        let mut usage =
            vk::BufferUsageFlags::DESCRIPTOR_HEAP_EXT | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        if memory == DescriptorHeapMemory::DeviceLocal {
            usage |= vk::BufferUsageFlags::TRANSFER_DST;
        }
        let create = vk::BufferCreateInfo::builder()
            .size(heap_size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { owner.device.create_buffer(&create, None) }
            .map_err(|source| Error::vulkan("vkCreateBuffer(descriptor heap)", source))?;
        match storage::create_descriptor_heap_memory(DescriptorHeapStorageRequest {
            owner: Arc::clone(&owner),
            buffer,
            descriptor,
            allocator,
            limits,
            memory_types: &self.device_info().memory_types,
            non_coherent_atom_size: self.device_info().non_coherent_atom_size,
            memory,
        }) {
            Ok(heap) => Ok(heap),
            Err(error) => {
                unsafe { owner.device.destroy_buffer(buffer, None) };
                Err(error)
            }
        }
    }
}

impl DescriptorHeap {
    pub const fn kind(&self) -> DescriptorHeapKind {
        self.kind
    }

    pub const fn memory(&self) -> DescriptorHeapMemory {
        self.memory
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
        self.allocate_range(descriptor_type, 1)
    }

    /// Allocates `descriptor_count` contiguous direct-heap elements.
    ///
    /// A multi-element allocation is useful for one frame's compact sampled
    /// image table: shader indices remain byte-strided and no implicit
    /// descriptor-set layout is introduced.
    pub fn allocate_range(
        &self,
        descriptor_type: HeapDescriptorType,
        descriptor_count: u64,
    ) -> std::result::Result<DescriptorAllocation, DescriptorHeapError> {
        if descriptor_count == 0 {
            return Err(DescriptorHeapError::ZeroSize);
        }
        let (_, alignment, expected_heap) = self.descriptor_layout(descriptor_type);
        if self.kind != expected_heap {
            return Err(DescriptorHeapError::WrongHeapKind);
        }
        let (allocation_size, allocation_alignment) = match expected_heap {
            DescriptorHeapKind::Resource => (
                self.limits
                    .unified_resource_descriptor_stride()
                    .ok_or(DescriptorHeapError::InvalidDescriptorLayout)?,
                self.limits
                    .image_descriptor_alignment
                    .max(self.limits.buffer_descriptor_alignment),
            ),
            DescriptorHeapKind::Sampler => (
                self.limits
                    .sampler_descriptor_stride()
                    .ok_or(DescriptorHeapError::InvalidDescriptorLayout)?,
                alignment,
            ),
        };
        let allocation_size = allocation_size
            .checked_mul(descriptor_count)
            .ok_or(DescriptorHeapError::RangeOverflow)?;
        self.allocator
            .allocate(allocation_size, allocation_alignment)
    }

    pub fn retire(
        &self,
        allocation: DescriptorAllocation,
        after: FrameToken,
    ) -> std::result::Result<(), DescriptorHeapError> {
        self.allocator.retire(allocation, after)
    }

    /// Releases an allocation that was never referenced by submitted GPU
    /// work. Descriptors used by a command buffer MUST instead be retired
    /// with that submission's [`FrameToken`].
    pub fn release(
        &self,
        allocation: DescriptorAllocation,
    ) -> std::result::Result<(), DescriptorHeapError> {
        self.allocator.release(allocation)
    }

    pub fn reclaim(&self, completed_timeline: u64) -> usize {
        self.allocator.reclaim(completed_timeline)
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

    pub const fn application_capacity(&self) -> u64 {
        self.reserved_range_offset
    }

    /// Returns the shared allocation state for this heap.
    ///
    /// A product may retain frame policy through this clone, but all ranges
    /// still belong to this direct heap and therefore remain compatible with
    /// its descriptor encoding and upload methods.
    pub fn allocator(&self) -> DescriptorHeapAllocator {
        self.allocator.clone()
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }

    pub(crate) fn owns(&self, allocation: &DescriptorAllocation) -> bool {
        self.allocator.owns(allocation)
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

    pub fn allocation_stride(
        &self,
        descriptor_type: HeapDescriptorType,
    ) -> std::result::Result<u64, DescriptorHeapError> {
        let (_, _, expected_heap) = self.descriptor_layout(descriptor_type);
        if self.kind != expected_heap {
            return Err(DescriptorHeapError::WrongHeapKind);
        }
        match expected_heap {
            DescriptorHeapKind::Resource => self
                .limits
                .unified_resource_descriptor_stride()
                .ok_or(DescriptorHeapError::InvalidDescriptorLayout),
            DescriptorHeapKind::Sampler => self
                .limits
                .sampler_descriptor_stride()
                .ok_or(DescriptorHeapError::InvalidDescriptorLayout),
        }
    }

    /// Returns the alignment required when allocating a descriptor table for
    /// `descriptor_type` from this heap's shared allocation state.
    pub fn allocation_alignment(
        &self,
        descriptor_type: HeapDescriptorType,
    ) -> std::result::Result<u64, DescriptorHeapError> {
        let (_, alignment, expected_heap) = self.descriptor_layout(descriptor_type);
        if self.kind != expected_heap {
            return Err(DescriptorHeapError::WrongHeapKind);
        }
        Ok(match expected_heap {
            DescriptorHeapKind::Resource => alignment.max(self.limits.buffer_descriptor_alignment),
            DescriptorHeapKind::Sampler => alignment,
        })
    }
}

impl Drop for DescriptorHeap {
    fn drop(&mut self) {
        let unmap = vk::MemoryUnmapInfo::builder().memory(self.mapped_memory);
        unsafe {
            let _ = self.owner.device.unmap_memory2(&unmap);
            if let Some(buffer) = self.staging_buffer.take() {
                self.owner.device.destroy_buffer(buffer, None);
            }
            if let Some(memory) = self.staging_memory.take() {
                self.owner.device.free_memory(memory, None);
            }
            self.owner.device.destroy_buffer(self.buffer, None);
            self.owner.device.free_memory(self.target_memory, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::FrameClock;

    use super::*;

    #[test]
    fn resource_heap_uses_slang_unified_image_buffer_stride() {
        let limits = DescriptorHeapLimits {
            image_descriptor_size: 32,
            image_descriptor_alignment: 8,
            buffer_descriptor_size: 16,
            buffer_descriptor_alignment: 16,
            ..DescriptorHeapLimits::default()
        };

        assert_eq!(limits.unified_resource_descriptor_stride(), Some(32));
    }

    #[test]
    fn reserved_suffix_and_descriptor_alignment_are_never_violated() {
        let heap = DescriptorHeapAllocator::new(1024, 65, 64).unwrap();
        let allocation = heap.allocate(32, 128).unwrap();
        assert_eq!(heap.reserved_range_offset(), 896);
        assert_eq!(heap.reserved_range_size(), 128);
        assert_eq!(allocation.offset(), 0);
        assert_eq!(allocation.offset() % 128, 0);
    }

    #[test]
    fn heap_binding_alignment_does_not_make_descriptor_elements_sparse() {
        let heap = DescriptorHeapAllocator::new(1024, 256, 256).unwrap();
        let first = heap.allocate(32, 8).unwrap();
        let second = heap.allocate(32, 8).unwrap();
        let third = heap.allocate(32, 8).unwrap();

        assert_eq!(heap.reserved_range_offset(), 768);
        assert_eq!(first.range(), &(0..32));
        assert_eq!(second.range(), &(32..64));
        assert_eq!(third.range(), &(64..96));
    }

    #[test]
    fn range_is_reused_only_after_timeline_completion() {
        let mut clock = FrameClock::default();
        let frame = clock.allocate().unwrap();
        let heap = DescriptorHeapAllocator::new(256, 64, 64).unwrap();
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
        let heap = DescriptorHeapAllocator::new(512, 64, 64).unwrap();
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
        let first = DescriptorHeapAllocator::new(256, 64, 64).unwrap();
        let second = DescriptorHeapAllocator::new(256, 64, 64).unwrap();
        let allocation = first.allocate(64, 64).unwrap();
        assert_eq!(
            second.retire(allocation, frame),
            Err(DescriptorHeapError::WrongAllocator)
        );
    }

    #[test]
    fn allocator_clones_share_ranges_and_timeline_retirement() {
        let heap = DescriptorHeapAllocator::new(256, 64, 64).unwrap();
        let frame_policy = heap.clone();
        let allocation = frame_policy.allocate(64, 64).unwrap();

        assert_eq!(heap.available_bytes(), 128);
        frame_policy.retire_at_timeline(allocation, 7).unwrap();
        assert_eq!(heap.pending_retirements(), 1);
        assert_eq!(heap.reclaim(6), 0);
        assert_eq!(heap.reclaim(7), 1);
        assert_eq!(heap.allocate(192, 64).unwrap().range(), &(0..192));
    }

    #[test]
    fn unsubmitted_allocation_is_immediately_reusable() {
        let heap = DescriptorHeapAllocator::new(256, 64, 64).unwrap();
        let allocation = heap.allocate(128, 64).unwrap();
        heap.release(allocation).unwrap();
        assert_eq!(heap.allocate(192, 64).unwrap().range(), &(0..192));
    }
}

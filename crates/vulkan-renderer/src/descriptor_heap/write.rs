use vulkanalia::{
    prelude::v1_4::*,
    vk::{self, ExtDescriptorHeapExtensionDeviceCommands},
};

use super::{DescriptorAllocation, DescriptorHeap, HeapDescriptorType, align_down, align_up};
use crate::{Error, Result};

impl DescriptorHeap {
    /// Writes a uniform or storage-buffer descriptor into mapped heap memory.
    ///
    /// # Safety
    ///
    /// `device_address..device_address + size` must remain a valid compatible
    /// buffer range for every GPU use of this descriptor.
    pub unsafe fn write_buffer(
        &self,
        allocation: &DescriptorAllocation,
        descriptor_type: HeapDescriptorType,
        device_address: vk::DeviceAddress,
        size: u64,
    ) -> Result<()> {
        if !matches!(
            descriptor_type,
            HeapDescriptorType::UniformBuffer | HeapDescriptorType::StorageBuffer
        ) || device_address == 0
            || size == 0
        {
            return Err(Error::Validation("invalid descriptor buffer range".into()));
        }
        let vk_type = match descriptor_type {
            HeapDescriptorType::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
            HeapDescriptorType::StorageBuffer => vk::DescriptorType::STORAGE_BUFFER,
            _ => unreachable!(),
        };
        let address_range = vk::DeviceAddressRangeEXT::builder()
            .address(device_address)
            .size(size)
            .build();
        let info = vk::ResourceDescriptorInfoEXT::builder()
            .type_(vk_type)
            .data(vk::ResourceDescriptorDataEXT {
                address_range: &address_range,
            })
            .build();
        unsafe { self.write_resource(allocation, descriptor_type, info) }
    }

    /// Writes a sampled-image, storage-image, or input-attachment descriptor.
    ///
    /// # Safety
    ///
    /// The view create info and the image/view it describes must be valid for
    /// the device and remain compatible for every GPU use of the descriptor.
    pub unsafe fn write_image(
        &self,
        allocation: &DescriptorAllocation,
        descriptor_type: HeapDescriptorType,
        view: &vk::ImageViewCreateInfo,
        layout: vk::ImageLayout,
    ) -> Result<()> {
        let vk_type = match descriptor_type {
            HeapDescriptorType::SampledImage => vk::DescriptorType::SAMPLED_IMAGE,
            HeapDescriptorType::StorageImage => {
                if layout != vk::ImageLayout::GENERAL {
                    return Err(Error::Validation(
                        "storage-image descriptors require GENERAL layout".into(),
                    ));
                }
                vk::DescriptorType::STORAGE_IMAGE
            }
            HeapDescriptorType::InputAttachment => vk::DescriptorType::INPUT_ATTACHMENT,
            _ => {
                return Err(Error::Validation(
                    "descriptor type is not an image descriptor".into(),
                ));
            }
        };
        if matches!(
            layout,
            vk::ImageLayout::UNDEFINED | vk::ImageLayout::PREINITIALIZED
        ) {
            return Err(Error::Validation(
                "image descriptor layout must be GPU-accessible".into(),
            ));
        }
        let image = vk::ImageDescriptorInfoEXT::builder()
            .view(view)
            .layout(layout)
            .build();
        let info = vk::ResourceDescriptorInfoEXT::builder()
            .type_(vk_type)
            .data(vk::ResourceDescriptorDataEXT { image: &image })
            .build();
        unsafe { self.write_resource(allocation, descriptor_type, info) }
    }

    /// Writes a sampler descriptor.
    ///
    /// # Safety
    ///
    /// `sampler` must describe a valid sampler for this device and its pNext
    /// chain must remain valid for the duration of the call.
    pub unsafe fn write_sampler(
        &self,
        allocation: &DescriptorAllocation,
        sampler: &vk::SamplerCreateInfo,
    ) -> Result<()> {
        self.validate_allocation(allocation, HeapDescriptorType::Sampler)?;
        let range = self.host_range(allocation, HeapDescriptorType::Sampler)?;
        let _write_guard = self
            .write_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            self.owner
                .device
                .write_sampler_descriptors_ext(&[*sampler], &[range])
        }
        .map_err(|source| Error::vulkan("vkWriteSamplerDescriptorsEXT", source))?;
        self.flush(allocation.offset(), range.size as u64)
    }

    fn validate_allocation(
        &self,
        allocation: &DescriptorAllocation,
        descriptor_type: HeapDescriptorType,
    ) -> Result<()> {
        let (size, alignment, expected_heap) = self.descriptor_layout(descriptor_type);
        let owns_allocation = self
            .allocator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .owns(allocation);
        let end = allocation
            .offset()
            .checked_add(size)
            .ok_or_else(|| Error::Validation("descriptor range overflows".into()))?;
        if !owns_allocation
            || self.kind != expected_heap
            || allocation.size() < size
            || !allocation.offset().is_multiple_of(alignment)
            || end > self.reserved_range_offset
        {
            return Err(Error::Validation(
                "descriptor allocation is incompatible with this heap/type".into(),
            ));
        }
        Ok(())
    }

    fn host_range(
        &self,
        allocation: &DescriptorAllocation,
        descriptor_type: HeapDescriptorType,
    ) -> Result<vk::HostAddressRangeEXT> {
        self.validate_allocation(allocation, descriptor_type)?;
        let (size, _, _) = self.descriptor_layout(descriptor_type);
        let offset = usize::try_from(allocation.offset())
            .map_err(|_| Error::Validation("descriptor offset exceeds usize".into()))?;
        let size = usize::try_from(size)
            .map_err(|_| Error::Validation("descriptor size exceeds usize".into()))?;
        Ok(vk::HostAddressRangeEXT {
            address: (self.mapped_address as *mut u8).wrapping_add(offset).cast(),
            size,
        })
    }

    unsafe fn write_resource(
        &self,
        allocation: &DescriptorAllocation,
        descriptor_type: HeapDescriptorType,
        info: vk::ResourceDescriptorInfoEXT,
    ) -> Result<()> {
        let range = self.host_range(allocation, descriptor_type)?;
        let _write_guard = self
            .write_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            self.owner
                .device
                .write_resource_descriptors_ext(&[info], &[range])
        }
        .map_err(|source| Error::vulkan("vkWriteResourceDescriptorsEXT", source))?;
        self.flush(allocation.offset(), range.size as u64)
    }

    fn flush(&self, offset: u64, size: u64) -> Result<()> {
        if self.host_coherent {
            return Ok(());
        }
        let atom = self.non_coherent_atom_size;
        let flush_offset = align_down(offset, atom);
        let end = offset
            .checked_add(size)
            .and_then(|end| align_up(end, atom))
            .ok_or_else(|| Error::Validation("descriptor flush range overflows".into()))?;
        let flush_size = if end <= self.mapped_size {
            end - flush_offset
        } else {
            vk::WHOLE_SIZE
        };
        let range = vk::MappedMemoryRange::builder()
            .memory(self.memory)
            .offset(flush_offset)
            .size(flush_size)
            .build();
        unsafe { self.owner.device.flush_mapped_memory_ranges(&[range]) }
            .map_err(|source| Error::vulkan("vkFlushMappedMemoryRanges(descriptor heap)", source))
    }
}

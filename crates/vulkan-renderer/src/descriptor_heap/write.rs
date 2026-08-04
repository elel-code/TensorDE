use std::ptr;

use vulkanalia::{
    prelude::v1_4::*,
    vk::{self, ExtDescriptorHeapExtensionDeviceCommands},
};

use super::allocator::{align_down, align_up};
use super::{DescriptorAllocation, DescriptorHeap, HeapDescriptorType};
use crate::{Error, ExportedDmaBufImage, ImageView, ImportedDmaBufImage, Result, TextureLayout};

/// A sampled-image descriptor source backed by a renderer-owned view or
/// retained dma-buf image. The source owns no independent Vulkan handle; the
/// caller keeps the matching image alive through command submission.
#[derive(Clone, Debug)]
pub struct SampledImageDescriptor {
    view: vk::ImageViewCreateInfo,
}

impl SampledImageDescriptor {
    pub fn from_image_view(view: &ImageView) -> Self {
        Self {
            view: view.create_info(),
        }
    }

    pub fn from_imported_dma_buf(image: &ImportedDmaBufImage) -> Self {
        Self {
            view: image.view_create_info(),
        }
    }

    /// Selects one explicitly declared compatible view of an imported image.
    pub fn from_imported_dma_buf_format(
        image: &ImportedDmaBufImage,
        format: crate::TextureFormat,
    ) -> Result<Self> {
        Ok(Self {
            view: image.view_create_info_for_format(format)?,
        })
    }

    pub fn from_exported_dma_buf(image: &ExportedDmaBufImage) -> Self {
        Self {
            view: image.view_create_info(),
        }
    }
}

/// Reusable backing storage for one batched sampled-image descriptor write.
///
/// The vectors are deliberately retained by the caller, typically one frame
/// executor, so steady-state descriptor updates do not allocate. The raw
/// descriptor-info pointers are rebuilt after `image_infos` reaches its final
/// capacity and are consumed before this batch can be mutated again.
#[derive(Debug, Default)]
pub struct SampledImageDescriptorWriteBatch {
    image_infos: Vec<vk::ImageDescriptorInfoEXT>,
    resource_infos: Vec<vk::ResourceDescriptorInfoEXT>,
    destinations: Vec<vk::HostAddressRangeEXT>,
}

impl SampledImageDescriptorWriteBatch {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            image_infos: Vec::with_capacity(capacity),
            resource_infos: Vec::with_capacity(capacity),
            destinations: Vec::with_capacity(capacity),
        }
    }
}

impl DescriptorHeap {
    /// Writes a contiguous sampled-image table from typed renderer image
    /// sources. `layout` must match the image state used by the upcoming GPU
    /// commands.
    ///
    /// # Safety
    ///
    /// Every source image must remain valid and compatible with `layout`
    /// through every GPU use of the resulting descriptors.
    pub unsafe fn write_sampled_images(
        &self,
        allocation: &DescriptorAllocation,
        images: &[SampledImageDescriptor],
        layout: TextureLayout,
        batch: &mut SampledImageDescriptorWriteBatch,
    ) -> Result<()> {
        let layout = layout.to_vk();
        if images.is_empty()
            || matches!(
                layout,
                vk::ImageLayout::UNDEFINED | vk::ImageLayout::PREINITIALIZED
            )
        {
            return Err(Error::Validation(
                "sampled-image descriptor table requires non-empty GPU-accessible views".into(),
            ));
        }
        self.validate_allocation(allocation, HeapDescriptorType::SampledImage)?;
        let stride = self
            .allocation_stride(HeapDescriptorType::SampledImage)
            .map_err(|error| Error::Validation(error.to_string()))?;
        let required_size = stride
            .checked_mul(u64::try_from(images.len()).map_err(|_| {
                Error::Validation("sampled-image descriptor count exceeds u64".into())
            })?)
            .ok_or_else(|| Error::Validation("sampled-image descriptor table overflows".into()))?;
        if required_size > allocation.size() {
            return Err(Error::Validation(
                "sampled-image descriptor table exceeds its allocation".into(),
            ));
        }
        let descriptor_size = self.limits.image_descriptor_size;
        let allocation_offset = usize::try_from(allocation.offset())
            .map_err(|_| Error::Validation("descriptor offset exceeds usize".into()))?;
        let allocation_size = usize::try_from(allocation.size())
            .map_err(|_| Error::Validation("descriptor allocation exceeds usize".into()))?;
        let descriptor_size = usize::try_from(descriptor_size)
            .map_err(|_| Error::Validation("descriptor size exceeds usize".into()))?;
        let _write_guard = self
            .write_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        batch.image_infos.clear();
        batch.resource_infos.clear();
        batch.destinations.clear();
        batch.image_infos.reserve(images.len());
        batch.resource_infos.reserve(images.len());
        batch.destinations.reserve(images.len());
        batch.image_infos.extend(images.iter().map(|image| {
            vk::ImageDescriptorInfoEXT::builder()
                .view(&image.view)
                .layout(layout)
                .build()
        }));
        batch
            .resource_infos
            .extend(batch.image_infos.iter().map(|image| {
                vk::ResourceDescriptorInfoEXT::builder()
                    .type_(vk::DescriptorType::SAMPLED_IMAGE)
                    .data(vk::ResourceDescriptorDataEXT {
                        image: ptr::from_ref(image),
                    })
                    .build()
            }));
        let base = self.mapped_address as *mut u8;
        for index in 0..images.len() {
            let offset = stride
                .checked_mul(u64::try_from(index).expect("usize always converts to u64"))
                .and_then(|offset| allocation.offset().checked_add(offset))
                .ok_or_else(|| Error::Validation("descriptor table offset overflows".into()))?;
            let offset = usize::try_from(offset)
                .map_err(|_| Error::Validation("descriptor table offset exceeds usize".into()))?;
            batch.destinations.push(vk::HostAddressRangeEXT {
                address: unsafe { base.add(offset).cast() },
                size: descriptor_size,
            });
        }
        unsafe { ptr::write_bytes(base.add(allocation_offset), 0, allocation_size) };
        unsafe {
            self.owner
                .device
                .write_resource_descriptors_ext(&batch.resource_infos, &batch.destinations)
        }
        .map_err(|source| Error::vulkan("vkWriteResourceDescriptorsEXT", source))?;
        self.flush(allocation.offset(), allocation.size())
    }

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
        let owns_allocation = self.allocator.owns(allocation);
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
            .memory(self.mapped_memory)
            .offset(flush_offset)
            .size(flush_size)
            .build();
        unsafe { self.owner.device.flush_mapped_memory_ranges(&[range]) }
            .map_err(|source| Error::vulkan("vkFlushMappedMemoryRanges(descriptor heap)", source))
    }
}

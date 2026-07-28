use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use super::{DedicatedResource, MemoryAllocator, MemoryBlock, MemoryClass, align_up};
use crate::{
    AllocationRequirements, Error, ImageDescriptor, MemoryLocation, MemoryTypeSelector, Result,
};

impl MemoryAllocator {
    /// Creates an image from an image-only memory pool. Optimal, linear, and
    /// buffer allocations never share a block.
    pub fn create_image(&self, descriptor: &ImageDescriptor) -> Result<Image> {
        descriptor
            .validate()
            .map_err(|error| Error::Validation(error.to_string()))?;
        let create = vk::ImageCreateInfo::builder()
            .image_type(descriptor.image_type)
            .format(descriptor.format)
            .extent(descriptor.extent)
            .mip_levels(descriptor.mip_levels)
            .array_layers(descriptor.array_layers)
            .samples(descriptor.samples)
            .tiling(descriptor.tiling)
            .usage(descriptor.usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { self.owner.device.create_image(&create, None) }
            .map_err(|source| Error::vulkan("vkCreateImage", source))?;
        match self.bind_image(image, descriptor) {
            Ok(image) => Ok(image),
            Err(error) => {
                unsafe { self.owner.device.destroy_image(image, None) };
                Err(error)
            }
        }
    }

    fn bind_image(&self, image: vk::Image, descriptor: &ImageDescriptor) -> Result<Image> {
        let (requirements, dedicated_requirements) = image_memory_requirements(&self.owner, image);
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
        let class = match descriptor.tiling {
            vk::ImageTiling::LINEAR => MemoryClass::LinearImage,
            vk::ImageTiling::OPTIMAL => MemoryClass::OptimalImage,
            _ => {
                return Err(Error::Validation(
                    "DRM modifier image tiling requires an explicit import allocator".into(),
                ));
            }
        };
        let dedicated = requirements.size >= self.config.dedicated_threshold
            || dedicated_requirements.prefers_dedicated_allocation != 0
            || dedicated_requirements.requires_dedicated_allocation != 0;
        let mut blocks = self
            .blocks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !dedicated {
            for block in blocks.iter() {
                if block.compatible(class, descriptor.memory, selection.memory_type_index)
                    && let Some(range) = block.allocate(requirements.size, requirements.alignment)
                {
                    return self.finish_image(image, descriptor, Arc::clone(block), range);
                }
            }
        }

        let configured = match descriptor.memory {
            MemoryLocation::Device => self.config.image_block_size,
            MemoryLocation::Upload => self.config.upload_block_size,
            MemoryLocation::Readback => self.config.readback_block_size,
        };
        let desired = align_up(
            if dedicated {
                requirements.size
            } else {
                configured.max(requirements.size)
            },
            requirements.alignment,
        )
        .ok_or_else(|| Error::Validation("image memory block size overflows".into()))?;
        let heap_size = self
            .memory_types
            .iter()
            .find(|memory| memory.index == selection.memory_type_index)
            .map(|memory| memory.heap_size)
            .ok_or_else(|| Error::Validation("selected image memory type disappeared".into()))?;
        let block_size = if desired <= heap_size {
            desired
        } else {
            align_up(requirements.size, requirements.alignment).ok_or_else(|| {
                Error::Validation("minimum Vulkan image allocation size overflows".into())
            })?
        };
        let block = Arc::new(MemoryBlock::new(
            Arc::clone(&self.owner),
            class,
            descriptor.memory,
            selection,
            block_size,
            dedicated,
            dedicated.then_some(DedicatedResource::Image(image)),
        )?);
        let range = block
            .allocate(requirements.size, requirements.alignment)
            .ok_or_else(|| {
                Error::Validation("new image memory block cannot satisfy allocation".into())
            })?;
        blocks.push(Arc::clone(&block));
        self.finish_image(image, descriptor, block, range)
    }

    fn finish_image(
        &self,
        image: vk::Image,
        descriptor: &ImageDescriptor,
        block: Arc<MemoryBlock>,
        range: Range<u64>,
    ) -> Result<Image> {
        let bind = vk::BindImageMemoryInfo::builder()
            .image(image)
            .memory(block.memory)
            .memory_offset(range.start)
            .build();
        if let Err(source) = unsafe { self.owner.device.bind_image_memory2(&[bind]) } {
            block.release(range);
            return Err(Error::vulkan("vkBindImageMemory2", source));
        }
        Ok(Image {
            inner: Arc::new(ImageInner {
                owner: Arc::clone(&self.owner),
                block,
                range: Some(range),
                handle: image,
                image_type: descriptor.image_type,
                format: descriptor.format,
                extent: descriptor.extent,
                mip_levels: descriptor.mip_levels,
                array_layers: descriptor.array_layers,
                samples: descriptor.samples,
                usage: descriptor.usage,
                label: descriptor.label.clone(),
            }),
        })
    }
}

/// Cloneable ownership handle for one Vulkan image. The underlying image and
/// memory range are released after the final image/view owner is dropped.
#[derive(Clone)]
pub struct Image {
    inner: Arc<ImageInner>,
}

impl fmt::Debug for Image {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Image")
            .field("label", &self.inner.label)
            .field("handle", &self.inner.handle)
            .field("format", &self.inner.format)
            .field("extent", &self.inner.extent)
            .field("mip_levels", &self.inner.mip_levels)
            .field("array_layers", &self.inner.array_layers)
            .field("usage", &self.inner.usage)
            .finish_non_exhaustive()
    }
}

impl Image {
    pub fn raw(&self) -> vk::Image {
        self.inner.handle
    }

    pub fn format(&self) -> vk::Format {
        self.inner.format
    }

    pub fn image_type(&self) -> vk::ImageType {
        self.inner.image_type
    }

    pub fn extent(&self) -> vk::Extent3D {
        self.inner.extent
    }

    pub fn mip_levels(&self) -> u32 {
        self.inner.mip_levels
    }

    pub fn array_layers(&self) -> u32 {
        self.inner.array_layers
    }

    pub fn usage(&self) -> vk::ImageUsageFlags {
        self.inner.usage
    }

    pub fn sample_count(&self) -> vk::SampleCountFlags {
        self.inner.samples
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<crate::backend::DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.inner.owner, owner)
    }

    pub fn full_subresource_range(
        &self,
        aspect_mask: vk::ImageAspectFlags,
    ) -> vk::ImageSubresourceRange {
        vk::ImageSubresourceRange {
            aspect_mask,
            base_mip_level: 0,
            level_count: self.inner.mip_levels,
            base_array_layer: 0,
            layer_count: self.inner.array_layers,
        }
    }

    pub fn create_view(&self, descriptor: &ImageViewDescriptor) -> Result<ImageView> {
        if descriptor.format != self.inner.format {
            return Err(Error::Validation(
                "image view format reinterpretation is not enabled for this image".into(),
            ));
        }
        validate_subresource_range(
            descriptor.subresource_range,
            self.inner.mip_levels,
            self.inner.array_layers,
        )?;
        let create = vk::ImageViewCreateInfo::builder()
            .image(self.inner.handle)
            .view_type(descriptor.view_type)
            .format(descriptor.format)
            .components(descriptor.components)
            .subresource_range(descriptor.subresource_range);
        let handle = unsafe { self.inner.owner.device.create_image_view(&create, None) }
            .map_err(|source| Error::vulkan("vkCreateImageView", source))?;
        Ok(ImageView {
            image: Arc::clone(&self.inner),
            handle,
            descriptor: descriptor.clone(),
        })
    }
}

impl crate::SubmissionResource for Image {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.inner))
    }
}

struct ImageInner {
    owner: Arc<crate::backend::DeviceOwner>,
    block: Arc<MemoryBlock>,
    range: Option<Range<u64>>,
    handle: vk::Image,
    image_type: vk::ImageType,
    format: vk::Format,
    extent: vk::Extent3D,
    mip_levels: u32,
    array_layers: u32,
    samples: vk::SampleCountFlags,
    usage: vk::ImageUsageFlags,
    label: Option<String>,
}

impl Drop for ImageInner {
    fn drop(&mut self) {
        unsafe { self.owner.device.destroy_image(self.handle, None) };
        if let Some(range) = self.range.take() {
            self.block.release(range);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageViewDescriptor {
    pub label: Option<String>,
    pub view_type: vk::ImageViewType,
    pub format: vk::Format,
    pub components: vk::ComponentMapping,
    pub subresource_range: vk::ImageSubresourceRange,
}

/// Image view retaining its parent image allocation.
pub struct ImageView {
    image: Arc<ImageInner>,
    handle: vk::ImageView,
    descriptor: ImageViewDescriptor,
}

impl fmt::Debug for ImageView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageView")
            .field("label", &self.descriptor.label)
            .field("handle", &self.handle)
            .field("format", &self.descriptor.format)
            .field("subresource_range", &self.descriptor.subresource_range)
            .finish_non_exhaustive()
    }
}

impl ImageView {
    pub const fn raw(&self) -> vk::ImageView {
        self.handle
    }

    pub const fn format(&self) -> vk::Format {
        self.descriptor.format
    }

    pub fn sample_count(&self) -> vk::SampleCountFlags {
        self.image.samples
    }

    pub(crate) fn owner(&self) -> &Arc<crate::backend::DeviceOwner> {
        &self.image.owner
    }

    pub fn create_info(&self) -> vk::ImageViewCreateInfo {
        vk::ImageViewCreateInfo::builder()
            .image(self.image.handle)
            .view_type(self.descriptor.view_type)
            .format(self.descriptor.format)
            .components(self.descriptor.components)
            .subresource_range(self.descriptor.subresource_range)
            .build()
    }
}

impl crate::SubmissionResource for ImageView {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.image))
    }
}

impl Drop for ImageView {
    fn drop(&mut self) {
        unsafe {
            self.image
                .owner
                .device
                .destroy_image_view(self.handle, None)
        };
    }
}

fn validate_subresource_range(
    range: vk::ImageSubresourceRange,
    mip_levels: u32,
    array_layers: u32,
) -> Result<()> {
    if range.aspect_mask.is_empty() || range.level_count == 0 || range.layer_count == 0 {
        return Err(Error::Validation(
            "image view subresource range must be non-empty".into(),
        ));
    }
    let mip_end = range
        .base_mip_level
        .checked_add(range.level_count)
        .ok_or_else(|| Error::Validation("image view mip range overflows".into()))?;
    let layer_end = range
        .base_array_layer
        .checked_add(range.layer_count)
        .ok_or_else(|| Error::Validation("image view layer range overflows".into()))?;
    if mip_end > mip_levels || layer_end > array_layers {
        return Err(Error::Validation(
            "image view subresource range exceeds its image".into(),
        ));
    }
    Ok(())
}

fn image_memory_requirements(
    owner: &crate::backend::DeviceOwner,
    image: vk::Image,
) -> (vk::MemoryRequirements, vk::MemoryDedicatedRequirements) {
    let info = vk::ImageMemoryRequirementsInfo2::builder().image(image);
    let mut dedicated = vk::MemoryDedicatedRequirements::default();
    let mut requirements = vk::MemoryRequirements2::builder().push_next(&mut dedicated);
    unsafe {
        owner
            .device
            .get_image_memory_requirements2(&info, &mut requirements)
    };
    (requirements.memory_requirements, dedicated)
}

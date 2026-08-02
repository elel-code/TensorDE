use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use super::{DedicatedResource, MemoryAllocator, MemoryBlock, MemoryClass, align_up};
use crate::{
    AllocationRequirements, ComponentMapping, Error, ImageDescriptor, ImageViewDimension,
    MemoryLocation, MemoryTypeSelector, Result, TextureAspects, TextureSubresourceRange,
};

impl MemoryAllocator {
    /// Creates an image from an image-only memory pool. Optimal, linear, and
    /// buffer allocations never share a block.
    pub fn create_image(&self, descriptor: &ImageDescriptor) -> Result<Image> {
        descriptor
            .validate()
            .map_err(|error| Error::Validation(error.to_string()))?;
        let create = vk::ImageCreateInfo::builder()
            .image_type(descriptor.dimension.to_vk())
            .format(descriptor.format.to_vk())
            .extent(descriptor.extent.to_vk())
            .mip_levels(descriptor.mip_levels)
            .array_layers(descriptor.array_layers)
            .samples(descriptor.samples.to_vk())
            .tiling(descriptor.tiling.to_vk())
            .usage(descriptor.usage.to_vk())
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
            crate::ImageTiling::Linear => MemoryClass::LinearImage,
            crate::ImageTiling::Optimal => MemoryClass::OptimalImage,
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
                dimension: descriptor.dimension,
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

    pub fn format(&self) -> crate::TextureFormat {
        self.inner.format
    }

    pub fn dimension(&self) -> crate::ImageDimension {
        self.inner.dimension
    }

    pub fn extent(&self) -> crate::Extent3D {
        self.inner.extent
    }

    pub fn mip_levels(&self) -> u32 {
        self.inner.mip_levels
    }

    pub fn array_layers(&self) -> u32 {
        self.inner.array_layers
    }

    pub fn usage(&self) -> crate::TextureUsages {
        self.inner.usage
    }

    /// Bytes reserved from the allocator for this image.
    ///
    /// This is the bound Vulkan memory range, not host RSS/PSS and not a
    /// format-derived estimate. Several images may still share one allocator
    /// block.
    pub fn allocation_size(&self) -> u64 {
        self.inner
            .range
            .as_ref()
            .map_or(0, |range| range.end - range.start)
    }

    pub fn sample_count(&self) -> crate::SampleCount {
        self.inner.samples
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<crate::backend::DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.inner.owner, owner)
    }

    pub fn full_subresource_range(&self, aspects: TextureAspects) -> TextureSubresourceRange {
        TextureSubresourceRange::new(
            aspects,
            0,
            self.inner.mip_levels,
            0,
            self.inner.array_layers,
        )
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
            .view_type(descriptor.dimension.to_vk())
            .format(descriptor.format.to_vk())
            .components(descriptor.components.to_vk())
            .subresource_range(descriptor.subresource_range.to_vk());
        let handle = unsafe { self.inner.owner.device.create_image_view(&create, None) }
            .map_err(|source| Error::vulkan("vkCreateImageView", source))?;
        Ok(ImageView {
            inner: Arc::new(ImageViewInner {
                image: Arc::clone(&self.inner),
                handle,
                descriptor: descriptor.clone(),
            }),
        })
    }

    /// Creates an identity-swizzled view covering every color mip and layer.
    pub fn create_color_view(&self, label: impl Into<Option<String>>) -> Result<ImageView> {
        self.create_view(&ImageViewDescriptor {
            label: label.into(),
            dimension: ImageViewDimension::D2,
            format: self.format(),
            components: ComponentMapping::default(),
            subresource_range: self.full_subresource_range(TextureAspects::COLOR),
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
    dimension: crate::ImageDimension,
    format: crate::TextureFormat,
    extent: crate::Extent3D,
    mip_levels: u32,
    array_layers: u32,
    samples: crate::SampleCount,
    usage: crate::TextureUsages,
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
    pub dimension: ImageViewDimension,
    pub format: crate::TextureFormat,
    pub components: ComponentMapping,
    pub subresource_range: TextureSubresourceRange,
}

/// Cloneable ownership handle for an image view and its parent allocation.
///
/// The Vulkan view is destroyed only after both the application handle and
/// every submitted [`crate::SubmissionLease`] have been released.
#[derive(Clone)]
pub struct ImageView {
    inner: Arc<ImageViewInner>,
}

struct ImageViewInner {
    image: Arc<ImageInner>,
    handle: vk::ImageView,
    descriptor: ImageViewDescriptor,
}

impl fmt::Debug for ImageView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageView")
            .field("label", &self.inner.descriptor.label)
            .field("handle", &self.inner.handle)
            .field("format", &self.inner.descriptor.format)
            .field(
                "subresource_range",
                &self.inner.descriptor.subresource_range,
            )
            .finish_non_exhaustive()
    }
}

impl ImageView {
    pub fn raw(&self) -> vk::ImageView {
        self.inner.handle
    }

    pub fn format(&self) -> crate::TextureFormat {
        self.inner.descriptor.format
    }

    pub fn sample_count(&self) -> crate::SampleCount {
        self.inner.image.samples
    }

    pub fn usage(&self) -> crate::TextureUsages {
        self.inner.image.usage
    }

    pub(crate) fn owner(&self) -> &Arc<crate::backend::DeviceOwner> {
        &self.inner.image.owner
    }

    pub(crate) fn create_info(&self) -> vk::ImageViewCreateInfo {
        vk::ImageViewCreateInfo::builder()
            .image(self.inner.image.handle)
            .view_type(self.inner.descriptor.dimension.to_vk())
            .format(self.inner.descriptor.format.to_vk())
            .components(self.inner.descriptor.components.to_vk())
            .subresource_range(self.inner.descriptor.subresource_range.to_vk())
            .build()
    }
}

impl crate::SubmissionResource for ImageView {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.inner))
    }
}

impl Drop for ImageViewInner {
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
    range: TextureSubresourceRange,
    mip_levels: u32,
    array_layers: u32,
) -> Result<()> {
    if range.aspects.is_empty() || range.level_count == 0 || range.layer_count == 0 {
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

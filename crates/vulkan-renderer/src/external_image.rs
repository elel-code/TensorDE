//! Retained views over Vulkan images owned by decoders or host integrations.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use crate::backend::DeviceOwner;
use crate::{Backend, Error, ResourceBinding, Result};

/// Complete metadata required to validate and create a view over an externally
/// owned `VkImage` from the same logical device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalImageViewDescriptor {
    pub label: Option<String>,
    pub image: vk::Image,
    pub view_type: vk::ImageViewType,
    pub format: vk::Format,
    pub extent: vk::Extent3D,
    pub mip_levels: u32,
    pub array_layers: u32,
    pub samples: vk::SampleCountFlags,
    pub usage: vk::ImageUsageFlags,
    pub components: vk::ComponentMapping,
    pub subresource_range: vk::ImageSubresourceRange,
}

impl ExternalImageViewDescriptor {
    fn validate(&self) -> Result<()> {
        if self.image == vk::Image::null() {
            return Err(Error::Validation(
                "external image handle must not be null".into(),
            ));
        }
        if self.format == vk::Format::UNDEFINED {
            return Err(Error::Validation(
                "external image view format must be defined".into(),
            ));
        }
        if self.extent.width == 0 || self.extent.height == 0 || self.extent.depth == 0 {
            return Err(Error::Validation(
                "external image extent must be non-zero".into(),
            ));
        }
        if self.mip_levels == 0 || self.array_layers == 0 {
            return Err(Error::Validation(
                "external image mip and array-layer counts must be non-zero".into(),
            ));
        }
        if self.samples.is_empty() || self.samples.bits().count_ones() != 1 {
            return Err(Error::Validation(
                "external image sample count must contain exactly one bit".into(),
            ));
        }
        if self.usage.is_empty() {
            return Err(Error::Validation(
                "external image usage must be non-empty".into(),
            ));
        }
        validate_subresources(self.subresource_range, self.mip_levels, self.array_layers)
    }
}

/// Cloneable image view that keeps a decoder/host lease alive but never
/// destroys the externally owned image or its memory.
#[derive(Clone)]
pub struct RetainedExternalImageView {
    inner: Arc<RetainedExternalImageViewInner>,
}

/// Descriptor-only retained external image source. Descriptor-heap sampling
/// uses this object without allocating a Vulkan image view.
#[derive(Clone)]
pub struct RetainedExternalImage {
    inner: Arc<RetainedExternalImageInner>,
}

impl fmt::Debug for RetainedExternalImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetainedExternalImage")
            .field("label", &self.inner.descriptor.label)
            .field("image", &self.inner.descriptor.image)
            .field("format", &self.inner.descriptor.format)
            .field(
                "subresource_range",
                &self.inner.descriptor.subresource_range,
            )
            .finish_non_exhaustive()
    }
}

impl RetainedExternalImage {
    pub fn raw_image(&self) -> vk::Image {
        self.inner.descriptor.image
    }

    pub fn format(&self) -> vk::Format {
        self.inner.descriptor.format
    }

    pub fn extent(&self) -> vk::Extent3D {
        self.inner.descriptor.extent
    }

    pub fn usage(&self) -> vk::ImageUsageFlags {
        self.inner.descriptor.usage
    }

    pub fn subresource_range(&self) -> vk::ImageSubresourceRange {
        self.inner.descriptor.subresource_range
    }

    pub fn label(&self) -> Option<&str> {
        self.inner.descriptor.label.as_deref()
    }

    /// Produces the metadata consumed directly by descriptor-heap image
    /// writes without allocating a `VkImageView`.
    pub fn view_create_info(&self) -> vk::ImageViewCreateInfo {
        image_view_create_info(&self.inner.descriptor)
    }

    pub fn resource_binding(&self) -> ResourceBinding {
        ResourceBinding::Image {
            image: self.inner.descriptor.image,
            subresource_range: self.inner.descriptor.subresource_range,
        }
    }

    /// Materializes a real Vulkan image view only for APIs such as dynamic
    /// rendering that consume a `VkImageView` handle.
    pub fn create_view(&self) -> Result<RetainedExternalImageView> {
        let create = image_view_create_info(&self.inner.descriptor);
        let view = unsafe { self.inner.owner.device.create_image_view(&create, None) }
            .map_err(|source| Error::vulkan("vkCreateImageView(external image)", source))?;
        Ok(RetainedExternalImageView {
            inner: Arc::new(RetainedExternalImageViewInner {
                owner: Arc::clone(&self.inner.owner),
                descriptor: self.inner.descriptor.clone(),
                view,
                _host_lease: Arc::new(self.clone()),
            }),
        })
    }
}

impl crate::SubmissionResource for RetainedExternalImage {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.inner))
    }
}

struct RetainedExternalImageInner {
    owner: Arc<DeviceOwner>,
    descriptor: ExternalImageViewDescriptor,
    _host_lease: Arc<dyn Any + Send + Sync>,
}

impl fmt::Debug for RetainedExternalImageView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetainedExternalImageView")
            .field("label", &self.inner.descriptor.label)
            .field("image", &self.inner.descriptor.image)
            .field("view", &self.inner.view)
            .field("format", &self.inner.descriptor.format)
            .field(
                "subresource_range",
                &self.inner.descriptor.subresource_range,
            )
            .finish_non_exhaustive()
    }
}

impl RetainedExternalImageView {
    pub fn raw_image(&self) -> vk::Image {
        self.inner.descriptor.image
    }

    pub fn raw_view(&self) -> vk::ImageView {
        self.inner.view
    }

    pub fn format(&self) -> vk::Format {
        self.inner.descriptor.format
    }

    pub fn extent(&self) -> vk::Extent3D {
        self.inner.descriptor.extent
    }

    pub fn sample_count(&self) -> vk::SampleCountFlags {
        self.inner.descriptor.samples
    }

    pub fn usage(&self) -> vk::ImageUsageFlags {
        self.inner.descriptor.usage
    }

    pub fn subresource_range(&self) -> vk::ImageSubresourceRange {
        self.inner.descriptor.subresource_range
    }

    pub fn label(&self) -> Option<&str> {
        self.inner.descriptor.label.as_deref()
    }

    /// Reconstructs the view metadata consumed by descriptor-heap image
    /// writes. No pointer into the host lease escapes.
    pub fn view_create_info(&self) -> vk::ImageViewCreateInfo {
        image_view_create_info(&self.inner.descriptor)
    }

    pub fn resource_binding(&self) -> ResourceBinding {
        ResourceBinding::Image {
            image: self.inner.descriptor.image,
            subresource_range: self.inner.descriptor.subresource_range,
        }
    }

    pub(crate) fn owner(&self) -> &Arc<DeviceOwner> {
        &self.inner.owner
    }
}

impl crate::SubmissionResource for RetainedExternalImageView {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.inner))
    }
}

struct RetainedExternalImageViewInner {
    owner: Arc<DeviceOwner>,
    descriptor: ExternalImageViewDescriptor,
    view: vk::ImageView,
    // Dropped only after `Drop::drop` destroys the view.
    _host_lease: Arc<dyn Any + Send + Sync>,
}

impl Drop for RetainedExternalImageViewInner {
    fn drop(&mut self) {
        unsafe { self.owner.device.destroy_image_view(self.view, None) };
    }
}

impl Backend {
    /// Retains descriptor metadata and a host lease without allocating a
    /// Vulkan image view. This is the preferred descriptor-heap sampling path.
    ///
    /// # Safety
    ///
    /// `descriptor.image` must have been created from this exact logical
    /// device, match every supplied metadata field and remain valid until
    /// `host_lease` is dropped.
    pub unsafe fn retain_external_image<T>(
        &self,
        descriptor: &ExternalImageViewDescriptor,
        host_lease: Arc<T>,
    ) -> Result<RetainedExternalImage>
    where
        T: Any + Send + Sync,
    {
        descriptor.validate()?;
        Ok(RetainedExternalImage {
            inner: Arc::new(RetainedExternalImageInner {
                owner: self.shared_owner(),
                descriptor: descriptor.clone(),
                _host_lease: host_lease,
            }),
        })
    }

    /// Creates a renderer-owned view over a decoder/host-owned image and keeps
    /// the supplied lease alive through every clone of the result.
    ///
    /// # Safety
    ///
    /// `descriptor.image` must have been created from this exact Vulkan
    /// logical device, match every supplied metadata field and remain valid
    /// until `host_lease` is dropped. The lease destructor must not require the
    /// image view to remain live.
    pub unsafe fn create_retained_external_image_view<T>(
        &self,
        descriptor: &ExternalImageViewDescriptor,
        host_lease: Arc<T>,
    ) -> Result<RetainedExternalImageView>
    where
        T: Any + Send + Sync,
    {
        let image = unsafe { self.retain_external_image(descriptor, host_lease)? };
        image.create_view()
    }
}

fn image_view_create_info(descriptor: &ExternalImageViewDescriptor) -> vk::ImageViewCreateInfo {
    vk::ImageViewCreateInfo::builder()
        .image(descriptor.image)
        .view_type(descriptor.view_type)
        .format(descriptor.format)
        .components(descriptor.components)
        .subresource_range(descriptor.subresource_range)
        .build()
}

fn validate_subresources(
    range: vk::ImageSubresourceRange,
    mip_levels: u32,
    array_layers: u32,
) -> Result<()> {
    if range.aspect_mask.is_empty() || range.level_count == 0 || range.layer_count == 0 {
        return Err(Error::Validation(
            "external image view subresources must be non-empty".into(),
        ));
    }
    let mip_end = range
        .base_mip_level
        .checked_add(range.level_count)
        .ok_or_else(|| Error::Validation("external image mip range overflows".into()))?;
    let layer_end = range
        .base_array_layer
        .checked_add(range.layer_count)
        .ok_or_else(|| Error::Validation("external image layer range overflows".into()))?;
    if mip_end > mip_levels || layer_end > array_layers {
        return Err(Error::Validation(
            "external image view subresources exceed the image".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use vulkanalia::vk::Handle;

    use super::*;

    fn descriptor() -> ExternalImageViewDescriptor {
        ExternalImageViewDescriptor {
            label: Some("decoder-y-plane".into()),
            image: vk::Image::from_raw(7),
            view_type: vk::ImageViewType::_2D,
            format: vk::Format::R8_UNORM,
            extent: vk::Extent3D {
                width: 1920,
                height: 1080,
                depth: 1,
            },
            mip_levels: 1,
            array_layers: 2,
            samples: vk::SampleCountFlags::_1,
            usage: vk::ImageUsageFlags::SAMPLED,
            components: vk::ComponentMapping::default(),
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::PLANE_0,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 1,
                layer_count: 1,
            },
        }
    }

    #[test]
    fn decoder_plane_view_validates_layer_bounds() {
        let mut descriptor = descriptor();
        assert!(descriptor.validate().is_ok());
        descriptor.subresource_range.base_array_layer = 2;
        assert!(descriptor.validate().is_err());
    }
}

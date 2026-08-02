//! Retained views over Vulkan images owned by decoders or host integrations.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use crate::backend::DeviceOwner;
use crate::{
    Backend, Error, Extent3D, ResourceBinding, Result, SampleCount, TextureFormat, TextureUsages,
};

/// Complete metadata required to validate and create a view over an externally
/// owned `VkImage` from the same logical device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalImageViewDescriptor {
    pub label: Option<String>,
    pub image: vk::Image,
    pub view_type: vk::ImageViewType,
    pub format: TextureFormat,
    pub extent: Extent3D,
    pub mip_levels: u32,
    pub array_layers: u32,
    pub samples: SampleCount,
    pub usage: TextureUsages,
    /// Optional usage restriction for mutable-format plane views. This maps
    /// to `VkImageViewUsageCreateInfo` and must be a subset of `usage`.
    pub view_usage: Option<TextureUsages>,
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
        if self.extent.is_empty() {
            return Err(Error::Validation(
                "external image extent must be non-zero".into(),
            ));
        }
        if self.mip_levels == 0 || self.array_layers == 0 {
            return Err(Error::Validation(
                "external image mip and array-layer counts must be non-zero".into(),
            ));
        }
        if self.usage.is_empty() {
            return Err(Error::Validation(
                "external image usage must be non-empty".into(),
            ));
        }
        if self
            .view_usage
            .is_some_and(|view_usage| view_usage.is_empty() || !self.usage.contains(view_usage))
        {
            return Err(Error::Validation(
                "external image view usage must be non-empty and contained by image usage".into(),
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

    pub fn format(&self) -> TextureFormat {
        self.inner.descriptor.format
    }

    pub fn extent(&self) -> Extent3D {
        self.inner.descriptor.extent
    }

    pub fn usage(&self) -> TextureUsages {
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

    pub(crate) fn with_view_create_info<R>(
        &self,
        callback: impl FnOnce(&vk::ImageViewCreateInfo) -> R,
    ) -> R {
        with_image_view_create_info(&self.inner.descriptor, callback)
    }

    pub fn resource_binding(&self) -> ResourceBinding {
        ResourceBinding::raw_image(
            self.inner.descriptor.image,
            self.inner.descriptor.subresource_range,
        )
    }

    pub(crate) fn owner(&self) -> &Arc<DeviceOwner> {
        &self.inner.owner
    }

    /// Materializes a real Vulkan image view only for APIs such as dynamic
    /// rendering that consume a `VkImageView` handle.
    pub fn create_view(&self) -> Result<RetainedExternalImageView> {
        let view = with_image_view_create_info(&self.inner.descriptor, |create| unsafe {
            self.inner.owner.device.create_image_view(create, None)
        })
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

    pub fn format(&self) -> TextureFormat {
        self.inner.descriptor.format
    }

    pub fn extent(&self) -> Extent3D {
        self.inner.descriptor.extent
    }

    pub fn sample_count(&self) -> SampleCount {
        self.inner.descriptor.samples
    }

    pub fn usage(&self) -> TextureUsages {
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
        ResourceBinding::raw_image(
            self.inner.descriptor.image,
            self.inner.descriptor.subresource_range,
        )
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
        retain_external_image_for_owner(self.shared_owner(), descriptor, host_lease)
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

pub(crate) fn retain_external_image_for_owner<T>(
    owner: Arc<DeviceOwner>,
    descriptor: &ExternalImageViewDescriptor,
    host_lease: Arc<T>,
) -> Result<RetainedExternalImage>
where
    T: Any + Send + Sync,
{
    descriptor.validate()?;
    Ok(RetainedExternalImage {
        inner: Arc::new(RetainedExternalImageInner {
            owner,
            descriptor: descriptor.clone(),
            _host_lease: host_lease,
        }),
    })
}

fn image_view_create_info(descriptor: &ExternalImageViewDescriptor) -> vk::ImageViewCreateInfo {
    vk::ImageViewCreateInfo::builder()
        .image(descriptor.image)
        .view_type(descriptor.view_type)
        .format(descriptor.format.to_vk())
        .components(descriptor.components)
        .subresource_range(descriptor.subresource_range)
        .build()
}

fn with_image_view_create_info<R>(
    descriptor: &ExternalImageViewDescriptor,
    callback: impl FnOnce(&vk::ImageViewCreateInfo) -> R,
) -> R {
    let mut view_usage = descriptor.view_usage.map(|usage| {
        vk::ImageViewUsageCreateInfo::builder()
            .usage(usage.to_vk())
            .build()
    });
    if let Some(view_usage) = view_usage.as_mut() {
        let create = vk::ImageViewCreateInfo::builder()
            .image(descriptor.image)
            .view_type(descriptor.view_type)
            .format(descriptor.format.to_vk())
            .components(descriptor.components)
            .subresource_range(descriptor.subresource_range)
            .push_next(view_usage)
            .build();
        callback(&create)
    } else {
        let create = image_view_create_info(descriptor);
        callback(&create)
    }
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
            format: TextureFormat::R8Unorm,
            extent: Extent3D::new(1920, 1080, 1),
            mip_levels: 1,
            array_layers: 2,
            samples: SampleCount::One,
            usage: TextureUsages::SAMPLED,
            view_usage: None,
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

    #[test]
    fn plane_view_usage_must_be_a_non_empty_image_usage_subset() {
        let mut descriptor = descriptor();
        descriptor.usage = TextureUsages::SAMPLED | TextureUsages::VIDEO_DECODE_DESTINATION;
        descriptor.view_usage = Some(TextureUsages::SAMPLED);
        assert!(descriptor.validate().is_ok());
        descriptor.view_usage = Some(TextureUsages::COLOR_ATTACHMENT);
        assert!(descriptor.validate().is_err());
        descriptor.view_usage = Some(TextureUsages::empty());
        assert!(descriptor.validate().is_err());
    }
}

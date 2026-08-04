use std::fmt;
use std::sync::Arc;

use vulkanalia::{
    prelude::v1_4::*,
    vk::{self, KhrSwapchainExtensionDeviceCommands},
};

use super::{PresentMode, Surface, SurfaceCapabilities};
use crate::backend::DeviceOwner;
use crate::{
    Backend, BinarySemaphore, CompositeAlphaMode, Error, Extent2D, Features, Queue,
    ResourceBinding, Result, SurfaceFormat, SurfaceTransform, TextureFormat, TextureUsages,
};

#[derive(Clone, Copy, Debug)]
pub struct SurfaceConfigurationRequest<'a> {
    pub width: u32,
    pub height: u32,
    pub usage: TextureUsages,
    pub formats: &'a [SurfaceFormat],
    pub present_modes: &'a [PresentMode],
    pub composite_alpha: &'a [CompositeAlphaMode],
    pub pre_transforms: &'a [SurfaceTransform],
    pub desired_image_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceConfiguration {
    pub format: TextureFormat,
    pub color_space: crate::ColorSpace,
    pub extent: Extent2D,
    pub usage: TextureUsages,
    pub present_mode: PresentMode,
    pub composite_alpha: CompositeAlphaMode,
    pub pre_transform: SurfaceTransform,
    pub image_count: u32,
}

impl SurfaceConfiguration {
    /// Selects a concrete configuration using caller-ordered preferences.
    /// No format, present-mode, or alpha fallback is synthesized.
    pub fn choose(
        capabilities: &SurfaceCapabilities,
        features: Features,
        request: SurfaceConfigurationRequest<'_>,
    ) -> Result<Self> {
        if !capabilities.present_supported {
            return Err(Error::Validation(
                "selected graphics queue cannot present to this surface".into(),
            ));
        }
        if request.width == 0 || request.height == 0 {
            return Err(Error::Validation(
                "surface configuration extent must be non-empty".into(),
            ));
        }
        if request.usage.is_empty() || !capabilities.supported_usage.contains(request.usage) {
            return Err(Error::Validation(
                "surface image usage is empty or unsupported".into(),
            ));
        }
        let format = request
            .formats
            .iter()
            .find(|preferred| capabilities.formats.contains(preferred))
            .copied()
            .ok_or_else(|| {
                Error::Validation(
                    "surface has none of the requested format/color-space pairs".into(),
                )
            })?;
        let present_mode = capabilities
            .present_modes
            .choose(request.present_modes, features)
            .ok_or_else(|| Error::Validation("surface has no requested present mode".into()))?;
        let composite_alpha = request
            .composite_alpha
            .iter()
            .copied()
            .find(|mode| capabilities.supported_composite_alpha.contains(*mode))
            .ok_or_else(|| {
                Error::Validation("surface has no requested composite alpha mode".into())
            })?;
        let pre_transform = request
            .pre_transforms
            .iter()
            .copied()
            .find(|transform| capabilities.supported_transforms.contains(*transform))
            .ok_or_else(|| Error::Validation("surface has no requested pre-transform".into()))?;
        let extent = capabilities.current_extent.unwrap_or(Extent2D {
            width: request.width.clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: request.height.clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        });
        let image_count = request
            .desired_image_count
            .max(capabilities.min_image_count);
        if capabilities
            .max_image_count
            .is_some_and(|maximum| image_count > maximum)
        {
            return Err(Error::Validation(format!(
                "requested swapchain image count {image_count} exceeds the surface maximum"
            )));
        }
        Ok(Self {
            format: format.format,
            color_space: format.color_space,
            extent,
            usage: request.usage,
            present_mode,
            composite_alpha,
            pre_transform,
            image_count,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SwapchainDescriptor<'a> {
    pub label: Option<&'a str>,
    pub configuration: SurfaceConfiguration,
    pub old_swapchain: Option<&'a Swapchain>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentStatus {
    Optimal,
    Suboptimal,
}

#[derive(Clone, Copy, Debug)]
struct SwapchainImage {
    image: vk::Image,
    view: vk::ImageView,
}

pub struct Swapchain {
    owner: Arc<DeviceOwner>,
    surface: Surface,
    raw: vk::SwapchainKHR,
    images: Vec<SwapchainImage>,
    configuration: SurfaceConfiguration,
    label: Option<String>,
}

impl fmt::Debug for Swapchain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Swapchain")
            .field("raw", &self.raw)
            .field("label", &self.label)
            .field("configuration", &self.configuration)
            .field("image_count", &self.images.len())
            .finish_non_exhaustive()
    }
}

impl Swapchain {
    pub(crate) const fn raw(&self) -> vk::SwapchainKHR {
        self.raw
    }

    pub const fn configuration(&self) -> SurfaceConfiguration {
        self.configuration
    }

    pub fn image_count(&self) -> usize {
        self.images.len()
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn surface(&self) -> &Surface {
        &self.surface
    }

    /// Acquires one image, signalling a device-owned binary semaphore.
    ///
    /// # Safety
    ///
    /// `semaphore` must be unsignalled with no pending signal or wait operation.
    /// Calls for this swapchain must be externally synchronized.
    pub unsafe fn acquire_next_image(
        &self,
        timeout_ns: u64,
        semaphore: &BinarySemaphore,
    ) -> Result<AcquiredSurfaceTexture<'_>> {
        if !semaphore.belongs_to(&self.owner) {
            return Err(Error::Validation(
                "acquire semaphore was created by a different Device".into(),
            ));
        }
        unsafe { self.acquire_next_image_raw(timeout_ns, semaphore.raw(), vk::Fence::null()) }
    }

    /// Raw acquire interoperability accepting a semaphore or fence.
    ///
    /// # Safety
    ///
    /// At least one synchronization object must be non-null, belong to this
    /// device, be unsignalled, and remain live until acquisition completes.
    /// Calls for this swapchain must be externally synchronized.
    pub unsafe fn acquire_next_image_raw(
        &self,
        timeout_ns: u64,
        semaphore: vk::Semaphore,
        fence: vk::Fence,
    ) -> Result<AcquiredSurfaceTexture<'_>> {
        if semaphore == vk::Semaphore::null() && fence == vk::Fence::null() {
            return Err(Error::Validation(
                "swapchain acquisition requires a semaphore or fence".into(),
            ));
        }
        let (index, success) = unsafe {
            self.owner
                .device
                .acquire_next_image_khr(self.raw, timeout_ns, semaphore, fence)
        }
        .map_err(|source| Error::vulkan("vkAcquireNextImageKHR", source))?;
        let image = self.images.get(index as usize).ok_or_else(|| {
            Error::Validation("vkAcquireNextImageKHR returned an invalid image index".into())
        })?;
        Ok(AcquiredSurfaceTexture {
            swapchain: self,
            index,
            image: image.image,
            view: image.view,
            status: if success == vk::SuccessCode::SUBOPTIMAL_KHR {
                PresentStatus::Suboptimal
            } else {
                PresentStatus::Optimal
            },
        })
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }

    pub(crate) fn contains_index(&self, index: u32) -> bool {
        (index as usize) < self.images.len()
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            for image in &self.images {
                self.owner.device.destroy_image_view(image.view, None);
            }
            self.owner.device.destroy_swapchain_khr(self.raw, None);
        }
    }
}

#[derive(Debug)]
pub struct AcquiredSurfaceTexture<'a> {
    swapchain: &'a Swapchain,
    index: u32,
    image: vk::Image,
    view: vk::ImageView,
    status: PresentStatus,
}

impl AcquiredSurfaceTexture<'_> {
    pub const fn index(&self) -> u32 {
        self.index
    }

    pub(crate) const fn image(&self) -> vk::Image {
        self.image
    }

    pub(crate) const fn view(&self) -> vk::ImageView {
        self.view
    }

    pub const fn format(&self) -> TextureFormat {
        self.swapchain.configuration.format
    }

    pub const fn extent(&self) -> Extent2D {
        self.swapchain.configuration.extent
    }

    /// Returns the usages selected when this swapchain was configured.
    pub const fn usage(&self) -> TextureUsages {
        self.swapchain.configuration.usage
    }

    pub const fn status(&self) -> PresentStatus {
        self.status
    }

    /// Returns the complete color-image binding for this acquired surface slot.
    pub fn resource_binding(&self) -> ResourceBinding {
        ResourceBinding::raw_image(
            self.image,
            vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
        )
    }

    pub(crate) fn owner(&self) -> &Arc<DeviceOwner> {
        &self.swapchain.owner
    }

    /// Presents this acquired image and consumes its acquisition token.
    ///
    /// # Safety
    ///
    /// Every wait semaphore must have a pending signal operation for rendering
    /// that writes this image and must not be consumed by another wait.
    pub unsafe fn present(
        self,
        queue: &Queue,
        wait_semaphores: &[&BinarySemaphore],
    ) -> Result<PresentStatus> {
        unsafe { queue.present(self.swapchain, self.index, wait_semaphores) }
    }
}

impl Backend {
    pub fn create_swapchain(
        &self,
        surface: &Surface,
        descriptor: &SwapchainDescriptor<'_>,
    ) -> Result<Swapchain> {
        let owner = self.shared_owner();
        if !surface.belongs_to(owner.instance_owner()) {
            return Err(Error::Validation(
                "surface was created by a different Instance".into(),
            ));
        }
        if let Some(old) = descriptor.old_swapchain
            && !old.belongs_to(&owner)
        {
            return Err(Error::Validation(
                "old swapchain was created by a different Device".into(),
            ));
        }
        let capabilities = SurfaceCapabilities::query(
            owner.instance_owner(),
            owner.physical_device(),
            self.device_info().queues.graphics,
            surface,
        )?;
        validate_configuration(&capabilities, self.features(), descriptor.configuration)?;
        create_swapchain(owner, surface.clone(), descriptor, self.features())
    }
}

fn validate_configuration(
    capabilities: &SurfaceCapabilities,
    features: Features,
    configuration: SurfaceConfiguration,
) -> Result<()> {
    if !capabilities.present_supported {
        return Err(Error::Validation(
            "selected graphics queue cannot present to this surface".into(),
        ));
    }
    if !capabilities.formats.contains(&SurfaceFormat {
        format: configuration.format,
        color_space: configuration.color_space,
    }) {
        return Err(Error::Validation(
            "swapchain format/color-space pair is unsupported".into(),
        ));
    }
    if !capabilities
        .present_modes
        .supports(configuration.present_mode, features)
    {
        return Err(Error::Validation(
            "swapchain present mode is unsupported or was not enabled".into(),
        ));
    }
    if features.contains(Features::PRESENT_ID2) && !capabilities.present_id2_supported {
        return Err(Error::Validation(
            "device enables present-id2 but the surface does not support it".into(),
        ));
    }
    if features.contains(Features::PRESENT_WAIT2) && !capabilities.present_wait2_supported {
        return Err(Error::Validation(
            "device enables present-wait2 but the surface does not support it".into(),
        ));
    }
    if configuration.extent.width == 0
        || configuration.extent.height == 0
        || configuration.extent.width < capabilities.min_image_extent.width
        || configuration.extent.height < capabilities.min_image_extent.height
        || configuration.extent.width > capabilities.max_image_extent.width
        || configuration.extent.height > capabilities.max_image_extent.height
        || capabilities
            .current_extent
            .is_some_and(|current| current != configuration.extent)
    {
        return Err(Error::Validation(
            "swapchain extent violates the surface capability range".into(),
        ));
    }
    if configuration.image_count < capabilities.min_image_count
        || capabilities
            .max_image_count
            .is_some_and(|maximum| configuration.image_count > maximum)
    {
        return Err(Error::Validation(
            "swapchain image count violates the surface capability range".into(),
        ));
    }
    if configuration.usage.is_empty() || !capabilities.supported_usage.contains(configuration.usage)
    {
        return Err(Error::Validation(
            "swapchain image usage is empty or unsupported".into(),
        ));
    }
    if !capabilities
        .supported_composite_alpha
        .contains(configuration.composite_alpha)
        || !capabilities
            .supported_transforms
            .contains(configuration.pre_transform)
    {
        return Err(Error::Validation(
            "swapchain alpha mode or transform is unsupported".into(),
        ));
    }
    Ok(())
}

fn create_swapchain(
    owner: Arc<DeviceOwner>,
    surface: Surface,
    descriptor: &SwapchainDescriptor<'_>,
    features: Features,
) -> Result<Swapchain> {
    let configuration = descriptor.configuration;
    let mut flags = vk::SwapchainCreateFlagsKHR::empty();
    if features.contains(Features::PRESENT_ID2) {
        flags |= vk::SwapchainCreateFlagsKHR::PRESENT_ID_2;
    }
    if features.contains(Features::PRESENT_WAIT2) {
        flags |= vk::SwapchainCreateFlagsKHR::PRESENT_WAIT_2;
    }
    let create = vk::SwapchainCreateInfoKHR::builder()
        .flags(flags)
        .surface(surface.raw())
        .min_image_count(configuration.image_count)
        .image_format(configuration.format.to_vk())
        .image_color_space(configuration.color_space.to_vk())
        .image_extent(configuration.extent.to_vk())
        .image_array_layers(1)
        .image_usage(configuration.usage.to_vk())
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(configuration.pre_transform.to_vk())
        .composite_alpha(configuration.composite_alpha.to_vk())
        .present_mode(configuration.present_mode.as_vk())
        .clipped(true)
        .old_swapchain(
            descriptor
                .old_swapchain
                .map_or(vk::SwapchainKHR::null(), Swapchain::raw),
        );
    let raw = unsafe { owner.device.create_swapchain_khr(&create, None) }
        .map_err(|source| Error::vulkan("vkCreateSwapchainKHR", source))?;
    let result = create_swapchain_images(&owner, raw, configuration.format.to_vk());
    match result {
        Ok(images) => Ok(Swapchain {
            owner,
            surface,
            raw,
            images,
            configuration,
            label: descriptor.label.map(str::to_owned),
        }),
        Err(error) => {
            unsafe { owner.device.destroy_swapchain_khr(raw, None) };
            Err(error)
        }
    }
}

fn create_swapchain_images(
    owner: &Arc<DeviceOwner>,
    swapchain: vk::SwapchainKHR,
    format: vk::Format,
) -> Result<Vec<SwapchainImage>> {
    let images = unsafe { owner.device.get_swapchain_images_khr(swapchain) }
        .map_err(|source| Error::vulkan("vkGetSwapchainImagesKHR", source))?;
    if images.is_empty() {
        return Err(Error::Validation(
            "Vulkan returned no swapchain images".into(),
        ));
    }
    let mut created = Vec::with_capacity(images.len());
    for image in images {
        let create = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        match unsafe { owner.device.create_image_view(&create, None) } {
            Ok(view) => created.push(SwapchainImage { image, view }),
            Err(source) => {
                for created in created {
                    unsafe { owner.device.destroy_image_view(created.view, None) };
                }
                return Err(Error::vulkan("vkCreateImageView(swapchain)", source));
            }
        }
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SurfacePresentCapabilities;

    fn capabilities() -> SurfaceCapabilities {
        SurfaceCapabilities {
            present_supported: true,
            min_image_count: 2,
            max_image_count: Some(4),
            current_extent: None,
            min_image_extent: Extent2D::new(16, 16),
            max_image_extent: Extent2D::new(4096, 4096),
            max_image_array_layers: 1,
            supported_transforms: crate::SurfaceTransforms::from_vk(
                vk::SurfaceTransformFlagsKHR::IDENTITY,
            ),
            current_transform: SurfaceTransform::Identity,
            supported_composite_alpha: crate::CompositeAlphaModes::from_vk(
                vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            ),
            supported_usage: TextureUsages::COLOR_ATTACHMENT,
            formats: vec![SurfaceFormat::new(
                TextureFormat::Bgra8Unorm,
                crate::ColorSpace::SrgbNonlinear,
            )],
            present_modes: SurfacePresentCapabilities::from_vk(&[
                vk::PresentModeKHR::FIFO,
                vk::PresentModeKHR::FIFO_LATEST_READY,
            ]),
            present_id2_supported: true,
            present_wait2_supported: true,
        }
    }

    #[test]
    fn configuration_preferences_are_explicit_and_extent_is_clamped() {
        let formats = [SurfaceFormat::new(
            TextureFormat::Bgra8Unorm,
            crate::ColorSpace::SrgbNonlinear,
        )];
        let modes = [PresentMode::FifoLatestReady, PresentMode::Fifo];
        let alpha = [CompositeAlphaMode::PreMultiplied];
        let configuration = SurfaceConfiguration::choose(
            &capabilities(),
            Features::FIFO_LATEST_READY,
            SurfaceConfigurationRequest {
                width: 8,
                height: 8192,
                usage: TextureUsages::COLOR_ATTACHMENT,
                formats: &formats,
                present_modes: &modes,
                composite_alpha: &alpha,
                pre_transforms: &[SurfaceTransform::Identity],
                desired_image_count: 3,
            },
        )
        .unwrap();
        assert_eq!(configuration.extent, Extent2D::new(16, 4096));
        assert_eq!(configuration.present_mode, PresentMode::FifoLatestReady);
        assert_eq!(configuration.image_count, 3);
    }

    #[test]
    fn missing_preference_is_not_replaced_by_an_implicit_fallback() {
        let formats = [SurfaceFormat::new(
            TextureFormat::Rgba8Unorm,
            crate::ColorSpace::SrgbNonlinear,
        )];
        assert!(
            SurfaceConfiguration::choose(
                &capabilities(),
                Features::FIFO_LATEST_READY,
                SurfaceConfigurationRequest {
                    width: 1280,
                    height: 720,
                    usage: TextureUsages::COLOR_ATTACHMENT,
                    formats: &formats,
                    present_modes: &[PresentMode::Fifo],
                    composite_alpha: &[CompositeAlphaMode::PreMultiplied],
                    pre_transforms: &[SurfaceTransform::Identity],
                    desired_image_count: 3,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn requested_identity_transform_is_not_replaced_by_the_surface_rotation() {
        let mut capabilities = capabilities();
        capabilities.supported_transforms = crate::SurfaceTransforms::from_vk(
            vk::SurfaceTransformFlagsKHR::IDENTITY | vk::SurfaceTransformFlagsKHR::ROTATE_180,
        );
        capabilities.current_transform = SurfaceTransform::Rotate180;
        let configuration = SurfaceConfiguration::choose(
            &capabilities,
            Features::FIFO_LATEST_READY,
            SurfaceConfigurationRequest {
                width: 1280,
                height: 720,
                usage: TextureUsages::COLOR_ATTACHMENT,
                formats: &capabilities.formats,
                present_modes: &[PresentMode::FifoLatestReady],
                composite_alpha: &[CompositeAlphaMode::PreMultiplied],
                pre_transforms: &[SurfaceTransform::Identity],
                desired_image_count: 3,
            },
        )
        .unwrap();
        assert_eq!(configuration.pre_transform, SurfaceTransform::Identity);
    }
}

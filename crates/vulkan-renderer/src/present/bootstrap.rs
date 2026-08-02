use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use super::{
    PresentMode, Surface, SurfaceCapabilities, SurfaceConfiguration, SurfaceConfigurationRequest,
    Swapchain, SwapchainDescriptor,
};
use crate::{
    Adapter, AdapterSelector, Backend, BackendProfile, CompositeAlphaMode, DeviceDescriptor, Error,
    Extent2D, Features, Instance, InstanceDescriptor, MemoryAllocator, MemoryAllocatorConfig,
    PipelineBinaryArchiveCache, PipelineBinaryCacheIdentity, PowerPreference, Result, SampleCounts,
    SurfaceFormat, SurfaceTransform, TextureUsages, UploadBelt, UploadBeltDescriptor,
    VideoDecodeDevice, VideoDecodeRequirements,
};

/// Explicit adapter-selection policy for a renderer-owned presentation root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationAdapterRequest {
    pub power_preference: PowerPreference,
    pub force_fallback_adapter: bool,
    pub selector: Option<AdapterSelector>,
}

/// Explicit swapchain image-count policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationImageCount {
    /// Request exactly this many images, rejecting an unsupported count.
    Exact(u32),
    /// Request the surface minimum plus this many retained images, clamped to
    /// the surface maximum as an explicitly selected policy.
    MinimumPlus(u32),
}

impl PresentationImageCount {
    fn resolve(self, min_image_count: u32, max_image_count: Option<u32>) -> Result<u32> {
        match self {
            Self::Exact(0) => Err(Error::Validation(
                "exact presentation image count must be non-zero".into(),
            )),
            Self::Exact(count) => Ok(count),
            Self::MinimumPlus(extra) => Ok(min_image_count
                .max(1)
                .saturating_add(extra)
                .min(max_image_count.unwrap_or(u32::MAX))),
        }
    }
}

/// Ordered, typed swapchain configuration choices for one presentation root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationSurfaceConfigurationDescriptor {
    pub usage: TextureUsages,
    pub formats: Vec<SurfaceFormat>,
    pub present_modes: Vec<PresentMode>,
    pub composite_alpha: Vec<CompositeAlphaMode>,
    /// When set, the surface's current transform is tried before the explicit
    /// transform preferences.
    pub prefer_current_transform: bool,
    pub pre_transforms: Vec<SurfaceTransform>,
    pub image_count: PresentationImageCount,
}

impl PresentationSurfaceConfigurationDescriptor {
    fn choose(
        &self,
        capabilities: &SurfaceCapabilities,
        features: Features,
        requested_extent: Extent2D,
    ) -> Result<SurfaceConfiguration> {
        if self.formats.is_empty()
            || self.present_modes.is_empty()
            || self.composite_alpha.is_empty()
            || (!self.prefer_current_transform && self.pre_transforms.is_empty())
        {
            return Err(Error::Validation(
                "presentation surface preferences must explicitly select formats, present modes, alpha, and transforms"
                    .into(),
            ));
        }
        let pre_transforms = self.preferred_transforms(capabilities.current_transform);
        SurfaceConfiguration::choose(
            capabilities,
            features,
            SurfaceConfigurationRequest {
                width: requested_extent.width,
                height: requested_extent.height,
                usage: self.usage,
                formats: &self.formats,
                present_modes: &self.present_modes,
                composite_alpha: &self.composite_alpha,
                pre_transforms: &pre_transforms,
                desired_image_count: self
                    .image_count
                    .resolve(capabilities.min_image_count, capabilities.max_image_count)?,
            },
        )
    }

    fn preferred_transforms(&self, current: SurfaceTransform) -> Vec<SurfaceTransform> {
        let mut transforms = Vec::with_capacity(
            self.pre_transforms.len() + usize::from(self.prefer_current_transform),
        );
        if self.prefer_current_transform {
            transforms.push(current);
        }
        for transform in &self.pre_transforms {
            if !transforms.contains(transform) {
                transforms.push(*transform);
            }
        }
        transforms
    }
}

/// Cold descriptor for a complete shared presentation ownership root.
///
/// Every adapter, device, swapchain, cache, sampling, and image-count choice
/// is explicit. This does not select a single-pass or offscreen frame path;
/// consumers choose that independently through the presentation path plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationBootstrapDescriptor {
    pub label: String,
    pub profile: BackendProfile,
    pub adapter: PresentationAdapterRequest,
    pub requested_extent: Extent2D,
    pub required_features: Features,
    /// Features enabled only when the selected adapter advertises them.
    pub optional_features: Features,
    pub required_color_samples: SampleCounts,
    pub video_decode: Option<VideoDecodeRequirements>,
    pub surface: PresentationSurfaceConfigurationDescriptor,
    pub pipeline_binary_cache_root: PathBuf,
}

/// Typed owner for a presentation surface, selected adapter, logical device,
/// swapchain, retained allocator/upload state, pipeline cache, and optional
/// Vulkan Video decode endpoint.
pub struct PresentationBootstrap {
    pub instance: Instance,
    pub adapter: Adapter,
    pub surface: Surface,
    pub device: Backend,
    pub queue: crate::Queue,
    pub swapchain: Swapchain,
    pub allocator: MemoryAllocator,
    pub upload_belt: UploadBelt,
    pub pipeline_binary_cache: PipelineBinaryArchiveCache,
    pub video_decode: Option<VideoDecodeDevice>,
}

impl fmt::Debug for PresentationBootstrap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PresentationBootstrap")
            .field("adapter", &self.adapter)
            .field("swapchain", &self.swapchain)
            .field("has_video_decode", &self.video_decode.is_some())
            .finish_non_exhaustive()
    }
}

impl PresentationBootstrap {
    /// Creates all retained Vulkan presentation ownership from explicit typed
    /// policy. The host window/display lease is retained by Surface.
    pub fn create<T>(host: Arc<T>, descriptor: PresentationBootstrapDescriptor) -> Result<Self>
    where
        T: HasDisplayHandle + HasWindowHandle + Send + Sync + 'static,
    {
        validate_descriptor(&descriptor)?;
        let instance = Instance::new(InstanceDescriptor::for_window(
            descriptor.profile,
            host.as_ref(),
        )?)?;
        let surface = instance.create_surface(host)?;
        let adapter = instance.request_adapter(crate::RequestAdapterOptions {
            power_preference: descriptor.adapter.power_preference,
            force_fallback_adapter: descriptor.adapter.force_fallback_adapter,
            compatible_surface: Some(&surface),
            selector: descriptor.adapter.selector.as_ref(),
        })?;
        if !adapter
            .info()
            .properties
            .framebuffer_color_sample_counts
            .contains(descriptor.required_color_samples)
        {
            return Err(Error::Validation(format!(
                "selected adapter {:?} does not support required {:?} color rasterization for {}",
                adapter.info().name,
                descriptor.required_color_samples,
                descriptor.label,
            )));
        }
        let required_features =
            descriptor.required_features | (descriptor.optional_features & adapter.features());
        let (device, queue) = adapter.request_device(DeviceDescriptor {
            label: Some(format!("{}-device", descriptor.label)),
            required_features,
            video_decode: descriptor.video_decode,
            ..DeviceDescriptor::default()
        })?;
        let video_decode = descriptor
            .video_decode
            .map(|_| {
                device.video_decode_device().ok_or_else(|| {
                    Error::Validation(format!(
                        "presentation device for {} omitted the requested Vulkan Video endpoint",
                        descriptor.label
                    ))
                })
            })
            .transpose()?;
        let capabilities = adapter.surface_capabilities(&surface)?;
        let configuration = descriptor.surface.choose(
            &capabilities,
            device.features(),
            descriptor.requested_extent,
        )?;
        let swapchain = device.create_swapchain(
            &surface,
            &SwapchainDescriptor {
                label: Some(&descriptor.label),
                configuration,
                old_swapchain: None,
            },
        )?;
        let allocator = device.create_memory_allocator(MemoryAllocatorConfig::default())?;
        let upload_belt = device.create_upload_belt(&allocator, UploadBeltDescriptor::default())?;
        let pipeline_binary_cache = PipelineBinaryArchiveCache::new(
            descriptor.pipeline_binary_cache_root,
            PipelineBinaryCacheIdentity::from_device(&device),
        );
        Ok(Self {
            instance,
            adapter,
            surface,
            device,
            queue,
            swapchain,
            allocator,
            upload_belt,
            pipeline_binary_cache,
            video_decode,
        })
    }
}

fn validate_descriptor(descriptor: &PresentationBootstrapDescriptor) -> Result<()> {
    if descriptor.label.is_empty() {
        return Err(Error::Validation(
            "presentation bootstrap label must not be empty".into(),
        ));
    }
    if descriptor.requested_extent.is_empty() {
        return Err(Error::Validation(
            "presentation bootstrap requested extent must be non-empty".into(),
        ));
    }
    if descriptor.required_color_samples.is_empty() {
        return Err(Error::Validation(
            "presentation bootstrap requires at least one color sample count".into(),
        ));
    }
    if descriptor.pipeline_binary_cache_root.as_os_str().is_empty() {
        return Err(Error::Validation(
            "presentation bootstrap pipeline binary cache root must not be empty".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_plus_image_count_clamps_to_surface_maximum() {
        assert_eq!(
            PresentationImageCount::MinimumPlus(2)
                .resolve(1, Some(2))
                .unwrap(),
            2
        );
        assert!(
            PresentationImageCount::Exact(0)
                .resolve(1, Some(2))
                .is_err()
        );
    }

    #[test]
    fn current_transform_precedes_and_deduplicates_explicit_preferences() {
        let descriptor = PresentationSurfaceConfigurationDescriptor {
            usage: TextureUsages::COLOR_ATTACHMENT,
            formats: Vec::new(),
            present_modes: Vec::new(),
            composite_alpha: Vec::new(),
            prefer_current_transform: true,
            pre_transforms: vec![SurfaceTransform::Rotate90, SurfaceTransform::Identity],
            image_count: PresentationImageCount::Exact(2),
        };
        assert_eq!(
            descriptor.preferred_transforms(SurfaceTransform::Rotate90),
            vec![SurfaceTransform::Rotate90, SurfaceTransform::Identity]
        );
    }
}

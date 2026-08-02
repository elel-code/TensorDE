//! Gilder presentation policy lowered into shared renderer descriptors.

mod adapter_policy;

use std::path::PathBuf;

use adapter_policy::SharedAdapterPolicy;
use vulkan_renderer::{
    BackendProfile, ColorSpace, CompositeAlphaMode, Extent2D, Features, PresentMode,
    PresentationAdapterRequest, PresentationBootstrapDescriptor, PresentationImageCount,
    PresentationSurfaceConfigurationDescriptor, SampleCounts, SurfaceFormat, SurfaceTransform,
    TextureFormat, TextureUsages, VideoDecodeRequirements,
};

pub(in crate::renderer::native_vulkan) fn gilder_presentation_bootstrap_descriptor(
    label: &str,
    requested_extent: (u32, u32),
    required_features: Features,
    optional_features: Features,
    required_color_samples: SampleCounts,
    video_decode: Option<VideoDecodeRequirements>,
) -> Result<PresentationBootstrapDescriptor, String> {
    let policy = SharedAdapterPolicy::from_environment()?;
    Ok(PresentationBootstrapDescriptor {
        label: label.into(),
        profile: BackendProfile::Roadmap2026,
        adapter: PresentationAdapterRequest {
            power_preference: policy.preference,
            force_fallback_adapter: false,
            selector: policy.selector,
        },
        requested_extent: Extent2D::new(requested_extent.0.max(1), requested_extent.1.max(1)),
        required_features,
        optional_features,
        required_color_samples,
        video_decode,
        surface: PresentationSurfaceConfigurationDescriptor {
            usage: TextureUsages::COLOR_ATTACHMENT,
            formats: vec![
                SurfaceFormat::new(TextureFormat::Bgra8Unorm, ColorSpace::SrgbNonlinear),
                SurfaceFormat::new(TextureFormat::Bgra8Srgb, ColorSpace::SrgbNonlinear),
                SurfaceFormat::new(TextureFormat::Rgba8Unorm, ColorSpace::SrgbNonlinear),
            ],
            present_modes: vec![PresentMode::FifoLatestReady],
            composite_alpha: vec![
                CompositeAlphaMode::PreMultiplied,
                CompositeAlphaMode::Opaque,
                CompositeAlphaMode::PostMultiplied,
                CompositeAlphaMode::Inherit,
            ],
            prefer_current_transform: true,
            pre_transforms: vec![SurfaceTransform::Identity],
            image_count: PresentationImageCount::MinimumPlus(2),
        },
        pipeline_binary_cache_root: pipeline_binary_cache_root()?,
    })
}

fn pipeline_binary_cache_root() -> Result<PathBuf, String> {
    const CACHE_DIRECTORY_ENV: &str = "GILDER_PIPELINE_BINARY_CACHE_DIR";

    if let Some(root) = std::env::var_os(CACHE_DIRECTORY_ENV) {
        if root.is_empty() {
            return Err(format!("{CACHE_DIRECTORY_ENV} cannot be empty"));
        }
        return Ok(PathBuf::from(root));
    }
    if let Some(root) = std::env::var_os("XDG_CACHE_HOME")
        && !root.is_empty()
    {
        return Ok(PathBuf::from(root)
            .join("gilder")
            .join("vulkan-pipeline-binaries"));
    }
    if let Some(home) = std::env::var_os("HOME")
        && !home.is_empty()
    {
        return Ok(PathBuf::from(home)
            .join(".cache")
            .join("gilder")
            .join("vulkan-pipeline-binaries"));
    }
    Err(format!(
        "pipeline binary persistence requires {CACHE_DIRECTORY_ENV}, XDG_CACHE_HOME, or HOME"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_has_product_and_payload_scope() {
        assert_eq!(
            PathBuf::from("/tmp/example-home")
                .join(".cache")
                .join("gilder")
                .join("vulkan-pipeline-binaries"),
            PathBuf::from("/tmp/example-home/.cache/gilder/vulkan-pipeline-binaries")
        );
    }
}

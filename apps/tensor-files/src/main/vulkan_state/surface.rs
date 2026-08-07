use vulkan_renderer::{
    Adapter, BinarySemaphore, BinarySemaphoreDescriptor, ColorSpace, CompositeAlphaMode, Device,
    PresentMode, Surface, SurfaceConfiguration, SurfaceConfigurationRequest, SurfaceFormat,
    SurfaceTransform, TextureFormat, TextureUsages,
};

use crate::windowing::PhysicalSize;

pub(super) fn choose_surface_configuration(
    adapter: &Adapter,
    surface: &Surface,
    features: vulkan_renderer::Features,
    size: PhysicalSize<u32>,
) -> Result<SurfaceConfiguration, String> {
    let capabilities = adapter
        .surface_capabilities(surface)
        .map_err(|error| format!("query Vulkan surface capabilities: {error}"))?;
    let desired_image_count =
        preferred_image_count(capabilities.min_image_count, capabilities.max_image_count);
    let formats = [
        SurfaceFormat::new(TextureFormat::Bgra8Unorm, ColorSpace::SrgbNonlinear),
        SurfaceFormat::new(TextureFormat::Bgra8Srgb, ColorSpace::SrgbNonlinear),
        SurfaceFormat::new(TextureFormat::Rgba8Unorm, ColorSpace::SrgbNonlinear),
    ];
    SurfaceConfiguration::choose(
        &capabilities,
        features,
        SurfaceConfigurationRequest {
            width: size.width.max(1),
            height: size.height.max(1),
            usage: TextureUsages::COLOR_ATTACHMENT
                | (capabilities.supported_usage & TextureUsages::COPY_SOURCE),
            formats: &formats,
            present_modes: &[PresentMode::FifoLatestReady, PresentMode::Fifo],
            composite_alpha: &[
                CompositeAlphaMode::PreMultiplied,
                CompositeAlphaMode::PostMultiplied,
                CompositeAlphaMode::Opaque,
                CompositeAlphaMode::Inherit,
            ],
            pre_transforms: &[SurfaceTransform::Identity],
            desired_image_count,
        },
    )
    .map_err(|error| format!("choose Vulkan surface configuration: {error}"))
}

pub(super) fn create_present_semaphores(
    device: &Device,
    image_count: usize,
) -> Result<Vec<BinarySemaphore>, String> {
    (0..image_count)
        .map(|index| {
            device.create_binary_semaphore(&BinarySemaphoreDescriptor {
                label: Some(format!("tensor-files-vulkan-present-{index}")),
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("create Vulkan present semaphores: {error}"))
}

fn preferred_image_count(minimum: u32, maximum: Option<u32>) -> u32 {
    maximum.map_or_else(
        || minimum.saturating_add(1),
        |maximum| minimum.saturating_add(1).min(maximum),
    )
}

#[cfg(test)]
mod tests {
    use super::preferred_image_count;

    #[test]
    fn image_count_prefers_one_image_beyond_the_surface_minimum() {
        assert_eq!(preferred_image_count(2, None), 3);
        assert_eq!(preferred_image_count(2, Some(4)), 3);
        assert_eq!(preferred_image_count(2, Some(2)), 2);
        assert_eq!(preferred_image_count(u32::MAX, None), u32::MAX);
    }
}

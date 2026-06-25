use std::time::Duration;

use crate::renderer::StaticWallpaperPlan;

use super::static_image_upload::native_vulkan_static_background_clear_color;
use super::{
    NativeVulkanError, NativeVulkanOptions,
    NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions,
    NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot,
    run_native_vulkan_vulkanalia_scene_lite_sampled_image_present,
};

pub fn run_static_image_vulkanalia(
    mut options: NativeVulkanOptions,
    duration: Duration,
    plan: StaticWallpaperPlan,
) -> Result<NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot, NativeVulkanError> {
    if options.host.output_name.is_none() {
        options.host.output_name = Some(plan.output_name.clone());
    }
    let clear_color = native_vulkan_static_background_clear_color(plan.background.as_deref());

    run_native_vulkan_vulkanalia_scene_lite_sampled_image_present(
        NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions {
            host: options.host,
            wait_configure_roundtrips: options.wait_configure_roundtrips,
            duration,
            target_max_fps: options.target_max_fps,
            source: plan.source,
            clear_color,
            fit: Some(plan.fit),
            geometry: None,
        },
    )
    .map_err(NativeVulkanError::StaticImage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::FitMode;
    use crate::renderer::native_wayland::NativeWaylandHostOptions;

    #[test]
    fn vulkanalia_static_no_longer_rejects_tile_before_runtime_setup() {
        let err = run_static_image_vulkanalia(
            NativeVulkanOptions {
                host: NativeWaylandHostOptions::default(),
                ..Default::default()
            },
            Duration::ZERO,
            StaticWallpaperPlan {
                output_name: "HDMI-A-1".to_owned(),
                source: "missing.png".into(),
                fit: FitMode::Tile,
                background: Some("#000000".to_owned()),
            },
        )
        .unwrap_err();

        assert!(!err.to_string().contains("does not yet support tile"));
    }
}

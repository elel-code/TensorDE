use std::time::Duration;

use crate::core::FitMode;
use crate::renderer::StaticWallpaperPlan;

use super::super::{
    NativeVulkanClearColor, NativeVulkanError, NativeVulkanOptions,
    NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
    NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot,
    native_vulkan_vulkanalia_configure_scene_sampled_image_allocator,
    run_native_vulkan_vulkanalia_scene_sampled_image_present,
};

pub fn run_static_image(
    mut options: NativeVulkanOptions,
    duration: Duration,
    plan: StaticWallpaperPlan,
) -> Result<NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot, NativeVulkanError> {
    native_vulkan_vulkanalia_configure_scene_sampled_image_allocator();

    if !native_vulkan_static_source_is_gtex(&plan.source) {
        return Err(NativeVulkanError::StaticImage(format!(
            "native static image runtime requires a .gtex BC7 source {}; runtime PNG/JPG decoding is disabled",
            plan.source.display()
        )));
    }
    if options.host.output_name.is_none() {
        options.host.output_name = Some(plan.output_name.clone());
    }
    let clear_color = native_vulkan_static_background_clear_color(plan.background.as_deref());
    let source = plan.source.clone();
    let fit = plan.fit;

    run_native_vulkan_vulkanalia_scene_sampled_image_present(
        NativeVulkanVulkanaliaSceneSampledImagePresentOptions {
            host: options.host,
            wait_configure_roundtrips: options.wait_configure_roundtrips,
            duration,
            target_max_fps: options.target_max_fps,
            source,
            clear_color,
            fit: Some(fit),
            scene_size: None,
            scene_fit: FitMode::Cover,
            solid_geometry: None,
            geometry: None,
            dynamic_solid_geometry: None,
            dynamic_geometry: None,
        },
    )
    .map_err(NativeVulkanError::StaticImage)
}

pub fn run_static_image_vulkanalia(
    options: NativeVulkanOptions,
    duration: Duration,
    plan: StaticWallpaperPlan,
) -> Result<NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot, NativeVulkanError> {
    run_static_image(options, duration, plan)
}

fn native_vulkan_static_background_clear_color(background: Option<&str>) -> NativeVulkanClearColor {
    let Some(hex) = background
        .and_then(|value| value.trim().strip_prefix('#'))
        .filter(|hex| hex.len() == 6)
    else {
        return NativeVulkanClearColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
    };
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    NativeVulkanClearColor {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

fn native_vulkan_static_source_is_gtex(source: &std::path::Path) -> bool {
    source
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gtex"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::FitMode;
    use crate::renderer::native_wayland::NativeWaylandHostOptions;

    #[test]
    fn vulkanalia_static_no_longer_rejects_tile_before_runtime_setup() {
        let err = run_static_image(
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

    #[test]
    fn vulkanalia_static_rejects_runtime_png_decode() {
        let err = run_static_image(
            NativeVulkanOptions {
                host: NativeWaylandHostOptions::default(),
                ..Default::default()
            },
            Duration::ZERO,
            StaticWallpaperPlan {
                output_name: "HDMI-A-1".to_owned(),
                source: "wallpaper.png".into(),
                fit: FitMode::Cover,
                background: None,
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("requires a .gtex BC7 source"));
        assert!(
            err.to_string()
                .contains("runtime PNG/JPG decoding is disabled")
        );
    }

    #[test]
    fn static_background_clear_color_parses_hex_or_defaults_black() {
        let color = native_vulkan_static_background_clear_color(Some("#336699"));
        assert_eq!(color.r, 0x33 as f32 / 255.0);
        assert_eq!(color.g, 0x66 as f32 / 255.0);
        assert_eq!(color.b, 0x99 as f32 / 255.0);
        assert_eq!(color.a, 1.0);

        assert_eq!(
            native_vulkan_static_background_clear_color(None),
            NativeVulkanClearColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }
        );
    }
}

use std::time::Duration;

use serde::Serialize;

use crate::renderer::SceneLiteWallpaperPlan;

use super::super::present::render_item::native_vulkan_scene_lite_item;
use super::super::present::render_plan::{
    native_vulkan_clear_color_from_hex, native_vulkan_render_item_clear_color,
};
use super::super::{
    NativeVulkanError, NativeVulkanOptions, NativeVulkanVulkanaliaClearPresentSnapshot,
    NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions,
    NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot,
    NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions,
    NativeVulkanVulkanaliaSceneLiteSolidQuadPresentSnapshot, run_clear,
    run_native_vulkan_vulkanalia_scene_lite_sampled_image_present,
    run_native_vulkan_vulkanalia_scene_lite_solid_quad_present,
};
use super::lite_runtime::{
    NativeVulkanSceneLiteRuntimeSnapshot, native_vulkan_scene_lite_runtime_snapshot,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "scene_present_route",
    content = "snapshot",
    rename_all = "kebab-case"
)]
pub enum NativeVulkanSceneLitePresentSnapshot {
    Clear {
        runtime: NativeVulkanSceneLiteRuntimeSnapshot,
        present: NativeVulkanVulkanaliaClearPresentSnapshot,
    },
    SolidQuad {
        runtime: NativeVulkanSceneLiteRuntimeSnapshot,
        present: NativeVulkanVulkanaliaSceneLiteSolidQuadPresentSnapshot,
    },
    SampledImage {
        runtime: NativeVulkanSceneLiteRuntimeSnapshot,
        present: NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeVulkanSceneLitePresentRouteKind {
    Clear,
    SolidQuad,
    SampledImage,
}

pub fn run_scene_lite(
    mut options: NativeVulkanOptions,
    duration: Duration,
    plan: SceneLiteWallpaperPlan,
) -> Result<NativeVulkanSceneLitePresentSnapshot, NativeVulkanError> {
    if options.host.output_name.is_none() {
        options.host.output_name = Some(plan.output_name.clone());
    }
    let target_max_fps = options.target_max_fps.or(plan.target_max_fps);
    options.target_max_fps = target_max_fps;
    let render_item = native_vulkan_scene_lite_item(&plan);
    options.clear_color = native_vulkan_render_item_clear_color(&render_item, options.clear_color);
    let runtime = native_vulkan_scene_lite_runtime_snapshot(&render_item).ok_or_else(|| {
        NativeVulkanError::SceneLite("scene-lite runtime snapshot is unavailable".to_owned())
    })?;
    if let Some(color) = runtime
        .draw_pass_background_clear_color
        .as_deref()
        .and_then(native_vulkan_clear_color_from_hex)
    {
        options.clear_color = color;
    }
    match native_vulkan_scene_lite_present_route(&runtime)? {
        NativeVulkanSceneLitePresentRouteKind::Clear => {
            let color = runtime
                .draw_pass_fast_clear_color
                .as_deref()
                .and_then(native_vulkan_clear_color_from_hex)
                .ok_or_else(|| {
                    NativeVulkanError::SceneLite(
                        "scene-lite fast-clear draw plan has no valid #rrggbb color".to_owned(),
                    )
                })?;
            options.clear_color = color;
            run_clear(options, duration)
                .map(|present| NativeVulkanSceneLitePresentSnapshot::Clear { runtime, present })
        }
        NativeVulkanSceneLitePresentRouteKind::SolidQuad => {
            let geometry = runtime
                .vulkanalia_solid_quad_geometry_input()
                .ok_or_else(|| {
                    NativeVulkanError::SceneLite(format!(
                        "scene-lite draw plan is not solid-quad recordable: {}",
                        runtime.draw_pass_backend_status
                    ))
                })?;

            run_native_vulkan_vulkanalia_scene_lite_solid_quad_present(
                NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions {
                    host: options.host,
                    wait_configure_roundtrips: options.wait_configure_roundtrips,
                    duration,
                    target_max_fps,
                    quad_color: options.clear_color,
                    geometry: Some(geometry),
                },
            )
            .map(|present| NativeVulkanSceneLitePresentSnapshot::SolidQuad { runtime, present })
            .map_err(NativeVulkanError::SceneLite)
        }
        NativeVulkanSceneLitePresentRouteKind::SampledImage => {
            let (source, fit, geometry) = if let Some((source, geometry)) =
                runtime.vulkanalia_sampled_image_geometry_input()
            {
                (source, None, Some(geometry))
            } else if let Some((source, fit)) =
                runtime.vulkanalia_sampled_image_full_extent_fallback_input()
            {
                (source, Some(fit), None)
            } else {
                return Err(NativeVulkanError::SceneLite(format!(
                    "scene-lite draw plan is not sampled-image recordable: {}",
                    runtime.draw_pass_backend_status
                )));
            };
            let solid_geometry = runtime.vulkanalia_mixed_solid_quad_geometry_input();

            run_native_vulkan_vulkanalia_scene_lite_sampled_image_present(
                NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions {
                    host: options.host,
                    wait_configure_roundtrips: options.wait_configure_roundtrips,
                    duration,
                    target_max_fps,
                    source,
                    clear_color: options.clear_color,
                    fit,
                    solid_geometry,
                    geometry,
                },
            )
            .map(|present| NativeVulkanSceneLitePresentSnapshot::SampledImage { runtime, present })
            .map_err(NativeVulkanError::SceneLite)
        }
    }
}

fn native_vulkan_scene_lite_present_route(
    runtime: &NativeVulkanSceneLiteRuntimeSnapshot,
) -> Result<NativeVulkanSceneLitePresentRouteKind, NativeVulkanError> {
    if !runtime.draw_pass_backend_ready {
        return Err(NativeVulkanError::SceneLite(format!(
            "scene-lite draw plan is not presentable by the native Vulkan scene backend: {}",
            runtime.draw_pass_backend_status
        )));
    }

    match runtime.draw_pass_backend_status {
        "fast-clear-color-ready" => Ok(NativeVulkanSceneLitePresentRouteKind::Clear),
        "solid-quad-recording-ready" | "clear-background-solid-quad-recording-ready" => {
            Ok(NativeVulkanSceneLitePresentRouteKind::SolidQuad)
        }
        "sampled-image-recording-ready"
        | "clear-background-sampled-image-recording-ready"
        | "sampled-image-full-extent-fallback-ready"
        | "clear-background-sampled-image-full-extent-fallback-ready"
        | "mixed-quad-sampled-image-full-extent-fallback-ready"
        | "clear-background-mixed-quad-sampled-image-full-extent-fallback-ready"
        | "clear-background-mixed-quad-sampled-image-recording-ready"
        | "mixed-quad-sampled-image-recording-ready" => {
            Ok(NativeVulkanSceneLitePresentRouteKind::SampledImage)
        }
        status => Err(NativeVulkanError::SceneLite(format!(
            "scene-lite draw plan has no native Vulkan present route: {status}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{FitMode, SceneLiteLayerKind, SceneLiteTransform};
    use crate::renderer::{SceneLiteDisplayPlan, SceneLiteRenderLayer};
    use std::path::PathBuf;

    fn layer(id: &str, kind: SceneLiteLayerKind) -> SceneLiteRenderLayer {
        SceneLiteRenderLayer {
            id: id.to_owned(),
            kind,
            source: None,
            color: None,
            stroke_color: None,
            stroke_width: None,
            corner_radius: None,
            width: None,
            height: None,
            text: None,
            font_size: None,
            font_family: None,
            font_weight: None,
            text_align: None,
            path_data: None,
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneLiteTransform::default(),
        }
    }

    fn plan(layers: Vec<SceneLiteRenderLayer>) -> SceneLiteWallpaperPlan {
        SceneLiteWallpaperPlan {
            output_name: "HDMI-A-1".to_owned(),
            source: None,
            fallback: None,
            manifest_max_fps: None,
            target_max_fps: Some(60),
            snapshot_time_ms: 0,
            bound_properties: Vec::new(),
            display: Some(SceneLiteDisplayPlan::Color {
                color: "#000000".to_owned(),
            }),
            layers,
        }
    }

    fn route_for_layers(
        layers: Vec<SceneLiteRenderLayer>,
    ) -> Result<NativeVulkanSceneLitePresentRouteKind, NativeVulkanError> {
        let render_item = native_vulkan_scene_lite_item(&plan(layers));
        let runtime =
            native_vulkan_scene_lite_runtime_snapshot(&render_item).expect("runtime snapshot");
        native_vulkan_scene_lite_present_route(&runtime)
    }

    #[test]
    fn scene_lite_main_present_route_selects_fast_clear() {
        let mut color = layer("background", SceneLiteLayerKind::Color);
        color.color = Some("#102030".to_owned());

        assert_eq!(
            route_for_layers(vec![color]).unwrap(),
            NativeVulkanSceneLitePresentRouteKind::Clear
        );
    }

    #[test]
    fn scene_lite_main_present_route_selects_solid_quad() {
        let mut rectangle = layer("panel", SceneLiteLayerKind::Rectangle);
        rectangle.color = Some("#336699".to_owned());
        rectangle.width = Some(320.0);
        rectangle.height = Some(180.0);

        assert_eq!(
            route_for_layers(vec![rectangle]).unwrap(),
            NativeVulkanSceneLitePresentRouteKind::SolidQuad
        );
    }

    #[test]
    fn scene_lite_main_present_route_selects_sampled_image_for_image_and_mixed_scenes() {
        let mut image = layer("hero", SceneLiteLayerKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.png"));

        assert_eq!(
            route_for_layers(vec![image.clone()]).unwrap(),
            NativeVulkanSceneLitePresentRouteKind::SampledImage
        );

        image.width = Some(640.0);
        image.height = Some(360.0);
        assert_eq!(
            route_for_layers(vec![image.clone()]).unwrap(),
            NativeVulkanSceneLitePresentRouteKind::SampledImage
        );

        let mut rectangle = layer("panel", SceneLiteLayerKind::Rectangle);
        rectangle.color = Some("#203040".to_owned());
        rectangle.width = Some(320.0);
        rectangle.height = Some(180.0);

        assert_eq!(
            route_for_layers(vec![rectangle, image]).unwrap(),
            NativeVulkanSceneLitePresentRouteKind::SampledImage
        );

        let mut background = layer("background", SceneLiteLayerKind::Image);
        background.source = Some(PathBuf::from("/tmp/background.png"));
        let mut overlay = layer("overlay", SceneLiteLayerKind::Rectangle);
        overlay.color = Some("#ffffff".to_owned());
        overlay.width = Some(64.0);
        overlay.height = Some(64.0);

        assert_eq!(
            route_for_layers(vec![background, overlay]).unwrap(),
            NativeVulkanSceneLitePresentRouteKind::SampledImage
        );
    }
}

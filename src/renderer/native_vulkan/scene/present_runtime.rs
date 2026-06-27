use std::time::Duration;

use serde::Serialize;

use crate::renderer::SceneWallpaperPlan;

use super::super::present::render_item::native_vulkan_scene_item;
use super::super::present::render_plan::{
    native_vulkan_clear_color_from_hex, native_vulkan_render_item_clear_color,
};
use super::super::{
    NativeVulkanAudioOutputMode, NativeVulkanError, NativeVulkanOptions,
    NativeVulkanVideoSessionCodec, NativeVulkanVulkanaliaClearPresentSnapshot,
    NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
    NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot,
    NativeVulkanVulkanaliaSceneSolidQuadPresentOptions,
    NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot, run_clear,
    run_native_vulkan_vulkanalia_scene_sampled_image_present,
    run_native_vulkan_vulkanalia_scene_solid_quad_present,
};
#[cfg(feature = "native-vulkan-video")]
use super::super::{
    NativeVulkanVulkanaliaReadyPrefixRuntimeSnapshot, run_vulkanalia_ready_prefix_video,
};
use super::runtime::{NativeVulkanSceneRuntimeSnapshot, native_vulkan_scene_runtime_snapshot};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "scene_present_route",
    content = "snapshot",
    rename_all = "kebab-case"
)]
pub enum NativeVulkanScenePresentSnapshot {
    Clear {
        runtime: NativeVulkanSceneRuntimeSnapshot,
        present: NativeVulkanVulkanaliaClearPresentSnapshot,
    },
    SolidQuad {
        runtime: NativeVulkanSceneRuntimeSnapshot,
        present: NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot,
    },
    SampledImage {
        runtime: NativeVulkanSceneRuntimeSnapshot,
        present: NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot,
    },
    #[cfg(feature = "native-vulkan-video")]
    Video {
        runtime: NativeVulkanSceneRuntimeSnapshot,
        present: NativeVulkanVulkanaliaReadyPrefixRuntimeSnapshot,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeVulkanScenePresentRouteKind {
    Clear,
    SolidQuad,
    SampledImage,
    #[cfg(feature = "native-vulkan-video")]
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanSceneVideoBridgeOptions {
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
    pub bitstream_extract_max_samples: u32,
    pub ready_prefix_frames: u32,
    pub playback_frames: u32,
    pub audio_clock_probe_requested: bool,
    pub audio_output_mode: NativeVulkanAudioOutputMode,
}

pub fn run_scene(
    mut options: NativeVulkanOptions,
    duration: Duration,
    plan: SceneWallpaperPlan,
    video_bridge: Option<NativeVulkanSceneVideoBridgeOptions>,
) -> Result<NativeVulkanScenePresentSnapshot, NativeVulkanError> {
    #[cfg(not(feature = "native-vulkan-video"))]
    let _ = video_bridge;

    if options.host.output_name.is_none() {
        options.host.output_name = Some(plan.output_name.clone());
    }
    let target_max_fps = options.target_max_fps.or(plan.target_max_fps);
    options.target_max_fps = target_max_fps;
    let render_item = native_vulkan_scene_item(&plan);
    options.clear_color = native_vulkan_render_item_clear_color(&render_item, options.clear_color);
    let runtime = native_vulkan_scene_runtime_snapshot(&render_item).ok_or_else(|| {
        NativeVulkanError::Scene("scene runtime snapshot is unavailable".to_owned())
    })?;
    if let Some(color) = runtime
        .draw_pass_background_clear_color
        .as_deref()
        .and_then(native_vulkan_clear_color_from_hex)
    {
        options.clear_color = color;
    }
    match native_vulkan_scene_present_route(&runtime)? {
        NativeVulkanScenePresentRouteKind::Clear => {
            let color = runtime
                .draw_pass_fast_clear_color
                .as_deref()
                .and_then(native_vulkan_clear_color_from_hex)
                .ok_or_else(|| {
                    NativeVulkanError::Scene(
                        "scene fast-clear draw plan has no valid #rrggbb color".to_owned(),
                    )
                })?;
            options.clear_color = color;
            run_clear(options, duration)
                .map(|present| NativeVulkanScenePresentSnapshot::Clear { runtime, present })
        }
        NativeVulkanScenePresentRouteKind::SolidQuad => {
            let geometry = runtime
                .vulkanalia_solid_quad_geometry_input()
                .ok_or_else(|| {
                    NativeVulkanError::Scene(format!(
                        "scene draw plan is not solid-quad recordable: {}",
                        runtime.draw_pass_backend_status
                    ))
                })?;

            run_native_vulkan_vulkanalia_scene_solid_quad_present(
                NativeVulkanVulkanaliaSceneSolidQuadPresentOptions {
                    host: options.host,
                    wait_configure_roundtrips: options.wait_configure_roundtrips,
                    duration,
                    target_max_fps,
                    quad_color: options.clear_color,
                    geometry: Some(geometry),
                },
            )
            .map(|present| NativeVulkanScenePresentSnapshot::SolidQuad { runtime, present })
            .map_err(NativeVulkanError::Scene)
        }
        NativeVulkanScenePresentRouteKind::SampledImage => {
            let (source, fit, geometry) = if let Some((source, geometry)) =
                runtime.vulkanalia_sampled_image_geometry_input()
            {
                (source, None, Some(geometry))
            } else if let Some((source, fit)) =
                runtime.vulkanalia_sampled_image_implicit_full_extent_input()
            {
                (source, Some(fit), None)
            } else {
                return Err(NativeVulkanError::Scene(format!(
                    "scene draw plan is not sampled-image recordable: {}",
                    runtime.draw_pass_backend_status
                )));
            };
            let solid_geometry = runtime.vulkanalia_mixed_solid_quad_geometry_input();

            run_native_vulkan_vulkanalia_scene_sampled_image_present(
                NativeVulkanVulkanaliaSceneSampledImagePresentOptions {
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
            .map(|present| NativeVulkanScenePresentSnapshot::SampledImage { runtime, present })
            .map_err(NativeVulkanError::Scene)
        }
        #[cfg(feature = "native-vulkan-video")]
        NativeVulkanScenePresentRouteKind::Video => {
            let video_bridge = video_bridge.ok_or_else(|| {
                NativeVulkanError::Scene(
                    "scene video layer requires Vulkan Video bridge options".to_owned(),
                )
            })?;
            let video = runtime
                .draw_ops
                .iter()
                .find(|op| op.kind == "video")
                .ok_or_else(|| {
                    NativeVulkanError::Scene("scene video route has no video draw op".to_owned())
                })?;
            let source = video.source.clone().ok_or_else(|| {
                NativeVulkanError::Scene("scene video route has no video source".to_owned())
            })?;
            let width = native_vulkan_scene_video_extent(video_bridge.width, video.width);
            let height = native_vulkan_scene_video_extent(video_bridge.height, video.height);

            run_vulkanalia_ready_prefix_video(
                options,
                video_bridge.codec,
                source,
                width,
                height,
                video.fit,
                video_bridge.bitstream_extract_max_samples,
                video_bridge.ready_prefix_frames,
                video_bridge.playback_frames,
                video_bridge.audio_clock_probe_requested,
                video_bridge.audio_output_mode,
            )
            .map(|present| NativeVulkanScenePresentSnapshot::Video { runtime, present })
        }
    }
}

fn native_vulkan_scene_present_route(
    runtime: &NativeVulkanSceneRuntimeSnapshot,
) -> Result<NativeVulkanScenePresentRouteKind, NativeVulkanError> {
    if !runtime.draw_pass_backend_ready {
        return Err(NativeVulkanError::Scene(format!(
            "scene draw plan is not presentable by the native Vulkan scene backend: {}",
            runtime.draw_pass_backend_status
        )));
    }

    match runtime.draw_pass_backend_status {
        "fast-clear-color-ready" => Ok(NativeVulkanScenePresentRouteKind::Clear),
        "solid-quad-recording-ready" | "clear-background-solid-quad-recording-ready" => {
            Ok(NativeVulkanScenePresentRouteKind::SolidQuad)
        }
        "sampled-image-recording-ready"
        | "clear-background-sampled-image-recording-ready"
        | "sampled-image-implicit-full-extent-ready"
        | "clear-background-sampled-image-implicit-full-extent-ready"
        | "mixed-quad-sampled-image-implicit-full-extent-ready"
        | "clear-background-mixed-quad-sampled-image-implicit-full-extent-ready"
        | "clear-background-mixed-quad-sampled-image-recording-ready"
        | "mixed-quad-sampled-image-recording-ready" => {
            Ok(NativeVulkanScenePresentRouteKind::SampledImage)
        }
        #[cfg(feature = "native-vulkan-video")]
        "video-layer-vulkan-video-scene-bridge-ready"
        | "clear-background-video-layer-vulkan-video-scene-bridge-ready" => {
            Ok(NativeVulkanScenePresentRouteKind::Video)
        }
        status => Err(NativeVulkanError::Scene(format!(
            "scene draw plan has no native Vulkan present route: {status}"
        ))),
    }
}

fn native_vulkan_scene_video_extent(option_extent: u32, layer_extent: Option<f64>) -> u32 {
    if option_extent > 0 {
        return option_extent;
    }
    layer_extent
        .filter(|extent| extent.is_finite() && *extent > 0.0)
        .map(|extent| extent.round().clamp(1.0, f64::from(u32::MAX)) as u32)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{FitMode, SceneNodeKind, SceneTransform};
    use crate::renderer::{SceneDisplayPlan, SceneRenderLayer};
    use std::path::PathBuf;

    fn layer(id: &str, kind: SceneNodeKind) -> SceneRenderLayer {
        SceneRenderLayer {
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
            transform: SceneTransform::default(),
        }
    }

    fn plan(layers: Vec<SceneRenderLayer>) -> SceneWallpaperPlan {
        SceneWallpaperPlan {
            output_name: "HDMI-A-1".to_owned(),
            source: None,
            fallback: None,
            manifest_max_fps: None,
            target_max_fps: Some(60),
            snapshot_time_ms: 0,
            bound_properties: Vec::new(),
            timeline_animation_count: 0,
            timeline_animated_layer_count: 0,
            property_binding_count: 0,
            display: Some(SceneDisplayPlan::Color {
                color: "#000000".to_owned(),
            }),
            layers,
        }
    }

    fn route_for_layers(
        layers: Vec<SceneRenderLayer>,
    ) -> Result<NativeVulkanScenePresentRouteKind, NativeVulkanError> {
        let render_item = native_vulkan_scene_item(&plan(layers));
        let runtime = native_vulkan_scene_runtime_snapshot(&render_item).expect("runtime snapshot");
        native_vulkan_scene_present_route(&runtime)
    }

    #[test]
    fn scene_main_present_route_selects_fast_clear() {
        let mut color = layer("background", SceneNodeKind::Color);
        color.color = Some("#102030".to_owned());

        assert_eq!(
            route_for_layers(vec![color]).unwrap(),
            NativeVulkanScenePresentRouteKind::Clear
        );
    }

    #[test]
    fn scene_main_present_route_selects_solid_quad() {
        let mut rectangle = layer("panel", SceneNodeKind::Rectangle);
        rectangle.color = Some("#336699".to_owned());
        rectangle.width = Some(320.0);
        rectangle.height = Some(180.0);

        assert_eq!(
            route_for_layers(vec![rectangle]).unwrap(),
            NativeVulkanScenePresentRouteKind::SolidQuad
        );
    }

    #[test]
    fn scene_main_present_route_selects_sampled_image_for_image_and_mixed_scenes() {
        let mut image = layer("hero", SceneNodeKind::Image);
        image.source = Some(PathBuf::from("/tmp/hero.png"));

        assert_eq!(
            route_for_layers(vec![image.clone()]).unwrap(),
            NativeVulkanScenePresentRouteKind::SampledImage
        );

        image.width = Some(640.0);
        image.height = Some(360.0);
        assert_eq!(
            route_for_layers(vec![image.clone()]).unwrap(),
            NativeVulkanScenePresentRouteKind::SampledImage
        );

        let mut rectangle = layer("panel", SceneNodeKind::Rectangle);
        rectangle.color = Some("#203040".to_owned());
        rectangle.width = Some(320.0);
        rectangle.height = Some(180.0);

        assert_eq!(
            route_for_layers(vec![rectangle, image]).unwrap(),
            NativeVulkanScenePresentRouteKind::SampledImage
        );

        let mut background = layer("background", SceneNodeKind::Image);
        background.source = Some(PathBuf::from("/tmp/background.png"));
        let mut overlay = layer("overlay", SceneNodeKind::Rectangle);
        overlay.color = Some("#ffffff".to_owned());
        overlay.width = Some(64.0);
        overlay.height = Some(64.0);

        assert_eq!(
            route_for_layers(vec![background, overlay]).unwrap(),
            NativeVulkanScenePresentRouteKind::SampledImage
        );
    }
}

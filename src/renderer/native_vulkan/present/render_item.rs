use std::path::PathBuf;

use serde::Serialize;

use crate::config::VideoDecoderPolicy;
use crate::core::{FitMode, SceneNodeKind, SceneSize, SceneSystems, SceneTransform, Transition};
use crate::renderer::{
    SceneDisplayPlan, SceneRenderLayer, SceneWallpaperPlan, SlideshowWallpaperPlan,
    StaticRenderSyncPlan, StaticWallpaperPlan, VideoWallpaperPlan,
};

use super::super::NativeVulkanWallpaperType;

const NATIVE_VULKAN_STATIC_SCENE_RENDERER_STATUS: &str =
    "static-image-lowered-to-scene-sampled-image-layer";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NativeVulkanRenderItem {
    Clear {
        output_name: String,
    },
    Video {
        output_name: String,
        source: PathBuf,
        poster: Option<PathBuf>,
        fit: FitMode,
        loop_playback: bool,
        muted: bool,
        manifest_max_fps: Option<u32>,
        target_max_fps: Option<u32>,
        decoder_policy: VideoDecoderPolicy,
        start_offset_ms: u64,
        renderer_status: &'static str,
    },
    Slideshow {
        output_name: String,
        sources: Vec<PathBuf>,
        interval_ms: u64,
        transition: Transition,
        fit: FitMode,
        target_max_fps: Option<u32>,
        renderer_status: &'static str,
    },
    Scene {
        output_name: String,
        scene_source: Option<PathBuf>,
        display: Option<SceneDisplayPlan>,
        display_image: Option<PathBuf>,
        display_color: Option<String>,
        manifest_max_fps: Option<u32>,
        layer_count: usize,
        layers: Vec<SceneRenderLayer>,
        scene_systems: SceneSystems,
        audio_cue_count: usize,
        bound_properties: Vec<String>,
        timeline_animation_count: usize,
        timeline_animated_layer_count: usize,
        property_binding_count: usize,
        cursor_parallax_input_ready: bool,
        scene_scenescript_binding_count: usize,
        scene_material_graph_count: usize,
        scene_material_graph_resource_count: usize,
        scene_effect_graph_count: usize,
        scene_audio_response_binding_count: usize,
        unsupported_scene_features: Vec<String>,
        snapshot_time_ms: u64,
        scene_size: Option<SceneSize>,
        scene_fit: FitMode,
        target_max_fps: Option<u32>,
        renderer_status: &'static str,
    },
}

impl NativeVulkanRenderItem {
    pub fn wallpaper_type(&self) -> NativeVulkanWallpaperType {
        match self {
            Self::Clear { .. } => NativeVulkanWallpaperType::StaticImage,
            Self::Video { .. } => NativeVulkanWallpaperType::Video,
            Self::Slideshow { .. } => NativeVulkanWallpaperType::Playlist,
            Self::Scene { .. } => NativeVulkanWallpaperType::Scene,
        }
    }
}

pub fn render_items_from_sync_plan(plan: &StaticRenderSyncPlan) -> Vec<NativeVulkanRenderItem> {
    plan.plans
        .iter()
        .map(native_vulkan_static_scene_item)
        .chain(plan.video_plans.iter().map(native_vulkan_video_item))
        .chain(
            plan.slideshow_plans
                .iter()
                .map(native_vulkan_slideshow_item),
        )
        .chain(plan.scene_plans.iter().map(native_vulkan_scene_item))
        .collect()
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_static_scene_item(
    plan: &StaticWallpaperPlan,
) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Scene {
        output_name: plan.output_name.clone(),
        scene_source: None,
        display: Some(SceneDisplayPlan::Image {
            source: plan.source.clone(),
            fit: plan.fit,
            background: plan.background.clone(),
        }),
        display_image: Some(plan.source.clone()),
        display_color: None,
        manifest_max_fps: None,
        layer_count: 1,
        layers: vec![SceneRenderLayer {
            id: "static-image".to_owned(),
            kind: SceneNodeKind::Image,
            source: Some(plan.source.clone()),
            texture_region: None,
            audio: Vec::new(),
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
            path_fill_rule: crate::core::ScenePathFillRule::default(),
            fit: plan.fit,
            opacity: 1.0,
            transform: SceneTransform::default(),
        }],
        scene_systems: SceneSystems::default(),
        audio_cue_count: 0,
        bound_properties: Vec::new(),
        timeline_animation_count: 0,
        timeline_animated_layer_count: 0,
        property_binding_count: 0,
        cursor_parallax_input_ready: false,
        scene_scenescript_binding_count: 0,
        scene_material_graph_count: 0,
        scene_material_graph_resource_count: 0,
        scene_effect_graph_count: 0,
        scene_audio_response_binding_count: 0,
        unsupported_scene_features: Vec::new(),
        snapshot_time_ms: 0,
        scene_size: None,
        scene_fit: plan.fit,
        target_max_fps: None,
        renderer_status: NATIVE_VULKAN_STATIC_SCENE_RENDERER_STATUS,
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_video_item(
    plan: &VideoWallpaperPlan,
) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Video {
        output_name: plan.output_name.clone(),
        source: plan.source.clone(),
        poster: plan.poster.clone(),
        fit: plan.fit,
        loop_playback: plan.loop_playback,
        muted: plan.muted,
        manifest_max_fps: plan.manifest_max_fps,
        target_max_fps: plan.target_max_fps,
        decoder_policy: plan.decoder_policy,
        start_offset_ms: plan.start_offset_ms,
        renderer_status: "vulkan-lifecycle-video-placeholder",
    }
}

fn native_vulkan_slideshow_item(plan: &SlideshowWallpaperPlan) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Slideshow {
        output_name: plan.output_name.clone(),
        sources: plan.sources.clone(),
        interval_ms: plan.interval_ms,
        transition: plan.transition,
        fit: plan.fit,
        target_max_fps: plan.target_max_fps,
        renderer_status: "planned-slideshow-static-texture-sequence",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_wallpaper_lowers_to_single_image_scene_layer() {
        let plan = StaticWallpaperPlan {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/static.png"),
            fit: FitMode::Contain,
            background: Some("#010203".to_owned()),
        };

        let item = native_vulkan_static_scene_item(&plan);

        let NativeVulkanRenderItem::Scene {
            output_name,
            scene_source,
            display,
            display_image,
            layer_count,
            layers,
            bound_properties,
            renderer_status,
            ..
        } = item
        else {
            panic!("static image should lower to a scene render item");
        };
        assert_eq!(output_name, "HDMI-A-1");
        assert_eq!(scene_source, None);
        assert_eq!(display_image, Some(PathBuf::from("/tmp/static.png")));
        assert_eq!(
            display,
            Some(SceneDisplayPlan::Image {
                source: PathBuf::from("/tmp/static.png"),
                fit: FitMode::Contain,
                background: Some("#010203".to_owned()),
            })
        );
        assert_eq!(layer_count, 1);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].kind, SceneNodeKind::Image);
        assert_eq!(layers[0].source, Some(PathBuf::from("/tmp/static.png")));
        assert_eq!(layers[0].fit, FitMode::Contain);
        assert!(bound_properties.is_empty());
        assert_eq!(renderer_status, NATIVE_VULKAN_STATIC_SCENE_RENDERER_STATUS);
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_item(
    plan: &SceneWallpaperPlan,
) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Scene {
        output_name: plan.output_name.clone(),
        scene_source: plan.source.clone(),
        display: plan.display.clone(),
        display_image: match &plan.display {
            Some(SceneDisplayPlan::Image { source, .. }) => Some(source.clone()),
            Some(SceneDisplayPlan::Color { .. }) | None => None,
        },
        display_color: match &plan.display {
            Some(SceneDisplayPlan::Color { color }) => Some(color.clone()),
            Some(SceneDisplayPlan::Image { .. }) | None => None,
        },
        manifest_max_fps: plan.manifest_max_fps,
        layer_count: plan.layers.len(),
        layers: plan.layers.clone(),
        scene_systems: plan.scene_systems.clone(),
        audio_cue_count: plan.audio_cue_count,
        bound_properties: plan.bound_properties.clone(),
        timeline_animation_count: plan.timeline_animation_count,
        timeline_animated_layer_count: plan.timeline_animated_layer_count,
        property_binding_count: plan.property_binding_count,
        cursor_parallax_input_ready: plan.cursor_parallax_input_ready,
        scene_scenescript_binding_count: plan.scene_scenescript_binding_count,
        scene_material_graph_count: plan.scene_material_graph_count,
        scene_material_graph_resource_count: plan.scene_material_graph_resource_count,
        scene_effect_graph_count: plan.scene_effect_graph_count,
        scene_audio_response_binding_count: plan.scene_audio_response_binding_count,
        unsupported_scene_features: plan.unsupported_scene_features.clone(),
        snapshot_time_ms: plan.snapshot_time_ms,
        scene_size: plan.scene_size,
        scene_fit: plan.scene_fit,
        target_max_fps: plan.target_max_fps,
        renderer_status: "deterministic-scene-snapshot-ready-for-vulkan-passes",
    }
}

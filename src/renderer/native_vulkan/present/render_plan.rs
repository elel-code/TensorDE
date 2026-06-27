#![allow(dead_code)]

use std::path::PathBuf;

use crate::core::{FitMode, SceneLiteLayerKind, SceneLiteTextAlign, SceneLiteTransform};
use crate::renderer::{SceneLiteDisplayPlan, SceneLiteRenderLayer};

use super::super::NativeVulkanClearColor;
use super::render_item::NativeVulkanRenderItem;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanStaticUploadPlan {
    pub(in crate::renderer::native_vulkan) source: PathBuf,
    pub(in crate::renderer::native_vulkan) fit: FitMode,
    pub(in crate::renderer::native_vulkan) background: Option<String>,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_static_upload_plan(
    render_item: &NativeVulkanRenderItem,
) -> Option<NativeVulkanStaticUploadPlan> {
    match render_item {
        NativeVulkanRenderItem::StaticImage {
            source,
            fit,
            background,
            ..
        } => Some(NativeVulkanStaticUploadPlan {
            source: source.clone(),
            fit: *fit,
            background: background.clone(),
        }),
        NativeVulkanRenderItem::Video {
            poster: Some(poster),
            fit,
            ..
        } => Some(NativeVulkanStaticUploadPlan {
            source: poster.clone(),
            fit: *fit,
            background: None,
        }),
        NativeVulkanRenderItem::SceneLite {
            display:
                Some(SceneLiteDisplayPlan::Image {
                    source,
                    fit,
                    background,
                }),
            ..
        } => Some(NativeVulkanStaticUploadPlan {
            source: source.clone(),
            fit: *fit,
            background: background.clone(),
        }),
        _ => None,
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_render_item_clear_color(
    render_item: &NativeVulkanRenderItem,
    fallback: NativeVulkanClearColor,
) -> NativeVulkanClearColor {
    match render_item {
        NativeVulkanRenderItem::SceneLite {
            display: Some(SceneLiteDisplayPlan::Color { color }),
            ..
        } => native_vulkan_clear_color_from_hex(color).unwrap_or(fallback),
        _ => fallback,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLiteDrawOpKind {
    Image,
    Video,
    ColorQuad,
    Rectangle,
    Ellipse,
    Text,
    Path,
}

impl NativeVulkanSceneLiteDrawOpKind {
    pub(in crate::renderer::native_vulkan) fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::ColorQuad => "color-quad",
            Self::Rectangle => "rectangle",
            Self::Ellipse => "ellipse",
            Self::Text => "text",
            Self::Path => "path",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLiteDrawOp {
    pub(in crate::renderer::native_vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan) layer_id: String,
    pub(in crate::renderer::native_vulkan) kind: NativeVulkanSceneLiteDrawOpKind,
    pub(in crate::renderer::native_vulkan) opacity: f64,
    pub(in crate::renderer::native_vulkan) source: Option<PathBuf>,
    pub(in crate::renderer::native_vulkan) color: Option<String>,
    pub(in crate::renderer::native_vulkan) stroke_color: Option<String>,
    pub(in crate::renderer::native_vulkan) stroke_width: Option<f64>,
    pub(in crate::renderer::native_vulkan) corner_radius: Option<f64>,
    pub(in crate::renderer::native_vulkan) width: Option<f64>,
    pub(in crate::renderer::native_vulkan) height: Option<f64>,
    pub(in crate::renderer::native_vulkan) text: Option<String>,
    pub(in crate::renderer::native_vulkan) font_size: Option<f64>,
    pub(in crate::renderer::native_vulkan) font_family: Option<String>,
    pub(in crate::renderer::native_vulkan) font_weight: Option<String>,
    pub(in crate::renderer::native_vulkan) text_align: Option<SceneLiteTextAlign>,
    pub(in crate::renderer::native_vulkan) path_data: Option<String>,
    pub(in crate::renderer::native_vulkan) fit: FitMode,
    pub(in crate::renderer::native_vulkan) transform: SceneLiteTransform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLiteUnsupportedLayer {
    pub(in crate::renderer::native_vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan) layer_id: String,
    pub(in crate::renderer::native_vulkan) reason: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLiteDrawPlan {
    pub(in crate::renderer::native_vulkan) snapshot_time_ms: u64,
    pub(in crate::renderer::native_vulkan) draw_ops: Vec<NativeVulkanSceneLiteDrawOp>,
    pub(in crate::renderer::native_vulkan) unsupported_layers:
        Vec<NativeVulkanSceneLiteUnsupportedLayer>,
    pub(in crate::renderer::native_vulkan) manifest_preview_available: bool,
}

impl NativeVulkanSceneLiteDrawPlan {
    pub(in crate::renderer::native_vulkan) fn native_draw_ready(&self) -> bool {
        !self.draw_ops.is_empty() && self.unsupported_layers.is_empty()
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_lite_draw_plan(
    render_item: &NativeVulkanRenderItem,
) -> Option<NativeVulkanSceneLiteDrawPlan> {
    let NativeVulkanRenderItem::SceneLite {
        layers,
        display,
        fallback,
        snapshot_time_ms,
        ..
    } = render_item
    else {
        return None;
    };
    let (draw_ops, unsupported_layers) = native_vulkan_scene_lite_draw_layers(layers);

    Some(NativeVulkanSceneLiteDrawPlan {
        snapshot_time_ms: *snapshot_time_ms,
        draw_ops,
        unsupported_layers,
        manifest_preview_available: display.is_some() || fallback.is_some(),
    })
}

fn native_vulkan_scene_lite_draw_layers(
    layers: &[SceneLiteRenderLayer],
) -> (
    Vec<NativeVulkanSceneLiteDrawOp>,
    Vec<NativeVulkanSceneLiteUnsupportedLayer>,
) {
    let mut draw_ops = Vec::new();
    let mut unsupported_layers = Vec::new();
    for (index, layer) in layers.iter().enumerate() {
        if layer.opacity <= 0.0 {
            continue;
        }
        match native_vulkan_scene_lite_draw_op_kind(layer) {
            Ok(kind) => draw_ops.push(NativeVulkanSceneLiteDrawOp {
                layer_index: index,
                layer_id: layer.id.clone(),
                kind,
                opacity: layer.opacity.clamp(0.0, 1.0),
                source: layer.source.clone(),
                color: layer.color.clone(),
                stroke_color: layer.stroke_color.clone(),
                stroke_width: layer.stroke_width,
                corner_radius: layer.corner_radius,
                width: layer.width,
                height: layer.height,
                text: layer.text.clone(),
                font_size: layer.font_size,
                font_family: layer.font_family.clone(),
                font_weight: layer.font_weight.clone(),
                text_align: layer.text_align,
                path_data: layer.path_data.clone(),
                fit: layer.fit,
                transform: layer.transform,
            }),
            Err(reason) => unsupported_layers.push(NativeVulkanSceneLiteUnsupportedLayer {
                layer_index: index,
                layer_id: layer.id.clone(),
                reason,
            }),
        }
    }
    (draw_ops, unsupported_layers)
}

fn native_vulkan_scene_lite_draw_op_kind(
    layer: &SceneLiteRenderLayer,
) -> Result<NativeVulkanSceneLiteDrawOpKind, &'static str> {
    match layer.kind {
        SceneLiteLayerKind::Image => layer
            .source
            .as_ref()
            .map(|_| NativeVulkanSceneLiteDrawOpKind::Image)
            .ok_or("image-layer-missing-source"),
        SceneLiteLayerKind::Video => layer
            .source
            .as_ref()
            .map(|_| NativeVulkanSceneLiteDrawOpKind::Video)
            .ok_or("video-layer-missing-source"),
        SceneLiteLayerKind::Color => layer
            .color
            .as_ref()
            .map(|_| NativeVulkanSceneLiteDrawOpKind::ColorQuad)
            .ok_or("color-layer-missing-color"),
        SceneLiteLayerKind::Rectangle => {
            if native_vulkan_scene_lite_layer_has_shape_paint(layer) {
                Ok(NativeVulkanSceneLiteDrawOpKind::Rectangle)
            } else {
                Err("rectangle-layer-missing-paint")
            }
        }
        SceneLiteLayerKind::Ellipse => {
            if native_vulkan_scene_lite_layer_has_shape_paint(layer) {
                Ok(NativeVulkanSceneLiteDrawOpKind::Ellipse)
            } else {
                Err("ellipse-layer-missing-paint")
            }
        }
        SceneLiteLayerKind::Text => layer
            .text
            .as_ref()
            .filter(|text| !text.is_empty())
            .ok_or("text-layer-missing-text")
            .and_then(|_| {
                layer
                    .color
                    .as_ref()
                    .filter(|color| !color.is_empty())
                    .map(|_| NativeVulkanSceneLiteDrawOpKind::Text)
                    .ok_or("text-layer-missing-color")
            }),
        SceneLiteLayerKind::Path => layer
            .path_data
            .as_ref()
            .filter(|path| !path.is_empty())
            .ok_or("path-layer-missing-data")
            .and_then(|_| {
                if layer
                    .color
                    .as_deref()
                    .is_some_and(|color| !color.is_empty())
                    || layer
                        .stroke_color
                        .as_deref()
                        .is_some_and(|color| !color.is_empty())
                {
                    Ok(NativeVulkanSceneLiteDrawOpKind::Path)
                } else {
                    Err("path-layer-missing-paint")
                }
            }),
        SceneLiteLayerKind::Group => Err("group-layer-needs-flattened-children"),
    }
}

fn native_vulkan_scene_lite_layer_has_shape_paint(layer: &SceneLiteRenderLayer) -> bool {
    layer
        .color
        .as_deref()
        .is_some_and(|color| !color.is_empty())
        || (layer
            .stroke_color
            .as_deref()
            .is_some_and(|color| !color.is_empty())
            && layer.stroke_width.unwrap_or(1.0) > 0.0)
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_clear_color_from_hex(
    value: &str,
) -> Option<NativeVulkanClearColor> {
    let hex = value.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
    Some(NativeVulkanClearColor { r, g, b, a: 1.0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::FitMode;
    use crate::renderer::SceneLiteDisplayPlan;
    use crate::renderer::native_vulkan::{NativeVulkanClearColor, NativeVulkanRenderItem};
    use std::path::PathBuf;

    #[test]
    fn scene_lite_image_display_uses_static_upload_plan() {
        let item = NativeVulkanRenderItem::SceneLite {
            output_name: "HDMI-A-1".to_owned(),
            scene_source: Some(PathBuf::from("/tmp/scene-lite.json")),
            fallback: Some(PathBuf::from("/tmp/scene-fallback.svg")),
            display: Some(SceneLiteDisplayPlan::Image {
                source: PathBuf::from("/tmp/scene-snapshot.png"),
                fit: FitMode::Contain,
                background: Some("#010203".to_owned()),
            }),
            display_image: Some(PathBuf::from("/tmp/scene-snapshot.png")),
            display_color: None,
            manifest_max_fps: Some(60),
            layer_count: 0,
            layers: Vec::new(),
            bound_properties: Vec::new(),
            snapshot_time_ms: 0,
            target_max_fps: Some(60),
            renderer_status: "deterministic-scene-lite-snapshot-ready-for-vulkan-passes",
        };

        let plan = native_vulkan_static_upload_plan(&item).expect("scene-lite image display plan");

        assert_eq!(plan.source, PathBuf::from("/tmp/scene-snapshot.png"));
        assert_eq!(plan.fit, FitMode::Contain);
        assert_eq!(plan.background.as_deref(), Some("#010203"));
    }

    #[test]
    fn scene_lite_color_display_overrides_default_clear_color() {
        let fallback = NativeVulkanClearColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let item = NativeVulkanRenderItem::SceneLite {
            output_name: "HDMI-A-1".to_owned(),
            scene_source: Some(PathBuf::from("/tmp/scene-lite.json")),
            fallback: None,
            display: Some(SceneLiteDisplayPlan::Color {
                color: "#102030".to_owned(),
            }),
            display_image: None,
            display_color: Some("#102030".to_owned()),
            manifest_max_fps: Some(60),
            layer_count: 0,
            layers: Vec::new(),
            bound_properties: Vec::new(),
            snapshot_time_ms: 0,
            target_max_fps: Some(60),
            renderer_status: "deterministic-scene-lite-snapshot-ready-for-vulkan-passes",
        };

        let color = native_vulkan_render_item_clear_color(&item, fallback);

        assert!((color.r - 16.0 / 255.0).abs() < f32::EPSILON);
        assert!((color.g - 32.0 / 255.0).abs() < f32::EPSILON);
        assert!((color.b - 48.0 / 255.0).abs() < f32::EPSILON);
        assert_eq!(color.a, 1.0);
    }
}

use std::path::PathBuf;

use crate::core::{FitMode, SceneLiteLayerKind, SceneLiteTextAlign, SceneLiteTransform};
use crate::renderer::{SceneLiteDisplayPlan, SceneLiteRenderLayer};

use super::{NativeVulkanClearColor, NativeVulkanRenderItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanStaticUploadPlan {
    pub(super) source: PathBuf,
    pub(super) fit: FitMode,
    pub(super) background: Option<String>,
}

pub(super) fn native_vulkan_static_upload_plan(
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

pub(super) fn native_vulkan_render_item_clear_color(
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
pub(super) enum NativeVulkanSceneLiteDrawOpKind {
    Image,
    ColorQuad,
    Rectangle,
    Ellipse,
    Text,
    Path,
}

impl NativeVulkanSceneLiteDrawOpKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::ColorQuad => "color-quad",
            Self::Rectangle => "rectangle",
            Self::Ellipse => "ellipse",
            Self::Text => "text",
            Self::Path => "path",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneLiteDrawOp {
    pub(super) layer_index: usize,
    pub(super) layer_id: String,
    pub(super) kind: NativeVulkanSceneLiteDrawOpKind,
    pub(super) opacity: f64,
    pub(super) source: Option<PathBuf>,
    pub(super) color: Option<String>,
    pub(super) stroke_color: Option<String>,
    pub(super) stroke_width: Option<f64>,
    pub(super) corner_radius: Option<f64>,
    pub(super) width: Option<f64>,
    pub(super) height: Option<f64>,
    pub(super) text: Option<String>,
    pub(super) font_size: Option<f64>,
    pub(super) font_family: Option<String>,
    pub(super) font_weight: Option<String>,
    pub(super) text_align: Option<SceneLiteTextAlign>,
    pub(super) path_data: Option<String>,
    pub(super) fit: FitMode,
    pub(super) transform: SceneLiteTransform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanSceneLiteUnsupportedLayer {
    pub(super) layer_index: usize,
    pub(super) layer_id: String,
    pub(super) reason: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NativeVulkanSceneLiteDrawPlan {
    pub(super) snapshot_time_ms: u64,
    pub(super) draw_ops: Vec<NativeVulkanSceneLiteDrawOp>,
    pub(super) unsupported_layers: Vec<NativeVulkanSceneLiteUnsupportedLayer>,
    pub(super) fallback_display_available: bool,
}

impl NativeVulkanSceneLiteDrawPlan {
    pub(super) fn native_draw_ready(&self) -> bool {
        !self.draw_ops.is_empty() && self.unsupported_layers.is_empty()
    }
}

pub(super) fn native_vulkan_scene_lite_draw_plan(
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
        fallback_display_available: display.is_some() || fallback.is_some(),
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
        SceneLiteLayerKind::Color => layer
            .color
            .as_ref()
            .map(|_| NativeVulkanSceneLiteDrawOpKind::ColorQuad)
            .ok_or("color-layer-missing-color"),
        SceneLiteLayerKind::Rectangle => layer
            .color
            .as_ref()
            .map(|_| NativeVulkanSceneLiteDrawOpKind::Rectangle)
            .ok_or("rectangle-layer-missing-fill"),
        SceneLiteLayerKind::Ellipse => layer
            .color
            .as_ref()
            .map(|_| NativeVulkanSceneLiteDrawOpKind::Ellipse)
            .ok_or("ellipse-layer-missing-fill"),
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

fn native_vulkan_clear_color_from_hex(value: &str) -> Option<NativeVulkanClearColor> {
    let hex = value.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
    Some(NativeVulkanClearColor { r, g, b, a: 1.0 })
}

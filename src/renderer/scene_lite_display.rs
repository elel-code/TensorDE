use crate::core::{FitMode, SceneLiteLayerKind, SceneLiteTransform};

use super::{RenderTargetSize, SceneLiteDisplayPlan, SceneLiteRenderLayer};

pub(super) fn scene_lite_direct_display_color(
    layers: &[SceneLiteRenderLayer],
    size: RenderTargetSize,
) -> Option<String> {
    let mut renderable_layers = layers
        .iter()
        .filter(|layer| scene_lite_layer_is_snapshot_renderable(layer));
    let layer = renderable_layers.next()?;
    if renderable_layers.next().is_some()
        || layer.opacity < 1.0
        || layer.transform != SceneLiteTransform::default()
    {
        return None;
    }
    match layer.kind {
        SceneLiteLayerKind::Color => layer
            .color
            .as_deref()
            .filter(|color| !color.is_empty())
            .map(str::to_owned),
        SceneLiteLayerKind::Rectangle
            if scene_lite_rectangle_covers_target_without_shape_effects(layer, size) =>
        {
            layer
                .color
                .as_deref()
                .filter(|color| !color.is_empty())
                .map(str::to_owned)
        }
        _ => None,
    }
}

pub(super) fn scene_lite_direct_display_image(
    layers: &[SceneLiteRenderLayer],
    fit_override: Option<FitMode>,
) -> Option<SceneLiteDisplayPlan> {
    let mut renderable_layers = layers
        .iter()
        .filter(|layer| scene_lite_layer_is_snapshot_renderable(layer));
    let layer = renderable_layers.next()?;
    if renderable_layers.next().is_some()
        || layer.kind != SceneLiteLayerKind::Image
        || layer.opacity < 1.0
        || layer.transform != SceneLiteTransform::default()
    {
        return None;
    }
    Some(SceneLiteDisplayPlan::Image {
        source: layer.source.clone()?,
        fit: fit_override.unwrap_or(layer.fit),
        background: scene_lite_background_color(layers).or_else(|| Some("#000000".to_owned())),
    })
}

pub(super) fn scene_lite_layer_is_snapshot_renderable(layer: &SceneLiteRenderLayer) -> bool {
    match layer.kind {
        SceneLiteLayerKind::Image => layer.source.is_some() && layer.opacity > 0.0,
        SceneLiteLayerKind::Video => false,
        SceneLiteLayerKind::Color => {
            layer.opacity > 0.0
                && layer
                    .color
                    .as_deref()
                    .is_some_and(|color| !color.is_empty())
        }
        SceneLiteLayerKind::Rectangle | SceneLiteLayerKind::Ellipse => {
            layer.opacity > 0.0
                && layer
                    .color
                    .as_deref()
                    .is_some_and(|color| !color.is_empty())
        }
        SceneLiteLayerKind::Text => {
            layer.opacity > 0.0
                && layer.text.as_deref().is_some_and(|text| !text.is_empty())
                && layer
                    .color
                    .as_deref()
                    .is_some_and(|color| !color.is_empty())
        }
        SceneLiteLayerKind::Path => {
            layer.opacity > 0.0
                && layer
                    .path_data
                    .as_deref()
                    .is_some_and(|path| !path.is_empty())
                && (layer
                    .color
                    .as_deref()
                    .is_some_and(|color| !color.is_empty())
                    || layer
                        .stroke_color
                        .as_deref()
                        .is_some_and(|color| !color.is_empty()))
        }
        SceneLiteLayerKind::Group => false,
    }
}

pub(super) fn scene_lite_background_color(layers: &[SceneLiteRenderLayer]) -> Option<String> {
    layers
        .iter()
        .find(|layer| {
            layer.kind == SceneLiteLayerKind::Color
                && layer.opacity > 0.0
                && layer
                    .color
                    .as_deref()
                    .is_some_and(|color| !color.is_empty())
        })
        .and_then(|layer| layer.color.clone())
}

fn scene_lite_rectangle_covers_target_without_shape_effects(
    layer: &SceneLiteRenderLayer,
    size: RenderTargetSize,
) -> bool {
    let has_stroke = layer
        .stroke_color
        .as_deref()
        .is_some_and(|color| !color.is_empty())
        && layer.stroke_width.unwrap_or(1.0) > 0.0;
    let has_corner_radius = layer.corner_radius.unwrap_or(0.0) > 0.0;
    let width = layer.width.unwrap_or(f64::from(size.width));
    let height = layer.height.unwrap_or(f64::from(size.height));
    !has_stroke
        && !has_corner_radius
        && width.is_finite()
        && height.is_finite()
        && width >= f64::from(size.width)
        && height >= f64::from(size.height)
}

use crate::core::{FitMode, SceneNodeKind, SceneTransform};

use super::{RenderTargetSize, SceneDisplayPlan, SceneRenderLayer};

pub(super) fn scene_direct_display_color(
    layers: &[SceneRenderLayer],
    size: RenderTargetSize,
) -> Option<String> {
    let mut renderable_layers = layers
        .iter()
        .filter(|layer| scene_layer_is_snapshot_renderable(layer));
    let layer = renderable_layers.next()?;
    if renderable_layers.next().is_some()
        || layer.opacity < 1.0
        || layer.transform != SceneTransform::default()
    {
        return None;
    }
    match layer.kind {
        SceneNodeKind::Color => layer
            .color
            .as_deref()
            .filter(|color| !color.is_empty())
            .map(str::to_owned),
        SceneNodeKind::Rectangle
            if scene_rectangle_covers_target_without_shape_effects(layer, size) =>
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

pub(super) fn scene_direct_display_image(
    layers: &[SceneRenderLayer],
    fit_override: Option<FitMode>,
) -> Option<SceneDisplayPlan> {
    let mut renderable_layers = layers
        .iter()
        .filter(|layer| scene_layer_is_snapshot_renderable(layer));
    let layer = renderable_layers.next()?;
    if renderable_layers.next().is_some()
        || layer.kind != SceneNodeKind::Image
        || layer.opacity < 1.0
        || layer.transform != SceneTransform::default()
    {
        return None;
    }
    Some(SceneDisplayPlan::Image {
        source: layer.source.clone()?,
        fit: fit_override.unwrap_or(layer.fit),
        background: scene_background_color(layers).or_else(|| Some("#000000".to_owned())),
    })
}

pub(super) fn scene_layer_is_snapshot_renderable(layer: &SceneRenderLayer) -> bool {
    match layer.kind {
        SceneNodeKind::Image => layer.source.is_some() && layer.opacity > 0.0,
        SceneNodeKind::Video => false,
        SceneNodeKind::Color => {
            layer.opacity > 0.0
                && layer
                    .color
                    .as_deref()
                    .is_some_and(|color| !color.is_empty())
        }
        SceneNodeKind::Rectangle | SceneNodeKind::Ellipse => {
            layer.opacity > 0.0
                && (layer
                    .color
                    .as_deref()
                    .is_some_and(|color| !color.is_empty())
                    || (layer
                        .stroke_color
                        .as_deref()
                        .is_some_and(|color| !color.is_empty())
                        && layer.stroke_width.unwrap_or(1.0) > 0.0))
        }
        SceneNodeKind::Text => {
            layer.opacity > 0.0
                && layer.text.as_deref().is_some_and(|text| !text.is_empty())
                && layer
                    .color
                    .as_deref()
                    .is_some_and(|color| !color.is_empty())
        }
        SceneNodeKind::Path => {
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
        SceneNodeKind::Group => false,
        SceneNodeKind::AudioResponse => {
            layer
                .color
                .as_deref()
                .is_some_and(|color| !color.is_empty())
                && layer
                    .width
                    .is_some_and(|width| width.is_finite() && width > 0.0)
                && layer
                    .height
                    .is_some_and(|height| height.is_finite() && height > 0.0)
        }
        SceneNodeKind::Shader
        | SceneNodeKind::ParticleEmitter
        | SceneNodeKind::Script
        | SceneNodeKind::Unknown => false,
    }
}

pub(super) fn scene_background_color(layers: &[SceneRenderLayer]) -> Option<String> {
    layers
        .iter()
        .find(|layer| {
            layer.kind == SceneNodeKind::Color
                && layer.opacity > 0.0
                && layer
                    .color
                    .as_deref()
                    .is_some_and(|color| !color.is_empty())
        })
        .and_then(|layer| layer.color.clone())
}

fn scene_rectangle_covers_target_without_shape_effects(
    layer: &SceneRenderLayer,
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

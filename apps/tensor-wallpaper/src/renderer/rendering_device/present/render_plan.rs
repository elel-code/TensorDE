#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use crate::core::scene::{
    SceneEffectUvTransform, SceneLayerCompositeKey, SceneMesh, SceneEffectMotion,
};
use crate::core::{
    FitMode, SceneBlendMode, SceneNodeKind, ScenePathFillRule, SceneSize, SceneTextAlign,
    SceneTextureRegion, SceneTransform,
};
use crate::renderer::rendering_device::effect_debug::{
    rendering_device_effect_debug_enabled, rendering_device_effect_debug_log,
};
use crate::renderer::{
    SceneDisplayPlan, SceneRenderAlphaTextureMode, SceneRenderImageEffectPass, SceneRenderLayer,
    SceneRenderTextureSlot,
};

use super::super::RenderingDeviceClearColor;
use super::render_item::RenderingDeviceRenderItem;

pub(in crate::renderer::rendering_device) fn rendering_device_render_item_clear_color(
    render_item: &RenderingDeviceRenderItem,
    fallback: RenderingDeviceClearColor,
) -> RenderingDeviceClearColor {
    match render_item {
        RenderingDeviceRenderItem::Scene { scene } => match &scene.display {
            Some(SceneDisplayPlan::Color { color }) => {
                rendering_device_clear_color_from_hex(color).unwrap_or(fallback)
            }
            _ => fallback,
        },
        _ => fallback,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::rendering_device) enum RenderingDeviceSceneDrawOpKind {
    Image,
    Video,
    ColorQuad,
    Rectangle,
    Ellipse,
    Text,
    Path,
    AudioResponse,
}

impl RenderingDeviceSceneDrawOpKind {
    pub(in crate::renderer::rendering_device) fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::ColorQuad => "color-quad",
            Self::Rectangle => "rectangle",
            Self::Ellipse => "ellipse",
            Self::Text => "text",
            Self::Path => "path",
            Self::AudioResponse => "audio-response",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::rendering_device) struct RenderingDeviceSceneEffectUvBounds {
    pub(in crate::renderer::rendering_device) left: f64,
    pub(in crate::renderer::rendering_device) top: f64,
    pub(in crate::renderer::rendering_device) width: f64,
    pub(in crate::renderer::rendering_device) height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::rendering_device) enum RenderingDeviceSceneEffectUvMapping {
    ScenePositionBounds,
    MaterialUvTransformed {
        scale_u: f64,
        scale_v: f64,
        offset_u: f64,
        offset_v: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::rendering_device) struct RenderingDeviceSceneEffectUvSpace {
    pub(in crate::renderer::rendering_device) mapping: RenderingDeviceSceneEffectUvMapping,
    pub(in crate::renderer::rendering_device) width: f64,
    pub(in crate::renderer::rendering_device) height: f64,
    pub(in crate::renderer::rendering_device) texture_region: Option<SceneTextureRegion>,
    pub(in crate::renderer::rendering_device) transform: SceneTransform,
    pub(in crate::renderer::rendering_device) bounds: Option<RenderingDeviceSceneEffectUvBounds>,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::rendering_device) struct RenderingDeviceSceneDrawOp {
    pub(in crate::renderer::rendering_device) layer_index: usize,
    pub(in crate::renderer::rendering_device) layer_id: String,
    pub(in crate::renderer::rendering_device) kind: RenderingDeviceSceneDrawOpKind,
    pub(in crate::renderer::rendering_device) opacity: f64,
    pub(in crate::renderer::rendering_device) source: Option<PathBuf>,
    pub(in crate::renderer::rendering_device) texture_slots: Vec<SceneRenderTextureSlot>,
    pub(in crate::renderer::rendering_device) alpha_texture_slot: Option<u32>,
    pub(in crate::renderer::rendering_device) alpha_texture_mode: SceneRenderAlphaTextureMode,
    pub(in crate::renderer::rendering_device) image_effect_passes: Vec<SceneRenderImageEffectPass>,
    pub(in crate::renderer::rendering_device) composite_key: Option<SceneLayerCompositeKey>,
    pub(in crate::renderer::rendering_device) texture_region: Option<SceneTextureRegion>,
    pub(in crate::renderer::rendering_device) effect_uv_space: Option<RenderingDeviceSceneEffectUvSpace>,
    pub(in crate::renderer::rendering_device) effect_motion: SceneEffectMotion,
    pub(in crate::renderer::rendering_device) blend_mode: SceneBlendMode,
    pub(in crate::renderer::rendering_device) color: Option<String>,
    pub(in crate::renderer::rendering_device) stroke_color: Option<String>,
    pub(in crate::renderer::rendering_device) stroke_width: Option<f64>,
    pub(in crate::renderer::rendering_device) corner_radius: Option<f64>,
    pub(in crate::renderer::rendering_device) width: Option<f64>,
    pub(in crate::renderer::rendering_device) height: Option<f64>,
    pub(in crate::renderer::rendering_device) mesh: Option<Arc<SceneMesh>>,
    pub(in crate::renderer::rendering_device) text: Option<String>,
    pub(in crate::renderer::rendering_device) font_size: Option<f64>,
    pub(in crate::renderer::rendering_device) font_family: Option<String>,
    pub(in crate::renderer::rendering_device) font_source: Option<PathBuf>,
    pub(in crate::renderer::rendering_device) font_weight: Option<String>,
    pub(in crate::renderer::rendering_device) text_align: Option<SceneTextAlign>,
    pub(in crate::renderer::rendering_device) path_data: Option<String>,
    pub(in crate::renderer::rendering_device) path_fill_rule: ScenePathFillRule,
    pub(in crate::renderer::rendering_device) fit: FitMode,
    pub(in crate::renderer::rendering_device) transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::rendering_device) struct RenderingDeviceSceneUnsupportedLayer {
    pub(in crate::renderer::rendering_device) layer_index: usize,
    pub(in crate::renderer::rendering_device) layer_id: String,
    pub(in crate::renderer::rendering_device) reason: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::rendering_device) struct RenderingDeviceSceneDrawPlan {
    pub(in crate::renderer::rendering_device) snapshot_time_ms: u64,
    pub(in crate::renderer::rendering_device) scene_size: Option<SceneSize>,
    pub(in crate::renderer::rendering_device) scene_fit: FitMode,
    pub(in crate::renderer::rendering_device) dynamic_topology_required: bool,
    pub(in crate::renderer::rendering_device) draw_ops: Vec<RenderingDeviceSceneDrawOp>,
    pub(in crate::renderer::rendering_device) unsupported_layers:
        Vec<RenderingDeviceSceneUnsupportedLayer>,
    pub(in crate::renderer::rendering_device) runtime_display_available: bool,
}

impl RenderingDeviceSceneDrawPlan {
    pub(in crate::renderer::rendering_device) fn has_draw_geometry(&self) -> bool {
        !self.draw_ops.is_empty() && self.unsupported_layers.is_empty()
    }
}

pub(in crate::renderer::rendering_device) fn rendering_device_scene_draw_plan(
    render_item: &RenderingDeviceRenderItem,
) -> Option<RenderingDeviceSceneDrawPlan> {
    let RenderingDeviceRenderItem::Scene { scene } = render_item
    else {
        return None;
    };
    Some(rendering_device_scene_draw_plan_from_layers(
        scene.snapshot_time_ms,
        scene.scene_size,
        scene.scene_fit,
        scene.dynamic_topology_required,
        scene.display.is_some(),
        &scene.layers,
    ))
}

pub(in crate::renderer::rendering_device) fn rendering_device_scene_draw_plan_from_layers(
    snapshot_time_ms: u64,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
    dynamic_topology_required: bool,
    runtime_display_available: bool,
    layers: &[SceneRenderLayer],
) -> RenderingDeviceSceneDrawPlan {
    let (draw_ops, unsupported_layers) = rendering_device_scene_draw_layers(layers);

    RenderingDeviceSceneDrawPlan {
        snapshot_time_ms,
        scene_size,
        scene_fit,
        dynamic_topology_required,
        draw_ops,
        unsupported_layers,
        runtime_display_available,
    }
}

fn rendering_device_scene_draw_layers(
    layers: &[SceneRenderLayer],
) -> (
    Vec<RenderingDeviceSceneDrawOp>,
    Vec<RenderingDeviceSceneUnsupportedLayer>,
) {
    let mut draw_ops = Vec::new();
    let mut unsupported_layers = Vec::new();
    for (index, layer) in layers.iter().enumerate() {
        if rendering_device_scene_layer_has_no_visual_draw(layer) {
            continue;
        }
        match rendering_device_scene_draw_op_kind(layer) {
            Ok(kind) => {
                let mut op = RenderingDeviceSceneDrawOp {
                    layer_index: index,
                    layer_id: layer.id.clone(),
                    kind,
                    opacity: layer.opacity.clamp(0.0, 1.0),
                    source: layer.source.clone(),
                    texture_slots: layer.texture_slots.clone(),
                    alpha_texture_slot: layer.alpha_texture_slot,
                    alpha_texture_mode: layer.alpha_texture_mode,
                    image_effect_passes: layer.image_effect_passes.clone(),
                    composite_key: layer.composite_key.clone(),
                    texture_region: layer.texture_region,
                    effect_uv_space: None,
                    effect_motion: layer.effect_motion,
                    blend_mode: layer.blend_mode,
                    color: layer.color.clone(),
                    stroke_color: layer.stroke_color.clone(),
                    stroke_width: layer.stroke_width,
                    corner_radius: layer.corner_radius,
                    width: layer.width,
                    height: layer.height,
                    mesh: layer.mesh.clone(),
                    text: layer.text.clone(),
                    font_size: layer.font_size,
                    font_family: layer.font_family.clone(),
                    font_source: layer.font_source.clone(),
                    font_weight: layer.font_weight.clone(),
                    text_align: layer.text_align,
                    path_data: layer.path_data.clone(),
                    path_fill_rule: layer.path_fill_rule,
                    fit: layer.fit,
                    transform: layer.transform,
                };
                op.effect_uv_space =
                    rendering_device_scene_opacity_effect_uv_space_from_render_op(&op);
                if rendering_device_effect_debug_enabled()
                    && (op.alpha_texture_slot.is_some() || !op.image_effect_passes.is_empty())
                {
                    rendering_device_effect_debug_log(
                        "render-plan.we-image-effect",
                        format_args!(
                            "layer_index={} id={} alpha_slot={:?} mode={} slots={} we_passes={} geometry={} effect_uv_space={}",
                            op.layer_index,
                            op.layer_id,
                            op.alpha_texture_slot,
                            op.alpha_texture_mode.as_str(),
                            rendering_device_scene_render_texture_slots_label(&op.texture_slots),
                            rendering_device_scene_image_effect_passes_label(&op.image_effect_passes),
                            rendering_device_scene_draw_op_geometry_label(&op),
                            rendering_device_scene_effect_uv_space_label(op.effect_uv_space)
                        ),
                    );
                }
                draw_ops.push(op);
            }
            Err(reason) => unsupported_layers.push(RenderingDeviceSceneUnsupportedLayer {
                layer_index: index,
                layer_id: layer.id.clone(),
                reason,
            }),
        }
    }
    (draw_ops, unsupported_layers)
}

fn rendering_device_scene_draw_op_geometry_label(op: &RenderingDeviceSceneDrawOp) -> String {
    format!(
        "size={}x{} opacity={:.3} transform=({:.3},{:.3}, scale={:.3}/{:.3}, rot={:.3}, anchor={:.3}/{:.3}) effect_chain={} mesh={}",
        op.width
            .map(|width| format!("{width:.3}"))
            .unwrap_or_else(|| "<none>".to_owned()),
        op.height
            .map(|height| format!("{height:.3}"))
            .unwrap_or_else(|| "<none>".to_owned()),
        op.opacity,
        op.transform.x,
        op.transform.y,
        op.transform.scale_x,
        op.transform.scale_y,
        op.transform.rotation_deg,
        op.transform.anchor_x,
        op.transform.anchor_y,
        rendering_device_scene_draw_op_effect_chain_label(op),
        op.mesh
            .as_ref()
            .map(|mesh| format!(
                "vertices={} indices={} bounds={}",
                mesh.vertices.len(),
                mesh.indices.len(),
                rendering_device_scene_mesh_bounds_label(mesh)
            ))
            .unwrap_or_else(|| "<none>".to_owned())
    )
}

fn rendering_device_scene_draw_op_effect_chain_label(op: &RenderingDeviceSceneDrawOp) -> &'static str {
    if rendering_device_scene_draw_op_has_effect_runtime(op, "builtin-iris-mask")
        && op.alpha_texture_slot.is_some()
        && matches!(op.alpha_texture_mode, SceneRenderAlphaTextureMode::Iris)
    {
        "we-known-iris-pass-inline-active"
    } else if rendering_device_scene_draw_op_has_effect_runtime(op, "builtin-opacity-mask")
        && op.alpha_texture_slot.is_some()
        && matches!(op.alpha_texture_mode, SceneRenderAlphaTextureMode::Multiply)
    {
        "we-known-opacity-pass-inline-active"
    } else if !op.image_effect_passes.is_empty() {
        "we-effect-pass-chain-present-not-executed"
    } else if op.alpha_texture_slot.is_some() {
        "alpha-texture-inline-mask"
    } else {
        "direct"
    }
}

fn rendering_device_scene_draw_op_has_effect_runtime(
    op: &RenderingDeviceSceneDrawOp,
    runtime: &str,
) -> bool {
    op.image_effect_passes
        .iter()
        .any(|pass| pass.runtime.as_deref() == Some(runtime))
}

fn rendering_device_scene_image_effect_passes_label(passes: &[SceneRenderImageEffectPass]) -> String {
    if passes.is_empty() {
        return "[]".to_owned();
    }
    let mut label = String::new();
    label.push('[');
    for (index, pass) in passes.iter().enumerate() {
        if index > 0 {
            label.push_str(", ");
        }
        label.push_str(&format!(
            "{}#{} runtime={} shader={} blend={} slots={}",
            pass.effect_file,
            pass.pass_index,
            pass.runtime.as_deref().unwrap_or("<none>"),
            pass.shader.as_deref().unwrap_or("<none>"),
            pass.blending.as_deref().unwrap_or("<none>"),
            rendering_device_scene_render_texture_slots_label(&pass.texture_slots)
        ));
    }
    label.push(']');
    label
}

fn rendering_device_scene_mesh_bounds_label(mesh: &SceneMesh) -> String {
    if mesh.vertices.is_empty() {
        return "<empty>".to_owned();
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut min_u = f64::INFINITY;
    let mut min_v = f64::INFINITY;
    let mut max_u = f64::NEG_INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for vertex in &mesh.vertices {
        min_x = min_x.min(vertex.x);
        min_y = min_y.min(vertex.y);
        max_x = max_x.max(vertex.x);
        max_y = max_y.max(vertex.y);
        min_u = min_u.min(vertex.u);
        min_v = min_v.min(vertex.v);
        max_u = max_u.max(vertex.u);
        max_v = max_v.max(vertex.v);
    }
    format!(
        "xy={min_x:.3}..{max_x:.3}/{min_y:.3}..{max_y:.3} uv={min_u:.3}..{max_u:.3}/{min_v:.3}..{max_v:.3}"
    )
}

fn rendering_device_scene_render_texture_slots_label(slots: &[SceneRenderTextureSlot]) -> String {
    let mut label = String::new();
    label.push('[');
    for (index, slot) in slots.iter().enumerate() {
        if index > 0 {
            label.push_str(", ");
        }
        label.push_str(&format!(
            "{}:{}{}",
            slot.slot,
            slot.source.display(),
            rendering_device_scene_render_texture_slot_extent_label(slot.width, slot.height)
        ));
    }
    label.push(']');
    label
}

fn rendering_device_scene_render_texture_slot_extent_label(
    width: Option<u32>,
    height: Option<u32>,
) -> String {
    match (width, height) {
        (Some(width), Some(height)) => format!("({width}x{height})"),
        _ => String::new(),
    }
}

fn rendering_device_scene_effect_uv_space_label(
    space: Option<RenderingDeviceSceneEffectUvSpace>,
) -> String {
    let Some(space) = space else {
        return "<none>".to_owned();
    };
    let bounds = space
        .bounds
        .map(|bounds| {
            format!(
                "bounds(left={:.3}, top={:.3}, width={:.3}, height={:.3})",
                bounds.left, bounds.top, bounds.width, bounds.height
            )
        })
        .unwrap_or_else(|| "bounds=<none>".to_owned());
    format!(
        "width={:.3} height={:.3} {} texture_region={:?} transform=({:.3},{:.3}, scale={:.3}/{:.3}, rot={:.3}, anchor={:.3}/{:.3}) {}",
        space.width,
        space.height,
        rendering_device_scene_effect_uv_mapping_label(space.mapping),
        space.texture_region,
        space.transform.x,
        space.transform.y,
        space.transform.scale_x,
        space.transform.scale_y,
        space.transform.rotation_deg,
        space.transform.anchor_x,
        space.transform.anchor_y,
        bounds
    )
}

fn rendering_device_scene_effect_uv_mapping_label(
    mapping: RenderingDeviceSceneEffectUvMapping,
) -> String {
    match mapping {
        RenderingDeviceSceneEffectUvMapping::ScenePositionBounds => {
            "mapping=scene-position-bounds".to_owned()
        }
        RenderingDeviceSceneEffectUvMapping::MaterialUvTransformed {
            scale_u,
            scale_v,
            offset_u,
            offset_v,
        } => {
            format!(
                "mapping=material-uv-transform(scale={scale_u:.6}/{scale_v:.6}, offset={offset_u:.6}/{offset_v:.6})"
            )
        }
    }
}

pub(in crate::renderer::rendering_device) fn rendering_device_scene_effect_uv_space_from_parts(
    width: Option<f64>,
    height: Option<f64>,
    mesh: Option<&SceneMesh>,
    texture_region: Option<SceneTextureRegion>,
    transform: SceneTransform,
) -> RenderingDeviceSceneEffectUvSpace {
    RenderingDeviceSceneEffectUvSpace {
        mapping: RenderingDeviceSceneEffectUvMapping::ScenePositionBounds,
        width: width.unwrap_or(0.0),
        height: height.unwrap_or(0.0),
        texture_region,
        transform,
        bounds: rendering_device_scene_effect_uv_bounds(width, height, mesh, transform),
    }
}

fn rendering_device_scene_opacity_effect_uv_space_from_render_ops(
    _target: &RenderingDeviceSceneDrawOp,
    carrier: &RenderingDeviceSceneDrawOp,
) -> RenderingDeviceSceneEffectUvSpace {
    rendering_device_scene_effect_uv_space_from_transform(
        rendering_device_scene_effect_uv_transform_for_render_passes(
            &carrier.image_effect_passes,
            carrier.alpha_texture_slot,
        ),
        carrier.width.unwrap_or(0.0),
        carrier.height.unwrap_or(0.0),
        carrier.texture_region,
        carrier.transform,
    )
}

pub(in crate::renderer::rendering_device) fn rendering_device_scene_effect_uv_space_from_transform(
    transform: Option<SceneEffectUvTransform>,
    width: f64,
    height: f64,
    texture_region: Option<SceneTextureRegion>,
    scene_transform: SceneTransform,
) -> RenderingDeviceSceneEffectUvSpace {
    let transform = transform.unwrap_or(SceneEffectUvTransform {
        mapping: Default::default(),
        source_slot: 0,
        mask_slot: 0,
        scale: [1.0, 1.0],
        offset: [0.0, 0.0],
        input_extent: None,
        mask_extent: None,
        mask_backing_extent: None,
    });
    RenderingDeviceSceneEffectUvSpace {
        mapping: RenderingDeviceSceneEffectUvMapping::MaterialUvTransformed {
            scale_u: transform.scale[0],
            scale_v: transform.scale[1],
            offset_u: transform.offset[0],
            offset_v: transform.offset[1],
        },
        width,
        height,
        texture_region,
        transform: scene_transform,
        bounds: None,
    }
}

pub(in crate::renderer::rendering_device) fn rendering_device_scene_effect_uv_transform_for_render_passes(
    passes: &[SceneRenderImageEffectPass],
    alpha_texture_slot: Option<u32>,
) -> Option<SceneEffectUvTransform> {
    passes
        .iter()
        .filter_map(|pass| pass.effect_uv_transform)
        .find(|transform| match alpha_texture_slot {
            Some(slot) => transform.mask_slot == slot,
            None => transform.mask_slot > 0,
        })
}

fn rendering_device_scene_opacity_effect_uv_space_from_render_op(
    op: &RenderingDeviceSceneDrawOp,
) -> Option<RenderingDeviceSceneEffectUvSpace> {
    op.alpha_texture_slot?;
    Some(rendering_device_scene_effect_uv_space_from_transform(
        rendering_device_scene_effect_uv_transform_for_render_passes(
            &op.image_effect_passes,
            op.alpha_texture_slot,
        ),
        op.width.unwrap_or(0.0),
        op.height.unwrap_or(0.0),
        op.texture_region,
        op.transform,
    ))
}

pub(in crate::renderer::rendering_device) fn rendering_device_scene_effect_uv_transform_for_scene_passes(
    passes: &[crate::core::scene::SceneImageEffectPass],
    alpha_texture_slot: Option<u32>,
) -> Option<SceneEffectUvTransform> {
    passes
        .iter()
        .filter_map(|pass| pass.effect_uv_transform)
        .find(|transform| match alpha_texture_slot {
            Some(slot) => transform.mask_slot == slot,
            None => transform.mask_slot > 0,
        })
}

pub(in crate::renderer::rendering_device) fn rendering_device_scene_effect_uv_bounds(
    width: Option<f64>,
    height: Option<f64>,
    mesh: Option<&SceneMesh>,
    transform: SceneTransform,
) -> Option<RenderingDeviceSceneEffectUvBounds> {
    let mesh = mesh?;
    let width = width?;
    let height = height?;
    if !width.is_finite() || !height.is_finite() || width <= f64::EPSILON || height <= f64::EPSILON
    {
        return None;
    }
    let local_offset_x = (0.5 - transform.anchor_x) * width;
    let local_offset_y = (0.5 - transform.anchor_y) * height;
    let mut left = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for vertex in &mesh.vertices {
        if !vertex.x.is_finite() || !vertex.y.is_finite() {
            return None;
        }
        let x = vertex.x + local_offset_x;
        let y = vertex.y + local_offset_y;
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    let bounds_width = right - left;
    let bounds_height = bottom - top;
    if !bounds_width.is_finite()
        || !bounds_height.is_finite()
        || bounds_width <= f64::EPSILON
        || bounds_height <= f64::EPSILON
    {
        return None;
    }
    Some(RenderingDeviceSceneEffectUvBounds {
        left,
        top,
        width: bounds_width,
        height: bounds_height,
    })
}

fn rendering_device_scene_draw_op_kind(
    layer: &SceneRenderLayer,
) -> Result<RenderingDeviceSceneDrawOpKind, &'static str> {
    match layer.kind {
        SceneNodeKind::Image => layer
            .source
            .as_ref()
            .map(|_| RenderingDeviceSceneDrawOpKind::Image)
            .ok_or("image-layer-missing-source"),
        SceneNodeKind::Video => layer
            .source
            .as_ref()
            .map(|_| RenderingDeviceSceneDrawOpKind::Video)
            .ok_or("video-layer-missing-source"),
        SceneNodeKind::Color => layer
            .color
            .as_ref()
            .map(|_| RenderingDeviceSceneDrawOpKind::ColorQuad)
            .ok_or("color-layer-missing-color"),
        SceneNodeKind::Rectangle => {
            if rendering_device_scene_layer_has_shape_paint(layer) {
                Ok(RenderingDeviceSceneDrawOpKind::Rectangle)
            } else {
                Err("rectangle-layer-missing-paint")
            }
        }
        SceneNodeKind::Ellipse => {
            if rendering_device_scene_layer_has_shape_paint(layer) {
                Ok(RenderingDeviceSceneDrawOpKind::Ellipse)
            } else {
                Err("ellipse-layer-missing-paint")
            }
        }
        SceneNodeKind::Text => layer
            .text
            .as_ref()
            .filter(|text| !text.is_empty())
            .ok_or("text-layer-missing-text")
            .and_then(|_| {
                layer
                    .color
                    .as_ref()
                    .filter(|color| !color.is_empty())
                    .map(|_| RenderingDeviceSceneDrawOpKind::Text)
                    .ok_or("text-layer-missing-color")
            }),
        SceneNodeKind::Path => layer
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
                    Ok(RenderingDeviceSceneDrawOpKind::Path)
                } else {
                    Err("path-layer-missing-paint")
                }
            }),
        SceneNodeKind::Group => Err("group-layer-needs-flattened-children"),
        SceneNodeKind::Shader => Err("shader-layer-needs-scene-shader-runtime"),
        SceneNodeKind::ParticleEmitter => Err("particle-layer-needs-scene-particle-runtime"),
        SceneNodeKind::AudioResponse => {
            if rendering_device_scene_layer_has_shape_paint(layer)
                && layer
                    .width
                    .is_some_and(|width| width.is_finite() && width > 0.0)
                && layer
                    .height
                    .is_some_and(|height| height.is_finite() && height > 0.0)
            {
                Ok(RenderingDeviceSceneDrawOpKind::AudioResponse)
            } else {
                Err("audio-response-layer-missing-builtin-visual-geometry")
            }
        }
        SceneNodeKind::Audio => Err("audio-layer-has-no-visual-draw-op"),
        SceneNodeKind::Script => Err("script-layer-needs-scene-script-runtime"),
        SceneNodeKind::Unknown => Err("unknown-layer-kind"),
    }
}

fn rendering_device_scene_layer_has_no_visual_draw(layer: &SceneRenderLayer) -> bool {
    if layer.opacity <= 0.0 {
        return true;
    }
    match layer.kind {
        SceneNodeKind::Audio | SceneNodeKind::Script => true,
        SceneNodeKind::Color => layer.color.as_deref().is_none_or(|color| color.is_empty()),
        SceneNodeKind::Rectangle | SceneNodeKind::Ellipse => {
            !rendering_device_scene_layer_has_shape_paint(layer)
        }
        _ => false,
    }
}

fn rendering_device_scene_layer_has_shape_paint(layer: &SceneRenderLayer) -> bool {
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

pub(in crate::renderer::rendering_device) fn rendering_device_clear_color_from_hex(
    value: &str,
) -> Option<RenderingDeviceClearColor> {
    let hex = value.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
    Some(RenderingDeviceClearColor { r, g, b, a: 1.0 })
}

#[cfg(test)]
mod tests;

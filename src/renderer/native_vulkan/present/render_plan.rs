#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use crate::core::scene::{SceneLayerCompositeKey, SceneMesh, SceneNativeEffectMotion};
use crate::core::{
    FitMode, SceneBlendMode, SceneNodeKind, ScenePathFillRule, SceneSize, SceneTextAlign,
    SceneTextureRegion, SceneTransform,
};
use crate::renderer::native_vulkan::effect_debug::{
    native_vulkan_effect_debug_enabled, native_vulkan_effect_debug_log,
};
use crate::renderer::{
    SceneDisplayPlan, SceneRenderAlphaTextureMode, SceneRenderImageEffectPass, SceneRenderLayer,
    SceneRenderTextureSlot,
};

use super::super::NativeVulkanClearColor;
use super::render_item::NativeVulkanRenderItem;

pub(in crate::renderer::native_vulkan) fn native_vulkan_render_item_clear_color(
    render_item: &NativeVulkanRenderItem,
    fallback: NativeVulkanClearColor,
) -> NativeVulkanClearColor {
    match render_item {
        NativeVulkanRenderItem::Scene {
            display: Some(SceneDisplayPlan::Color { color }),
            ..
        } => native_vulkan_clear_color_from_hex(color).unwrap_or(fallback),
        _ => fallback,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneDrawOpKind {
    Image,
    Video,
    ColorQuad,
    Rectangle,
    Ellipse,
    Text,
    Path,
    AudioResponse,
}

impl NativeVulkanSceneDrawOpKind {
    pub(in crate::renderer::native_vulkan) fn as_str(self) -> &'static str {
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
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectUvBounds {
    pub(in crate::renderer::native_vulkan) left: f64,
    pub(in crate::renderer::native_vulkan) top: f64,
    pub(in crate::renderer::native_vulkan) width: f64,
    pub(in crate::renderer::native_vulkan) height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneEffectUvMapping {
    ScenePositionBounds,
    MaterialUvScaled { scale_u: f64, scale_v: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectUvSpace {
    pub(in crate::renderer::native_vulkan) mapping: NativeVulkanSceneEffectUvMapping,
    pub(in crate::renderer::native_vulkan) width: f64,
    pub(in crate::renderer::native_vulkan) height: f64,
    pub(in crate::renderer::native_vulkan) texture_region: Option<SceneTextureRegion>,
    pub(in crate::renderer::native_vulkan) transform: SceneTransform,
    pub(in crate::renderer::native_vulkan) bounds: Option<NativeVulkanSceneEffectUvBounds>,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneDrawOp {
    pub(in crate::renderer::native_vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan) layer_id: String,
    pub(in crate::renderer::native_vulkan) kind: NativeVulkanSceneDrawOpKind,
    pub(in crate::renderer::native_vulkan) opacity: f64,
    pub(in crate::renderer::native_vulkan) source: Option<PathBuf>,
    pub(in crate::renderer::native_vulkan) texture_slots: Vec<SceneRenderTextureSlot>,
    pub(in crate::renderer::native_vulkan) alpha_texture_slot: Option<u32>,
    pub(in crate::renderer::native_vulkan) alpha_texture_mode: SceneRenderAlphaTextureMode,
    pub(in crate::renderer::native_vulkan) image_effect_passes: Vec<SceneRenderImageEffectPass>,
    pub(in crate::renderer::native_vulkan) composite_key: Option<SceneLayerCompositeKey>,
    pub(in crate::renderer::native_vulkan) texture_region: Option<SceneTextureRegion>,
    pub(in crate::renderer::native_vulkan) effect_uv_space: Option<NativeVulkanSceneEffectUvSpace>,
    pub(in crate::renderer::native_vulkan) effect_motion: SceneNativeEffectMotion,
    pub(in crate::renderer::native_vulkan) blend_mode: SceneBlendMode,
    pub(in crate::renderer::native_vulkan) color: Option<String>,
    pub(in crate::renderer::native_vulkan) stroke_color: Option<String>,
    pub(in crate::renderer::native_vulkan) stroke_width: Option<f64>,
    pub(in crate::renderer::native_vulkan) corner_radius: Option<f64>,
    pub(in crate::renderer::native_vulkan) width: Option<f64>,
    pub(in crate::renderer::native_vulkan) height: Option<f64>,
    pub(in crate::renderer::native_vulkan) mesh: Option<Arc<SceneMesh>>,
    pub(in crate::renderer::native_vulkan) text: Option<String>,
    pub(in crate::renderer::native_vulkan) font_size: Option<f64>,
    pub(in crate::renderer::native_vulkan) font_family: Option<String>,
    pub(in crate::renderer::native_vulkan) font_source: Option<PathBuf>,
    pub(in crate::renderer::native_vulkan) font_weight: Option<String>,
    pub(in crate::renderer::native_vulkan) text_align: Option<SceneTextAlign>,
    pub(in crate::renderer::native_vulkan) path_data: Option<String>,
    pub(in crate::renderer::native_vulkan) path_fill_rule: ScenePathFillRule,
    pub(in crate::renderer::native_vulkan) fit: FitMode,
    pub(in crate::renderer::native_vulkan) transform: SceneTransform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneUnsupportedLayer {
    pub(in crate::renderer::native_vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan) layer_id: String,
    pub(in crate::renderer::native_vulkan) reason: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneDrawPlan {
    pub(in crate::renderer::native_vulkan) snapshot_time_ms: u64,
    pub(in crate::renderer::native_vulkan) scene_size: Option<SceneSize>,
    pub(in crate::renderer::native_vulkan) scene_fit: FitMode,
    pub(in crate::renderer::native_vulkan) dynamic_topology_required: bool,
    pub(in crate::renderer::native_vulkan) draw_ops: Vec<NativeVulkanSceneDrawOp>,
    pub(in crate::renderer::native_vulkan) unsupported_layers:
        Vec<NativeVulkanSceneUnsupportedLayer>,
    pub(in crate::renderer::native_vulkan) runtime_display_available: bool,
}

impl NativeVulkanSceneDrawPlan {
    pub(in crate::renderer::native_vulkan) fn native_draw_ready(&self) -> bool {
        !self.draw_ops.is_empty() && self.unsupported_layers.is_empty()
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_draw_plan(
    render_item: &NativeVulkanRenderItem,
) -> Option<NativeVulkanSceneDrawPlan> {
    let NativeVulkanRenderItem::Scene {
        layers,
        display,
        snapshot_time_ms,
        scene_size,
        scene_fit,
        dynamic_topology_required,
        ..
    } = render_item
    else {
        return None;
    };
    Some(native_vulkan_scene_draw_plan_from_layers(
        *snapshot_time_ms,
        *scene_size,
        *scene_fit,
        *dynamic_topology_required,
        display.is_some(),
        layers,
    ))
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_draw_plan_from_layers(
    snapshot_time_ms: u64,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
    dynamic_topology_required: bool,
    runtime_display_available: bool,
    layers: &[SceneRenderLayer],
) -> NativeVulkanSceneDrawPlan {
    let (draw_ops, unsupported_layers) = native_vulkan_scene_draw_layers(layers);

    NativeVulkanSceneDrawPlan {
        snapshot_time_ms,
        scene_size,
        scene_fit,
        dynamic_topology_required,
        draw_ops,
        unsupported_layers,
        runtime_display_available,
    }
}

fn native_vulkan_scene_draw_layers(
    layers: &[SceneRenderLayer],
) -> (
    Vec<NativeVulkanSceneDrawOp>,
    Vec<NativeVulkanSceneUnsupportedLayer>,
) {
    let mut draw_ops = Vec::new();
    let mut unsupported_layers = Vec::new();
    for (index, layer) in layers.iter().enumerate() {
        if native_vulkan_scene_layer_has_no_visual_draw(layer) {
            continue;
        }
        match native_vulkan_scene_draw_op_kind(layer) {
            Ok(kind) => {
                let mut op = NativeVulkanSceneDrawOp {
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
                    native_vulkan_scene_opacity_effect_uv_space_from_render_op(&op);
                if native_vulkan_effect_debug_enabled()
                    && (op.alpha_texture_slot.is_some() || !op.image_effect_passes.is_empty())
                {
                    native_vulkan_effect_debug_log(
                        "render-plan.we-image-effect",
                        format_args!(
                            "layer_index={} id={} alpha_slot={:?} mode={} slots={} we_passes={} geometry={} effect_uv_space={}",
                            op.layer_index,
                            op.layer_id,
                            op.alpha_texture_slot,
                            op.alpha_texture_mode.as_str(),
                            native_vulkan_scene_render_texture_slots_label(&op.texture_slots),
                            native_vulkan_scene_image_effect_passes_label(&op.image_effect_passes),
                            native_vulkan_scene_draw_op_geometry_label(&op),
                            native_vulkan_scene_effect_uv_space_label(op.effect_uv_space)
                        ),
                    );
                }
                draw_ops.push(op);
            }
            Err(reason) => unsupported_layers.push(NativeVulkanSceneUnsupportedLayer {
                layer_index: index,
                layer_id: layer.id.clone(),
                reason,
            }),
        }
    }
    (draw_ops, unsupported_layers)
}

fn native_vulkan_scene_draw_op_geometry_label(op: &NativeVulkanSceneDrawOp) -> String {
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
        native_vulkan_scene_draw_op_effect_chain_label(op),
        op.mesh
            .as_ref()
            .map(|mesh| format!(
                "vertices={} indices={} bounds={}",
                mesh.vertices.len(),
                mesh.indices.len(),
                native_vulkan_scene_mesh_bounds_label(mesh)
            ))
            .unwrap_or_else(|| "<none>".to_owned())
    )
}

fn native_vulkan_scene_draw_op_effect_chain_label(op: &NativeVulkanSceneDrawOp) -> &'static str {
    if native_vulkan_scene_draw_op_has_effect_runtime(op, "native-iris-mask")
        && op.alpha_texture_slot.is_some()
        && matches!(op.alpha_texture_mode, SceneRenderAlphaTextureMode::Iris)
    {
        "we-known-iris-pass-inline-active"
    } else if native_vulkan_scene_draw_op_has_effect_runtime(op, "native-opacity-mask")
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

fn native_vulkan_scene_draw_op_has_effect_runtime(
    op: &NativeVulkanSceneDrawOp,
    runtime: &str,
) -> bool {
    op.image_effect_passes
        .iter()
        .any(|pass| pass.runtime.as_deref() == Some(runtime))
}

fn native_vulkan_scene_image_effect_passes_label(passes: &[SceneRenderImageEffectPass]) -> String {
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
            native_vulkan_scene_render_texture_slots_label(&pass.texture_slots)
        ));
    }
    label.push(']');
    label
}

fn native_vulkan_scene_mesh_bounds_label(mesh: &SceneMesh) -> String {
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

fn native_vulkan_scene_render_texture_slots_label(slots: &[SceneRenderTextureSlot]) -> String {
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
            native_vulkan_scene_render_texture_slot_extent_label(slot.width, slot.height)
        ));
    }
    label.push(']');
    label
}

fn native_vulkan_scene_render_texture_slot_extent_label(
    width: Option<u32>,
    height: Option<u32>,
) -> String {
    match (width, height) {
        (Some(width), Some(height)) => format!("({width}x{height})"),
        _ => String::new(),
    }
}

fn native_vulkan_scene_effect_uv_space_label(
    space: Option<NativeVulkanSceneEffectUvSpace>,
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
        native_vulkan_scene_effect_uv_mapping_label(space.mapping),
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

fn native_vulkan_scene_effect_uv_mapping_label(
    mapping: NativeVulkanSceneEffectUvMapping,
) -> String {
    match mapping {
        NativeVulkanSceneEffectUvMapping::ScenePositionBounds => {
            "mapping=scene-position-bounds".to_owned()
        }
        NativeVulkanSceneEffectUvMapping::MaterialUvScaled { scale_u, scale_v } => {
            format!("mapping=material-uv-scaled(scale={scale_u:.6}/{scale_v:.6})")
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_effect_uv_space_from_parts(
    width: Option<f64>,
    height: Option<f64>,
    mesh: Option<&SceneMesh>,
    texture_region: Option<SceneTextureRegion>,
    transform: SceneTransform,
) -> NativeVulkanSceneEffectUvSpace {
    NativeVulkanSceneEffectUvSpace {
        mapping: NativeVulkanSceneEffectUvMapping::ScenePositionBounds,
        width: width.unwrap_or(0.0),
        height: height.unwrap_or(0.0),
        texture_region,
        transform,
        bounds: native_vulkan_scene_effect_uv_bounds(width, height, mesh, transform),
    }
}

fn native_vulkan_scene_opacity_effect_uv_space_from_render_ops(
    _target: &NativeVulkanSceneDrawOp,
    carrier: &NativeVulkanSceneDrawOp,
) -> NativeVulkanSceneEffectUvSpace {
    let (scale_u, scale_v) = native_vulkan_scene_opacity_effect_material_uv_scale_for_render_slots(
        &carrier.texture_slots,
        carrier.alpha_texture_slot,
    );
    NativeVulkanSceneEffectUvSpace {
        mapping: NativeVulkanSceneEffectUvMapping::MaterialUvScaled { scale_u, scale_v },
        width: carrier.width.unwrap_or(0.0),
        height: carrier.height.unwrap_or(0.0),
        texture_region: carrier.texture_region,
        transform: carrier.transform,
        bounds: None,
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_opacity_effect_material_uv_scale(
    _base_width: Option<u32>,
    _base_height: Option<u32>,
    _alpha_width: Option<u32>,
    _alpha_height: Option<u32>,
) -> (f64, f64) {
    // These dimensions are Gilder's decoded logical extents, not Wallpaper
    // Engine backing texture extents. Scaling by them re-samples the eye masks
    // into the wrong half-sized area. Keep pass material UV identity until the
    // converter preserves separate backing extents.
    (1.0, 1.0)
}

fn native_vulkan_scene_opacity_effect_material_uv_scale_for_render_slots(
    slots: &[SceneRenderTextureSlot],
    alpha_texture_slot: Option<u32>,
) -> (f64, f64) {
    let Some(alpha_slot) = alpha_texture_slot else {
        return (1.0, 1.0);
    };
    let base = slots.iter().find(|slot| slot.slot == 0);
    let alpha = slots.iter().find(|slot| slot.slot == alpha_slot);
    native_vulkan_scene_opacity_effect_material_uv_scale(
        base.and_then(|slot| slot.width),
        base.and_then(|slot| slot.height),
        alpha.and_then(|slot| slot.width),
        alpha.and_then(|slot| slot.height),
    )
}

fn native_vulkan_scene_opacity_effect_uv_space_from_render_op(
    op: &NativeVulkanSceneDrawOp,
) -> Option<NativeVulkanSceneEffectUvSpace> {
    op.alpha_texture_slot?;
    let (scale_u, scale_v) = native_vulkan_scene_opacity_effect_material_uv_scale_for_render_slots(
        &op.texture_slots,
        op.alpha_texture_slot,
    );
    Some(NativeVulkanSceneEffectUvSpace {
        mapping: NativeVulkanSceneEffectUvMapping::MaterialUvScaled { scale_u, scale_v },
        width: op.width.unwrap_or(0.0),
        height: op.height.unwrap_or(0.0),
        texture_region: op.texture_region,
        transform: op.transform,
        bounds: None,
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_effect_uv_bounds(
    width: Option<f64>,
    height: Option<f64>,
    mesh: Option<&SceneMesh>,
    transform: SceneTransform,
) -> Option<NativeVulkanSceneEffectUvBounds> {
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
    Some(NativeVulkanSceneEffectUvBounds {
        left,
        top,
        width: bounds_width,
        height: bounds_height,
    })
}

fn native_vulkan_scene_draw_op_kind(
    layer: &SceneRenderLayer,
) -> Result<NativeVulkanSceneDrawOpKind, &'static str> {
    match layer.kind {
        SceneNodeKind::Image => layer
            .source
            .as_ref()
            .map(|_| NativeVulkanSceneDrawOpKind::Image)
            .ok_or("image-layer-missing-source"),
        SceneNodeKind::Video => layer
            .source
            .as_ref()
            .map(|_| NativeVulkanSceneDrawOpKind::Video)
            .ok_or("video-layer-missing-source"),
        SceneNodeKind::Color => layer
            .color
            .as_ref()
            .map(|_| NativeVulkanSceneDrawOpKind::ColorQuad)
            .ok_or("color-layer-missing-color"),
        SceneNodeKind::Rectangle => {
            if native_vulkan_scene_layer_has_shape_paint(layer) {
                Ok(NativeVulkanSceneDrawOpKind::Rectangle)
            } else {
                Err("rectangle-layer-missing-paint")
            }
        }
        SceneNodeKind::Ellipse => {
            if native_vulkan_scene_layer_has_shape_paint(layer) {
                Ok(NativeVulkanSceneDrawOpKind::Ellipse)
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
                    .map(|_| NativeVulkanSceneDrawOpKind::Text)
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
                    Ok(NativeVulkanSceneDrawOpKind::Path)
                } else {
                    Err("path-layer-missing-paint")
                }
            }),
        SceneNodeKind::Group => Err("group-layer-needs-flattened-children"),
        SceneNodeKind::Shader => Err("shader-layer-needs-scene-shader-runtime"),
        SceneNodeKind::ParticleEmitter => Err("particle-layer-needs-scene-particle-runtime"),
        SceneNodeKind::AudioResponse => {
            if native_vulkan_scene_layer_has_shape_paint(layer)
                && layer
                    .width
                    .is_some_and(|width| width.is_finite() && width > 0.0)
                && layer
                    .height
                    .is_some_and(|height| height.is_finite() && height > 0.0)
            {
                Ok(NativeVulkanSceneDrawOpKind::AudioResponse)
            } else {
                Err("audio-response-layer-missing-native-visual-geometry")
            }
        }
        SceneNodeKind::Audio => Err("audio-layer-has-no-visual-draw-op"),
        SceneNodeKind::Script => Err("script-layer-needs-scene-script-runtime"),
        SceneNodeKind::Unknown => Err("unknown-layer-kind"),
    }
}

fn native_vulkan_scene_layer_has_no_visual_draw(layer: &SceneRenderLayer) -> bool {
    if layer.opacity <= 0.0 {
        return true;
    }
    match layer.kind {
        SceneNodeKind::Audio | SceneNodeKind::Script => true,
        SceneNodeKind::Color => layer.color.as_deref().is_none_or(|color| color.is_empty()),
        SceneNodeKind::Rectangle | SceneNodeKind::Ellipse => {
            !native_vulkan_scene_layer_has_shape_paint(layer)
        }
        _ => false,
    }
}

fn native_vulkan_scene_layer_has_shape_paint(layer: &SceneRenderLayer) -> bool {
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
    use crate::core::path::PackagePath;
    use crate::core::scene::SceneMeshVertex;
    use crate::core::{FitMode, SceneBlendMode, SceneNodeKind, ScenePathFillRule, SceneSystems};
    use crate::renderer::native_vulkan::{NativeVulkanClearColor, NativeVulkanRenderItem};
    use crate::renderer::{
        SceneDisplayPlan, SceneRenderImageEffectPass, SceneRenderLayer, SceneRenderTextureSlot,
    };
    use std::path::PathBuf;

    #[test]
    fn scene_color_display_overrides_default_clear_color() {
        let fallback = NativeVulkanClearColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let item = NativeVulkanRenderItem::Scene {
            output_name: "HDMI-A-1".to_owned(),
            scene_source: Some(PathBuf::from("/tmp/scene.json")),
            display: Some(SceneDisplayPlan::Color {
                color: "#102030".to_owned(),
            }),
            display_image: None,
            display_color: Some("#102030".to_owned()),
            manifest_max_fps: Some(60),
            layer_count: 0,
            layers: Vec::new(),
            scene_systems: SceneSystems::default(),
            audio_cue_count: 0,
            bound_properties: Vec::new(),
            timeline_animation_count: 0,
            timeline_animated_layer_count: 0,
            puppet_animation_layer_count: 0,
            property_binding_count: 0,
            cursor_parallax_input_ready: false,
            dynamic_topology_required: false,
            scene_scenescript_binding_count: 0,
            scene_material_graph_count: 0,
            scene_material_graph_resource_count: 0,
            scene_effect_graph_count: 0,
            scene_audio_response_binding_count: 0,
            unsupported_scene_features: Vec::new(),
            snapshot_time_ms: 0,
            scene_size: None,
            scene_fit: FitMode::Cover,
            target_max_fps: Some(60),
            renderer_status: "deterministic-scene-snapshot-ready-for-vulkan-passes",
        };

        let color = native_vulkan_render_item_clear_color(&item, fallback);

        assert!((color.r - 16.0 / 255.0).abs() < f32::EPSILON);
        assert!((color.g - 32.0 / 255.0).abs() < f32::EPSILON);
        assert!((color.b - 48.0 / 255.0).abs() < f32::EPSILON);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn scene_draw_plan_keeps_opacity_mask_duplicate_as_independent_draw() {
        let composite_key = Some(SceneLayerCompositeKey {
            parent_source_id: Some("937".to_owned()),
            puppet_attachment: "eye".to_owned(),
            original_path: "models/eye.json".to_owned(),
            base_source: PackagePath::new("assets/eye.gtex").unwrap(),
        });
        let mut base = scene_layer("eye-base", SceneNodeKind::Image);
        base.source = Some(PathBuf::from("/tmp/eye.gtex"));
        base.composite_key = composite_key.clone();
        base.texture_slots = vec![SceneRenderTextureSlot {
            slot: 0,
            source: PathBuf::from("/tmp/eye.gtex"),
            width: Some(663),
            height: Some(230),
        }];
        base.mesh = Some(Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -10.0,
                    y: -20.0,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 10.0,
                    y: -20.0,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -10.0,
                    y: 20.0,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2],
            skin: None,
            puppet_clips: Vec::new(),
        }));
        let mut carrier = scene_layer("eye-opacity", SceneNodeKind::Image);
        carrier.source = Some(PathBuf::from("/tmp/eye.gtex"));
        carrier.composite_key = composite_key.clone();
        carrier.alpha_texture_slot = Some(1);
        carrier.alpha_texture_mode = SceneRenderAlphaTextureMode::Multiply;
        carrier.texture_slots = vec![
            SceneRenderTextureSlot {
                slot: 0,
                source: PathBuf::from("/tmp/eye.gtex"),
                width: Some(663),
                height: Some(230),
            },
            SceneRenderTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/eye-mask.gtex"),
                width: Some(331),
                height: Some(115),
            },
        ];
        carrier.image_effect_passes = vec![SceneRenderImageEffectPass {
            effect_file: "effects/opacity/effect.json".to_owned(),
            runtime: Some("native-opacity-mask".to_owned()),
            pass_index: 0,
            shader: Some("effects/opacity".to_owned()),
            blending: Some("normal".to_owned()),
            texture_slots: vec![SceneRenderTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/eye-mask.gtex"),
                width: Some(331),
                height: Some(115),
            }],
            constant_shader_values: Default::default(),
        }];
        carrier.mesh = base.mesh.clone();

        let plan = native_vulkan_scene_draw_plan_from_layers(
            0,
            None,
            FitMode::Cover,
            false,
            true,
            &[base, carrier],
        );

        assert_eq!(plan.draw_ops.len(), 2);
        assert_eq!(plan.draw_ops[0].layer_id, "eye-base");
        assert_eq!(plan.draw_ops[0].alpha_texture_slot, None);
        assert_eq!(
            plan.draw_ops[0].alpha_texture_mode,
            SceneRenderAlphaTextureMode::Multiply
        );
        assert!(plan.draw_ops[0].effect_uv_space.is_none());
        assert_eq!(plan.draw_ops[0].composite_key, composite_key);
        assert_eq!(plan.draw_ops[0].texture_slots.len(), 1);
        assert_eq!(plan.draw_ops[1].layer_id, "eye-opacity");
        assert_eq!(plan.draw_ops[1].alpha_texture_slot, Some(1));
        assert_eq!(
            plan.draw_ops[1].alpha_texture_mode,
            SceneRenderAlphaTextureMode::Multiply
        );
        assert_eq!(
            plan.draw_ops[1].effect_uv_space.map(|space| space.mapping),
            Some(NativeVulkanSceneEffectUvMapping::MaterialUvScaled {
                scale_u: 1.0,
                scale_v: 1.0
            })
        );
        assert_eq!(plan.draw_ops[1].composite_key, composite_key);
        assert_eq!(plan.draw_ops[1].texture_slots.len(), 2);
        assert_eq!(plan.draw_ops[1].image_effect_passes.len(), 1);
        assert_eq!(
            plan.draw_ops[1].image_effect_passes[0].effect_file,
            "effects/opacity/effect.json"
        );
        assert_eq!(
            plan.draw_ops[1].texture_slots[1].source,
            PathBuf::from("/tmp/eye-mask.gtex")
        );
    }

    fn scene_layer(id: &str, kind: SceneNodeKind) -> SceneRenderLayer {
        SceneRenderLayer {
            id: id.to_owned(),
            kind,
            source: None,
            texture_slots: Vec::new(),
            alpha_texture_slot: None,
            alpha_texture_mode: Default::default(),
            image_effect_passes: Vec::new(),
            composite_key: None,
            texture_region: None,
            effect_motion: Default::default(),
            blend_mode: SceneBlendMode::Alpha,
            audio: Vec::new(),
            color: None,
            stroke_color: None,
            stroke_width: None,
            corner_radius: None,
            width: Some(100.0),
            height: Some(50.0),
            mesh: None,
            text: None,
            font_size: None,
            font_family: None,
            font_source: None,
            font_weight: None,
            text_align: None,
            path_data: None,
            path_fill_rule: ScenePathFillRule::Nonzero,
            fit: FitMode::Cover,
            opacity: 1.0,
            transform: SceneTransform::default(),
        }
    }
}
